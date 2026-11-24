//! Internal structs threaded through the staged render pipeline.
//!
//! `ParsedDocumentInput` and `FrameRenderContext` are moved verbatim from `lib.rs`.
//! The stage-output structs bundle the values produced by one stage and consumed by the next.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;
use megi_net::{ResourceLoader, Url};

#[cfg(feature = "js")]
use super::imports::stylesheet_signature;
use super::url::FrameRenderContext;
#[cfg(feature = "js")]
use crate::stylesheets;
use crate::{ConsoleMessage, HistoryAction, LocationAction, RenderCache, RenderOptions};
use crate::{backgrounds, frames, images, inline_svg, webfonts};

pub(crate) struct CachedPostStyles {
    pub(crate) stylesheet_sig: u64,
    pub(crate) dom_sig: u128,
    pub(crate) styles: Arc<HashMap<NodeId, ComputedStyle>>,
    pub(crate) pseudo_styles: Arc<megi_css::PseudoStyles>,
}

pub(crate) type SharedPostStylesCache = Arc<Mutex<Option<CachedPostStyles>>>;

pub(crate) struct ParsedDocumentInput<'a> {
    pub(crate) document: Document,
    pub(crate) document_url: Url,
    pub(crate) document_headers: &'a [(String, String)],
    pub(crate) options: &'a RenderOptions,
    pub(crate) loader: ResourceLoader,
    pub(crate) cookie_jar: Option<Arc<Mutex<megi_net::CookieJar>>>,
    pub(crate) referrer_policy: megi_net::ReferrerPolicy,
    pub(crate) source_mentions_svg: bool,
    pub(crate) document_bytes: usize,
    pub(crate) charset: String,
    pub(crate) status: Option<u16>,
    /// Non-fatal CDN error-page diagnostic (#2249).
    pub(crate) initial_warning: Option<String>,
    pub(crate) content_encoding: Option<String>,
    pub(crate) limit_violations: megi_html::LimitViolations,
    pub(crate) cache: Option<&'a mut RenderCache>,
    pub(crate) frame_depth: usize,
    pub(crate) frame_ctx: FrameRenderContext,
    pub(crate) budget: &'a mut frames::FrameBudget,
    pub(crate) fetch_time: Duration,
    pub(crate) decode_parse_time: Duration,
    pub(crate) total_start: Instant,
}

/// Values produced by the scripting stage and needed by the resource-loading stage.
pub(crate) struct ScriptContext {
    pub(crate) warnings: Vec<String>,
    pub(crate) base_url: Url,
    pub(crate) csp: Option<megi_net::csp::ContentSecurityPolicy>,
    pub(crate) media_context: megi_css::MediaContext,
    pub(crate) reusable_stylesheets: Option<(
        Arc<megi_css::StyleCollection>,
        super::styles::StyleSummary,
        Vec<webfonts::LocatedFontFace>,
        u64,
    )>,
    pub(crate) cached_post_styles: SharedPostStylesCache,
    pub(crate) pre_warnings: Vec<String>,
    /// Baseline styles for CSS transitions (caller snapshot, else the pre-script cascade).
    pub(crate) transition_from_styles: Option<Arc<HashMap<NodeId, megi_css::ComputedStyle>>>,
    pub(crate) pre_scripted_frames: frames::PreScriptedFrameMap,
    pub(crate) scripts_executed: usize,
    #[cfg(feature = "js")]
    pub(crate) canvas_surfaces: HashMap<NodeId, megi_js::canvas::CanvasSurface>,
}

