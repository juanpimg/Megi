//! Container Queries MVP (CSS Containment Level 3 §3–4).
//!
//! Supported surface:
//!
//! - `container-type: normal | inline-size | size` and `container-name: <custom-ident>#`
//!   (computed in [`crate::ComputedStyle`]);
//! - `@container [<name>] (<feature>: <length>) [and (<feature>: <length>)]*` with
//!   `width`/`inline-size` and `height`/`block-size` features, both `min-`/`max-` and
//!   exact forms;
//! - the **nearest ancestor** whose `container-type` allows the queried axis and whose
//!   `container-name` (if the query names one) matches; without a container the rule does
//!   not apply.
//!
//! ## Approximation and stability
//!
//! The cascade resolves container rules **before layout** using predicted content-box
//! sizes: starting at the viewport width, block-level `auto` widths inherit the parent's
//! content width and `px`/`%` widths (and min/max clamps) are applied down the ancestor
//! chain. This is exactly what the block layout would use for simple block chains, but it
//! is an approximation for flex/grid/intrinsic sizing. `container-type: size` height
//! queries only match when the ancestor chain gives a definite height; otherwise
//! `block-size`/`height` conditions are false (documented limit). Evaluation is a single
//! deterministic pass, so there is no oscillation: the queried size never depends on the
//! container's own children.
//!
//! Not supported (documented): style queries, scroll-state queries, `cqi`/`cqw`/`cqh`/...
//! units, nested `@container` at-rules (nested container **elements** are supported), and
//! exact layout-derived intrinsic sizes.

use crate::properties::{ComputedStyle, ContainerType, Length};
use dom::{Document, NodeId};
use std::collections::HashMap;

/// One parsed `@container` condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    Width,
    MinWidth,
    MaxWidth,
    Height,
    MinHeight,
    MaxHeight,
    InlineSize,
    MinInlineSize,
    MaxInlineSize,
    BlockSize,
    MinBlockSize,
    MaxBlockSize,
}

impl Feature {
    fn parse(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "width" => Self::Width,
            "min-width" => Self::MinWidth,
            "max-width" => Self::MaxWidth,
            "height" => Self::Height,
            "min-height" => Self::MinHeight,
            "max-height" => Self::MaxHeight,
            "inline-size" => Self::InlineSize,
            "min-inline-size" => Self::MinInlineSize,
            "max-inline-size" => Self::MaxInlineSize,
            "block-size" => Self::BlockSize,
            "min-block-size" => Self::MinBlockSize,
            "max-block-size" => Self::MaxBlockSize,
            _ => return None,
        })
    }

    fn is_block_axis(self) -> bool {
        matches!(
            self,
            Self::Height
                | Self::MinHeight
                | Self::MaxHeight
                | Self::BlockSize
                | Self::MinBlockSize
                | Self::MaxBlockSize
        )
    }
}

/// Parsed `@container` query: optional name plus one or more `and`-joined conditions.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerQuery {
    pub name: Option<String>,
    pub conditions: Vec<(Feature, Length)>,
}

/// Parses an `@container` prelude (component-value text).
pub fn parse_query(prelude: &str) -> Option<ContainerQuery> {
    let text = prelude.trim();
    if text.is_empty() {
        return None;
    }
    let mut rest = text;
    let mut name = None;
    if !rest.starts_with('(') {
        let end = rest
            .find(|ch: char| ch == '(' || ch.is_whitespace())
            .unwrap_or(rest.len());
        let candidate = rest[..end].trim();
        if candidate.is_empty() || !is_custom_ident(candidate) {
            return None;
        }
        name = Some(candidate.to_owned());
        rest = rest[end..].trim_start();
        if !rest.starts_with('(') {
            return None;
        }
    }
    let mut conditions = Vec::new();
    while let Some(stripped) = rest.strip_prefix('(') {
        let end = find_matching_paren(stripped)?;
        let body = &stripped[..end];
        let (feature, value) = body.split_once(':')?;
        let feature = Feature::parse(feature)?;
        let length = Length::parse(value.trim(), 16.0, 16.0)?;
        conditions.push((feature, length));
        rest = stripped[end + 1..].trim_start();
        if rest.is_empty() {
            break;
        }
        rest = rest.strip_prefix("and")?.trim_start();
        if !rest.starts_with('(') {
            return None;
        }
    }
    if conditions.is_empty() || !rest.is_empty() {
        return None;
    }
    Some(ContainerQuery { name, conditions })
}

