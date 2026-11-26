//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

impl PseudoClass {
    /// Specificity contribution of this pseudo-class (Selectors Level 4 §16).
    ///
    /// `:is()`, `:not()` and `:has()` contribute the specificity of their most specific
    /// argument; `:where()` contributes zero; `::backdrop` (pseudo-element) contributes
    /// `(0, 0, 1)`; every other pseudo-class contributes `(0, 1, 0)`.
    pub fn specificity(&self) -> Specificity {
        match self {
            Self::Is(selectors) | Self::Not(selectors) => selectors.max_specificity(),
            Self::Where(_) => Specificity::ZERO,
            Self::Has(relatives) => relatives
                .iter()
                .map(|relative| relative.selector.specificity())
                .max()
                .unwrap_or(Specificity::ZERO),
            Self::Backdrop => Specificity::new(0, 0, 1),
            _ => Specificity::new(0, 1, 0),
        }
    }

    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        match self {
            // In HTML, the document element is the root node's child.
            Self::Root => doc.node(node_id).parent == Some(doc.root()),
            Self::Focus => crate::state::is_focused(doc, node_id),
            Self::FocusVisible => crate::state::is_focus_visible(doc, node_id),
            Self::Hover => crate::state::is_hovered(doc, node_id),
            Self::Active => crate::state::is_active(doc, node_id),
            Self::Checked => crate::state::is_checked(doc, node_id),
            Self::Disabled => crate::state::is_disabled(doc, node_id),
            Self::Enabled => crate::state::is_enabled(doc, node_id),
            Self::ReadOnly => crate::state::is_read_only(doc, node_id),
            Self::ReadWrite => crate::state::is_read_write(doc, node_id),
            Self::Required => crate::state::is_required(doc, node_id),
            Self::PlaceholderShown => crate::state::is_placeholder_shown(doc, node_id),
            Self::Modal => crate::state::is_modal(doc, node_id),
            Self::Visited => crate::state::is_visited(doc, node_id),
            // Selectors Level 4 §3.6.1: `:link`/`:any-link` match hyperlinks, i.e. elements
            // carrying an href (SVG also accepts xlink:href).
            Self::Link => doc.node(node_id).as_element().is_some_and(|el| {
                el.attributes.iter().any(|a| {
                    a.local_name.eq_ignore_ascii_case("href")
                        || a.local_name.eq_ignore_ascii_case("xlink:href")
                })
            }),
            Self::Lang(range) => matches_language(doc, node_id, range),
            Self::Backdrop | Self::Never => false,
            Self::Is(selectors) | Self::Where(selectors) => selectors.matches(doc, node_id),
            Self::Not(selectors) => !selectors.matches(doc, node_id),
            Self::Has(relatives) => relatives
                .iter()
                .any(|relative| relative.matches(doc, node_id)),
            Self::Empty => {
                // No element children and no non-empty text children.
                let mut curr = doc.node(node_id).first_child;
                while let Some(child_id) = curr {
                    match &doc.node(child_id).kind {
                        NodeKind::Element(_) => return false,
                        NodeKind::Text(text) if !text.is_empty() => return false,
                        _ => {}
                    }
                    curr = doc.node(child_id).next_sibling;
                }
                true
            }
            Self::FirstChild => element_index_from_start(doc, node_id) == Some(1),
            Self::LastChild => element_index_from_end(doc, node_id) == Some(1),
            Self::OnlyChild => {
                element_index_from_start(doc, node_id) == Some(1)
                    && element_index_from_end(doc, node_id) == Some(1)
            }
            Self::NthChild(anb) => element_index_from_start(doc, node_id)
                .is_some_and(|index| anb.matches(index as i32)),
            Self::FirstOfType => type_index_from_start(doc, node_id) == Some(1),
            Self::LastOfType => type_index_from_end(doc, node_id) == Some(1),
            Self::OnlyOfType => {
                type_index_from_start(doc, node_id) == Some(1)
                    && type_index_from_end(doc, node_id) == Some(1)
            }
            Self::NthOfType(anb) => {
                type_index_from_start(doc, node_id).is_some_and(|index| anb.matches(index as i32))
            }
            Self::NthLastChild(anb) => {
                element_index_from_end(doc, node_id).is_some_and(|index| anb.matches(index as i32))
            }
            Self::NthLastOfType(anb) => {
                type_index_from_end(doc, node_id).is_some_and(|index| anb.matches(index as i32))
            }
        }
    }
}

