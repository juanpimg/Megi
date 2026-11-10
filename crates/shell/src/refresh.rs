//! Auto-refresh scheduling for pages with live timers (`setInterval`, pending timers).
//!
//! The `render` pipeline executes page scripts, timers and lifecycle events synchronously
//! with a **virtual clock**; a render that leaves `script_timers_pending > 0` means the page
//! wants to keep running (typically `setInterval`, or a `setTimeout` chain that outlived the
//! virtual-time budget). The desktop window can then repaint on a timer so the page keeps
//! making progress, using the normal render worker and generation-based cancellation.
//!
//! Policy (all pure and unit-tested):
//! - disabled unless `--refresh-ms` was given ([`RefreshPolicy::disabled`]);
//! - refresh only when the last frame executed at least one script **and** left pending
//!   timers (`script_timers_pending > 0`); a page without timers therefore never enters a
//!   refresh loop;
//! - after each refresh the timer re-arms only when the new frame still reports pending
//!   timers (one-shot deadline: [`RefreshTimer::take_due`] clears it), so repeated renders
//!   cannot pile up;
//! - scroll state is preserved by the window (refresh requests do not reset it).
//!
//! The page is re-rendered from scratch with the same deterministic virtual clock, so the
//! refresh is byte-identical unless the page's scripts depend on external state; advancing
//! the virtual clock between refreshes is a documented follow-up.

use std::time::{Duration, Instant};

/// Default auto-refresh interval when `--refresh-ms` is passed without a value.
pub const DEFAULT_REFRESH_MS: u64 = 250;
/// Smallest accepted auto-refresh interval (avoid hammering the worker).
pub const MIN_REFRESH_MS: u64 = 16;

/// Auto-refresh configuration for one window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshPolicy {
    interval: Option<Duration>,
}

impl RefreshPolicy {
    /// Auto-refresh disabled (no `--refresh-ms`).
    pub const fn disabled() -> Self {
        Self { interval: None }
    }

    /// Auto-refresh every `ms` milliseconds; `0` disables it.
    pub fn every_ms(ms: u64) -> Self {
        if ms == 0 {
            Self::disabled()
        } else {
            Self {
                interval: Some(Duration::from_millis(ms.max(MIN_REFRESH_MS))),
            }
        }
    }

    /// Whether auto-refresh is enabled.
    pub const fn is_enabled(&self) -> bool {
        self.interval.is_some()
    }

    /// Configured interval, if enabled.
    pub const fn interval(&self) -> Option<Duration> {
        self.interval
    }

    /// Configured interval, or the default when auto-refresh is disabled.
    pub const fn interval_or_default(&self) -> Duration {
        match self.interval {
            Some(interval) => interval,
            None => Duration::from_millis(DEFAULT_REFRESH_MS),
        }
    }
}

/// One-shot refresh timer driven by the event loop.
#[derive(Clone, Debug, Default)]
pub struct RefreshTimer {
    policy: RefreshPolicy,
    deadline: Option<Instant>,
}

impl RefreshTimer {
    /// Creates a timer for `policy`.
    pub const fn new(policy: RefreshPolicy) -> Self {
        Self {
            policy,
            deadline: None,
        }
    }

    /// Replaces the policy (clears any pending deadline when disabled).
    pub fn set_policy(&mut self, policy: RefreshPolicy) {
        self.policy = policy;
        if !policy.is_enabled() {
            self.deadline = None;
        }
    }

    /// The active policy.
    pub const fn policy(&self) -> RefreshPolicy {
        self.policy
    }

    /// Decides whether the latest frame calls for a refresh.
    ///
    /// A page must have executed scripts and left timers pending; pages without timers
    /// therefore never refresh.
    pub fn should_refresh(&self, scripts_executed: usize, script_timers_pending: usize) -> bool {
        self.policy.is_enabled() && scripts_executed > 0 && script_timers_pending > 0
    }

    /// Arms the next refresh at `now + interval`; returns the deadline.
    pub fn schedule(&mut self, now: Instant) -> Option<Instant> {
        let interval = self.policy.interval?;
        let deadline = now + interval;
        self.deadline = Some(deadline);
        Some(deadline)
    }

    /// Clears any pending refresh.
    pub fn clear(&mut self) {
        self.deadline = None;
    }

    /// Pending deadline, if armed.
    pub const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Fires the refresh when due, clearing the deadline (the window re-arms it only
    /// after the next frame reports pending timers again).
    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn disabled_policy_never_refreshes() {
        let timer = RefreshTimer::new(RefreshPolicy::disabled());
        assert!(!timer.policy().is_enabled());
        assert!(!timer.should_refresh(5, 3));
        let mut timer = timer;
        assert_eq!(timer.schedule(Instant::now()), None);
    }

    #[test]
    fn every_ms_zero_disables_and_clamps_the_minimum() {
        assert!(!RefreshPolicy::every_ms(0).is_enabled());
        assert_eq!(
            RefreshPolicy::every_ms(1).interval(),
            Some(Duration::from_millis(MIN_REFRESH_MS))
        );
        assert_eq!(
            RefreshPolicy::every_ms(250).interval(),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            RefreshPolicy::every_ms(250).interval_or_default(),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn refresh_only_when_scripts_left_timers_pending() {
        let timer = RefreshTimer::new(RefreshPolicy::every_ms(250));
        assert!(
            timer.should_refresh(1, 1),
            "setInterval keeps pending timers"
        );
        assert!(!timer.should_refresh(0, 3), "no scripts executed");
        assert!(!timer.should_refresh(1, 0), "no pending timers ⇒ no loop");
    }

    #[test]
    fn deadline_fires_once_and_clears() {
        let start = Instant::now();
        let mut timer = RefreshTimer::new(RefreshPolicy::every_ms(250));
        assert!(!timer.take_due(start));
        let deadline = timer.schedule(start).expect("armed");
        assert_eq!(timer.deadline(), Some(deadline));
        assert!(!timer.take_due(start + Duration::from_millis(249)));
        assert!(timer.take_due(deadline));
        assert_eq!(
            timer.deadline(),
            None,
            "one-shot: must be re-armed by the next frame"
        );
        assert!(!timer.take_due(deadline + Duration::from_secs(1)));
    }

    #[test]
    fn disabling_the_policy_clears_the_deadline() {
        let start = Instant::now();
        let mut timer = RefreshTimer::new(RefreshPolicy::every_ms(100));
        timer.schedule(start);
        timer.set_policy(RefreshPolicy::disabled());
        assert_eq!(timer.deadline(), None);
        assert!(!timer.should_refresh(1, 1));
    }
}
