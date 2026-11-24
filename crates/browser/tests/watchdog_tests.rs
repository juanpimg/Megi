//! T5: a renderer that never answers must be killed and reported in under six seconds.
//!
//! Plan: docs/superpowers/plans/2026-11-24-security-isolation-plan.md §Task 3 (T5).
//! The child is a real `/usr/bin/sleep` process launched through the pool's injectable
//! executable/args. No mock renderer: the failure is observed through the framed IPC,
//! the PID table and the recovery counter.

use megi_browser::ipc::{IpcError, IpcRenderRequest, ProcessRenderPool, RENDER_WATCHDOG_TIMEOUT};
use megi_browser::site::site_key;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn unresponsive_renderer_times_out_below_six_seconds() {
    let mut pool = ProcessRenderPool::with_executable_and_args(
        2,
        Some(Path::new("/usr/bin/sleep").to_path_buf()),
        vec!["30".to_owned()],
    );
    let request = IpcRenderRequest {
        generation: 77,
        input: "https://watchdog.test/".to_owned(),
        css_width: 320,
        css_height: 200,
        base_override: None,
        scripting: false,
        find_query: None,
        options: Default::default(),
    };
    pool.send(request).expect("queue request");
    assert_eq!(pool.len(), 1);
    assert_eq!(pool.sites(), vec![site_key("https://watchdog.test/")]);

    let started = Instant::now();
    let mut failure = None;
    while started.elapsed() < Duration::from_secs(6) && failure.is_none() {
        for (generation, result) in pool.drain_tagged() {
            match result {
                Err(IpcError::Watchdog(generation)) => {
                    assert_eq!(
                        generation, 77,
                        "watchdog must carry the in-flight generation"
                    );
                    failure = Some(IpcError::Watchdog(generation));
                }
                Err(error) => panic!("unexpected error before the watchdog fired: {error:?}"),
                Ok(response) => panic!("sleep must not produce a frame: {response:?}"),
            }
            assert_eq!(generation, Some(77));
        }
        if failure.is_none() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    assert!(RENDER_WATCHDOG_TIMEOUT < Duration::from_secs(6));
    let error = failure.expect("watchdog failure must be published to the caller");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(6),
        "watchdog took {elapsed:?}"
    );
    assert_eq!(pool.recoveries(), 1, "timed-out renderer must be replaced");
    assert!(pool.pids().is_empty(), "timed-out renderer must be retired");
    assert!(error.is_fatal());
}
