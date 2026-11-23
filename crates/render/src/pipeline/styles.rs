//! Author/UA cascade collection, stylesheet resolution and CSP intersection.
//!
//! Moved verbatim from `lib.rs`.

use std::collections::HashMap;

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;
use megi_net::{ResourceLoader, Url};

use super::imports::build_author_stylesheet_collection;
use crate::{RenderError, RenderOptions};
use crate::{stylesheets, webfonts};

/// Author stylesheet summary for [`RenderStats`].
pub(crate) struct StyleSummary {
    pub(crate) external_stylesheets: usize,
    pub(crate) inline_stylesheets: usize,
    pub(crate) css_bytes: usize,
    pub(crate) imports_loaded: usize,
    pub(crate) imports_skipped: usize,
    pub(crate) max_import_depth: usize,
    pub(crate) media_skipped: usize,
    /// The `max_stylesheets` cap was hit and the warning already pushed (#654).
    pub(crate) sheets_capped: bool,
    /// Pre-parsed external `<link>` sheets (including their `@import`s) keyed by URL cache key,
    /// so timeline evaluation can reconstruct external `@keyframes` even if scripts add `<style>` (#2160).
    #[cfg(feature = "js")]
    pub(crate) external_sheets: super::imports::ExternalSheetCache,
}

/// Combines HTTP `Content-Security-Policy` headers with any `<meta http-equiv="Content-Security-Policy">`
/// elements in `document` via CSP Level 3 policy intersection.
pub(crate) fn document_csp(
    headers: &[(String, String)],
    document: &Document,
) -> Option<megi_net::csp::ContentSecurityPolicy> {
    let mut combined =
        megi_net::csp::ContentSecurityPolicy::from_headers(headers).unwrap_or_default();
    for node_id in document.descendants(document.root()) {
        let Some(element) = document.node(node_id).as_element() else {
            continue;
        };
        if element.namespace != megi_core::Namespace::Html
            || !element.local_name.eq_ignore_ascii_case("meta")
        {
            continue;
        }
        let is_csp = element
            .get_attribute("http-equiv")
            .is_some_and(|equiv| equiv.trim().eq_ignore_ascii_case("content-security-policy"));
        if !is_csp {
            continue;
        }
        if let Some(content) = element.get_attribute("content") {
            let parsed = megi_net::csp::ContentSecurityPolicy::parse(content);
            if !parsed.is_empty() {
                combined.merge(parsed);
            }
        }
    }
    (!combined.is_empty()).then_some(combined)
}

/// Builds the author [`megi_css::StyleCollection`] in cascade order, resolving `@import`
/// directives recursively (depth-limited, deduplicated, media-gated, non-fatal errors).
/// Collects the author stylesheets and builds the cascade collection for `document`.
///
/// Shared by the pre-script style pass (for `getComputedStyle`) and the post-script pass
/// that feeds layout/paint, so both use exactly the same cascade order and media context.
pub(crate) fn collect_styles(
    document: &Document,
    base_url: &Url,
    loader: &mut ResourceLoader,
    options: &RenderOptions,
    media: &megi_css::MediaContext,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
    warnings: &mut Vec<String>,
) -> Result<
    (
        megi_css::StyleCollection,
        StyleSummary,
        Vec<webfonts::LocatedFontFace>,
    ),
    RenderError,
> {
    let (vw, vh) = media.viewport.unwrap_or((1280.0, 720.0));
    let collected = megi_css::with_calc_viewport(vw, vh, || {
        stylesheets::collect_stylesheets(document, base_url, media)
    });
    warnings.extend(collected.warnings.iter().cloned());
    build_author_stylesheet_collection(&collected, base_url, loader, options, media, csp, warnings)
}

/// Adds `additional` bytes to the author CSS budget.
///
/// Exceeding the budget is **not** fatal (#654): the sheet that crosses the limit is skipped, the
/// limit is reported once as a warning, and rendering continues with the CSS collected so far —
/// which is what browsers do and what every other capped resource (images, backgrounds, fonts,
/// scripts) already did. Returns `false` when the budget is exhausted.
pub(crate) fn account_css_bytes(
    total: &mut usize,
    additional: usize,
    options: &RenderOptions,
) -> bool {
    let attempted = total.saturating_add(additional);
    if attempted > options.max_total_css_bytes {
        return false;
    }
    *total = attempted;
    true
}