/// 128-bit structural + attribute + text signature of `document` for style-cache validity.
pub(crate) fn style_dom_signature(document: &Document) -> u128 {
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x9e37_79b9_7f4a_7c15;
    let mix_bytes = |h1: &mut u64, h2: &mut u64, bytes: &[u8]| {
        for &byte in bytes {
            *h1 ^= u64::from(byte);
            *h1 = h1.wrapping_mul(0x0000_0100_0000_01b3);
            *h2 ^= u64::from(byte).wrapping_add(0x55);
            *h2 = h2.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        }
        *h1 ^= 0xff;
        *h1 = h1.wrapping_mul(0x0000_0100_0000_01b3);
        *h2 ^= 0xaa;
        *h2 = h2.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    };

    let mut stack = vec![(document.root(), 0u32)];
    while let Some((node_id, depth)) = stack.pop() {
        mix_bytes(&mut h1, &mut h2, &depth.to_le_bytes());
        mix_bytes(&mut h1, &mut h2, &node_id.get().to_le_bytes());
        let node = document.node(node_id);
        match &node.kind {
            megi_core::NodeKind::Document => mix_bytes(&mut h1, &mut h2, b"D"),
            megi_core::NodeKind::DocumentFragment => mix_bytes(&mut h1, &mut h2, b"F"),
            megi_core::NodeKind::Doctype { name, .. } => {
                mix_bytes(&mut h1, &mut h2, b"T");
                mix_bytes(&mut h1, &mut h2, name.as_bytes());
            }
            megi_core::NodeKind::Element(el) => {
                mix_bytes(&mut h1, &mut h2, b"E");
                let ns_tag = match el.namespace {
                    megi_core::Namespace::Html => 1u8,
                    megi_core::Namespace::Svg => 2u8,
                    megi_core::Namespace::MathMl => 3u8,
                    megi_core::Namespace::XLink => 4u8,
                    megi_core::Namespace::Xml => 5u8,
                    megi_core::Namespace::Xmlns => 6u8,
                    megi_core::Namespace::None => 7u8,
                };
                mix_bytes(&mut h1, &mut h2, &[ns_tag]);
                mix_bytes(&mut h1, &mut h2, el.local_name.as_bytes());
                for attr in &el.attributes {
                    mix_bytes(&mut h1, &mut h2, attr.local_name.as_bytes());
                    mix_bytes(&mut h1, &mut h2, attr.value.as_bytes());
                }
            }
            megi_core::NodeKind::Text(text) => {
                mix_bytes(&mut h1, &mut h2, b"X");
                mix_bytes(&mut h1, &mut h2, text.as_bytes());
            }
            megi_core::NodeKind::Comment(_) => mix_bytes(&mut h1, &mut h2, b"C"),
            megi_core::NodeKind::ProcessingInstruction { target, data } => {
                mix_bytes(&mut h1, &mut h2, b"P");
                mix_bytes(&mut h1, &mut h2, target.as_bytes());
                mix_bytes(&mut h1, &mut h2, data.as_bytes());
            }
        }
        let mut child = node.last_child;
        while let Some(c) = child {
            stack.push((c, depth.saturating_add(1)));
            child = document.node(c).prev_sibling;
        }
    }
    u128::from(h1) << 64 | u128::from(h2)
}

pub(crate) fn needs_pre_script_finalization(
    document: &Document,
    collection: &megi_css::StyleCollection,
) -> bool {
    if !collection.keyframes.is_empty() {
        return true;
    }
    for node_id in document.descendants(document.root()) {
        let Some(el) = document.node(node_id).as_element() else {
            continue;
        };
        if el.namespace != megi_core::Namespace::Html {
            continue;
        }
        if el.local_name.eq_ignore_ascii_case("dialog")
            && el.get_attribute("open").is_some()
            && el.get_attribute("data-megi-modal").is_some()
        {
            return true;
        }
        if el.local_name.eq_ignore_ascii_case("img")
            && (el.get_attribute("width").is_some() || el.get_attribute("height").is_some())
        {
            return true;
        }
    }
    false
}

