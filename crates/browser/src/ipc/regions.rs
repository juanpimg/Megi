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