/// Returns `true` when both nodes are elements of the same type (namespace plus a name that is
/// case-insensitive for HTML elements and case-sensitive for foreign SVG/MathML elements).
pub(crate) fn same_element_type(doc: &Document, a: NodeId, b: NodeId) -> bool {
    let (Some(a), Some(b)) = (doc.node(a).as_element(), doc.node(b).as_element()) else {
        return false;
    };
    a.namespace == b.namespace
        && if a.namespace == Namespace::Html {
            a.local_name.eq_ignore_ascii_case(&b.local_name)
        } else {
            a.local_name == b.local_name
        }
}

thread_local! {
    /// Active document-wide sibling-index cache (one style resolution pass).
    pub(crate) static SIBLING_CACHE: RefCell<Option<SiblingCache>> = const { RefCell::new(None) };
    /// Nesting depth of document scopes.
    pub(crate) static SIBLING_SCOPE_DEPTH: Cell<u32> = const { Cell::new(0) };
    /// Test-only: how many sibling tables were built.
    #[cfg(test)]
    pub(crate) static SIBLING_TABLES_BUILT: Cell<usize> = const { Cell::new(0) };
}

pub(crate) struct SiblingCache {
    /// Address of the document the cache applies to (avoids cross-document id collisions).
    pub(crate) document: usize,
    pub(crate) tables: HashMap<NodeId, SiblingTable>,
}

pub(crate) struct SiblingTable {
    /// For each element child: (index from start, index from end, same-type index from
    /// start, same-type index from end); all 1-based.
    pub(crate) entries: HashMap<NodeId, (usize, usize, usize, usize)>,
}

impl SiblingTable {
    pub(crate) fn build(doc: &Document, parent: NodeId) -> Self {
        #[cfg(test)]
        SIBLING_TABLES_BUILT.with(|count| count.set(count.get() + 1));
        let mut elements: Vec<(NodeId, (Namespace, String))> = Vec::new();
        let mut totals: HashMap<(Namespace, String), usize> = HashMap::new();
        for child in doc.children(parent) {
            if let Some(element) = doc.node(child).as_element() {
                let name = if element.namespace == Namespace::Html {
                    element.local_name.to_ascii_lowercase()
                } else {
                    element.local_name.clone()
                };
                let key = (element.namespace, name);
                *totals.entry(key.clone()).or_default() += 1;
                elements.push((child, key));
            }
        }
        let mut seen: HashMap<(Namespace, String), usize> = HashMap::new();
        let mut entries = HashMap::with_capacity(elements.len());
        let total = elements.len();
        for (position, (node, key)) in elements.iter().enumerate() {
            let count = seen.entry(key.clone()).or_default();
            *count += 1;
            let type_total = totals.get(key).copied().unwrap_or(0);
            entries.insert(
                *node,
                (
                    position + 1,
                    total - position,
                    *count,
                    type_total - *count + 1,
                ),
            );
        }
        Self { entries }
    }
}

/// Opens a document-wide sibling-index cache for one style resolution pass.
///
/// `:nth-child`/`:nth-of-type`/`:first-child`... used to scan the sibling chain for every
/// element; the cache builds one table per parent on first use, making them O(1) afterwards.
/// It is valid only while the DOM does not mutate (style resolution never does).
pub(crate) struct DocumentScope;

/// Opens a [`DocumentScope`]; drop it to end the scope.
pub(crate) fn document_scope() -> DocumentScope {
    let outermost = SIBLING_SCOPE_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current + 1);
        current == 0
    });
    if outermost {
        SIBLING_CACHE.with(|cache| {
            *cache.borrow_mut() = Some(SiblingCache {
                document: 0,
                tables: HashMap::new(),
            })
        });
    }
    DocumentScope
}

impl Drop for DocumentScope {
    fn drop(&mut self) {
        SIBLING_SCOPE_DEPTH.with(|depth| {
            let current = depth.get().saturating_sub(1);
            depth.set(current);
            if current == 0 {
                SIBLING_CACHE.with(|cache| *cache.borrow_mut() = None);
            }
        });
    }
}

/// Cached indices for `node`, when a document scope is active for this document.
pub(crate) fn cached_sibling_indices(
    doc: &Document,
    node: NodeId,
) -> Option<(usize, usize, usize, usize)> {
    let parent = doc.node(node).parent?;
    let document = std::ptr::from_ref(doc) as usize;
    SIBLING_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let cache = cache.as_mut()?;
        if cache.document == 0 {
            cache.document = document;
        } else if cache.document != document {
            return None;
        }
        let table = cache
            .tables
            .entry(parent)
            .or_insert_with(|| SiblingTable::build(doc, parent));
        table.entries.get(&node).copied()
    })
}

