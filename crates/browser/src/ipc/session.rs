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
    /// Converts a wire response into a parent-side frame. A failed render has no DOM to
    /// restore, so an empty document is used and the error is preserved in the response.
    pub fn from_response(response: IpcRenderResponse) -> Self {
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
    pub fn to_render_frame(&self, css_width: u32) -> Result<crate::worker::RenderFrame, IpcError> {
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
        Ok(crate::worker::RenderFrame {
            generation: self.response.generation,
            width: css_width,
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
                .map(|region| region.into_link_region_with_document(&self.document))
                .collect(),
            images: self
                .response
                .images
                .iter()
                .cloned()
                .map(|region| region.into_image_region(&self.document))
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
                .filter_map(|control| control.into_control(&self.document))
                .collect(),
            document: std::sync::Arc::new(self.document.clone()),
            find_matches: self
                .response
                .find_matches
                .iter()
                .cloned()
                .map(|m| m.into_match(&self.document))
                .collect(),
            find_rects: self
                .response
                .find_rects
                .iter()
                .copied()
                .map(IpcRect::into_rect)
                .collect(),
        })
    }
}

/// A stateful renderer-process session: each render returns the post-script DOM, which the
/// parent keeps and can mutate before sending the next `DomPatch` (ADR-004 phase 3).
pub struct ProcessRenderSession {
    renderer: ProcessRenderer,
    document: Option<megi_webview::core::Document>,
}

impl ProcessRenderSession {
    pub fn spawn() -> Result<Self, IpcError> {
        Ok(Self {
            renderer: ProcessRenderer::spawn()?,
            document: None,
        })
    }

    /// Renders a fresh request and caches the returned DOM.
    pub fn render(&mut self, request: IpcRenderRequest) -> Result<ProcessFrame, IpcError> {
        let response = self.renderer.render(request)?;
        let frame = ProcessFrame::from_response(response);
        self.document = Some(frame.document.clone());
        Ok(frame)
    }

    /// Patches the cached DOM and re-renders in the same process. Fails when no document has
    /// been rendered yet.
    pub fn patch(&mut self, base_url: &str) -> Result<ProcessFrame, IpcError> {
        let document = self
            .document
            .as_ref()
            .ok_or_else(|| IpcError::Io("no document to patch".to_owned()))?;
        let patch = DomPatch::from_document(document, base_url);
        let response = self.renderer.render(IpcRenderRequest {
            generation: 0,
            input: patch.input,
            css_width: 0,
            css_height: 0,
            base_override: patch.base_override,
            scripting: true,
            find_query: None,
        })?;
        let frame = ProcessFrame::from_response(response);
        self.document = Some(frame.document.clone());
        Ok(frame)
    }

    pub fn document(&self) -> Option<&megi_webview::core::Document> {
        self.document.as_ref()
    }
}
