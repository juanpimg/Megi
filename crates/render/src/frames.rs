//! Nested browsing contexts (`<iframe>`, #1544).
//!
//! Each `<iframe>` is rendered as an independent document into its own viewport and the
//! resulting pixels are blitted into the parent's replaced-element image store, so the existing
//! layout/paint path draws it like any other replaced box. Rendering is bounded by depth, count
//! and pixel caps so a self-referential frame tree cannot exhaust memory or hang the pipeline.

use std::collections::HashMap;

use megi_core::NodeId;
use megi_net::Url;

use crate::{FrameRenderContext, RenderError, RenderOptions, Viewport};

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

/// Pre-scripted child `<iframe>` state captured from the parent `JsRuntime` (#1544).
#[cfg(feature = "js")]
pub type PreScriptedFrameMap = HashMap<NodeId, (megi_core::Document, megi_js::SandboxFlags, String)>;
#[cfg(not(feature = "js"))]
pub type PreScriptedFrameMap = HashMap<NodeId, ()>;

/// A rendered frame ready to be inserted into the parent's image store.
pub struct FrameRender {
    /// The `<iframe>` element node in the parent document.
    pub node: NodeId,
    /// Rasterized child document.
    pub image: megi_image::RgbaImage,
    /// Intrinsic size in CSS px (the frame box size).
    pub size: (f32, f32),
}

/// Renders every `<iframe>` in `document` into an image, resolving `srcdoc`/`src` against
/// `base_url`. Failures are non-fatal warnings: the frame stays a gray placeholder.
pub fn collect_frames(
    document: &megi_core::Document,
    base_url: &Url,
    options: &RenderOptions,
    depth: usize,
    warnings: &mut Vec<String>,
    budget: &mut FrameBudget,
) -> Result<Vec<FrameRender>, RenderError> {
    let empty_styles = HashMap::new();
    let empty_frames = PreScriptedFrameMap::new();
    collect_frames_with_context(
        document,
        base_url,
        options,
        &empty_styles,
        None,
        &empty_frames,
        depth,
        warnings,
        budget,
    )
}