/// Resolves computed styles for every element with a prebuilt [`megi_css::StyleCollection`].
///
/// Mirrors `megi_css::resolve_document_styles_with_media` but accepts a collection that already
/// contains the author sheets (including recursively imported ones in cascade order).
/// Resolves the cascade with an explicit interaction state.
///
/// Render uses an empty [`megi_css::StateContext`] (no pointer, no focus ownership): dynamic
/// `:checked`/`:disabled`/`:read-only`/`:required`/`:placeholder-shown` are derived from
/// the DOM attributes by `megi_css::state`, and `:focus`-style states can be modelled statically
/// with the documented `data-megi-*` attributes (`data-megi-focused`, ...). The shell can
/// inject those attributes after a user interaction and re-render.
pub(crate) fn resolve_styles_with_state(
    doc: &Document,
    collection: &megi_css::StyleCollection,
    state: megi_css::StateContext,
    time_ms: f64,
    viewport: (f32, f32),
) -> (HashMap<NodeId, ComputedStyle>, megi_css::PseudoStyles) {
    // `calc()` uses a thread-local viewport for vw/vh; install the real one so
    // `calc(50vw)` resolves against the render viewport, not a 1280x720 default (#1469).
    megi_css::with_calc_viewport(viewport.0, viewport.1, || {
        megi_css::state::with_state(state, || {
            resolve_styles_with_collection(doc, collection, time_ms, None)
        })
    })
}

/// Resolves styles with an optional previous-frame style snapshot used as the baseline for
/// CSS transitions at `time_ms` (#941).
pub(crate) fn resolve_styles_with_state_and_transitions(
    doc: &Document,
    collection: &megi_css::StyleCollection,
    state: megi_css::StateContext,
    time_ms: f64,
    viewport: (f32, f32),
    transition_from: Option<&HashMap<NodeId, ComputedStyle>>,
) -> (HashMap<NodeId, ComputedStyle>, megi_css::PseudoStyles) {
    megi_css::with_calc_viewport(viewport.0, viewport.1, || {
        megi_css::state::with_state(state, || {
            resolve_styles_with_collection(doc, collection, time_ms, transition_from)
        })
    })
}

