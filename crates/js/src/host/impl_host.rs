use super::*;

impl Host {
    /// Creates the host around a document.
    pub fn new(document: Document, url: String) -> Self {
        Self {
            document,
            console: VecDeque::new(),
            title: String::new(),
            url,
            nodes: Vec::new(),
            node_index: HashMap::new(),
            max_handles: 1_000_000,
            listeners: Vec::new(),
            worker_dom_listeners: Vec::new(),
            free_listener_slots: Vec::new(),
            timers: TimerQueue::default(),
            next_timer_id: 1,
            document_write_bytes: 0,
            max_document_write_bytes: 8 * 1024 * 1024,
            max_document_write_nodes: 1_000_000,
            max_dom_nodes: 1_000_000,
            dom_budget_warned: false,
            document_write_warned: false,
            started_at: Instant::now(),
            virtual_now_ms: None,
            dispatch: None,
            computed_style_provider: None,
            inline_handlers: Vec::new(),
            media_provider: None,
            viewport_provider: None,
            ready_state: "loading".to_string(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            last_event_error: None,
            history_provider: None,
            history_length: 1,
            history_state: "null".to_string(),
            location_provider: None,
            local_storage: StorageMap::default(),
            session_storage: StorageMap::default(),
            storage_provider: None,
            cookie_provider: None,
            cookies_fallback: Vec::new(),
            observers: Vec::new(),
            observer_index: HashMap::new(),
            active_element: None,
            current_script: None,
            layout_provider: None,
            worker_loader: None,
            fetch_provider: None,
            websocket_provider: None,
            websockets: std::collections::HashMap::new(),
            websocket_next_handle: 1,
            shadow_roots: std::collections::HashMap::new(),
            shadow_hosts: std::collections::HashMap::new(),
            canvas_store: crate::canvas::CanvasStore::new(),
            custom_elements: std::collections::HashMap::new(),
            custom_element_next_handle: 1,
            frame_documents: std::collections::HashMap::new(),
            iframe_frames: std::collections::HashMap::new(),
            frame_next_handle: 1,
            frame_nodes: std::collections::HashMap::new(),
            frame_node_next_handle: 1,
            workers: Vec::new(),
            worker_id: None,
            worker_outbox: None,
            module_resolver: None,
            module_loader: None,
            import_map: None,
            initial_control_values: HashMap::new(),
            initial_control_checked: HashMap::new(),
            idl_values: HashMap::new(),
            idl_checked: HashMap::new(),
            opaque_origin: false,
            allow_forms: true,
            allow_popups: true,
            frame_depth: 0,
            frame_loader: None,
            is_child_frame: false,
            parent_origin: None,
            parent_same_origin: false,
            parent_outbox: None,
            self_messages: VecDeque::new(),
            frames: Vec::new(),
            frame_by_node: HashMap::new(),
        }
    }

    /// Returns (and caches) the numeric JS handle for `node_id`.
    pub fn handle_for(&mut self, node_id: NodeId) -> Result<u32, String> {
        let raw = node_id.get();
        if let Some(index) = self.node_index.get(&raw) {
            return u32::try_from(*index).map_err(|_| "node handle table overflow".to_string());
        }
        if self.nodes.len() >= self.max_handles {
            return Err("too many DOM node handles exposed to JavaScript".to_string());
        }
        let index = self.nodes.len();
        self.nodes.push(node_id);
        self.node_index.insert(raw, index);
        u32::try_from(index).map_err(|_| "node handle table overflow".to_string())
    }

    /// Registers or updates an observed target for `slot`, keeping `observer_index` in sync.
    pub fn observe_target(&mut self, slot: u32, target: ObserverTarget) -> Result<(), String> {
        let Some(obs_idx) = self.observers.iter().position(|r| r.object_slot == slot) else {
            return Ok(());
        };
        let registration = &mut self.observers[obs_idx];
        // Re-observing a node only updates its options: the DOM spec keeps one registered
        // observer per (observer, target) pair (#437).
        if let Some(existing) = registration
            .targets
            .iter_mut()
            .find(|existing| existing.node == target.node)
        {
            *existing = target;
            if let Some(entries) = self.observer_index.get_mut(&target.node) {
                for (indexed_obs, indexed_target) in entries.iter_mut() {
                    if *indexed_obs == obs_idx {
                        *indexed_target = target;
                    }
                }
            }
            return Ok(());
        }
        if registration.targets.len() >= MAX_OBSERVER_TARGETS {
            return Err(format!(
                "MutationObserver target limit of {MAX_OBSERVER_TARGETS} exceeded"
            ));
        }
        registration.targets.push(target);
        self.observer_index
            .entry(target.node)
            .or_default()
            .push((obs_idx, target));
        Ok(())
    }

    /// Clears targets and queued records for `slot`, keeping the observer registration re-armable (#1135).
    pub fn disconnect_observer(&mut self, slot: u32) {
        let Some(obs_idx) = self.observers.iter().position(|r| r.object_slot == slot) else {
            return;
        };
        let drained = std::mem::take(&mut self.observers[obs_idx].targets);
        self.observers[obs_idx].records.clear();
        for target in drained {
            if let Some(entries) = self.observer_index.get_mut(&target.node) {
                entries.retain(|(idx, _)| *idx != obs_idx);
                if entries.is_empty() {
                    self.observer_index.remove(&target.node);
                }
            }
        }
    }

    /// Adds a MutationObserver record to each matching observer's queue (bounded per observer).
    ///
    /// Walks `record.target`'s ancestor chain once against `observer_index` instead of scanning
    /// every observer and walking ancestors per observer (`O(depth + matches)` vs `O(observers * depth)`).
    pub fn record_mutation(&mut self, record: MutationRecord) {
        if self.observer_index.is_empty() || self.observers.is_empty() {
            return;
        }
        let mut matched_bits = [0u64; MAX_OBSERVERS.div_ceil(64)];
        let mut want_old_value = [0u64; MAX_OBSERVERS.div_ceil(64)];
        let mut matched_indices: Vec<usize> = Vec::new();
        let max_steps = self.document.len().saturating_add(1);
        let mut current = Some(record.target);
        let mut steps = 0usize;
        while let Some(node_id) = current {
            let is_direct = steps == 0;
            if let Some(entries) = self.observer_index.get(&node_id) {
                for &(obs_idx, target) in entries {
                    let word = obs_idx / 64;
                    let Some(slot_word) = matched_bits.get_mut(word) else {
                        continue;
                    };
                    if obs_idx >= self.observers.len()
                        || !target.matches_kind(record.kind)
                        || (!is_direct && !target.subtree)
                    {
                        continue;
                    }
                    let bit = 1u64 << (obs_idx % 64);
                    if target.attribute_old_value {
                        want_old_value[word] |= bit;
                    }
                    if *slot_word & bit == 0 {
                        *slot_word |= bit;
                        matched_indices.push(obs_idx);
                    }
                }
            }
            if steps >= max_steps {
                break;
            }
            steps += 1;
            current = self.document.try_node(node_id).and_then(|node| node.parent);
        }
        if matched_indices.is_empty() {
            return;
        }
        if matched_indices.len() > 1 {
            matched_indices.sort_unstable();
        }
        for obs_idx in matched_indices {
            let observer = &mut self.observers[obs_idx];
            if observer.records.len() >= MAX_OBSERVER_RECORDS {
                continue;
            }
            let keep_old = record.kind == MutationKind::Attributes
                && (want_old_value[obs_idx / 64] & (1u64 << (obs_idx % 64))) != 0;
            let mut pushed = record.clone();
            if !keep_old {
                pushed.old_value = None;
            }
            observer.records.push(pushed);
        }
    }

    /// Resolves `url` against the current document URL (absolute, root-relative or hash).
    pub fn resolve_url(&self, url: &str) -> String {
        if url.contains("://") {
            return url.to_string();
        }
        let base = self.url.clone();
        if url.starts_with('#') {
            let without_hash = base.split('#').next().unwrap_or(&base);
            return format!("{without_hash}{url}");
        }
        // #2151: WHATWG URL §4.3 — scheme-relative URLs (`//host/path`) inherit the base scheme.
        if url.starts_with("//") {
            if let Some(scheme) = base.find("://") {
                return format!("{}:{}", &base[..scheme], url);
            }
            return format!("https:{url}");
        }
        if url.starts_with('/') {
            if let Some(scheme) = base.find("://") {
                let rest = &base[scheme + 3..];
                let host = rest.split('/').next().unwrap_or(rest);
                return format!("{}://{}{}", &base[..scheme], host, url);
            }
            return url.to_string();
        }
        let (base_without_fragment, _) = base.split_once('#').unwrap_or((&base, ""));
        if url.starts_with('?') {
            return format!(
                "{}{}",
                base_without_fragment
                    .split('?')
                    .next()
                    .unwrap_or(base_without_fragment),
                url
            );
        }
        match base_without_fragment.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/{url}"),
            None => format!("{base_without_fragment}/{url}"),
        }
    }

