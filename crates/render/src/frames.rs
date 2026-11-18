//! Nested browsing contexts (`<iframe>`, #1544).
//!
//! Each `<iframe>` is rendered as an independent document into its own viewport and the
//! resulting pixels are blitted into the parent's replaced-element image store, so the existing
//! layout/paint path draws it like any other replaced box. Rendering is bounded by depth, count
//! and pixel caps so a self-referential frame tree cannot exhaust memory or hang the pipeline.

use dom::NodeId;
use net::Url;

use crate::{RenderError, RenderOptions, Viewport};

/// Global per-page frame budget shared across the whole recursion tree.
///
/// `MAX_FRAMES_PER_PAGE` used to be enforced per level (`out.len()` in
/// `collect_frames`), so a 4-wide self-nested tree rendered
/// 1 + 4 + 16 + 64 + 256 = 341 documents. The budget below is created once per
/// top-level page and threaded by `&mut` through `render_url_inner` and
/// `collect_frames`, so the total number of frame renders is bounded.
pub struct FrameBudget {
    remaining: usize,
}

impl FrameBudget {
    /// Creates a budget allowing up to `limit` frame renders.
    pub fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }

    /// Frames still available.
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Consumes one frame slot. Returns `false` when exhausted.
    pub fn try_consume(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}
/// Maximum nesting depth of frames (a frame at this depth renders no children).
pub const MAX_FRAME_DEPTH: usize = 4;
/// Maximum number of frames rendered per page render.
pub const MAX_FRAMES_PER_PAGE: usize = 16;
/// Maximum pixels per frame viewport.
pub const MAX_FRAME_PIXELS: u64 = 4_000_000;

/// A rendered frame ready to be inserted into the parent's image store.
pub struct FrameRender {
    /// The `<iframe>` element node in the parent document.
    pub node: NodeId,
    /// Rasterized child document.
    pub image: image::RgbaImage,
    /// Intrinsic size in CSS px (the frame box size).
    pub size: (f32, f32),
}

/// Renders every `<iframe>` in `document` into an image, resolving `srcdoc`/`src` against
/// `base_url`. Failures are non-fatal warnings: the frame stays a gray placeholder.
pub fn collect_frames(
    document: &dom::Document,
    base_url: &Url,
    options: &RenderOptions,
    depth: usize,
    warnings: &mut Vec<String>,
    budget: &mut FrameBudget,
) -> Result<Vec<FrameRender>, RenderError> {
    if depth >= MAX_FRAME_DEPTH {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for element in document.get_elements_by_tag_name("iframe") {
        if budget.remaining() == 0 {
            warnings.push(format!(
                "page has more than {MAX_FRAMES_PER_PAGE} iframes; remaining frames are not rendered (cap)"
            ));
            break;
        }
        let Some(data) = document
            .try_node(element)
            .and_then(|node| node.as_element())
        else {
            continue;
        };
        let (width, height) = frame_size(data.get_attribute("width"), data.get_attribute("height"));
        if u64::from(width) * u64::from(height) > MAX_FRAME_PIXELS {
            warnings.push(format!("skipping iframe {element:?}: viewport too large"));
            continue;
        }

        let child_input = if let Some(srcdoc) = data.get_attribute("srcdoc") {
            format!(
                "data:text/html;base64,{}",
                net::base64_encode(srcdoc.as_bytes())
            )
        } else if let Some(src) = data
            .get_attribute("src")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            match base_url.join(src) {
                Ok(url) => url.as_str().to_owned(),
                Err(error) => {
                    warnings.push(format!(
                        "skipping iframe {element:?}: invalid src ({error})"
                    ));
                    continue;
                }
            }
        } else {
            continue; // about:blank — nothing to render
        };

        let mut child_options = options.clone();
        child_options.viewport = match Viewport::new(width, height) {
            Ok(viewport) => viewport,
            Err(error) => {
                warnings.push(format!(
                    "skipping iframe {element:?}: invalid viewport ({error})"
                ));
                continue;
            }
        };
        child_options.encode_png = false;
        child_options.host_events = Vec::new();
        child_options.form_post = None;
        child_options.base_override = None;
        // `sandbox` (#1544): without an explicit `allow-scripts` token the framed document
        // cannot execute scripts. This is the core security guarantee of the attribute.
        if let Some(tokens) = data.get_attribute("sandbox") {
            let allows_scripts = tokens
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("allow-scripts"));
            if !allows_scripts {
                child_options.scripting = crate::ScriptingMode::Disabled;
            }
            // Sandboxed frames cannot navigate the top level or submit forms; the render
            // pipeline has no top-level navigation from frames, but form POSTs are disabled.
            child_options.form_post = None;
        }

        if !budget.try_consume() {
            warnings.push(format!(
                "page has more than {MAX_FRAMES_PER_PAGE} iframes; remaining frames are not rendered (cap)"
            ));
            break;
        }
        match crate::render_url_inner(&child_input, &child_options, None, depth + 1, budget) {
            Ok(child) => {
                let bytes = child.pixmap.to_rgba_bytes();
                match image::RgbaImage::new(width, height, bytes) {
                    Ok(image) => out.push(FrameRender {
                        node: element,
                        image,
                        size: (width as f32, height as f32),
                    }),
                    Err(error) => {
                        warnings.push(format!("iframe {element:?} raster mismatch: {error}"))
                    }
                }
            }
            Err(error) => warnings.push(format!("iframe {element:?} render failed: {error}")),
        }
    }
    Ok(out)
}

fn frame_size(width: Option<&str>, height: Option<&str>) -> (u32, u32) {
    let parse = |value: Option<&str>, default: u32| -> u32 {
        value
            .and_then(|v| v.trim().trim_end_matches("px").parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(default)
            .min(MAX_FRAME_PIXELS as u32)
    };
    (parse(width, 300), parse(height, 150))
}