#[cfg(test)]
thread_local! {
    static CASCADE_PASS_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PRE_LAYOUT_RUN_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn resolve_document_cascade(
    doc: &Document,
    collection: &megi_css::StyleCollection,
) -> (HashMap<NodeId, ComputedStyle>, megi_css::PseudoStyles) {
    #[cfg(test)]
    CASCADE_PASS_COUNT.with(|c| c.set(c.get() + 1));
    megi_css::resolve_document_styles_from_collection(doc, collection)
}

#[cfg(feature = "js")]
pub(crate) fn layout_snapshot(
    doc: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
    pseudos: &megi_css::PseudoStyles,
    viewport: megi_layout::Rect,
) -> Option<megi_layout::LayoutBox> {
    #[cfg(test)]
    PRE_LAYOUT_RUN_COUNT.with(|c| c.set(c.get() + 1));
    megi_layout::layout_document_with_pseudos(doc, styles, pseudos, viewport)
}

/// Resolves base cascaded styles without applying `::backdrop`, `<img>` hints,
/// transitions, or `@keyframes` animations.
pub(crate) fn resolve_base_styles_with_state(
    doc: &Document,
    collection: &megi_css::StyleCollection,
    state: megi_css::StateContext,
    viewport: (f32, f32),
) -> (HashMap<NodeId, ComputedStyle>, megi_css::PseudoStyles) {
    megi_css::with_calc_viewport(viewport.0, viewport.1, || {
        megi_css::state::with_state(state, || {
            let _pass = megi_css::style_pass_scope();
            resolve_document_cascade(doc, collection)
        })
    })
}

/// Applies post-cascade fixups (`::backdrop`, `<img>` hints, transitions, animations)
/// onto already-resolved base styles.
pub(crate) fn finalize_resolved_styles(
    doc: &Document,
    collection: &megi_css::StyleCollection,
    styles: &mut HashMap<NodeId, ComputedStyle>,
    pseudo_styles: &mut megi_css::PseudoStyles,
    time_ms: f64,
    transition_from: Option<&HashMap<NodeId, ComputedStyle>>,
) {
    // `#1546`: resolve the `::backdrop` color for modal dialogs after the main cascade, since
    // the generic collector does not know about the HTML top layer.
    let root = doc.root();
    let backdrop_updates: Vec<(NodeId, Option<megi_css::Color>)> = styles
        .keys()
        .filter_map(|node_id| {
            let element = doc.node(*node_id).as_element()?;
            if element.namespace != megi_core::Namespace::Html
                || !element.local_name.eq_ignore_ascii_case("dialog")
                || element.get_attribute("open").is_none()
                || element.get_attribute("data-megi-modal").is_none()
            {
                return None;
            }
            let parent_style = doc.node(*node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&root);
            Some((
                *node_id,
                megi_css::resolve_backdrop_color(
                    doc,
                    *node_id,
                    collection,
                    parent_style,
                    root_style,
                ),
            ))
        })
        .collect();
    for (node_id, color) in backdrop_updates {
        if let Some(computed) = styles.get_mut(&node_id) {
            computed.set_backdrop_color(color);
        }
    }
    // WHATWG HTML §15.2.6: the `width`/`height` attributes of <img> are
    // presentational hints with lower priority than any CSS rule. The generic
    // cascade only knows CSS, so apply them here when no CSS declaration won
    // (#1884: Wikipedia's absolutely positioned logo had 0x0 without them).
    let img_hints: Vec<(megi_core::NodeId, Option<f32>, Option<f32>)> = styles
        .keys()
        .filter_map(|node_id| {
            let element = doc.node(*node_id).as_element()?;
            if element.namespace != megi_core::Namespace::Html
                || !element.local_name.eq_ignore_ascii_case("img")
            {
                return None;
            }
            let parse_dimension = |name: &str| -> Option<f32> {
                element.get_attribute(name).and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let end = trimmed
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(trimmed.len());
                    let px = trimmed[..end].parse::<f32>().ok()?;
                    (px.is_finite() && px >= 0.0).then_some(px)
                })
            };
            Some((
                *node_id,
                parse_dimension("width"),
                parse_dimension("height"),
            ))
        })
        .collect();
    for (node_id, width, height) in img_hints {
        if let Some(computed) = styles.get_mut(&node_id) {
            if width.is_some() && matches!(computed.width, megi_css::Length::Auto) {
                computed.width = megi_css::Length::Px(width.unwrap_or(0.0));
            }
            if height.is_some() && matches!(computed.height, megi_css::Length::Auto) {
                computed.height = megi_css::Length::Px(height.unwrap_or(0.0));
            }
        }
    }

    if let Some(from_styles) = transition_from {
        megi_css::cascade::apply_transitions_to_styles(doc, from_styles, styles, time_ms as f32);
    }
    megi_css::cascade::apply_animations_to_styles(doc, styles, collection, time_ms as f32);
    megi_css::apply_animations_to_pseudo_styles(pseudo_styles, collection, time_ms as f32);
}

fn resolve_styles_with_collection(
    doc: &Document,
    collection: &megi_css::StyleCollection,
    time_ms: f64,
    transition_from: Option<&HashMap<NodeId, ComputedStyle>>,
) -> (HashMap<NodeId, ComputedStyle>, megi_css::PseudoStyles) {
    // One shared selector-matching budget for the whole document: a per-element budget would
    // grow the total cost with the element count (#1413).
    let _pass = megi_css::style_pass_scope();
    let (mut styles, mut pseudo_styles) = resolve_document_cascade(doc, collection);
    finalize_resolved_styles(
        doc,
        collection,
        &mut styles,
        &mut pseudo_styles,
        time_ms,
        transition_from,
    );
    (styles, pseudo_styles)
}

