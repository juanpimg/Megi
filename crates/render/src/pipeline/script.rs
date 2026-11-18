//! Scripting stage: pre-script styles, script execution and the bounded event loop.
//!
//! The stage body is moved verbatim from `lib.rs`; only the surrounding function
//! signature and the state (un)packing were added to keep the file within size caps.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use megi_core::NodeId;
use megi_css::ComputedStyle;

use super::imports::stylesheet_signature;
use super::state::{ParsedDocumentInput, ScriptContext, ScriptStats};
#[cfg(feature = "js")]
use super::styles::collect_layout_rects;
use super::styles::{StyleSummary, collect_styles, document_csp, resolve_styles_with_state};
use crate::{RenderError, ScriptingMode};
use crate::{frames, scripts, stylesheets, webfonts};

#[allow(unused_variables)] // input fields are threaded unchanged to the next stage
pub(crate) fn run_script_stage<'a>(
    input: ParsedDocumentInput<'a>,
) -> Result<(ParsedDocumentInput<'a>, ScriptContext, ScriptStats), RenderError> {
    let ParsedDocumentInput {
        mut document,
        document_url,
        document_headers,
        options,
        mut loader,
        cookie_jar,
        referrer_policy,
        source_mentions_svg,
        document_bytes,
        charset,
        status,
        content_encoding,
        limit_violations,
        cache,
        frame_depth,
        frame_ctx,
        budget,
        fetch_time,
        decode_parse_time,
        total_start,
    } = input;

    #[cfg(feature = "js")]
    let opaque_origin = frame_ctx
        .sandbox_flags
        .is_some_and(|flags| !flags.allow_same_origin);
    #[cfg(not(feature = "js"))]
    let opaque_origin = false;

    // Base URL for every relative subresource (scripts, stylesheets, images, backgrounds).
    // `<base href>` (or `--base`) sets the URL used for every relative resource.
    let mut warnings = Vec::new();
    let override_base = match &options.base_override {
        Some(base) => Some(document_url.join(base).map_err(RenderError::Url)?),
        None => None,
    };
    let (mut base_url, base_warnings) =
        stylesheets::find_base_url(&document, override_base.as_ref().unwrap_or(&document_url));
    warnings.extend(base_warnings);
    if override_base.is_some() && base_url == document_url {
        // `--base` should win over a missing `<base>`; keep the explicit value.
        base_url = override_base.clone().unwrap_or(document_url.clone());
    }
    let csp = document_csp(document_headers, &document);

    // Media context shared by the stylesheet cascade, the pre-script style snapshot and
    // the script `matchMedia`/viewport providers (same viewport as layout).
    let media_context = megi_css::MediaContext::with_viewport(
        options.viewport.width as f32,
        options.viewport.height as f32,
    );

    // Discover the page's classic scripts early (cheap DOM walk): the pre-script style
    // snapshot is only computed for pages that will actually run scripts. Discovery
    // warnings are collected once and merged with the scripting stage warnings.
    let mut script_warnings = Vec::new();
    let script_requested = !frame_ctx.skip_scripts && options.scripting == ScriptingMode::Enabled;
    let script_candidates = if script_requested && cfg!(feature = "js") {
        scripts::discover_scripts(
            &document,
            &mut script_warnings,
            scripts::MAX_DISCOVERED_SCRIPTS,
        )
    } else {
        Vec::new()
    };

    // Pre-script style pass (R96): resolve the author+UA cascade once over the parsed DOM so
    // `window.getComputedStyle` exposes real styles to scripts. The collection is reused by
    // the post-script pass when scripts did not add/remove `<style>`/`<link>` elements;
    // otherwise the post-script pass recollects (script-inserted stylesheets still apply).
    let mut styles_pre_script = Duration::ZERO;
    let mut pre_script_styles: Option<Arc<HashMap<NodeId, ComputedStyle>>> = None;
    #[cfg(feature = "js")]
    let mut pre_script_pseudos: Option<Arc<megi_css::PseudoStyles>> = None;
    let mut reusable_stylesheets: Option<(
        megi_css::StyleCollection,
        StyleSummary,
        Vec<webfonts::LocatedFontFace>,
        u64,
    )> = None;
    let mut pre_warnings = Vec::new();
    if !script_candidates.is_empty() {
        let pre_start = Instant::now();
        let stylesheet_signature_before = stylesheet_signature(&document);
        if let Ok((collection, summary, fonts)) = collect_styles(
            &document,
            &base_url,
            &mut loader,
            options,
            &media_context,
            csp.as_ref(),
            &mut pre_warnings,
        ) {
            let (resolved, resolved_pseudos) = resolve_styles_with_state(
                &document,
                &collection,
                megi_css::StateContext::new(),
                options.time_ms,
                (
                    options.viewport.width as f32,
                    options.viewport.height as f32,
                ),
            );
            pre_script_styles = Some(Arc::new(resolved));
            #[cfg(feature = "js")]
            {
                pre_script_pseudos = Some(Arc::new(resolved_pseudos));
            }
            #[cfg(not(feature = "js"))]
            let _ = resolved_pseudos;
            reusable_stylesheets = Some((collection, summary, fonts, stylesheet_signature_before));
        }
        styles_pre_script = pre_start.elapsed();
    }

    let scripting_start = Instant::now();
    let mut script_bytes = 0usize;
    let mut scripting_execution: Option<scripts::ScriptExecution> = None;
    // Canvas 2D surfaces painted by scripts (#1545); inserted into the image store after images load.
    // The map is only meaningful with the `js` feature: without scripts there is no canvas.
    #[cfg(feature = "js")]
    let mut canvas_surfaces: std::collections::HashMap<
        megi_core::NodeId,
        megi_js::canvas::CanvasSurface,
    > = std::collections::HashMap::new();
    if script_requested && !cfg!(feature = "js") {
        warnings.push(
            "scripting was requested but this build has no `js` feature; scripts are ignored"
                .to_owned(),
        );
    }
    if script_requested && cfg!(feature = "js") {
        let load_report = if script_candidates.is_empty() {
            scripts::ScriptLoadReport::default()
        } else {
            scripts::load_scripts_with_csp(
                &script_candidates,
                &base_url,
                &mut loader,
                options.scripting_limits,
                options.no_network,
                &mut script_warnings,
                csp.as_ref(),
            )
        };
        script_bytes = load_report.script_bytes;
        let mut host_context = scripts::ScriptingHostContext::new(
            pre_script_styles.clone(),
            media_context.clone(),
            (options.viewport.width, options.viewport.height),
        );
        #[cfg(feature = "js")]
        for iframe in document.get_elements_by_tag_name("iframe") {
            let Some(data) = document.try_node(iframe).and_then(|n| n.as_element()) else {
                continue;
            };
            if data.has_attribute("sandbox") {
                continue;
            }
            if let Some(srcdoc) = data.get_attribute("srcdoc") {
                host_context
                    .frame_documents
                    .push((iframe, megi_html::parse_document(srcdoc, false)));
            }
        }
        #[cfg(feature = "js")]
        {
            if let Some(styles) = pre_script_styles.clone() {
                let pre_pseudos = pre_script_pseudos.clone();
                let pre_layout_viewport = megi_layout::Rect::new(
                    0.0,
                    0.0,
                    options.viewport.width as f32,
                    options.viewport.height as f32,
                );
                let doc_snapshot = document.clone();
                host_context.layout_provider = Some(Arc::new(move || {
                    let mut rects = std::collections::HashMap::new();
                    let empty_pseudos = megi_css::PseudoStyles::new();
                    let pseudos_ref = pre_pseudos.as_deref().unwrap_or(&empty_pseudos);
                    if let Some(layout_root) = megi_layout::layout_document_with_pseudos(
                        &doc_snapshot,
                        &styles,
                        pseudos_ref,
                        pre_layout_viewport,
                    ) {
                        collect_layout_rects(&layout_root, &mut rects);
                    }
                    rects
                }));
            }
            host_context.sandbox_flags = frame_ctx.sandbox_flags;
        }
        #[cfg(feature = "js")]
        {
            host_context.animation_time_ms = options.time_ms;
            let transition_from = options
                .transition_from_styles
                .clone()
                .or_else(|| pre_script_styles.clone());
            let timeline_media = media_context.clone();
            host_context.timeline_events_provider = Some(Arc::new(
                move |post_doc: &megi_core::Document, elapsed_ms: f32| {
                    // Rebuild the author stylesheet set from the post-script DOM: scripts may
                    // have inserted `<style>` elements after the pre-script cascade (#941).
                    let mut collection = megi_css::create_user_agent_style_collection();
                    collection.media = timeline_media.clone();
                    for style_el in post_doc.get_elements_by_tag_name("style") {
                        collection
                            .add_css(&post_doc.text_content(style_el), megi_css::Origin::Author);
                    }
                    let (post_styles, _) =
                        megi_css::resolve_document_styles_from_collection(post_doc, &collection);
                    let slot_lookup = transition_from
                        .as_deref()
                        .and_then(|m| megi_css::cascade::previous_style_slot_lookup(post_doc, m));
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
                                out.push(scripts::TimelineEvent {
                                    target: node_id,
                                    event_type: ev.event_type,
                                    kind: scripts::TimelineEventKind::Animation {
                                        animation_name: ev.animation_name,
                                        elapsed_time_secs: ev.elapsed_time_secs,
                                    },
                                });
                            }
                        }
                        let from_style = match (&slot_lookup, transition_from.as_deref()) {
                            (Some(by_slot), _) => by_slot.get(&node_id.slot()).copied(),
                            (None, Some(m)) => m.get(&node_id),
                            (None, None) => None,
                        };
                        if !to.transitions.is_empty()
                            && let Some(from) = from_style
                        {
                            for ev in megi_css::animation_runtime::collect_transition_events(
                                from, to, elapsed_ms,
                            ) {
                                out.push(scripts::TimelineEvent {
                                    target: node_id,
                                    event_type: ev.event_type,
                                    kind: scripts::TimelineEventKind::Transition {
                                        property_name: ev.property_name,
                                        elapsed_time_secs: ev.elapsed_time_secs,
                                    },
                                });
                            }
                        }
                    }
                    out
                },
            ));
        }
        host_context.frame_depth = frame_depth;
        host_context.storage = if opaque_origin {
            None
        } else {
            options.storage.clone()
        };
        host_context.cookie_jar = if opaque_origin {
            None
        } else {
            cookie_jar.clone()
        };
        host_context.enable_workers = options.enable_workers;
        host_context.host_events = options.host_events.clone();
        host_context.host_dom_events = options.host_dom_events.clone();
        host_context.fetch_options = loader.fetch_options().clone();
        host_context.no_network = options.no_network;
        host_context.csp = csp.clone();
        let mut execution = scripts::execute_scripts(
            document,
            &document_url,
            load_report.scripts,
            options.scripting_limits,
            &host_context,
            &mut script_warnings,
        );
        document = std::mem::take(&mut execution.document);
        #[cfg(feature = "js")]
        {
            canvas_surfaces = std::mem::take(&mut execution.canvas_surfaces);
        }
        warnings.extend(script_warnings);
        scripting_execution = Some(execution);
    } else {
        warnings.extend(script_warnings);
    }
    let scripting_time = scripting_start.elapsed();
    #[cfg(feature = "js")]
    let pre_scripted_frames: frames::PreScriptedFrameMap = scripting_execution
        .as_mut()
        .map(|execution| {
            std::mem::take(&mut execution.frame_documents)
                .into_iter()
                .map(|(node, doc, sandbox, url)| (node, (doc, sandbox, url)))
                .collect()
        })
        .unwrap_or_default();
    #[cfg(not(feature = "js"))]
    let pre_scripted_frames = frames::PreScriptedFrameMap::new();
    let (
        scripts_executed,
        scripts_failed,
        console,
        script_tasks,
        script_loop_iterations,
        script_virtual_time_ms,
        script_events,
        script_timers_pending,
        event_loop_time,
        location_actions,
        history_actions,
        script_limited,
    ) = match scripting_execution {
        Some(execution) => (
            execution.executed,
            execution.failed,
            execution.console,
            execution.tasks_ran,
            execution.loop_iterations,
            execution.virtual_time_ms,
            execution.events_dispatched,
            execution.timers_pending,
            execution.event_loop,
            execution.location_actions,
            execution.history_actions,
            execution.limited,
        ),
        None => (
            0,
            0,
            Vec::new(),
            0,
            0,
            0,
            0,
            0,
            Duration::ZERO,
            Vec::new(),
            Vec::new(),
            false,
        ),
    };

    Ok((
        ParsedDocumentInput {
            document,
            document_url,
            document_headers,
            options,
            loader,
            cookie_jar,
            referrer_policy,
            source_mentions_svg,
            document_bytes,
            charset,
            status,
            content_encoding,
            limit_violations,
            cache,
            frame_depth,
            frame_ctx,
            budget,
            fetch_time,
            decode_parse_time,
            total_start,
        },
        ScriptContext {
            warnings,
            base_url,
            csp,
            media_context,
            reusable_stylesheets,
            pre_warnings,
            transition_from_styles: options
                .transition_from_styles
                .clone()
                .or_else(|| pre_script_styles.clone()),
            pre_scripted_frames,
            scripts_executed,
            #[cfg(feature = "js")]
            canvas_surfaces,
        },
        ScriptStats {
            styles_pre_script,
            script_bytes,
            scripting_time,
            scripts_executed,
            scripts_failed,
            console,
            script_tasks,
            script_loop_iterations,
            script_virtual_time_ms,
            script_events,
            script_timers_pending,
            event_loop_time,
            location_actions,
            history_actions,
            script_limited,
        },
    ))
}
