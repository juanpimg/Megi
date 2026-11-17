#![no_main]
//! Fuzz target: the full `render` pipeline on untrusted HTML (offline, bounded).
//!
//! The input bytes are decoded lossily, base64-encoded into an inline
//! `data:text/html;base64,…` URL (a `data:` document never touches the network, so flipping
//! `no_network` only changes which loader branch runs) and rendered through
//! [`render::render_url`].
//!
//! Earlier revisions pinned the whole option space — 96×96 viewport, scripting disabled,
//! `no_network: true` and one tiny cap set — so the budget arithmetic and the scripted
//! pipeline were unfuzzed (#842). Now:
//!
//! - the viewport is derived from the input (`32..=256` per side, with 96×96 as one of the
//!   reachable outcomes), keeping each iteration cheap while varying layout wrapping;
//! - the cap set alternates between the harness's tight profile and
//!   [`render::RenderOptions::default`] for small inputs, so both the defensive caps and the
//!   production budgets are covered;
//! - scripting is enabled for small inputs (the JS engine has its own target, but the
//!   *integration* — parser-inserted scripts, re-render, budgets — is what this target adds);
//! - `no_network` follows an input bit.
//!
//! Signals: panics, aborts, ASan findings, integer overflows and timeouts. The render result
//! (Ok/Err) is intentionally discarded.

use libfuzzer_sys::fuzz_target;

/// Hard cap on the fuzzer-provided document size (the render caps are stricter).
const MAX_FUZZ_INPUT: usize = 256 * 1024;
/// Below this size the harness is willing to pay for the default caps and for scripting.
const CHEAP_INPUT: usize = 32 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT {
        return;
    }
    let html = String::from_utf8_lossy(data);
    let url = format!(
        "data:text/html;base64,{}",
        net::base64_encode(html.as_bytes())
    );

    let flag = |index: usize| data.get(index).copied().unwrap_or(0);
    let width = 32 + u32::from(flag(0) % 225); // 32..=256
    let height = 32 + u32::from(flag(1) % 225);
    let Ok(viewport) = render::Viewport::new(width, height) else {
        return;
    };

    let cheap = data.len() <= CHEAP_INPUT;
    let mut options = if cheap && flag(4) & 1 == 0 {
        // Production defaults: maximal budgets, real scripting mode.
        render::RenderOptions {
            viewport,
            ..render::RenderOptions::default()
        }
    } else {
        let mut options = render::RenderOptions {
            viewport,
            no_network: flag(4) & 2 == 0,
            scripting: render::ScriptingMode::Disabled,
            ..render::RenderOptions::default()
        };
        options.max_document_bytes = MAX_FUZZ_INPUT;
        options.max_total_css_bytes = 256 * 1024;
        options.max_css_bytes_per_sheet = 128 * 1024;
        options.max_stylesheets = 4;
        options.max_import_depth = 2;
        options.max_images = 4;
        options.max_image_bytes = 64 * 1024;
        options.max_total_image_bytes = 128 * 1024;
        options.max_backgrounds = 2;
        options.max_background_bytes = 64 * 1024;
        options.max_total_background_bytes = 128 * 1024;
        options
    };

    // Scripting integration only for cheap documents: Boa is the expensive part and
    // `js_eval`/`js_module` already fuzz the engine itself.
    if cheap && flag(2) & 1 == 1 {
        options.scripting = render::ScriptingMode::Enabled;
    }
    if flag(3) & 1 == 1 {
        options.max_image_dim = Some(16 + u32::from(flag(5) % 64));
    }

    let _ = render::render_url(&url, &options);
});
