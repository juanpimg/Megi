/// Precomputed `NodeId -> document-order ordinal` index built in one O(N) pass.
#[derive(Debug, Clone)]
pub struct DocumentNodeOrdinals {
    ordinals: std::collections::HashMap<megi_webview::core::NodeId, usize>,
}

impl DocumentNodeOrdinals {
    /// Indexes every node reachable from `document.root()` in `descendants` order.
    pub fn build(document: &megi_webview::core::Document) -> Self {
        let ordinals = document
            .descendants(document.root())
            .enumerate()
            .map(|(ordinal, node_id)| (node_id, ordinal))
            .collect();
        Self { ordinals }
    }

    /// Returns the document-order ordinal for `node_id` in O(1) expected time.
    pub fn ordinal_of(&self, node_id: megi_webview::core::NodeId) -> Option<usize> {
        self.ordinals.get(&node_id).copied()
    }
}

/// Precomputed `ordinal -> NodeId` table built in one O(N) pass over `document`.
#[derive(Debug, Clone)]
pub struct DocumentOrdinalNodes {
    nodes: Vec<megi_webview::core::NodeId>,
}

impl DocumentOrdinalNodes {
    /// Collects every node reachable from `document.root()` in `descendants` order.
    pub fn build(document: &megi_webview::core::Document) -> Self {
        Self {
            nodes: document.descendants(document.root()).collect(),
        }
    }

    /// Resolves a document-order `ordinal` to its `NodeId` in O(1) time.
    pub fn node_at(&self, ordinal: usize) -> Option<megi_webview::core::NodeId> {
        self.nodes.get(ordinal).copied()
    }
}

fn sentinel_node_id() -> megi_webview::core::NodeId {
    megi_webview::core::NodeId::from_index(usize::MAX)
}

/// Serializable hit region for a link, with a DOM ordinal instead of a raw `NodeId`
/// (which is document-arena-specific and cannot cross the process boundary).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct IpcLinkRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub href: String,
    pub node_ordinal: Option<usize>,
    pub is_blocker: bool,
}

impl IpcLinkRegion {
    /// Captures a child-side region, translating its node to a stable document-order ordinal.
    pub fn from_region(
        region: &crate::links::LinkRegion,
        document: &megi_webview::core::Document,
    ) -> Self {
        let node_ordinal = region.node.and_then(|node| {
            document
                .descendants(document.root())
                .position(|candidate| candidate == node)
        });
        Self::with_ordinal(region, node_ordinal)
    }

    /// Captures a child-side region in O(1) using a precomputed [`DocumentNodeOrdinals`] index.
    pub fn from_region_indexed(
        region: &crate::links::LinkRegion,
        ordinals: &DocumentNodeOrdinals,
    ) -> Self {
        let node_ordinal = region.node.and_then(|node| ordinals.ordinal_of(node));
        Self::with_ordinal(region, node_ordinal)
    }

    fn with_ordinal(region: &crate::links::LinkRegion, node_ordinal: Option<usize>) -> Self {
        Self {
            x: region.rect.x,
            y: region.rect.y,
            width: region.rect.width,
            height: region.rect.height,
            href: region.href.clone(),
            node_ordinal,
            is_blocker: region.is_blocker,
        }
    }

    /// Reconstructs a region without a node (callers with a restored document should use
    /// [`Self::into_link_region_with_document`]).
    pub fn into_link_region(self) -> crate::links::LinkRegion {
        crate::links::LinkRegion {
            rect: crate::links::CssRect::new(self.x, self.y, self.width, self.height),
            href: self.href,
            node: None,
            is_blocker: self.is_blocker,
        }
    }

    /// Reconstructs a region and resolves its ordinal against the parent's restored document.
    pub fn into_link_region_with_document(
        self,
        document: &megi_webview::core::Document,
    ) -> crate::links::LinkRegion {
        let ordinal = self.node_ordinal;
        let mut region = self.into_link_region();
        region.node =
            ordinal.and_then(|ordinal| document.descendants(document.root()).nth(ordinal));
        region
    }

    /// Reconstructs a region in O(1) using a precomputed [`DocumentOrdinalNodes`] table.
    pub fn into_link_region_indexed(
        self,
        nodes: &DocumentOrdinalNodes,
    ) -> crate::links::LinkRegion {
        let ordinal = self.node_ordinal;
        let mut region = self.into_link_region();
        region.node = ordinal.and_then(|ordinal| nodes.node_at(ordinal));
        region
    }
}