/// Like [`collect_frames`], but resolves CSS box sizing (`width`/`height`), enforces CSP
/// `frame-src`/`child-src`, propagates full `<iframe sandbox>` flags, and renders any
/// parent-script-mutated child DOMs without re-parsing stale markup (#1544).
#[allow(clippy::too_many_arguments)]
pub fn collect_frames_with_context(
    document: &megi_core::Document,
    base_url: &Url,
    options: &RenderOptions,
    styles: &HashMap<NodeId, megi_css::ComputedStyle>,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
    pre_scripted: &PreScriptedFrameMap,
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
        let (width, height) = frame_size(
            styles.get(&element),
            options.viewport,
            data.get_attribute("width"),
            data.get_attribute("height"),
        );
        if u64::from(width) * u64::from(height) > MAX_FRAME_PIXELS {
            warnings.push(format!("skipping iframe {element:?}: viewport too large"));
            continue;
        }

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
        child_options.host_dom_events = Vec::new();
        child_options.form_post = None;
        child_options.base_override = None;

        #[cfg(feature = "js")]
        let sandbox_flags = data
            .get_attribute("sandbox")
            .map(megi_js::SandboxFlags::from_attribute);
        #[cfg(feature = "js")]
        if let Some(flags) = sandbox_flags {
            if !flags.allow_scripts {
                child_options.scripting = crate::ScriptingMode::Disabled;
            }
            if !flags.allow_same_origin {
                child_options.cookies = false;
                child_options.cookie_jar = None;
                child_options.storage = None;
                child_options.origin = None;
            }
        }
        #[cfg(not(feature = "js"))]
        if let Some(tokens) = data.get_attribute("sandbox") {
            let allows_scripts = tokens
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("allow-scripts"));
            if !allows_scripts {
                child_options.scripting = crate::ScriptingMode::Disabled;
            }
            let allows_same_origin = tokens
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("allow-same-origin"));
            if !allows_same_origin {
                child_options.cookies = false;
                child_options.cookie_jar = None;
                child_options.storage = None;
                child_options.origin = None;
            }
        }

        #[cfg(feature = "js")]
        if let Some((child_doc, child_sandbox, child_url)) = pre_scripted.get(&element) {
            // Check CSP for external `src` even when pre-initialized in JS.
            if data.get_attribute("srcdoc").is_none()
                && let Some(src) = data
                    .get_attribute("src")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                && !src.eq_ignore_ascii_case("about:blank")
                && let Ok(parsed_src) = base_url.join(src)
                && let Some(policy) = csp
                && !policy.allows_frame(&parsed_src, base_url)
            {
                warnings.push(format!(
                    "skipping iframe {parsed_src}: blocked by Content-Security-Policy"
                ));
                continue;
            }
            let parsed_url = Url::parse(child_url).unwrap_or_else(|_| base_url.clone());
            let child_ctx = FrameRenderContext {
                sandbox_flags: Some(*child_sandbox),
                skip_scripts: true,
            };
            match crate::render_pre_scripted_document(
                child_doc.clone(),
                parsed_url,
                &child_options,
                depth + 1,
                child_ctx,
                budget, // share the global per-page frame budget (#1632)
            ) {
                Ok(child) => {
                    let bytes = child.pixmap.to_rgba_bytes();
                    match megi_image::RgbaImage::new(width, height, bytes) {
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
            continue;
        }
        #[cfg(not(feature = "js"))]
        let _ = pre_scripted;

        let child_input = if let Some(srcdoc) = data.get_attribute("srcdoc") {
            format!(
                "data:text/html;base64,{}",
                megi_net::base64_encode(srcdoc.as_bytes())
            )
        } else if let Some(src) = data
            .get_attribute("src")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if src.eq_ignore_ascii_case("about:blank") {
                continue;
            }
            match base_url.join(src) {
                Ok(url) => {
                    if let Some(policy) = csp
                        && !policy.allows_frame(&url, base_url)
                    {
                        warnings.push(format!(
                            "skipping iframe {url}: blocked by Content-Security-Policy"
                        ));
                        continue;
                    }
                    url.as_str().to_owned()
                }
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

        if !budget.try_consume() {
            warnings.push(format!(
                "page has more than {MAX_FRAMES_PER_PAGE} iframes; remaining frames are not rendered (cap)"
            ));
            break;
        }
        let child_ctx = FrameRenderContext {
            #[cfg(feature = "js")]
            sandbox_flags,
            skip_scripts: false,
        };
        match crate::render_url_with_context(
            &child_input,
            &child_options,
            None,
            depth + 1,
            child_ctx,
            budget, // share the global per-page frame budget across the recursion (#1632)
        ) {
            Ok(child) => {
                let bytes = child.pixmap.to_rgba_bytes();
                match megi_image::RgbaImage::new(width, height, bytes) {
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

fn frame_size(
    style: Option<&megi_css::ComputedStyle>,
    parent_viewport: Viewport,
    width_attr: Option<&str>,
    height_attr: Option<&str>,
) -> (u32, u32) {
    let from_css = |len: megi_css::Length, basis: u32| -> Option<u32> {
        match len {
            megi_css::Length::Px(px) if px.is_finite() && px >= 1.0 => {
                Some((px.round() as u32).clamp(1, MAX_FRAME_PIXELS as u32))
            }
            megi_css::Length::Percent(pct) if pct.is_finite() && pct > 0.0 => {
                let px = (pct / 100.0) * (basis as f32);
                (px >= 1.0).then(|| (px.round() as u32).clamp(1, MAX_FRAME_PIXELS as u32))
            }
            _ => None,
        }
    };
    let parse_attr = |value: Option<&str>, default: u32| -> u32 {
        value
            .and_then(|v| v.trim().trim_end_matches("px").parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(default)
            .min(MAX_FRAME_PIXELS as u32)
    };
    let width = style
        .and_then(|s| from_css(s.width, parent_viewport.width))
        .unwrap_or_else(|| parse_attr(width_attr, 300));
    let height = style
        .and_then(|s| from_css(s.height, parent_viewport.height))
        .unwrap_or_else(|| parse_attr(height_attr, 150));
    (width, height)
}