/// 1-based index of `node_id` among the element children of its parent, from the start.
pub(crate) fn element_index_from_start(doc: &Document, node_id: NodeId) -> Option<usize> {
    if let Some((index, _, _, _)) = cached_sibling_indices(doc, node_id) {
        return Some(index);
    }
    let parent_id = doc.node(node_id).parent?;
    let mut index = 0;
    let mut curr = doc.node(parent_id).first_child;
    while let Some(child_id) = curr {
        if doc.node(child_id).as_element().is_some() {
            index += 1;
            if child_id == node_id {
                return Some(index);
            }
        }
        curr = doc.node(child_id).next_sibling;
    }
    None
}

/// 1-based index of `node_id` among the element children of its parent, from the end.
pub(crate) fn element_index_from_end(doc: &Document, node_id: NodeId) -> Option<usize> {
    if let Some((_, index, _, _)) = cached_sibling_indices(doc, node_id) {
        return Some(index);
    }
    let parent_id = doc.node(node_id).parent?;
    let mut index = 0;
    let mut curr = doc.node(parent_id).last_child;
    while let Some(child_id) = curr {
        if doc.node(child_id).as_element().is_some() {
            index += 1;
            if child_id == node_id {
                return Some(index);
            }
        }
        curr = doc.node(child_id).prev_sibling;
    }
    None
}

/// 1-based index of `node_id` among its same-type element siblings, from the start.
pub(crate) fn type_index_from_start(doc: &Document, node_id: NodeId) -> Option<usize> {
    if let Some((_, _, index, _)) = cached_sibling_indices(doc, node_id) {
        return Some(index);
    }
    let parent_id = doc.node(node_id).parent?;
    let mut index = 0;
    let mut curr = doc.node(parent_id).first_child;
    while let Some(child_id) = curr {
        if doc.node(child_id).as_element().is_some() && same_element_type(doc, child_id, node_id) {
            index += 1;
            if child_id == node_id {
                return Some(index);
            }
        }
        curr = doc.node(child_id).next_sibling;
    }
    None
}

/// 1-based index of `node_id` among its same-type element siblings, from the end.
pub(crate) fn type_index_from_end(doc: &Document, node_id: NodeId) -> Option<usize> {
    if let Some((_, _, _, index)) = cached_sibling_indices(doc, node_id) {
        return Some(index);
    }
    let parent_id = doc.node(node_id).parent?;
    let mut index = 0;
    let mut curr = doc.node(parent_id).last_child;
    while let Some(child_id) = curr {
        if doc.node(child_id).as_element().is_some() && same_element_type(doc, child_id, node_id) {
            index += 1;
            if child_id == node_id {
                return Some(index);
            }
        }
        curr = doc.node(child_id).prev_sibling;
    }
    None
}

/// Specificity of a selector per W3C Selectors Level 4 §16.
/// Represented as a 3-tuple `(a, b, c)`:
/// - `a`: ID selectors (`#id`)
/// - `b`: class selectors (`.class`), attribute selectors (`[attr]`), and pseudo-classes (`:first-child`)
/// - `c`: type selectors (`div`) and pseudo-elements (`::before`)
///
/// Universal selector (`*`) and combinators (` `, `>`, `+`, `~`) contribute zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

impl Specificity {
    pub const ZERO: Self = Self { a: 0, b: 0, c: 0 };

    pub const fn new(a: u32, b: u32, c: u32) -> Self {
        Self { a, b, c }
    }
}

impl std::ops::Add for Specificity {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        // Cascade sums can chain arbitrarily deep selector lists (`:is(#a, #a, ...)`);
        // saturate instead of panicking on an u32 overflow in debug builds.
        Self {
            a: self.a.saturating_add(rhs.a),
            b: self.b.saturating_add(rhs.b),
            c: self.c.saturating_add(rhs.c),
        }
    }
}

impl std::ops::AddAssign for Specificity {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.a = self.a.saturating_add(rhs.a);
        self.b = self.b.saturating_add(rhs.b);
        self.c = self.c.saturating_add(rhs.c);
    }
}

impl std::fmt::Display for Specificity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.a, self.b, self.c)
    }
}

/// Supported CSS pseudo-elements (#1540).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PseudoElement {
    /// `::before` / `:before`
    Before,
    /// `::after` / `:after`
    After,
}

/// A compound selector without combinators (e.g. `div#main.active[data-kind]:first-child`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompoundSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
    pub pseudo_classes: Vec<PseudoClass>,
    /// Optional trailing pseudo-element (`::before` / `::after`).
    pub pseudo_element: Option<PseudoElement>,
    /// `#a#b` with different ids can never match: an element has a single `id` attribute.
    pub never_matches: bool,
    /// Number of `#id` simple selectors in the compound (specificity counts every one).
    pub id_count: u32,
}