/// Serializable image hit region with a DOM ordinal (ADR-004 hit-region parity).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct IpcImageRegion {
    pub node_ordinal: Option<usize>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub url: String,
}

impl IpcImageRegion {
    pub fn from_region(
        region: &crate::images::ImageRegion,
        document: &megi_webview::core::Document,
    ) -> Self {
        let node_ordinal = document
            .descendants(document.root())
            .position(|candidate| candidate == region.node_id);
        Self::with_ordinal(region, node_ordinal)
    }

    /// Captures an image region in O(1) using a precomputed [`DocumentNodeOrdinals`] index.
    pub fn from_region_indexed(
        region: &crate::images::ImageRegion,
        ordinals: &DocumentNodeOrdinals,
    ) -> Self {
        Self::with_ordinal(region, ordinals.ordinal_of(region.node_id))
    }

    fn with_ordinal(region: &crate::images::ImageRegion, node_ordinal: Option<usize>) -> Self {
        Self {
            node_ordinal,
            x: region.rect.x,
            y: region.rect.y,
            width: region.rect.width,
            height: region.rect.height,
            url: region.url.clone(),
        }
    }

    pub fn into_image_region(
        self,
        document: &megi_webview::core::Document,
    ) -> crate::images::ImageRegion {
        let node_id = self
            .node_ordinal
            .and_then(|ordinal| document.descendants(document.root()).nth(ordinal))
            .unwrap_or_else(sentinel_node_id);
        self.with_node_id(node_id)
    }

    /// Reconstructs an image region in O(1) using a precomputed [`DocumentOrdinalNodes`] table.
    pub fn into_image_region_indexed(
        self,
        nodes: &DocumentOrdinalNodes,
    ) -> crate::images::ImageRegion {
        let node_id = self
            .node_ordinal
            .and_then(|ordinal| nodes.node_at(ordinal))
            .unwrap_or_else(sentinel_node_id);
        self.with_node_id(node_id)
    }

    fn with_node_id(self, node_id: megi_webview::core::NodeId) -> crate::images::ImageRegion {
        crate::images::ImageRegion {
            node_id,
            rect: crate::links::CssRect::new(self.x, self.y, self.width, self.height),
            url: self.url,
        }
    }
}

/// Serializable find highlight rectangle.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct IpcRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl IpcRect {
    pub fn from_rect(rect: crate::links::CssRect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }

    pub fn into_rect(self) -> crate::links::CssRect {
        crate::links::CssRect::new(self.x, self.y, self.width, self.height)
    }
}

/// Serializable find match (text node ordinal + byte range).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct IpcFindMatch {
    pub node_ordinal: Option<usize>,
    pub start: usize,
    pub end: usize,
}

impl IpcFindMatch {
    pub fn from_match(
        match_: &crate::find::FindMatch,
        document: &megi_webview::core::Document,
    ) -> Self {
        Self {
            node_ordinal: document
                .descendants(document.root())
                .position(|candidate| candidate == match_.node_id),
            start: match_.range.start,
            end: match_.range.end,
        }
    }

    /// Captures a find match in O(1) using a precomputed [`DocumentNodeOrdinals`] index.
    pub fn from_match_indexed(
        match_: &crate::find::FindMatch,
        ordinals: &DocumentNodeOrdinals,
    ) -> Self {
        Self {
            node_ordinal: ordinals.ordinal_of(match_.node_id),
            start: match_.range.start,
            end: match_.range.end,
        }
    }

    pub fn into_match(self, document: &megi_webview::core::Document) -> crate::find::FindMatch {
        crate::find::FindMatch {
            node_id: self
                .node_ordinal
                .and_then(|ordinal| document.descendants(document.root()).nth(ordinal))
                .unwrap_or_else(sentinel_node_id),
            range: self.start..self.end,
        }
    }

    /// Reconstructs a find match in O(1) using a precomputed [`DocumentOrdinalNodes`] table.
    pub fn into_match_indexed(self, nodes: &DocumentOrdinalNodes) -> crate::find::FindMatch {
        crate::find::FindMatch {
            node_id: self
                .node_ordinal
                .and_then(|ordinal| nodes.node_at(ordinal))
                .unwrap_or_else(sentinel_node_id),
            range: self.start..self.end,
        }
    }
}

