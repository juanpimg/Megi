//! Child-side IPC worker stream: serves versioned envelopes until stdin closes.
//!
//! The loop lives in its own module so `ipc.rs` stays below the 500-line hard cap
//! (AGENTS.md §4) while the validated envelope is wired into the live renderer path.

use std::io::{Read, Write};

use super::{
    DocumentNodeOrdinals, IPC_KIND_REQUEST, IPC_KIND_RESPONSE, IpcError, IpcFindMatch,
    IpcFormControl, IpcImageRegion, IpcLinkRegion, IpcRect, IpcRenderRequest, IpcRenderResponse,
    read_framed_envelope, write_framed_envelope,
};

/// Entry point for `--megi-ipc-worker`: serves framed requests until stdin closes.
pub fn run_ipc_worker() -> Result<(), IpcError> {
    // M3#4: restrict this process before any untrusted IPC frame is read. The parent only
    // prepares the scratch cwd; the child decides its own mode, so no unsafe pre_exec is used.
    let scratch = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let allow_network = std::env::var("MEGI_SANDBOX_ALLOW_NETWORK")
        .map(|value| value != "0")
        .unwrap_or(true);
    super::sandbox::enforce_worker_sandbox(&scratch, allow_network);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    run_ipc_worker_stream(&mut reader, &mut writer)
}

/// Stream-based worker loop: serves framed requests until EOF. Testable in memory, which
/// gives the per-site process backend real protocol E2E coverage without a display server.
pub fn run_ipc_worker_stream<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<(), IpcError> {
    run_ipc_worker_stream_with(reader, writer, crate::worker::render_document_with_options)
}

/// Injectable renderer variant of [`run_ipc_worker_stream`] used by protocol tests.
pub fn run_ipc_worker_stream_with<R: Read, W: Write, F>(
    reader: &mut R,
    writer: &mut W,
    mut render_fn: F,
) -> Result<(), IpcError>
where
    F: FnMut(
        &crate::worker::RenderRequest,
        &megi_webview::render::RenderOptions,
    ) -> Result<crate::worker::RenderFrame, String>,
{
    loop {
        // #2308: validate the wire envelope before deserializing a request. The kind and
        // request id are echoed back in the response so the parent can bind a reply to its
        // request and reject a renderer that answers the wrong slot.
        let (header, request): (super::WireHeader, IpcRenderRequest) =
            match read_framed_envelope(reader)? {
                Some(frame) => frame,
                None => return Ok(()), // parent closed stdin: clean shutdown
            };
        if header.kind != IPC_KIND_REQUEST {
            return Err(IpcError::InvalidPayload(format!(
                "unexpected IPC request kind {}",
                header.kind
            )));
        }
        let render_request = crate::worker::RenderRequest {
            generation: request.generation,
            input: request.input.clone(),
            css_width: request.css_width,
            css_height: request.css_height,
            raster_scale: request.options.raster_scale(),
            base_override: request.base_override.clone(),
            scripting: request.scripting,
            find_query: request.find_query.clone(),
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        };
        let options = request.options.to_render_options();
        let render_result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_fn(&render_request, &options)
        })) {
            Ok(result) => result,
            Err(payload) => {
                let reason = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".to_string());
                Err(format!("render worker panicked: {reason}"))
            }
        };
        let response = match render_result {
            Ok(frame) => {
                let ordinals = DocumentNodeOrdinals::build(&frame.document);
                IpcRenderResponse {
                    generation: frame.generation,
                    png_bytes: frame.png_bytes,
                    content_height: frame.content_height,
                    css_width: request.css_width,
                    title: frame.title,
                    document_url: frame.document_url,
                    scripts_executed: frame.scripts_executed,
                    script_timers_pending: frame.script_timers_pending,
                    warnings: frame.warnings,
                    script_errors: frame.script_errors,
                    links: frame
                        .links
                        .iter()
                        .map(|region| IpcLinkRegion::from_region_indexed(region, &ordinals))
                        .collect(),
                    images: frame
                        .images
                        .iter()
                        .map(|region| IpcImageRegion::from_region_indexed(region, &ordinals))
                        .collect(),
                    forms: frame
                        .forms
                        .iter()
                        .map(|control| IpcFormControl::from_control_indexed(control, &ordinals))
                        .collect(),
                    find_matches: frame
                        .find_matches
                        .iter()
                        .map(|m| IpcFindMatch::from_match_indexed(m, &ordinals))
                        .collect(),
                    find_rects: frame
                        .find_rects
                        .iter()
                        .copied()
                        .map(IpcRect::from_rect)
                        .collect(),
                    document_html: crate::download::serialize_document(&frame.document),
                    error: None,
                    sandbox_mode: super::sandbox::reported_sandbox_mode().to_owned(),
                }
            }
            Err(error) => IpcRenderResponse {
                generation: request.generation,
                png_bytes: Vec::new(),
                content_height: 0,
                css_width: 0,
                title: None,
                document_url: request.input,
                scripts_executed: 0,
                script_timers_pending: 0,
                warnings: 0,
                script_errors: 0,
                links: Vec::new(),
                images: Vec::new(),
                forms: Vec::new(),
                find_matches: Vec::new(),
                find_rects: Vec::new(),
                document_html: String::new(),
                error: Some(error),
                sandbox_mode: super::sandbox::reported_sandbox_mode().to_owned(),
            },
        };
        // Echo the caller's request id and site token: the parent refuses any response
        // whose header does not match the slot that sent the request (#2308).
        write_framed_envelope(
            writer,
            IPC_KIND_RESPONSE,
            header.request_id,
            header.site_token,
            &response,
        )?;
    }
}