#[cfg(feature = "js")]
fn contains_ascii_icase(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(feature = "js")]
fn has_inline_timeline_hints(document: &Document) -> (bool, bool) {
    let mut anim = false;
    let mut trans = false;
    for node_id in document.descendants(document.root()) {
        let Some(el) = document.node(node_id).as_element() else {
            continue;
        };
        if let Some(style) = el.get_attribute("style") {
            let bytes = style.as_bytes();
            if !anim && contains_ascii_icase(bytes, b"animation") {
                anim = true;
            }
            if !trans && contains_ascii_icase(bytes, b"transition") {
                trans = true;
            }
            if anim && trans {
                break;
            }
        }
    }
    (anim, trans)
}

#[cfg(feature = "js")]
fn rebuild_timeline_collection(
    post_doc: &Document,
    base_url: &Url,
    media: &megi_css::MediaContext,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
    external_sheets: &super::imports::ExternalSheetCache,
) -> megi_css::StyleCollection {
    let mut collection = megi_css::create_user_agent_style_collection();
    collection.media = media.clone();
    let collected = megi_css::with_calc_viewport(
        media.viewport.unwrap_or((1280.0, 720.0)).0,
        media.viewport.unwrap_or((1280.0, 720.0)).1,
        || stylesheets::collect_stylesheets(post_doc, base_url, media),
    );
    for source in &collected.sources {
        match source {
            stylesheets::CssSource::Inline { text, nonce, .. } => {
                if let Some(policy) = csp
                    && !policy.allows_inline_style(nonce.as_deref(), text)
                {
                    continue;
                }
                collection.add_css(text, megi_css::Origin::Author);
            }
            stylesheets::CssSource::External { url, .. } => {
                if let Some(entries) = external_sheets.get(&url.cache_key()) {
                    for (import_rule, sheet) in entries {
                        match import_rule {
                            Some(rule) => {
                                let _ = collection.add_import_stylesheet(
                                    rule,
                                    sheet,
                                    megi_css::Origin::Author,
                                );
                            }
                            None => collection.add_stylesheet(sheet, megi_css::Origin::Author),
                        }
                    }
                }
            }
        }
    }
    collection
}