fn find_matching_paren(text: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_custom_ident(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '-')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        && value != "none"
        && value != "and"
        && value != "or"
        && value != "not"
}

/// Query container size (content box).
#[derive(Clone, Copy, Debug, Default)]
pub struct ContainerSize {
    pub inline_size: Option<f32>,
    pub block_size: Option<f32>,
}

/// Resolves a query length to px for the given container style.
fn resolve_query_length(length: Length, _style: &ComputedStyle, parent_width: f32) -> Option<f32> {
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => Some(parent_width * pct / 100.0),
        Length::Auto => None,
    }
}

/// Evaluates a query against a container size.
pub fn evaluate(query: &ContainerQuery, size: &ContainerSize, style: &ComputedStyle) -> bool {
    query.conditions.iter().all(|(feature, length)| {
        let Some(length) = resolve_query_length(*length, style, size.inline_size.unwrap_or(0.0))
        else {
            return false;
        };
        let actual = if feature.is_block_axis() {
            size.block_size
        } else {
            size.inline_size
        };
        let Some(actual) = actual else {
            return false;
        };
        match feature {
            Feature::Width | Feature::InlineSize | Feature::BlockSize => {
                (actual - length).abs() <= 0.5
            }
            Feature::Height => (actual - length).abs() <= 0.5,
            Feature::MinWidth
            | Feature::MinInlineSize
            | Feature::MinBlockSize
            | Feature::MinHeight => actual >= length,
            Feature::MaxWidth
            | Feature::MaxInlineSize
            | Feature::MaxBlockSize
            | Feature::MaxHeight => actual <= length,
        }
    })
}

/// Memoized base-style resolver for one container-matching walk.
///
/// Recursive base-style resolution recomputes the whole ancestor chain per node;
/// `matching_size` called it for every ancestor and the size predictions repeated it per
/// chain node, which made a deep document cubic (O(depth³) when every element matches a
/// container rule). One cache per walk computes each node's base style once.
struct BaseStyles<'a> {
    doc: &'a Document,
    collection: &'a crate::StyleCollection,
    cache: HashMap<NodeId, ComputedStyle>,
}

impl<'a> BaseStyles<'a> {
    fn new(doc: &'a Document, collection: &'a crate::StyleCollection) -> Self {
        Self {
            doc,
            collection,
            cache: HashMap::new(),
        }
    }

    fn base(&mut self, node: NodeId) -> ComputedStyle {
        if let Some(style) = self.cache.get(&node) {
            return style.clone();
        }
        // Iterative: collect the ancestor chain of elements (root -> node) so we never
        // recurse per depth (issue #554).
        let mut chain = Vec::new();
        let mut current = Some(node);
        while let Some(id) = current {
            if self.doc.node(id).as_element().is_some() {
                chain.push(id);
            }
            current = self.doc.node(id).parent;
        }
        chain.reverse();
        let mut parent_style: Option<ComputedStyle> = None;
        let mut result = ComputedStyle::initial();
        for id in chain {
            if let Some(cached) = self.cache.get(&id) {
                parent_style = Some(cached.clone());
                result = cached.clone();
                continue;
            }
            let element = self.doc.node(id).as_element().unwrap();
            let inline = element
                .get_attribute("style")
                .map(|style| crate::expand_declarations(&crate::parse_list_of_declarations(style)));
            let cascaded = crate::cascade::resolve_cascaded_properties_base(
                self.doc,
                id,
                self.collection,
                inline.as_deref(),
            );
            let style = ComputedStyle::compute(&cascaded, parent_style.as_ref(), None);
            self.cache.insert(id, style.clone());
            parent_style = Some(style.clone());
            result = style;
        }
        // If node itself wasn't an element, return initial.
        if self.doc.node(node).as_element().is_none() {
            return ComputedStyle::initial();
        }
        result
    }
}

