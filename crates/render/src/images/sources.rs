//! DOM traversal that resolves the chosen `<img>`/`<picture>` source URLs.

use megi_core::{Document, NodeId};

use super::selection::{is_supported_image_mime, select_image_candidate};

/// Resolves the selected `(url, effective_density)` for an `<img>` element, taking into
/// account any enclosing `<picture>` container's `<source media/type/srcset/sizes>` children.
pub fn resolve_element_image_source(
    document: &Document,
    img_node: NodeId,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Option<(String, f32)> {
    let img_el = document.node(img_node).as_element()?;
    let img_sizes = img_el.get_attribute("sizes");

    // Check enclosing `<picture>` for preceding matching `<source>` siblings.
    if let Some(parent_id) = document.node(img_node).parent
        && let Some(parent_el) = document.node(parent_id).as_element()
        && parent_el.local_name.eq_ignore_ascii_case("picture")
    {
        for sibling_id in document.children(parent_id) {
            if sibling_id == img_node {
                break;
            }
            let Some(source_el) = document.node(sibling_id).as_element() else {
                continue;
            };
            if !source_el.local_name.eq_ignore_ascii_case("source") {
                continue;
            }
            if let Some(media_attr) = source_el.get_attribute("media").map(str::trim)
                && !media_attr.is_empty()
            {
                let list = megi_css::media::parse_media_query_list(media_attr);
                if !megi_css::media::evaluate(&list, media) {
                    continue;
                }
            }
            if let Some(type_attr) = source_el.get_attribute("type").map(str::trim)
                && !type_attr.is_empty()
                && !is_supported_image_mime(type_attr)
            {
                continue;
            }
            let Some(source_srcset) = source_el
                .get_attribute("srcset")
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let sizes = source_el.get_attribute("sizes").or(img_sizes);
            if let Some(chosen) =
                select_image_candidate(Some(source_srcset), None, sizes, media, dpr)
            {
                return Some(chosen);
            }
        }
    }

    select_image_candidate(
        img_el.get_attribute("srcset"),
        img_el.get_attribute("src"),
        img_sizes,
        media,
        dpr,
    )
}

/// Resolves only the chosen URL string for an `<img>` element (shared with shell callers).
pub fn resolve_img_source(
    document: &Document,
    img_node: NodeId,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Option<String> {
    resolve_element_image_source(document, img_node, media, dpr).map(|(url, _)| url)
}

/// Collects `(NodeId, src, effective_density)` triples for every `<img>` using responsive
/// selection (`<picture>` / `<source>`, `srcset`, `sizes`, `dpr`).
pub fn collect_responsive_image_sources(
    document: &Document,
    media: &megi_css::MediaContext,
    dpr: f32,
) -> Vec<(NodeId, String, f32)> {
    let mut sources = Vec::new();
    let mut stack = vec![document.root()];
    while let Some(node_id) = stack.pop() {
        if sources.len() >= 1024 {
            break;
        }
        let node = document.node(node_id);
        if let Some(element) = node.as_element()
            && element.local_name.eq_ignore_ascii_case("img")
            && let Some((src, density)) =
                resolve_element_image_source(document, node_id, media, dpr)
        {
            sources.push((node_id, src, density));
        }
        // Push children reversed so the stack pops them in document order.
        let children: Vec<NodeId> = document.children(node_id).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    sources
}

/// Collects `(NodeId, src)` pairs for every `<img>` element (including `srcset`/`<picture>`),
/// in tree order, evaluated with default media context and `1.0` DPR.
pub fn collect_image_sources(document: &Document) -> Vec<(NodeId, String)> {
    collect_responsive_image_sources(document, &megi_css::MediaContext::default(), 1.0)
        .into_iter()
        .map(|(id, src, _)| (id, src))
        .collect()
}
