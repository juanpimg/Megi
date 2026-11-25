//! M4#6: a document reload served by a service worker with the network cut.
//!
//! WHATWG Service Workers §5.2 (fetch event) / Appendix A «Match Service Worker
//! Registration»; W3C Cache API §5.4. The reload goes through `render_url`
//! with `no_network: true`, i.e. it is a real document navigation, not a
//! second `fetch()` inside the same page.

#![cfg(feature = "js")]

use megi_render::offline_demo::render_sw_offline;
use megi_render::{RenderOptions, Viewport, render_url};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/render/sw_offline.html"
);

#[test]
fn sw_offline_renders_after_network_cut() {
    let html = std::fs::read_to_string(FIXTURE).expect("read sw_offline.html fixture");
    let viewport = Viewport::new(800, 600).expect("viewport");

    let reload = render_sw_offline(&html, viewport).expect("offline document reload");
    assert_eq!(
        reload.cached.body,
        html.as_bytes(),
        "the worker must serve the document body it cached on the first load"
    );
    assert_eq!(
        reload.provider_calls_after_cut, 0,
        "the cut network provider must not be consulted for the cached document"
    );
    assert!(
        reload.network_cut_verified,
        "a same-origin request outside the cache must fail after the cut"
    );

    // Deterministic golden: a second independent reload is byte-identical.
    let again = render_sw_offline(&html, viewport).expect("second offline reload");
    assert_eq!(reload.png, again.png, "offline PNG must be deterministic");

    // Content equivalence: the same fixture rendered normally produces the same PNG.
    let path = std::fs::canonicalize(FIXTURE).expect("canonical fixture path");
    let file_url = format!("file://{}", path.display());
    let online = render_url(
        &file_url,
        &RenderOptions {
            viewport,
            ..RenderOptions::default()
        },
    )
    .expect("online render of the same fixture");
    assert_eq!(
        reload.png, online.png,
        "offline reload must render exactly the same document as the online load"
    );
}