#[cfg(feature = "js")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_timeline_events(
    post_doc: &Document,
    elapsed_ms: f32,
    base_url: &Url,
    media: &megi_css::MediaContext,
    csp: Option<&megi_net::csp::ContentSecurityPolicy>,
    pre_collection: Option<&Arc<megi_css::StyleCollection>>,
    pre_sheet_sig: u64,
    external_sheets: &super::imports::ExternalSheetCache,
    transition_from: Option<&Arc<HashMap<NodeId, ComputedStyle>>>,
    viewport: (f32, f32),
    cached_post_styles: &SharedPostStylesCache,
) -> Vec<crate::scripts::TimelineEvent> {
    let post_sheet_sig = stylesheet_signature(post_doc);
    let collection: Arc<megi_css::StyleCollection> = match pre_collection {
        Some(pre) if post_sheet_sig == pre_sheet_sig => Arc::clone(pre),
        _ => Arc::new(rebuild_timeline_collection(
            post_doc,
            base_url,
            media,
            csp,
            external_sheets,
        )),
    };

    let check_anims = !collection.keyframes.is_empty();
    let check_trans = transition_from.is_some();
    if !check_anims && !check_trans {
        return Vec::new();
    }
    let rule_anims = check_anims && collection.may_have_animations();
    let rule_trans = check_trans && collection.may_have_transitions();
    let (inline_anim, inline_trans) =
        if (!check_anims || rule_anims) && (!check_trans || rule_trans) {
            (false, false)
        } else {
            has_inline_timeline_hints(post_doc)
        };
    if !(check_anims && (rule_anims || inline_anim))
        && !(check_trans && (rule_trans || inline_trans))
    {
        return Vec::new();
    }

    let dom_sig = style_dom_signature(post_doc);
    let cached_hit = cached_post_styles
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .filter(|entry| entry.stylesheet_sig == post_sheet_sig && entry.dom_sig == dom_sig)
        .map(|entry| Arc::clone(&entry.styles));
    let post_styles = match cached_hit {
        Some(styles) => styles,
        None => {
            let (styles, pseudos) = super::styles::resolve_base_styles_with_state(
                post_doc,
                &collection,
                megi_css::StateContext::new(),
                viewport,
            );
            let styles = Arc::new(styles);
            *cached_post_styles.lock().unwrap_or_else(|p| p.into_inner()) =
                Some(CachedPostStyles {
                    stylesheet_sig: post_sheet_sig,
                    dom_sig,
                    styles: Arc::clone(&styles),
                    pseudo_styles: Arc::new(pseudos),
                });
            styles
        }
    };

    let from_map = transition_from.map(Arc::as_ref);
    let slot_lookup =
        from_map.and_then(|m| megi_css::cascade::previous_style_slot_lookup(post_doc, m));
    let mut out = Vec::new();
    for node_id in post_doc.descendants(post_doc.root()) {
        let Some(to) = post_styles.get(&node_id) else {
            continue;
        };
        if !to.animations.is_empty() && !collection.keyframes.is_empty() {
            for ev in megi_css::animation_runtime::collect_animation_events(
                &to.animations,
                &collection.keyframes,
                elapsed_ms,
            ) {
                out.push(crate::scripts::TimelineEvent {
                    target: node_id,
                    event_type: ev.event_type,
                    kind: crate::scripts::TimelineEventKind::Animation {
                        animation_name: ev.animation_name,
                        elapsed_time_secs: ev.elapsed_time_secs,
                    },
                });
            }
        }
        let from_style = match (&slot_lookup, from_map) {
            (Some(by_slot), _) => by_slot.get(&node_id.slot()).copied(),
            (None, Some(m)) => m.get(&node_id),
            (None, None) => None,
        };
        if !to.transitions.is_empty()
            && let Some(from) = from_style
        {
            for ev in megi_css::animation_runtime::collect_transition_events(from, to, elapsed_ms) {
                out.push(crate::scripts::TimelineEvent {
                    target: node_id,
                    event_type: ev.event_type,
                    kind: crate::scripts::TimelineEventKind::Transition {
                        property_name: ev.property_name,
                        elapsed_time_secs: ev.elapsed_time_secs,
                    },
                });
            }
        }
    }
    out
}

/// Values produced by the scripting stage and only read when assembling the final output.
pub(crate) struct ScriptStats {
    pub(crate) styles_pre_script: Duration,
    pub(crate) script_bytes: usize,
    pub(crate) scripting_time: Duration,
    pub(crate) scripts_executed: usize,
    pub(crate) scripts_failed: usize,
    pub(crate) console: Vec<ConsoleMessage>,
    pub(crate) script_tasks: usize,
    pub(crate) script_loop_iterations: usize,
    pub(crate) script_virtual_time_ms: u64,
    pub(crate) script_events: usize,
    pub(crate) script_timers_pending: usize,
    pub(crate) event_loop_time: Duration,
    pub(crate) location_actions: Vec<LocationAction>,
    pub(crate) history_actions: Vec<HistoryAction>,
    pub(crate) script_limited: bool,
}

/// Values produced by the stylesheet/font/image/background stage.
pub(crate) struct ResourceStageOutput {
    pub(crate) warnings: Vec<String>,
    pub(crate) inline_svg_report: inline_svg::InlineSvgReport,
    pub(crate) style_summary: super::styles::StyleSummary,
    pub(crate) styles: HashMap<NodeId, ComputedStyle>,
    pub(crate) pseudo_styles: megi_css::PseudoStyles,
    pub(crate) styles_time: Duration,
    pub(crate) fonts_time: Duration,
    pub(crate) images_time: Duration,
    pub(crate) backgrounds_time: Duration,
    pub(crate) loaded_webfonts: webfonts::LoadedWebFonts,
    pub(crate) loaded_images: images::LoadedImages,
    pub(crate) loaded_backgrounds: backgrounds::LoadedBackgrounds,
}
