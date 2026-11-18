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

use super::url::FrameRenderContext;
use crate::{ConsoleMessage, HistoryAction, LocationAction, RenderCache, RenderOptions};
use crate::{backgrounds, frames, images, inline_svg, webfonts};

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
        megi_css::StyleCollection,
        super::styles::StyleSummary,
        Vec<webfonts::LocatedFontFace>,
        u64,
    )>,
    pub(crate) pre_warnings: Vec<String>,
    pub(crate) pre_scripted_frames: frames::PreScriptedFrameMap,
    pub(crate) scripts_executed: usize,
    #[cfg(feature = "js")]
    pub(crate) canvas_surfaces: HashMap<NodeId, megi_js::canvas::CanvasSurface>,
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