    /// Validates a JS handle and returns the live node id.
    pub fn node_for_handle(&self, handle: u32) -> Result<NodeId, String> {
        let node_id = *self
            .nodes
            .get(handle as usize)
            .ok_or_else(|| format!("invalid DOM node handle {handle}"))?;
        if self.document.try_node(node_id).is_none() {
            return Err(format!(
                "stale DOM node handle {handle}: the node no longer exists"
            ));
        }
        Ok(node_id)
    }

    /// Pushes a console entry, dropping oldest when the cap is exceeded (issue #566).
    pub fn push_console(&mut self, entry: ConsoleEntry) {
        if self.console.len() >= MAX_CONSOLE_ENTRIES {
            self.console.pop_front();
        }
        self.console.push_back(entry);
    }

    /// Current scheduler time in milliseconds (virtual clock or real elapsed).
    pub fn now_ms(&self) -> u64 {
        match self.virtual_now_ms {
            Some(now) => now,
            None => self.started_at.elapsed().as_millis() as u64,
        }
    }

    /// Advances the virtual clock by `delta`; returns the new time.
    pub fn advance_clock(&mut self, delta: Duration) -> u64 {
        let now = self.now_ms().saturating_add(delta.as_millis() as u64);
        self.virtual_now_ms = Some(now);
        now
    }

    /// Sets the virtual clock (never backwards); returns the effective time.
    pub fn set_clock(&mut self, deadline_ms: u64) -> u64 {
        let now = self.now_ms().max(deadline_ms);
        self.virtual_now_ms = Some(now);
        now
    }

    /// Drains captured console entries.
    pub fn take_console(&mut self) -> Vec<ConsoleEntry> {
        std::mem::take(&mut self.console).into()
    }
}

// ---------------------------------------------------------------------------
// Context bridge: the host cell lives in Boa's `HostDefined` data.
// ---------------------------------------------------------------------------
