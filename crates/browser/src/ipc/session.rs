//! Renderer-process session and frame conversion (ADR-004).

use super::*;

/// A rendered frame returned by a renderer process, with the DOM restored locally so the
/// parent can build the next patch (ADR-004 phase 3).
#[derive(Debug)]
pub struct ProcessFrame {
    pub response: IpcRenderResponse,
    pub document: megi_webview::core::Document,
}

impl ProcessFrame {
    /// Converts a validated wire response into a parent-side frame (#2308). A failed render
    /// has no DOM to restore, so an empty document is used and the error is preserved.
    pub fn from_response(response: Validated<IpcRenderResponse>) -> Self {
        let response = response.into_inner();
        let document = if response.document_html.is_empty() {
            megi_webview::core::Document::new()
        } else {
            DomPatch::restore_document(&response.document_html)
        };
        Self { response, document }
    }
}

impl ProcessFrame {
    /// Converts the wire response into a shell `RenderFrame`. Hit regions and form controls
    /// are empty in process mode (they require the in-process layout tree); document editing
    /// still works because `DomPatch` round-trips the serialized DOM (ADR-004 phase 3).
    pub fn to_render_frame(&self, _css_width: u32) -> Result<crate::worker::RenderFrame, IpcError> {
        if let Some(error) = &self.response.error {
            return Err(IpcError::Io(error.clone()));
        }
        let image = megi_webview::image::decode_png(
            &self.response.png_bytes,
            &megi_webview::image::PngLimits::default(),
        )
        .map_err(|error| IpcError::Io(error.to_string()))?;
        let mut pixels = Vec::with_capacity((image.width as usize) * (image.height as usize));
        for chunk in image.data.as_chunks::<4>().0 {
            pixels.push(
                u32::from(chunk[0]) << 24
                    | u32::from(chunk[1]) << 16
                    | u32::from(chunk[2]) << 8
                    | 0xFF,
            );
        }
        let height = if self.response.content_height > 0 {
            self.response.content_height
        } else {
            image.height
        };
        let nodes = DocumentOrdinalNodes::build(&self.document);
        Ok(crate::worker::RenderFrame {
            generation: self.response.generation,
            width: self.response.css_width,
            height,
            pixels,
            png_bytes: self.response.png_bytes.clone(),
            content_height: height,
            capped: false,
            links: self
                .response
                .links
                .iter()
                .cloned()
                .map(|region| region.into_link_region_indexed(&nodes))
                .collect(),
            images: self
                .response
                .images
                .iter()
                .cloned()
                .map(|region| region.into_image_region_indexed(&nodes))
                .collect(),
            title: self.response.title.clone(),
            document_url: self.response.document_url.clone(),
            scripts_executed: self.response.scripts_executed,
            script_timers_pending: self.response.script_timers_pending,
            warnings: self.response.warnings,
            script_errors: self.response.script_errors,
            forms: self
                .response
                .forms
                .iter()
                .cloned()
                .filter_map(|control| control.into_control_indexed(&nodes))
                .collect(),
            document: std::sync::Arc::new(self.document.clone()),
            find_matches: self
                .response
                .find_matches
                .iter()
                .cloned()
                .map(|m| m.into_match_indexed(&nodes))
                .collect(),
            find_rects: self
                .response
                .find_rects
                .iter()
                .copied()
                .map(IpcRect::into_rect)
                .collect(),
            // #1828: navigation requests are not transported over the IPC
            // frame path yet; the in-process path consumes them.
            location_actions: Vec::new(),
            history_actions: Vec::new(),
        })
    }
}

/// A stateful renderer-process session: each render returns the post-script DOM, which the
/// parent keeps and can mutate before sending the next `DomPatch` (ADR-004 phase 3).
pub struct ProcessRenderSession {
    renderer: ProcessRenderer,
    document: Option<megi_webview::core::Document>,
    last_viewport: Option<(u32, u32)>,
    last_options: Option<IpcRenderOptions>,
    /// Last generation accepted from this renderer; patches must use `last + 1` (#2308).
    last_generation: u64,
}

impl ProcessRenderSession {
    pub fn spawn() -> Result<Self, IpcError> {
        Ok(Self {
            renderer: ProcessRenderer::spawn()?,
            document: None,
            last_viewport: None,
            last_options: None,
            last_generation: 0,
        })
    }

    /// Renders a fresh request and caches the returned DOM.
    pub fn render(&mut self, request: IpcRenderRequest) -> Result<ProcessFrame, IpcError> {
        let viewport = (request.css_width, request.css_height);
        let options = request.options.clone();
        let response = self.renderer.render_validated(request)?;
        self.last_generation = response.inner().generation;
        self.last_viewport = Some(viewport);
        self.last_options = Some(options);
        self.apply_response(response)
    }

    /// Patches the cached DOM and re-renders in the same process. Fails when no document has
    /// been rendered yet.
    pub fn patch(&mut self, base_url: &str) -> Result<ProcessFrame, IpcError> {
        let request = self.build_patch_request(base_url)?;
        let response = self.renderer.render_validated(request)?;
        self.last_generation = response.inner().generation;
        self.apply_response(response)
    }

    fn build_patch_request(&self, base_url: &str) -> Result<IpcRenderRequest, IpcError> {
        let document = self
            .document
            .as_ref()
            .ok_or_else(|| IpcError::Io("no document to patch".to_owned()))?;
        let patch = DomPatch::from_document(document, base_url);
        let (css_width, css_height) = self.last_viewport.unwrap_or((800, 600));
        Ok(IpcRenderRequest {
            // #2308: reuse of generation 0 would be rejected by the parent's validation.
            generation: self.last_generation.saturating_add(1),
            input: patch.input,
            css_width,
            css_height,
            base_override: patch.base_override,
            scripting: true,
            find_query: None,
            options: self.last_options.clone().unwrap_or_default(),
        })
    }

    fn apply_response(
        &mut self,
        response: Validated<IpcRenderResponse>,
    ) -> Result<ProcessFrame, IpcError> {
        if let Some(error) = &response.inner().error {
            return Err(IpcError::Io(error.clone()));
        }
        let frame = ProcessFrame::from_response(response);
        self.document = Some(frame.document.clone());
        Ok(frame)
    }

    pub fn document(&self) -> Option<&megi_webview::core::Document> {
        self.document.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_request_reuses_last_viewport_instead_of_zero() {
        let mut doc = megi_webview::core::Document::new();
        let html = doc.create_element(megi_webview::core::Namespace::Html, "html", Vec::new());
        doc.append_child(doc.root(), html);
        let mut session = ProcessRenderSession {
            renderer: ProcessRenderer::spawn().expect("spawn"),
            document: Some(doc),
            last_viewport: Some((800, 600)),
            last_options: None,
            last_generation: 4,
        };
        let req = session.build_patch_request("https://example.com/").unwrap();
        assert_eq!((req.css_width, req.css_height), (800, 600));
        assert_eq!(req.generation, 5, "a patch must advance the generation");

        // An error response must not overwrite the cached document.
        let err_resp = IpcRenderResponse {
            generation: 5,
            png_bytes: Vec::new(),
            content_height: 0,
            css_width: 800,
            title: None,
            document_url: String::new(),
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
            error: Some("render failed".to_owned()),
            sandbox_mode: String::new(),
        };
        let validated = validate_response(err_resp, 4).expect("error frame still validates");
        assert!(session.apply_response(validated).is_err());
        assert!(session.document().is_some());
    }
}
