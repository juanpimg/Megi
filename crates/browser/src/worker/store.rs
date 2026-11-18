use super::*;

impl FrameStore {
    /// Creates an empty store at generation 0 (no frame).
    pub const fn new() -> Self {
        Self {
            generation: 0,
            pending: false,
            frame: None,
            error: None,
        }
    }

    /// Current generation (0 before the first request).
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether a request is waiting for its result.
    pub const fn pending(&self) -> bool {
        self.pending
    }

    /// Latest successfully rendered frame, if any.
    pub fn frame(&self) -> Option<&RenderFrame> {
        self.frame.as_ref()
    }

    /// Latest error text, if the current generation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Takes the latest error (clearing it).
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// Starts a new generation and returns the request to send to the worker.
    pub fn begin_request(&mut self, input: &str, css_width: u32, css_height: u32) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a render that also computes find-in-page hits for `query`.
    pub fn begin_find_request(
        &mut self,
        input: &str,
        css_width: u32,
        css_height: u32,
        query: &str,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: None,
            scripting: true,
            find_query: Some(query.to_owned()),
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a render for a **patched-DOM data: snapshot** (form editing): scripts are
    /// disabled (the snapshot already contains their effects) and relative resources are
    /// resolved against `base_override` (the real page URL).
    pub fn begin_patched_request(
        &mut self,
        input: &str,
        css_width: u32,
        css_height: u32,
        base_override: Option<&str>,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: base_override.map(str::to_owned),
            scripting: false,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a render for a **patched-DOM data: snapshot** that replays shell UI events
    /// (`keydown`, `input`, `focus`, `blur`, `change`, `submit`) into the live JS realm (#1504).
    pub fn begin_ui_event_request(
        &mut self,
        snapshot: &str,
        css_width: u32,
        css_height: u32,
        base_override: Option<&str>,
        host_dom_events: Vec<megi_webview::render::HostDomEvent>,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: snapshot.to_owned(),
            css_width,
            css_height,
            base_override: base_override.map(str::to_owned),
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events,
            form_post: None,
        }
    }

    /// Starts a render of the current DOM snapshot with host-dispatched window events
    /// (`popstate`/`hashchange` after a same-document history navigation, #1503). Scripts run
    /// on the snapshot so their listeners observe the event and their mutations are painted;
    /// relative resources resolve against the real page URL via `base_override`.
    pub fn begin_history_event_request(
        &mut self,
        snapshot: &str,
        css_width: u32,
        css_height: u32,
        base_override: Option<&str>,
        events: &[&str],
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: snapshot.to_owned(),
            css_width,
            css_height,
            base_override: base_override.map(str::to_owned),
            scripting: true,
            find_query: None,
            host_events: events.iter().map(|event| (*event).to_owned()).collect(),
            host_dom_events: Vec::new(),
            form_post: None,
        }
    }

    /// Starts a form POST navigation (#1538): fetches `input` with the POST body through the
    /// render pipeline, so the response document is painted with the submission result.
    pub fn begin_form_post_request(
        &mut self,
        input: &str,
        css_width: u32,
        css_height: u32,
        content_type: &str,
        body: Vec<u8>,
    ) -> RenderRequest {
        self.generation = self.generation.saturating_add(1);
        self.pending = true;
        RenderRequest {
            generation: self.generation,
            input: input.to_owned(),
            css_width,
            css_height,
            base_override: None,
            scripting: true,
            find_query: None,
            host_events: Vec::new(),
            host_dom_events: Vec::new(),
            form_post: Some((content_type.to_owned(), body)),
        }
    }

    /// Applies a worker message; returns `true` only when it is current.
    pub fn accept(&mut self, message: WorkerMessage) -> bool {
        if message.generation() != self.generation {
            return false;
        }
        self.pending = false;
        match message {
            WorkerMessage::Frame(frame) => {
                self.frame = Some(frame);
                self.error = None;
            }
            WorkerMessage::Failed { message, .. } => {
                self.error = Some(message);
            }
        }
        true
    }
}
