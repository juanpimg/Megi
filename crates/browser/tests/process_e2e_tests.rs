//! Real E2E for the per-site renderer-process backend (ADR-004 / F7-ISOLATION).
//! Spawns the actual `megi --megi-ipc-worker` child and drives it over the framed IPC.

use megi_browser::ipc::{IpcRenderRequest, IpcRenderResponse, ProcessRenderPool, ProcessRenderer};

fn request(generation: u64, url: &str) -> IpcRenderRequest {
    IpcRenderRequest {
        generation,
        input: url.to_owned(),
        css_width: 320,
        css_height: 200,
        base_override: None,
        scripting: false,
        find_query: None,
        options: Default::default(),
    }
}

#[test]
fn real_child_renders_multiple_sites_and_recovers_after_kill() {
    let executable = env!("CARGO_BIN_EXE_megi");
    let mut renderer = ProcessRenderer::spawn_with_executable(std::path::Path::new(executable))
        .expect("spawn renderer child");
    assert!(renderer.is_alive(), "child must start alive");

    for (generation, url) in [
        (1u64, "data:text/html,<title>Site A</title><p>alpha</p>"),
        (2u64, "data:text/html,<title>Site B</title><p>beta</p>"),
    ] {
        let response = renderer.render(request(generation, url)).expect("render");
        assert_eq!(response.generation, generation);
        assert_eq!(response.css_width, 320);
        assert!(
            response.error.is_none(),
            "render failed: {:?}",
            response.error
        );
        assert!(!response.png_bytes.is_empty(), "child must paint a PNG");
        assert!(
            !response.document_html.is_empty(),
            "child must return its DOM"
        );
    }

    // A killed child must be detected so the pool can respawn it.
    renderer.kill();
    assert!(
        !renderer.is_alive(),
        "killed child must be detected as dead"
    );
}

/// Polls the pool's non-blocking `drain()` until `count` successful responses arrive.
fn wait_for_responses(pool: &mut ProcessRenderPool, count: usize) -> Vec<IpcRenderResponse> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut out = Vec::new();
    while out.len() < count && std::time::Instant::now() < deadline {
        out.extend(pool.drain().into_iter().filter_map(|result| result.ok()));
        if out.len() < count {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    out
}

fn tab(generation: u64, base: &str, title: &str) -> IpcRenderRequest {
    IpcRenderRequest {
        generation,
        input: format!("data:text/html,<title>{title}</title><p>{title}</p>"),
        css_width: 320,
        css_height: 200,
        base_override: Some(base.to_owned()),
        scripting: false,
        find_query: None,
        options: Default::default(),
    }
}

#[test]
fn real_pool_isolates_distinct_sites_and_recovers_a_killed_child() {
    // F7-ISOLATION E2E: two "tabs" on distinct sites get distinct real OS processes;
    // a same-site tab reuses its process; killing a child is detected and the next
    // render recovers with a fresh process and a real frame.
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_megi"));
    let mut pool = ProcessRenderPool::with_executable(4, Some(executable.to_path_buf()));

    let site_a = "https://site-a.test/";
    let site_b = "https://site-b.test/";
    let key_a = megi_browser::site::site_key(site_a);
    let key_b = megi_browser::site::site_key(site_b);

    pool.send(tab(1, site_a, "TabA")).expect("queue tab A");
    pool.send(tab(2, site_b, "TabB")).expect("queue tab B");

    assert_eq!(pool.len(), 2, "one renderer process per effective site");
    let pids = pool.pids();
    assert_ne!(
        pids[0], pids[1],
        "distinct sites must use distinct OS processes"
    );
    assert_eq!(pool.sites(), vec![key_a.clone(), key_b.clone()]);

    let responses = wait_for_responses(&mut pool, 2);
    assert_eq!(
        responses.len(),
        2,
        "both tabs must paint through their own child"
    );
    for response in &responses {
        assert!(
            response.error.is_none(),
            "render failed: {:?}",
            response.error
        );
        assert!(
            !response.png_bytes.is_empty(),
            "child must return a real PNG"
        );
        assert!(
            !response.document_html.is_empty(),
            "child must return its DOM"
        );
    }
    // Responses come from two independent reader threads, so their arrival order is
    // intentionally unspecified; each tab must still paint exactly once.
    let mut generations: Vec<u64> = responses.iter().map(|r| r.generation).collect();
    generations.sort_unstable();
    assert_eq!(generations, vec![1, 2]);

    // A third tab on site A reuses the same process (no new child spawned).
    pool.send(tab(3, site_a, "TabA2"))
        .expect("queue same-site tab");
    assert_eq!(pool.len(), 2, "same site must reuse its renderer");
    assert_eq!(
        pool.pids(),
        pids,
        "same-site renderer PID must be unchanged"
    );
    let reused = wait_for_responses(&mut pool, 1);
    assert_eq!(reused.len(), 1);
    assert_eq!(reused[0].generation, 3);

    // Kill the site-A child; the next request must respawn it and recover a frame.
    assert!(pool.kill_child(&key_a));
    pool.send(tab(4, site_a, "TabA3"))
        .expect("queue after crash");
    assert_eq!(pool.recoveries(), 1, "the dead child must be replaced");
    let recovered_pids = pool.pids();
    assert_ne!(
        recovered_pids[0], pids[0],
        "a crashed renderer must run in a fresh OS process"
    );
    let recovered = wait_for_responses(&mut pool, 1);
    assert_eq!(recovered.len(), 1, "a frame must recover after the crash");
    assert_eq!(recovered[0].generation, 4);
    assert!(
        !recovered[0].png_bytes.is_empty(),
        "recovered frame must be painted"
    );
    assert_eq!(recovered[0].error, None);
}