/// Serializable `<select>` option.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct IpcSelectOption {
    pub label: String,
    pub value: String,
    pub selected: bool,
    pub group: Option<String>,
}

/// Serializable form control with a DOM ordinal (ADR-004 form parity).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct IpcFormControl {
    pub form_id: u32,
    pub name: String,
    pub node_ordinal: Option<usize>,
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub value: String,
    pub checked: bool,
    pub disabled: bool,
    pub options: Vec<IpcSelectOption>,
    pub multiple: bool,
}

impl IpcFormControl {
    pub fn from_control(
        control: &crate::forms::FormControl,
        document: &megi_webview::core::Document,
    ) -> Self {
        let node_ordinal = document
            .descendants(document.root())
            .position(|candidate| candidate == control.node_id);
        Self::with_ordinal(control, node_ordinal)
    }

    /// Captures a form control in O(1) using a precomputed [`DocumentNodeOrdinals`] index.
    pub fn from_control_indexed(
        control: &crate::forms::FormControl,
        ordinals: &DocumentNodeOrdinals,
    ) -> Self {
        Self::with_ordinal(control, ordinals.ordinal_of(control.node_id))
    }

    fn with_ordinal(control: &crate::forms::FormControl, node_ordinal: Option<usize>) -> Self {
        Self {
            form_id: control.form_id,
            name: control.name.clone(),
            node_ordinal,
            kind: kind_str(control.kind).to_owned(),
            x: control.rect.x,
            y: control.rect.y,
            width: control.rect.width,
            height: control.rect.height,
            value: control.value.clone(),
            checked: control.checked,
            disabled: control.disabled,
            options: control
                .options
                .iter()
                .map(|option| IpcSelectOption {
                    label: option.label.clone(),
                    value: option.value.clone(),
                    selected: option.selected,
                    group: option.group.clone(),
                })
                .collect(),
            multiple: control.multiple,
        }
    }

    pub fn into_control(
        self,
        document: &megi_webview::core::Document,
    ) -> Option<crate::forms::FormControl> {
        let node_id = self
            .node_ordinal
            .and_then(|ordinal| document.descendants(document.root()).nth(ordinal))
            .unwrap_or_else(sentinel_node_id);
        self.with_node_id(node_id)
    }

    /// Reconstructs a form control in O(1) using a precomputed [`DocumentOrdinalNodes`] table.
    pub fn into_control_indexed(
        self,
        nodes: &DocumentOrdinalNodes,
    ) -> Option<crate::forms::FormControl> {
        let node_id = self
            .node_ordinal
            .and_then(|ordinal| nodes.node_at(ordinal))
            .unwrap_or_else(sentinel_node_id);
        self.with_node_id(node_id)
    }

    fn with_node_id(
        self,
        node_id: megi_webview::core::NodeId,
    ) -> Option<crate::forms::FormControl> {
        let kind = parse_kind(&self.kind)?;
        Some(crate::forms::FormControl {
            form_id: self.form_id,
            name: self.name,
            node_id,
            kind,
            rect: crate::links::CssRect::new(self.x, self.y, self.width, self.height),
            value: self.value,
            checked: self.checked,
            disabled: self.disabled,
            options: self
                .options
                .into_iter()
                .map(|option| crate::forms::SelectOption {
                    label: option.label,
                    value: option.value,
                    selected: option.selected,
                    group: option.group,
                })
                .collect(),
            multiple: self.multiple,
        })
    }
}

/// Wire name of a control kind (stable across processes).
fn kind_str(kind: crate::forms::FormControlKind) -> &'static str {
    use crate::forms::FormControlKind as K;
    match kind {
        K::TextInput => "text",
        K::Checkbox => "checkbox",
        K::Radio => "radio",
        K::Submit => "submit",
        K::Reset => "reset",
        K::Button => "button",
        K::Textarea => "textarea",
        K::Select => "select",
    }
}

fn parse_kind(name: &str) -> Option<crate::forms::FormControlKind> {
    use crate::forms::FormControlKind as K;
    Some(match name {
        "text" => K::TextInput,
        "checkbox" => K::Checkbox,
        "radio" => K::Radio,
        "submit" => K::Submit,
        "reset" => K::Reset,
        "button" => K::Button,
        "textarea" => K::Textarea,
        "select" => K::Select,
        _ => return None,
    })
}
