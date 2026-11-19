//! DOM node payload types and the error type returned by mutation helpers.

use std::fmt;

use crate::NodeId;

/// Document compatibility / quirks mode per WHATWG DOM §4.5 & HTML §13.2.6.4.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QuirksMode {
    #[default]
    NoQuirks,
    Quirks,
    LimitedQuirks,
}

/// Namespace of an element or attribute per WHATWG Infra / DOM Living Standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Namespace {
    #[default]
    Html,
    Svg,
    MathMl,
    XLink,
    Xml,
    Xmlns,
    None,
}

impl Namespace {
    /// Returns the canonical namespace URI string, if any.
    pub const fn uri(self) -> Option<&'static str> {
        match self {
            Self::Html => Some("http://www.w3.org/1999/xhtml"),
            Self::Svg => Some("http://www.w3.org/2000/svg"),
            Self::MathMl => Some("http://www.w3.org/1998/Math/MathML"),
            Self::XLink => Some("http://www.w3.org/1999/xlink"),
            Self::Xml => Some("http://www.w3.org/XML/1998/namespace"),
            Self::Xmlns => Some("http://www.w3.org/2000/xmlns/"),
            Self::None => None,
        }
    }
}

/// Error returned by the browser-style DOM mutation helpers
/// (`Document::remove_child`, `Document::insert_before_child`,
/// `Document::replace_all_children`, `Document::set_text_content`, ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomError {
    /// The handle does not belong to this document (out of range, stale or foreign).
    UnknownNode(NodeId),
    /// The handle does not address an [`NodeKind::Element`].
    NotAnElement(NodeId),
    /// The node is not a child of the given parent, so DOM-style removal is impossible.
    NotAChild { parent: NodeId, child: NodeId },
    /// The mutation would break the tree hierarchy (self-insertion or ancestor cycle).
    HierarchyRequest { parent: NodeId, child: NodeId },
    /// The node kind cannot contain children (`Text`, `Comment`, `Doctype`,
    /// `ProcessingInstruction`).
    CannotHaveChildren(NodeId),
    /// The node is detached, so an operation that needs its parent (e.g. `outerHTML`
    /// replacement) cannot proceed.
    DetachedNode(NodeId),
    /// The operation would exceed the maximum node count.
    NodeLimitExceeded,
}

impl fmt::Display for DomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode(node) => {
                write!(f, "node {node:?} does not belong to this document")
            }
            Self::NotAnElement(node) => write!(f, "node {node:?} is not an element"),
            Self::NotAChild { parent, child } => {
                write!(f, "node {child:?} is not a child of {parent:?}")
            }
            Self::HierarchyRequest { parent, child } => write!(
                f,
                "cannot move node {child:?} under {parent:?}: hierarchy request error"
            ),
            Self::CannotHaveChildren(node) => {
                write!(f, "node {node:?} cannot have children")
            }
            Self::DetachedNode(node) => {
                write!(f, "node {node:?} is detached and has no parent")
            }
            Self::NodeLimitExceeded => write!(f, "node limit exceeded"),
        }
    }
}

impl std::error::Error for DomError {}

/// An attribute on an [`ElementData`] node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub namespace: Namespace,
    pub prefix: Option<String>,
    pub local_name: String,
    pub value: String,
}

impl Attribute {
    /// Creates a standard attribute with no namespace or prefix.
    pub fn new(local_name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            namespace: Namespace::None,
            prefix: None,
            local_name: local_name.into(),
            value: value.into(),
        }
    }

    /// Creates a namespaced attribute (e.g., `xlink:href`, `xml:lang`, `xmlns:xlink`).
    pub fn with_namespace(
        namespace: Namespace,
        prefix: Option<String>,
        local_name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            namespace,
            prefix,
            local_name: local_name.into(),
            value: value.into(),
        }
    }
}

/// Payload specific to `NodeKind::Element`.
pub const MAX_ATTRIBUTES_PER_ELEMENT: usize = 256;
pub const MAX_ATTRIBUTE_VALUE_BYTES: usize = 64 * 1024;

/// Payload specific to `NodeKind::Element`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementData {
    pub namespace: Namespace,
    pub prefix: Option<String>,
    pub local_name: String,
    pub attributes: Vec<Attribute>,
    /// For `<template>` elements in the HTML namespace, holds the [`NodeId`] of its
    /// inert `DocumentFragment` allocated within the same `Document` arena.
    pub template_contents: Option<NodeId>,
}

impl ElementData {
    pub fn new(namespace: Namespace, local_name: impl Into<String>) -> Self {
        Self {
            namespace,
            prefix: None,
            local_name: local_name.into(),
            attributes: Vec::new(),
            template_contents: None,
        }
    }

    /// Returns the DOM `tagName`: uppercased local name for HTML elements, unchanged for
    /// foreign (SVG/MathML) elements, matching WHATWG DOM §4.4 "element tag name".
    pub fn tag_name(&self) -> String {
        if self.namespace == Namespace::Html {
            self.local_name.to_ascii_uppercase()
        } else {
            self.local_name.clone()
        }
    }

