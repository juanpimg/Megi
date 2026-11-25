//! Collection of `background-image`/`list-style-image` references from computed styles.

use std::collections::HashMap;

use megi_core::{Document, NodeId};
use megi_css::ComputedStyle;

use super::types::{BackgroundKind, BackgroundReference};

/// Collects `background-image`/`list-style-image` references in document order.
///
/// Only elements present in `styles` (i.e. styled) are considered. Elements that are not
/// in the live document tree are skipped (styles are keyed by arena handle).
pub fn collect_background_references(
    document: &Document,
    styles: &HashMap<NodeId, ComputedStyle>,
) -> Vec<BackgroundReference> {
    let mut references = Vec::new();
    for node_id in document.descendants(document.root()) {
        // Text nodes never carry computed styles: skip them before the hash lookup.
        if document.node(node_id).as_element().is_none() {
            continue;
        }
        let Some(style) = styles.get(&node_id) else {
            continue;
        };
        push_style_background_references(node_id, style, false, &mut references);
    }
    references
}

pub(super) fn push_style_background_references(
    node_id: NodeId,
    style: &ComputedStyle,
    include_content_url: bool,
    references: &mut Vec<BackgroundReference>,
) {
    let mut mirrored_first_layer = false;
    for (index, layer) in style.background_layers.iter().enumerate() {
        let Some(raw_url) = layer.image.as_deref() else {
            continue;
        };
        if !mirrored_first_layer && style.background_image.as_deref() == Some(raw_url) && index == 0
        {
            mirrored_first_layer = true;
        }
        references.push(BackgroundReference {
            node: node_id,
            raw_url: raw_url.to_owned(),
            kind: BackgroundKind::Background,
        });
    }
    if !mirrored_first_layer && let Some(raw_url) = style.background_image.as_deref() {
        references.push(BackgroundReference {
            node: node_id,
            raw_url: raw_url.to_owned(),
            kind: BackgroundKind::Background,
        });
    }
    if let Some(raw_url) = style.list_style_image.as_deref() {
        references.push(BackgroundReference {
            node: node_id,
            raw_url: raw_url.to_owned(),
            kind: BackgroundKind::ListStyleImage,
        });
    }
    // CSS Masking 1 §6.1: the mask image is decoded through the same background
    // image cache; the painter looks it up by the same raw URL string.
    if let Some(raw_url) = style
        .background_layers
        .first()
        .and_then(|layer| layer.mask_image.as_deref())
    {
        references.push(BackgroundReference {
            node: node_id,
            raw_url: raw_url.to_owned(),
            kind: BackgroundKind::MaskImage,
        });
    }
    if include_content_url && let Some(raw_url) = style.content.image_url() {
        references.push(BackgroundReference {
            node: node_id,
            raw_url: raw_url.to_owned(),
            kind: BackgroundKind::Background,
        });
    }
}