/// Collects viewport-space border/content rects for every laid-out DOM node (round 28).
#[cfg(feature = "js")]
pub(crate) fn collect_layout_rects(
    layout_box: &megi_layout::LayoutBox,
    rects: &mut std::collections::HashMap<NodeId, megi_js::host::LayoutRect>,
) {
    // Iterative traversal to avoid stack overflow on deep trees.
    let mut stack = vec![layout_box];
    let mut visited = 0usize;
    while let Some(current) = stack.pop() {
        if visited >= 100_000 {
            break;
        }
        visited += 1;
        if !current.is_pseudo
            && let Some(node_id) = current.node_id()
        {
            let border = current.dimensions.border_box();
            let content = current.dimensions.content;
            rects.entry(node_id).or_insert(megi_js::host::LayoutRect {
                border_box: (border.x, border.y, border.width, border.height),
                content_box: (content.x, content.y, content.width, content.height),
            });
        }
        for child in current.children.iter().rev() {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod calc_viewport_tests {
    use super::*;

    #[test]
    fn styles_use_the_render_viewport_for_calc_vw() {
        let doc = megi_html::parse_document(
            "<!doctype html><html><head></head><body><div>x</div></body></html>",
            false,
        );
        let mut collection = megi_css::StyleCollection::new();
        collection.add_css("div{width:calc(50vw)}", megi_css::Origin::Author);
        let (styles, _pseudos) = resolve_styles_with_state(
            &doc,
            &collection,
            megi_css::StateContext::new(),
            0.0,
            (400.0, 300.0),
        );
        let div = doc.get_elements_by_tag_name("div")[0];
        let width = styles
            .get(&div)
            .map(|style| style.width)
            .unwrap_or(megi_css::Length::Px(0.0));
        assert!(
            matches!(width, megi_css::Length::Px(px) if (px - 200.0).abs() < 1.0),
            "calc(50vw) must resolve against the 400px viewport, got {width:?}"
        );
    }

    #[cfg(feature = "js")]
    #[test]
    fn timeline_listener_without_animations_skips_extra_cascades() {
        let mut css = String::new();
        for i in 0..50 {
            css.push_str(&format!(".c{i} {{ color: #123456; }}\n"));
        }
        let mut divs = String::new();
        for i in 0..50 {
            divs.push_str(&format!("<div class='c{i}'>x</div>"));
        }
        let html = format!(
            "<!doctype html><html><head><style>{css}</style></head><body>\
             {divs}<div id='target'></div>\
             <script>document.getElementById('target').addEventListener('animationstart', function() {{}});</script>\
             </body></html>"
        );
        let url = format!(
            "data:text/html;base64,{}",
            megi_net::base64_encode(html.as_bytes())
        );
        let options = RenderOptions {
            scripting: crate::ScriptingMode::Enabled,
            encode_png: false,
            ..RenderOptions::default()
        };
        CASCADE_PASS_COUNT.with(|c| c.set(0));
        let _output = crate::render_url(&url, &options).expect("render");
        let passes = CASCADE_PASS_COUNT.with(std::cell::Cell::get);
        assert!(
            passes <= 1,
            "registering a timeline listener on an unchanged page without animations/transitions \
             must not run extra CSS cascades (got {passes} cascades)"
        );

        // When animations DO exist and a script mutates the DOM, timeline evaluation plus final
        // resource styling must share one post-script cascade (total <= 2, not 3).
        let anim_html = "<!doctype html><html><head><style>\
            @keyframes pop { from { opacity: 0; } to { opacity: 1; } }\
            #box.go { animation: pop 200ms linear both; }\
            </style></head><body><div id='box'></div><script>\
            var b = document.getElementById('box');\
            b.addEventListener('animationend', function() {});\
            b.className = 'go';\
            </script></body></html>";
        let anim_url = format!(
            "data:text/html;base64,{}",
            megi_net::base64_encode(anim_html.as_bytes())
        );
        let anim_opts = RenderOptions {
            time_ms: 500.0,
            ..options
        };
        CASCADE_PASS_COUNT.with(|c| c.set(0));
        let _anim_out = crate::render_url(&anim_url, &anim_opts).expect("anim render");
        let anim_passes = CASCADE_PASS_COUNT.with(std::cell::Cell::get);
        assert!(
            anim_passes <= 2,
            "timeline + post-script resource cascade must share one post-script resolution (got {anim_passes})"
        );
    }

    #[cfg(feature = "js")]
    #[test]
    fn repeated_get_bounding_client_rect_runs_pre_layout_once() {
        let html = "<!doctype html><html><body style='margin:0'>\
            <div id='b' style='width:80px;height:40px'></div><div id='out'></div>\
            <script>\
            var el = document.getElementById('b');\
            var last = 0;\
            for (var i = 0; i < 50; i++) { last = el.getBoundingClientRect().width; }\
            document.getElementById('out').textContent = String(last);\
            </script></body></html>";
        let url = format!(
            "data:text/html;base64,{}",
            megi_net::base64_encode(html.as_bytes())
        );
        let options = RenderOptions {
            scripting: crate::ScriptingMode::Enabled,
            encode_png: false,
            ..RenderOptions::default()
        };
        PRE_LAYOUT_RUN_COUNT.with(|c| c.set(0));
        let _output = crate::render_url(&url, &options).expect("render");
        let layout_runs = PRE_LAYOUT_RUN_COUNT.with(std::cell::Cell::get);
        assert_eq!(
            layout_runs, 1,
            "50 getBoundingClientRect() queries must execute pre-script layout once, got {layout_runs}"
        );
    }
}
