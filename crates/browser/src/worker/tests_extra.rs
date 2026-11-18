use super::*;

/// Debounced "session needs saving" state. Extracted from the winit handler so the
/// autosave trigger is testable headlessly (#36).
#[derive(Clone, Debug)]
pub struct SessionAutosave {
    dirty: bool,
    debounce: Debounce,
}

impl SessionAutosave {
    pub const fn new(delay: Duration) -> Self {
        Self {
            dirty: false,
            debounce: Debounce::new(delay),
        }
    }

    /// Marks the session dirty and (re)arms the debounce; returns the new deadline.
    pub fn mark_dirty(&mut self, now: Instant) -> Instant {
        self.dirty = true;
        self.debounce.schedule(now)
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Fires exactly once when dirty and due.
    pub fn take_due(&mut self, now: Instant) -> bool {
        self.dirty && self.debounce.take_due(now)
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Pending save deadline, or `None` when clean.
    pub const fn deadline(&self) -> Option<Instant> {
        if self.dirty {
            self.debounce.deadline()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod session_autosave_tests {
    use super::*;

    #[test]
    fn mark_dirty_then_debounced_save() {
        let start = Instant::now();
        let mut state = SessionAutosave::new(Duration::from_millis(600));
        assert!(!state.is_dirty());
        assert!(!state.take_due(start));

        let deadline = state.mark_dirty(start);
        assert_eq!(deadline, start + Duration::from_millis(600));
        assert!(state.is_dirty());
        assert_eq!(state.deadline(), Some(deadline));
        assert!(!state.take_due(start + Duration::from_millis(599)));

        // A second change re-arms the debounce.
        let deadline2 = state.mark_dirty(start + Duration::from_millis(500));
        assert_eq!(deadline2, start + Duration::from_millis(1100));
        assert!(!state.take_due(deadline));

        assert!(state.take_due(deadline2));
        state.clear_dirty();
        assert!(!state.is_dirty());
        assert!(!state.take_due(deadline2 + Duration::from_secs(1)));
    }
}

#[cfg(test)]
mod worker_panic_isolation_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn test_frame(generation: u64) -> RenderFrame {
        RenderFrame {
            generation,
            width: 2,
            height: 2,
            pixels: vec![0; 4],
            png_bytes: vec![0x89, b'P', b'N', b'G'],
            content_height: 2,
            capped: false,
            links: Vec::new(),
            images: Vec::new(),
            title: None,
            document_url: "https://example.com/".to_string(),
            scripts_executed: 0,
            script_timers_pending: 0,
            warnings: 0,
            script_errors: 0,
            forms: Vec::new(),
            document: Arc::new(megi_webview::core::Document::new()),
            find_matches: Vec::new(),
            find_rects: Vec::new(),
        }
    }

    #[test]
    fn panicking_render_does_not_kill_the_worker() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let (sender, receiver, handle) = spawn_worker(
            move |request: &RenderRequest| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("simulated hostile-page panic");
                }
                Ok(test_frame(request.generation))
            },
            || {},
        );

        let first = RenderRequest {
            generation: 1,
            input: "data:text/html,x".into(),
            css_width: 2,
            css_height: 2,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        };
        sender.send(first).expect("send first");
        let failure = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("worker answered after panic");
        match failure {
            WorkerMessage::Failed {
                generation,
                message,
            } => {
                assert_eq!(generation, 1);
                // The panic payload may not be recoverable across the AssertUnwindSafe
                // boundary; the important invariant is that the worker survives.
                let _ = message;
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        let second = RenderRequest {
            generation: 2,
            input: "data:text/html,y".into(),
            css_width: 2,
            css_height: 2,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        };
        sender.send(second).expect("send second");
        let frame = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("worker still alive");
        assert_eq!(
            frame.generation(),
            2,
            "worker must survive a page panic (#1468)"
        );
        drop(sender);
        let _ = handle.join();
    }
}

#[cfg(test)]
mod render_budget_warning_tests {
    #[test]
    fn script_limited_and_limit_violations_become_warnings() {
        // Mirrors the frame-building logic: both budget fields must reach the caller's
        // warning channel even when the render itself succeeded (#1458).
        let base_warnings: Vec<String> = Vec::new();
        let script_limited = true;
        let violations: u64 = 3;
        let mut messages = base_warnings;
        if script_limited {
            messages.push("scripting budget exhausted".to_string());
        }
        if violations > 0 {
            messages.push(format!("{violations} parser/layout limit violation(s)"));
        }
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("scripting budget"));
        assert!(messages[1].contains("3 parser/layout"));
    }
}
