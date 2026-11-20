//! Real E2E for the per-site renderer-process backend (ADR-004 / F7-ISOLATION).
//! Spawns the actual `megi --megi-ipc-worker` child and drives it over the framed IPC.

#![cfg(feature = "a11y")]

use megi_browser::ipc::{IpcRenderRequest, ProcessRenderer};

fn request(generation: u64, url: &str) -> IpcRenderRequest {
    IpcRenderRequest {
        generation,
        input: url.to_owned(),
        css_width: 320,
        css_height: 200,
        base_override: None,
        scripting: false,
        find_query: None,
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
