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
        crate::images::ImageRegion {
            node_id: self
                .node_ordinal
                .and_then(|ordinal| document.descendants(document.root()).nth(ordinal))
                .unwrap_or_else(|| megi_webview::core::NodeId::from_index(usize::MAX)),
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

    pub fn into_match(self, document: &megi_webview::core::Document) -> crate::find::FindMatch {
        crate::find::FindMatch {
            node_id: self
                .node_ordinal
                .and_then(|ordinal| document.descendants(document.root()).nth(ordinal))
                .unwrap_or_else(|| megi_webview::core::NodeId::from_index(usize::MAX)),
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
        Self {
            form_id: control.form_id,
            name: control.name.clone(),
            node_ordinal: document
                .descendants(document.root())
                .position(|candidate| candidate == control.node_id),
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
        let kind = parse_kind(&self.kind)?;
        Some(crate::forms::FormControl {
            form_id: self.form_id,
            name: self.name,
            node_id: self
                .node_ordinal
                .and_then(|ordinal| document.descendants(document.root()).nth(ordinal))
                .unwrap_or_else(|| megi_webview::core::NodeId::from_index(usize::MAX)),
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