    /// Returns `true` when an attribute with `name` is present.
    ///
    /// HTML elements compare names ASCII case-insensitively (DOM behaviour); foreign
    /// elements compare exactly.
    pub fn has_attribute(&self, name: &str) -> bool {
        self.attribute_position(name).is_some()
    }

    /// Removes the attribute named `name`, returning its previous value (DOM
    /// `removeAttribute` semantics). HTML elements compare names ASCII
    /// case-insensitively; foreign elements compare exactly.
    pub fn remove_attribute(&mut self, name: &str) -> Option<String> {
        let position = self.attribute_position(name)?;
        Some(self.attributes.remove(position).value)
    }

    /// Returns the names of all attributes, in insertion order.
    pub fn attribute_names(&self) -> Vec<String> {
        self.attributes
            .iter()
            .map(|attribute| attribute.local_name.clone())
            .collect()
    }

    /// Compares an attribute name against `name` the way the DOM does (#686).
    ///
    /// HTML elements treat attribute names ASCII case-insensitively for every operation
    /// (get/set/has/remove); foreign elements compare exactly.
    fn attribute_name_matches(&self, attribute_name: &str, name: &str) -> bool {
        if self.namespace == Namespace::Html {
            attribute_name.eq_ignore_ascii_case(name)
        } else {
            attribute_name == name
        }
    }

    /// Finds the position of the attribute called `name` (see [`ElementData::has_attribute`]).
    fn attribute_position(&self, name: &str) -> Option<usize> {
        self.attributes.iter().position(|attribute| {
            attribute.namespace == Namespace::None
                && self.attribute_name_matches(&attribute.local_name, name)
        })
    }

    /// Looks up an attribute value by local name (in `Namespace::None`).
    ///
    /// HTML elements match the name ASCII case-insensitively, like `has_attribute`,
    /// `remove_attribute` and `set_attribute` (#686).
    pub fn get_attribute(&self, local_name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| {
                a.namespace == Namespace::None
                    && self.attribute_name_matches(&a.local_name, local_name)
            })
            .map(|a| a.value.as_str())
    }

    /// Sets an attribute if not already present (per HTML parser duplicate-attribute rules)
    /// or overwrites if `overwrite` is true.
    ///
    /// The "already present" test uses the same case rules as lookup, so `set_attribute("ID")`
    /// overwrites an existing `id` instead of introducing a duplicate (#686).
    pub fn set_attribute(&mut self, attr: Attribute, overwrite: bool) -> bool {
        let position = self.attributes.iter().position(|existing| {
            existing.namespace == attr.namespace
                && if attr.namespace == Namespace::None {
                    self.attribute_name_matches(&existing.local_name, &attr.local_name)
                } else {
                    existing.local_name == attr.local_name
                }
        });
        match position {
            Some(position) => {
                if overwrite {
                    // Apply the same cap as the insert path (issue #1106): scripted
                    // setters use overwrite=true and must not bypass the anti-DoS bound.
                    if attr.value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
                        return false;
                    }
                    self.attributes[position].value = attr.value;
                }
                false
            }
            None => {
                if self.attributes.len() >= MAX_ATTRIBUTES_PER_ELEMENT {
                    return false;
                }
                if attr.value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
                    return false;
                }
                self.attributes.push(attr);
                true
            }
        }
    }
}

/// Discriminated union of DOM node payloads per WHATWG DOM §4.4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    DocumentFragment,
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element(ElementData),
    Text(String),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
}

/// A single DOM node stored contiguously inside `Document`.
///
/// The 5 tree links (`parent`, `first_child`, `last_child`, `prev_sibling`, `next_sibling`)
/// occupy exactly 20 bytes (`5 * 4` bytes) and allow `O(1)` splices without borrowing
/// conflicts. Each node also carries its 4-byte arena slot generation, used to reject
/// stale [`NodeId`] handles after a slot is freed and reused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Handle of this node, including the arena-seeded generation of its slot.
    pub id: NodeId,
    /// Per-slot local generation (without the arena seed). Incremented when the slot is
    /// freed so stale handles can be rejected; see [`NodeId`].
    pub generation: u32,
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub prev_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub kind: NodeKind,
}

impl Node {
    /// Returns a reference to [`ElementData`] if this node is an `Element`.
    #[inline]
    pub fn as_element(&self) -> Option<&ElementData> {
        match &self.kind {
            NodeKind::Element(el) => Some(el),
            _ => None,
        }
    }

    /// Returns a reference to the text string if this node is a `Text` node.
    #[inline]
    pub fn as_text(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns a mutable reference to [`ElementData`] if this node is an `Element`.
    #[inline]
    pub fn as_element_mut(&mut self) -> Option<&mut ElementData> {
        match &mut self.kind {
            NodeKind::Element(el) => Some(el),
            _ => None,
        }
    }
}
