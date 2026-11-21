//! Real-page performance budgets (F4-PERF-BUDGETS / #1014).
//!
//! Pure thresholds over measured stages: a render that exceeds them still succeeds, but the
//! breach is reported so real-site regressions become visible instead of silent.

use super::*;
use std::time::Duration;

/// Thresholds for stages that dominated the real-page audit (#1014).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerformanceBudget {
    pub total: Duration,
    pub styles_pre_script: Duration,
    pub images: Duration,
    pub backgrounds: Duration,
}

impl Default for PerformanceBudget {
    fn default() -> Self {
        Self {
            total: Duration::from_secs(3),
            styles_pre_script: Duration::from_millis(200),
            images: Duration::from_millis(500),
            backgrounds: Duration::from_millis(500),
        }
    }
}

impl PerformanceBudget {
    /// Total elapsed time across all measured stages.
    pub fn total_of(timings: &RenderTimings) -> Duration {
        timings
            .fetch
            .saturating_add(timings.decode_parse)
            .saturating_add(timings.styles)
            .saturating_add(timings.styles_pre_script)
            .saturating_add(timings.fonts)
            .saturating_add(timings.layout)
            .saturating_add(timings.images)
            .saturating_add(timings.backgrounds)
            .saturating_add(timings.scripting)
            .saturating_add(timings.event_loop)
            .saturating_add(timings.paint)
            .saturating_add(timings.raster)
    }

    /// Returns one deterministic message per exceeded budget (empty when all are met).
    pub fn check(&self, timings: &RenderTimings) -> Vec<String> {
        let mut out = Vec::new();
        let total = Self::total_of(timings);
        if total > self.total {
            out.push(format!(
                "performance budget: total {} > {:?}",
                total.as_millis(),
                self.total
            ));
        }
        if timings.styles_pre_script > self.styles_pre_script {
            out.push(format!(
                "performance budget: styles_pre_script {} ms > {:?}",
                timings.styles_pre_script.as_millis(),
                self.styles_pre_script
            ));
        }
        if timings.images > self.images {
            out.push(format!(
                "performance budget: images {} ms > {:?}",
                timings.images.as_millis(),
                self.images
            ));
        }
        if timings.backgrounds > self.backgrounds {
            out.push(format!(
                "performance budget: backgrounds {} ms > {:?}",
                timings.backgrounds.as_millis(),
                self.backgrounds
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timings() -> RenderTimings {
        RenderTimings {
            fetch: Duration::ZERO,
            decode_parse: Duration::ZERO,
            styles: Duration::ZERO,
            styles_pre_script: Duration::ZERO,
            fonts: Duration::ZERO,
            layout: Duration::ZERO,
            images: Duration::ZERO,
            backgrounds: Duration::ZERO,
            scripting: Duration::ZERO,
            event_loop: Duration::ZERO,
            paint: Duration::ZERO,
            raster: Duration::ZERO,
            total: Duration::ZERO,
        }
    }

    #[test]
    fn budget_reports_only_exceeded_stages() {
        let budget = PerformanceBudget::default();
        assert!(
            budget.check(&timings()).is_empty(),
            "zero timings meet every budget"
        );
        let mut slow = timings();
        slow.styles_pre_script = Duration::from_millis(695);
        slow.images = Duration::from_millis(1425);
        let warnings = budget.check(&slow);
        assert_eq!(
            warnings.len(),
            2,
            "styles and images must be reported: {warnings:?}"
        );
        assert!(warnings.iter().any(|w| w.contains("styles_pre_script")));
        assert!(warnings.iter().any(|w| w.contains("images")));
    }

    #[test]
    fn total_sums_all_stages_without_overflow() {
        let mut t = timings();
        t.fetch = Duration::from_secs(1);
        t.decode_parse = Duration::from_secs(1);
        t.styles = Duration::from_secs(1);
        assert_eq!(PerformanceBudget::total_of(&t), Duration::from_secs(3));
    }
}