fn edges(style: &ComputedStyle, width: f32) -> (f32, f32) {
    let left = match style.padding.left {
        Length::Px(px) => px,
        Length::Percent(pct) => width * pct / 100.0,
        _ => 0.0,
    } + style.border_width.left;
    let right = match style.padding.right {
        Length::Px(px) => px,
        Length::Percent(pct) => width * pct / 100.0,
        _ => 0.0,
    } + style.border_width.right;
    (left, right)
}

fn clamp_width(style: &ComputedStyle, width: f32, parent_width: f32) -> f32 {
    let mut width = width;
    if let Some(min) = resolve_query_length(style.min_width, style, parent_width) {
        width = width.max(min);
    }
    if let Some(max) = resolve_query_length(style.max_width, style, parent_width) {
        width = width.min(max);
    }
    width
}

/// Predicts the content-box inline size of `node` from the viewport down the block chain.
fn predicted_inline_size(
    doc: &Document,
    node: NodeId,
    collection: &crate::StyleCollection,
    cache: &mut BaseStyles<'_>,
) -> Option<f32> {
    let viewport_width = collection.media.viewport.map(|(width, _)| width)?;
    let mut chain = Vec::new();
    let mut current = Some(node);
    while let Some(id) = current {
        if doc.node(id).as_element().is_some() {
            chain.push(id);
        }
        current = doc.node(id).parent;
    }
    chain.reverse();
    let mut parent_width = viewport_width;
    for id in chain {
        let style = cache.base(id);
        let border_box = match style.width {
            Length::Px(px) => px,
            Length::Percent(pct) => parent_width * pct / 100.0,
            _ => parent_width,
        };
        let (left, right) = edges(&style, border_box);
        let content = (border_box - left - right).max(0.0);
        let content = clamp_width(&style, content, parent_width);
        parent_width = content;
    }
    Some(parent_width)
}

/// Predicts the content-box block size of `node` when it is definite (explicit height, or
/// percentage of a definite ancestor height). Auto heights are content-dependent and are
/// therefore not predicted (documented limit).
fn predicted_block_size(doc: &Document, node: NodeId, cache: &mut BaseStyles<'_>) -> Option<f32> {
    // Iterative walk up the ancestor chain to find a definite height.
    let mut current = Some(node);
    let mut pct_chain: Vec<f32> = Vec::new();
    loop {
        let id = current?;
        if doc.node(id).as_element().is_none() {
            // Non-element ancestor: stop.
            if pct_chain.is_empty() {
                return None;
            }
            // Shouldn't reach here with pending percents; bail.
            return None;
        }
        let style = cache.base(id);
        match style.height {
            Length::Px(px) => {
                // Resolve accumulated percentages from this definite height.
                let mut h = px.max(0.0);
                for pct in pct_chain.iter().rev() {
                    h = (h * pct / 100.0).max(0.0);
                }
                return Some(h);
            }
            Length::Percent(pct) => {
                pct_chain.push(pct);
                current = doc.node(id).parent;
            }
            Length::Auto => {
                // Auto breaks the chain for percentage resolution.
                return None;
            }
        }
    }
}

/// Returns the size of the nearest ancestor container compatible with `query`.
pub fn matching_size(
    doc: &Document,
    node: NodeId,
    query: &ContainerQuery,
    collection: &crate::StyleCollection,
) -> Option<(ContainerSize, ComputedStyle)> {
    let needs_block = query
        .conditions
        .iter()
        .any(|(feature, _)| feature.is_block_axis());
    let mut cache = BaseStyles::new(doc, collection);
    let mut current = doc.node(node).parent;
    while let Some(id) = current {
        if doc.node(id).as_element().is_some() {
            let style = cache.base(id);
            let eligible = style.container_type.is_inline_queryable()
                && (!needs_block || style.container_type.is_block_queryable())
                && query.name.as_ref().is_none_or(|name| {
                    style
                        .container_names
                        .iter()
                        .any(|candidate| candidate == name)
                });
            if eligible && style.container_type != ContainerType::Normal {
                let inline_size = predicted_inline_size(doc, id, collection, &mut cache);
                let block_size = if style.container_type.is_block_queryable() {
                    predicted_block_size(doc, id, &mut cache)
                } else {
                    None
                };
                return Some((
                    ContainerSize {
                        inline_size,
                        block_size,
                    },
                    style,
                ));
            }
        }
        current = doc.node(id).parent;
    }
    None
}
