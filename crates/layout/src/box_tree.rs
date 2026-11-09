//! # Layout Box Tree Representation
//!
//! Defines [`LayoutBox`], [`BoxType`], and tree structural invariants according to
//! CSS 2.2 §9 and CSS Display Module Level 3.

use css::ComputedStyle;
use dom::NodeId;

use crate::geometry::Dimensions;

/// The nature of a box in the layout tree.
#[derive(Clone, Debug, PartialEq)]
pub enum BoxType {
    /// A block container element (e.g. `div`, `p`, `body`).
    BlockNode(NodeId),
    /// An inline box (e.g. `span`, `b`, `a`).
    InlineNode(NodeId),
    /// An anonymous block box generated to wrap inline children inside a block container (§9.2.1.1).
    AnonymousBlock,
    /// An inline text fragment.
    TextNode(String),
}

/// A node in the CSS layout box tree.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutBox {
    /// Geometric dimensions and box-model edge thicknesses in pixels.
    pub dimensions: Dimensions,
    /// Box classification (block, inline, anonymous, text).
    pub box_type: BoxType,
    /// Computed style associated with this box (if generated from an element).
    pub style: Option<ComputedStyle>,
    /// Child boxes in layout order.
    pub children: Vec<LayoutBox>,
}

impl LayoutBox {
    /// Creates a new layout box with zero dimensions and empty children.
    pub fn new(box_type: BoxType, style: Option<ComputedStyle>) -> Self {
        Self {
            dimensions: Dimensions::default(),
            box_type,
            style,
            children: Vec::new(),
        }
    }

    /// Returns `true` if this box is a block-level container (either an element block or anonymous).
    #[inline]
    pub fn is_block(&self) -> bool {
        matches!(
            self.box_type,
            BoxType::BlockNode(_) | BoxType::AnonymousBlock
        )
    }

    /// Returns `true` if this box is an inline box or text node.
    #[inline]
    pub fn is_inline(&self) -> bool {
        matches!(self.box_type, BoxType::InlineNode(_) | BoxType::TextNode(_))
    }

    /// Returns `true` if this box is an anonymous block box.
    #[inline]
    pub fn is_anonymous(&self) -> bool {
        matches!(self.box_type, BoxType::AnonymousBlock)
    }

    /// Returns the DOM `NodeId` if this box corresponds directly to an element.
    #[inline]
    pub fn node_id(&self) -> Option<NodeId> {
        match self.box_type {
            BoxType::BlockNode(id) | BoxType::InlineNode(id) => Some(id),
            _ => None,
        }
    }
}
