//! # W3C Selectors Level 4 AST & DOM Matching Engine
//!
//! Provides compound and complex selector representation, token stream parsing,
//! and high-performance matching over [`dom::Document`] and [`dom::NodeId`].
//!
//! Supported: type/universal selectors (case-insensitive for HTML elements, case-sensitive
//! for foreign SVG/MathML elements), `#id`, `.class`, attribute selectors (`=`, `~=`, `|=`,
//! `^=`, `$=`, `*=` plus the `i` flag), descendant/child/next-sibling/subsequent-sibling
//! combinators, and the structural pseudo-classes `:first-child`, `:last-child`,
//! `:only-child`, `:empty`, `:root` and `:nth-child(An+B)`.
//!
//! Also supported: the functional `:is()`, `:where()`, `:not()` and `:has()` pseudo-classes
//! (with Selectors Level 4 §16 specificity: the maximum argument for `:is()`/`:not()`/`:has()`
//! and zero for `:where()`) and the structural `:first/last/only-of-type`,
//! `:nth-of-type()`, `:nth-last-child()` and `:nth-last-of-type()`. `@media` is handled by the
//! cascade with a basic [`crate::MediaContext`].
//!
//! Still pending (documented, out of scope for this milestone): `:lang()`, `:nth-col()`,
//! namespace prefixes, pseudo-element boxes and full relative-selector anchoring for `:has()`
//! (`:scope`). Dynamic pseudo-classes without tracked interaction state match nothing, and
//! unknown/unsupported pseudo-classes invalidate the whole selector list instead of silently
//! over-matching (audit findings F13/F14/F15/F16).

use dom::{Document, Namespace, NodeId, NodeKind};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// Maximum selector-match steps for one element resolution (cascade) or one top-level
/// match (direct API use).
pub const MAX_MATCH_STEPS: u32 = 200_000;

/// Maximum selector-match steps for one whole style pass over a document.
///
/// The per-element budget above is reset for every element, so a stylesheet with a
/// pathological selector grows the total work with the number of elements: 100 KB of CSS with
/// many candidate rules against 100 KB of HTML used to reach ~3.2x10^8 map operations (#1413).
/// A pass shares one budget, so the cost of resolving a document stays bounded no matter how
/// many elements it has.
pub const MAX_PASS_MATCH_STEPS: u32 = 50_000_000;

/// Maximum nesting depth of functional pseudo-class arguments while parsing: `:is(:is(…))`.
///
/// The selector grammar is recursive (`:is()`/`:where()`/`:not()` arguments are selector
/// lists), so a stylesheet of `:is(` repeated tens of thousands of times used to overflow the
/// host stack before any declaration was validated (#376).
pub const MAX_SELECTOR_NESTING: usize = 32;

thread_local! {
    static MATCH_STEPS: Cell<u32> = const { Cell::new(0) };
    static MATCH_DEPTH: Cell<u32> = const { Cell::new(0) };
    static MATCH_PASS_DEPTH: Cell<u32> = const { Cell::new(0) };
    static MATCH_EXHAUSTED: Cell<bool> = const { Cell::new(false) };
    static PARSER_NESTING: Cell<usize> = const { Cell::new(0) };
}

/// Tracks the [`MAX_SELECTOR_NESTING`] recursion depth of [`parse_selectors`].
struct ParserNestingGuard;

impl ParserNestingGuard {
    /// Returns `None` once the nesting limit is reached, so the caller can reject the
    /// selector instead of recursing further.
    fn enter() -> Option<Self> {
        PARSER_NESTING.with(|depth| {
            if depth.get() >= MAX_SELECTOR_NESTING {
                None
            } else {
                depth.set(depth.get() + 1);
                Some(ParserNestingGuard)
            }
        })
    }
}

impl Drop for ParserNestingGuard {
    fn drop(&mut self) {
        PARSER_NESTING.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

struct MatchDepthGuard;

impl Drop for MatchDepthGuard {
    fn drop(&mut self) {
        MATCH_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

fn with_match_budget(f: impl FnOnce() -> bool) -> bool {
    let outermost = MATCH_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current + 1);
        current == 0
    });
    let _guard = MatchDepthGuard;
    if outermost {
        MATCH_STEPS.with(|steps| steps.set(MAX_MATCH_STEPS));
        MATCH_EXHAUSTED.with(|flag| flag.set(false));
    }
    let result = f();
    if outermost && MATCH_EXHAUSTED.with(|flag| flag.get()) {
        return false;
    }
    result
}

pub(crate) struct MatchScope {
    _guard: MatchDepthGuard,
}

pub(crate) fn match_scope() -> MatchScope {
    MATCH_DEPTH.with(|depth| depth.set(depth.get() + 1));
    // Only reset the step budget outside a style pass: a per-element reset is exactly the
    // quadratic blow-up #1413 describes.
    if MATCH_PASS_DEPTH.with(|depth| depth.get()) == 0 {
        MATCH_STEPS.with(|steps| steps.set(MAX_MATCH_STEPS));
        MATCH_EXHAUSTED.with(|flag| flag.set(false));
    }
    MatchScope {
        _guard: MatchDepthGuard,
    }
}

/// Scope for resolving the styles of a whole document: every element resolution inside it
/// shares one [`MAX_PASS_MATCH_STEPS`] budget instead of resetting it per element.
///
/// Nesting is allowed and only the outermost scope installs the budget, so wrapping an
/// already-scoped resolution (the renderer styles a document in several passes) is harmless.
pub struct StylePassScope {
    _guard: MatchPassGuard,
}

struct MatchPassGuard;

impl Drop for MatchPassGuard {
    fn drop(&mut self) {
        MATCH_PASS_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Returns a nested style-pass scope; see [`StylePassScope`].
#[must_use]
pub fn style_pass_scope() -> StylePassScope {
    let outermost = MATCH_PASS_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current + 1);
        current == 0
    });
    if outermost {
        MATCH_STEPS.with(|steps| steps.set(MAX_PASS_MATCH_STEPS));
        MATCH_EXHAUSTED.with(|flag| flag.set(false));
    }
    StylePassScope {
        _guard: MatchPassGuard,
    }
}

fn spend_match_step() -> bool {
    MATCH_STEPS.with(|steps| {
        let remaining = steps.get();
        if remaining == 0 {
            MATCH_EXHAUSTED.with(|flag| flag.set(true));
            false
        } else {
            steps.set(remaining - 1);
            true
        }
    })
}

/// Operator for attribute selectors (`[attr=value]`, `[attr~=value]`, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrOperator {
    /// Exact equality `[attr=val]`
    Exact,
    /// Whitespace-separated word list `[attr~=val]`
    Includes,
    /// Dash-separated match `[attr|=val]` (`val` or `val-*`)
    DashMatch,
    /// Prefix match `[attr^=val]`
    Prefix,
    /// Suffix match `[attr$=val]`
    Suffix,
    /// Substring match `[attr*=val]`
    Substring,
}

/// An attribute selector requirement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeSelector {
    pub name: String,
    pub operator: Option<AttrOperator>,
    pub value: Option<String>,
    pub case_insensitive: bool,
}

impl AttributeSelector {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, node_id))
    }

    fn matches_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        let Some(el) = doc.node(node_id).as_element() else {
            return false;
        };
        let attr_val = el
            .attributes
            .iter()
            .find(|a| a.local_name.eq_ignore_ascii_case(&self.name))
            .map(|a| a.value.as_str());

        let Some(val) = attr_val else {
            return false;
        };

        let Some(op) = self.operator else {
            return true; // Simple existence check `[attr]`
        };

        let expected = self.value.as_deref().unwrap_or_default();
        let eq = |a: &str, b: &str| {
            if self.case_insensitive {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        };

        match op {
            AttrOperator::Exact => eq(val, expected),
            AttrOperator::Includes => val.split_ascii_whitespace().any(|word| eq(word, expected)),
            AttrOperator::DashMatch => {
                eq(val, expected)
                    || (val.len() > expected.len()
                        && val.as_bytes().get(expected.len()) == Some(&b'-')
                        && eq(&val[..expected.len()], expected))
            }
            AttrOperator::Prefix => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .starts_with(&expected.to_ascii_lowercase())
                } else {
                    val.starts_with(expected)
                }
            }
            AttrOperator::Suffix => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .ends_with(&expected.to_ascii_lowercase())
                } else {
                    val.ends_with(expected)
                }
            }
            AttrOperator::Substring => {
                if self.case_insensitive {
                    val.to_ascii_lowercase()
                        .contains(&expected.to_ascii_lowercase())
                } else {
                    val.contains(expected)
                }
            }
        }
    }
}

/// Formula $An+B$ for `:nth-child(An+B)` pseudo-classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnPlusB {
    pub a: i32,
    pub b: i32,
}

impl AnPlusB {
    pub fn matches(&self, index_1_based: i32) -> bool {
        if self.a == 0 {
            return index_1_based == self.b;
        }
        // Widen to i64 so neither the subtraction nor the division can overflow or panic
        // (issue #674: `:nth-child(-1n-2147483648)` previously panicked in debug).
        let diff = index_1_based as i64 - self.b as i64;
        let a = self.a as i64;
        diff % a == 0 && diff / a >= 0
    }
}

/// Parses an An+B formula according to W3C CSS Syntax Level 3 §3.3 & Selectors Level 4 §15.
pub fn parse_an_plus_b(s: &str) -> Option<AnPlusB> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.eq_ignore_ascii_case("odd") {
        return Some(AnPlusB { a: 2, b: 1 });
    }
    if s.eq_ignore_ascii_case("even") {
        return Some(AnPlusB { a: 2, b: 0 });
    }

    // Integer only `B`
    if let Ok(b) = s.parse::<i32>() {
        return Some(AnPlusB { a: 0, b });
    }

    // Must contain 'n' or 'N'
    let n_idx = s.find(['n', 'N'])?;
    let (prefix, suffix) = (&s[..n_idx], &s[n_idx + 1..]);

    let a = match prefix.trim() {
        "" | "+" => 1,
        "-" => -1,
        num_str => num_str.parse::<i32>().ok()?,
    };

    let suffix_trimmed = suffix.replace(' ', "");
    let b = if suffix_trimmed.is_empty() {
        0
    } else {
        suffix_trimmed.parse::<i32>().ok()?
    };

    Some(AnPlusB { a, b })
}

/// Supported pseudo-classes in Selectors Level 4.
///
/// Structural pseudo-classes are evaluated against the DOM tree; the functional
/// `:is()`, `:where()`, `:not()` and `:has()` pseudo-classes recurse into nested selector
/// lists. Pseudo-classes that depend on interaction or document state that this engine does
/// not track yet (`:hover`, `:active`, `:focus`, `:visited`, `:link`, ...) and pseudo-elements
/// (`::before`, `::after`, ...) are parsed as [`PseudoClass::Never`]: the selector stays
/// valid but can never match, instead of over-matching every element.
///
/// Not implemented yet (selectors using them are currently rejected as invalid): `:lang()`,
/// `:nth-col()`, attribute namespaces and the `s`/`i` case-sensitivity flags of `@namespace`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    OnlyChild,
    Empty,
    Root,
    NthChild(AnPlusB),
    FirstOfType,
    LastOfType,
    OnlyOfType,
    NthOfType(AnPlusB),
    NthLastChild(AnPlusB),
    NthLastOfType(AnPlusB),
    /// `:is(<selector-list>)`.
    Is(SelectorList),
    /// `:where(<selector-list>)`; matches like `:is()` but contributes zero specificity.
    Where(SelectorList),
    /// `:not(<selector-list>)`.
    Not(SelectorList),
    /// `:has(<relative-selector-list>)`; basic descendant/child/sibling support.
    Has(Vec<RelativeSelector>),
    /// `:focus` — host state or `data-megi-focused`.
    Focus,
    /// `:focus-visible` — focus-visible state or `data-megi-focus-visible`, falling back to `:focus`.
    FocusVisible,
    /// `:hover` — host state or `data-megi-hover` (no pointer in headless rendering).
    Hover,
    /// `:active` — host state or `data-megi-active` (no pointer in headless rendering).
    Active,
    /// `:checked` — `checked` attribute on form controls or `data-megi-checked`.
    Checked,
    /// `:disabled` — disabled form control (`disabled`/`data-megi-disabled`).
    Disabled,
    /// `:enabled` — form control that is not disabled.
    Enabled,
    /// `:read-only` — non-editable element or read-only form control.
    ReadOnly,
    /// `:read-write` — editable, non-read-only form control.
    ReadWrite,
    /// `:required` — form control with a `required` attribute.
    Required,
    /// `:placeholder-shown` — editable control with a placeholder and an empty value.
    PlaceholderShown,
    /// `:modal` — `<dialog open>` opened modally into the top layer (`data-megi-modal`).
    Modal,
    /// `::backdrop` — pseudo-element behind a top-layer modal `<dialog>`.
    Backdrop,
    /// Known pseudo-class/pseudo-element whose state this engine cannot observe.
    Never,
}

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
fn same_element_type(doc: &Document, a: NodeId, b: NodeId) -> bool {
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
    static SIBLING_CACHE: RefCell<Option<SiblingCache>> = const { RefCell::new(None) };
    /// Nesting depth of document scopes.
    static SIBLING_SCOPE_DEPTH: Cell<u32> = const { Cell::new(0) };
    /// Test-only: how many sibling tables were built.
    #[cfg(test)]
    static SIBLING_TABLES_BUILT: Cell<usize> = const { Cell::new(0) };
}

struct SiblingCache {
    /// Address of the document the cache applies to (avoids cross-document id collisions).
    document: usize,
    tables: HashMap<NodeId, SiblingTable>,
}

struct SiblingTable {
    /// For each element child: (index from start, index from end, same-type index from
    /// start, same-type index from end); all 1-based.
    entries: HashMap<NodeId, (usize, usize, usize, usize)>,
}

impl SiblingTable {
    fn build(doc: &Document, parent: NodeId) -> Self {
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
fn cached_sibling_indices(doc: &Document, node: NodeId) -> Option<(usize, usize, usize, usize)> {
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
fn element_index_from_start(doc: &Document, node_id: NodeId) -> Option<usize> {
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
fn element_index_from_end(doc: &Document, node_id: NodeId) -> Option<usize> {
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
fn type_index_from_start(doc: &Document, node_id: NodeId) -> Option<usize> {
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
fn type_index_from_end(doc: &Document, node_id: NodeId) -> Option<usize> {
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

/// A compound selector without combinators (e.g. `div#main.active[data-kind]:first-child`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompoundSelector {
    pub tag_name: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
    pub pseudo_classes: Vec<PseudoClass>,
    /// `#a#b` with different ids can never match: an element has a single `id` attribute.
    pub never_matches: bool,
    /// Number of `#id` simple selectors in the compound (specificity counts every one).
    pub id_count: u32,
}

impl CompoundSelector {
    /// Calculates the specificity of this compound selector according to Selectors Level 4 §16.
    pub fn specificity(&self) -> Specificity {
        let a = self.id_count.max(u32::from(self.id.is_some()));
        let b = (self.classes.len() + self.attributes.len()) as u32;
        let c = if self.tag_name.as_deref().is_some_and(|t| t != "*") {
            1
        } else {
            0
        };
        let mut specificity = Specificity::new(a, b, c);
        for pseudo in &self.pseudo_classes {
            specificity += pseudo.specificity();
        }
        specificity
    }

    /// Matches this compound selector against a DOM element.
    ///
    /// Type selectors match HTML elements case-insensitively and foreign (SVG/MathML)
    /// elements case-sensitively. Unlike the previous implementation, type/class/id/attribute
    /// and universal selectors are no longer restricted to the HTML namespace, so styles now
    /// apply to SVG/MathML elements (F14).
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        self.matches_with_backdrop(doc, node_id, false)
    }

    pub(crate) fn matches_for_backdrop(&self, doc: &Document, node_id: NodeId) -> bool {
        self.matches_with_backdrop(doc, node_id, true)
    }

    fn matches_with_backdrop(&self, doc: &Document, node_id: NodeId, allow_backdrop: bool) -> bool {
        if self.never_matches {
            return false;
        }
        let Some(el) = doc.node(node_id).as_element() else {
            return false;
        };

        if let Some(tag) = self.tag_name.as_deref()
            && tag != "*"
        {
            let name_matches = if el.namespace == Namespace::Html {
                el.local_name.eq_ignore_ascii_case(tag)
            } else {
                el.local_name == tag
            };
            if !name_matches {
                return false;
            }
        }

        if let Some(expected_id) = self.id.as_deref()
            && el.get_attribute("id") != Some(expected_id)
        {
            return false;
        }

        if !self.classes.is_empty() {
            let Some(class_attr) = el.get_attribute("class") else {
                return false;
            };
            // Class names are case-sensitive, HTML included (Selectors 4 §6.6): only
            // type selectors get the ASCII-insensitive treatment for HTML elements (#682).
            for c in &self.classes {
                if !class_attr.split_ascii_whitespace().any(|ec| ec == c) {
                    return false;
                }
            }
        }

        for attr_sel in &self.attributes {
            if !attr_sel.matches(doc, node_id) {
                return false;
            }
        }

        for pseudo in &self.pseudo_classes {
            if allow_backdrop && matches!(pseudo, PseudoClass::Backdrop) {
                continue;
            }
            if !pseudo.matches(doc, node_id) {
                return false;
            }
        }

        true
    }
}

/// Combinator joining two compound selectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    /// Descendant combinator (space)
    Descendant,
    /// Child combinator (`>`)
    Child,
    /// Next-sibling combinator (`+`)
    NextSibling,
    /// Subsequent-sibling combinator (`~`)
    SubsequentSibling,
}

/// A complex selector chain (e.g. `body > div.main p + span`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplexSelector {
    /// Compound selectors from left to right with their following combinators.
    /// The last compound selector has `None` as combinator and matches the target element.
    pub compounds: Vec<(CompoundSelector, Option<Combinator>)>,
}

impl ComplexSelector {
    /// Returns `true` when this selector targets the `::backdrop` pseudo-element on its subject.
    pub fn has_backdrop(&self) -> bool {
        self.compounds.last().is_some_and(|(compound, _)| {
            compound
                .pseudo_classes
                .iter()
                .any(|pseudo| matches!(pseudo, PseudoClass::Backdrop))
        })
    }

    /// Matches the `::backdrop` pseudo-element whose originating element is `node_id`.
    pub fn matches_backdrop(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_backdrop_inner(doc, node_id))
    }

    fn matches_backdrop_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        if self.compounds.is_empty() || !self.has_backdrop() {
            return false;
        }
        let last_idx = self.compounds.len() - 1;
        if !self.compounds[last_idx]
            .0
            .matches_for_backdrop(doc, node_id)
        {
            return false;
        }
        self.matches_from_right(doc, node_id, last_idx)
    }

    /// Matches the target element `node_id` against this complex selector.
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, node_id))
    }

    fn matches_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        if self.compounds.is_empty() {
            return false;
        }

        // Rightmost compound must match node_id
        let last_idx = self.compounds.len() - 1;
        if !self.compounds[last_idx].0.matches(doc, node_id) {
            return false;
        }

        self.matches_from_right(doc, node_id, last_idx)
    }

    /// Calculates the total specificity of this complex selector chain (§16).
    pub fn specificity(&self) -> Specificity {
        let mut spec = Specificity::ZERO;
        for (compound, _) in &self.compounds {
            spec += compound.specificity();
        }
        spec
    }

    fn matches_from_right(&self, doc: &Document, current_node: NodeId, index: usize) -> bool {
        if !spend_match_step() {
            return false;
        }
        if index == 0 {
            return true;
        }

        let prev_index = index - 1;
        let (ref prev_compound, comb) = self.compounds[prev_index];
        let Some(combinator) = comb else {
            return false;
        };

        match combinator {
            Combinator::Child => {
                let Some(parent_id) = doc.node(current_node).parent else {
                    return false;
                };
                if prev_compound.matches(doc, parent_id) {
                    self.matches_from_right(doc, parent_id, prev_index)
                } else {
                    false
                }
            }
            Combinator::Descendant => {
                let mut ancestor = doc.node(current_node).parent;
                while let Some(anc_id) = ancestor {
                    if prev_compound.matches(doc, anc_id)
                        && self.matches_from_right(doc, anc_id, prev_index)
                    {
                        return true;
                    }
                    ancestor = doc.node(anc_id).parent;
                }
                false
            }
            Combinator::NextSibling => {
                let mut prev = doc.node(current_node).prev_sibling;
                while let Some(prev_id) = prev {
                    if doc.node(prev_id).as_element().is_some() {
                        return if prev_compound.matches(doc, prev_id) {
                            self.matches_from_right(doc, prev_id, prev_index)
                        } else {
                            false
                        };
                    }
                    prev = doc.node(prev_id).prev_sibling;
                }
                false
            }
            Combinator::SubsequentSibling => {
                let mut prev = doc.node(current_node).prev_sibling;
                while let Some(prev_id) = prev {
                    if doc.node(prev_id).as_element().is_some()
                        && prev_compound.matches(doc, prev_id)
                        && self.matches_from_right(doc, prev_id, prev_index)
                    {
                        return true;
                    }
                    prev = doc.node(prev_id).prev_sibling;
                }
                false
            }
        }
    }
}

/// A relative selector as accepted by `:has()`: a combinator from the anchor element followed
/// by a complex selector (Selectors Level 4 §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeSelector {
    /// Combinator between the anchor element and the first compound (`Descendant` by default).
    pub leading: Combinator,
    /// Complex selector whose subject must be reachable from the anchor element.
    pub selector: ComplexSelector,
}

impl RelativeSelector {
    /// Returns whether `anchor` has a descendant/child/sibling matching this relative selector.
    ///
    /// Limitation: the matcher only supports combinators *after* the first compound when they
    /// are ordinary descendant/child/sibling combinators; it does not evaluate `:scope` and
    /// does not implement the full relative-selector anchoring rules with backtracking budgets.
    pub fn matches(&self, doc: &Document, anchor: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, anchor))
    }

    fn matches_inner(&self, doc: &Document, anchor: NodeId) -> bool {
        let compounds = &self.selector.compounds;
        if compounds.is_empty() {
            return false;
        }
        relative_chain_matches(doc, anchor, compounds, 0, self.leading)
    }
}

/// Matches `compounds[index]` relative to `current` through `combinator`, recursing forward.
fn relative_chain_matches(
    doc: &Document,
    current: NodeId,
    compounds: &[(CompoundSelector, Option<Combinator>)],
    index: usize,
    combinator: Combinator,
) -> bool {
    if !spend_match_step() {
        return false;
    }
    let (compound, next) = &compounds[index];
    let is_last = index + 1 == compounds.len();

    // Test each candidate as it is visited (lazy DFS): this spends the match budget once
    // per candidate instead of once per call, and removes the old fixed 10,000-node cut
    // that silently produced false negatives on large documents (#1133).
    let check = |candidate: NodeId| -> bool {
        if !spend_match_step() {
            return false;
        }
        if !compound.matches(doc, candidate) {
            return false;
        }
        if is_last {
            return true;
        }
        if let Some(next_combinator) = next {
            return relative_chain_matches(doc, candidate, compounds, index + 1, *next_combinator);
        }
        false
    };
    let exhausted = || MATCH_EXHAUSTED.with(|flag| flag.get());

    match combinator {
        Combinator::Child => {
            let mut child = doc.node(current).first_child;
            while let Some(child_id) = child {
                if check(child_id) {
                    return true;
                }
                if exhausted() {
                    return false;
                }
                child = doc.node(child_id).next_sibling;
            }
        }
        Combinator::Descendant => {
            let mut stack = Vec::new();
            let mut child = doc.node(current).first_child;
            while let Some(child_id) = child {
                stack.push(child_id);
                child = doc.node(child_id).next_sibling;
            }
            while let Some(candidate) = stack.pop() {
                if check(candidate) {
                    return true;
                }
                if exhausted() {
                    return false;
                }
                let mut child = doc.node(candidate).first_child;
                while let Some(child_id) = child {
                    stack.push(child_id);
                    child = doc.node(child_id).next_sibling;
                }
            }
        }
        Combinator::NextSibling => {
            let mut sibling = doc.node(current).next_sibling;
            while let Some(sibling_id) = sibling {
                if exhausted() {
                    return false;
                }
                if doc.node(sibling_id).as_element().is_some() && check(sibling_id) {
                    return true;
                }
                sibling = doc.node(sibling_id).next_sibling;
            }
        }
        Combinator::SubsequentSibling => {
            let mut sibling = doc.node(current).next_sibling;
            while let Some(sibling_id) = sibling {
                if exhausted() {
                    return false;
                }
                if doc.node(sibling_id).as_element().is_some() && check(sibling_id) {
                    return true;
                }
                sibling = doc.node(sibling_id).next_sibling;
            }
        }
    }
    false
}

/// A comma-separated list of complex selectors (`h1, h2, .title`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectorList {
    pub selectors: Vec<ComplexSelector>,
}

impl SelectorList {
    pub fn matches(&self, doc: &Document, node_id: NodeId) -> bool {
        with_match_budget(|| self.matches_inner(doc, node_id))
    }

    fn matches_inner(&self, doc: &Document, node_id: NodeId) -> bool {
        self.selectors.iter().any(|sel| sel.matches(doc, node_id))
    }

    /// If this selector list matches the given element, returns the maximum specificity
    /// among all the complex selectors in the list that matched.
    pub fn matching_specificity(&self, doc: &Document, node_id: NodeId) -> Option<Specificity> {
        // Share one budget across the alternative selectors of the list.
        let _scope = match_scope();
        self.selectors
            .iter()
            .filter(|sel| sel.matches(doc, node_id))
            .map(|sel| sel.specificity())
            .max()
    }

    /// Calculates the maximum specificity among all selectors in this list.
    pub fn max_specificity(&self) -> Specificity {
        self.selectors
            .iter()
            .map(|s| s.specificity())
            .max()
            .unwrap_or(Specificity::ZERO)
    }
}

/// Parses a selector string into a [`SelectorList`].
///
/// Commas only split the list at the top level: commas inside attribute values, functional
/// pseudo-class arguments or quoted strings do not start a new selector (F15). Per CSS Syntax
/// Level 3 §5.4.4, a selector list containing one invalid or unsupported selector is itself
/// invalid, so the whole list is dropped instead of silently over-matching with its valid
/// parts (F13).
///
/// Pending and not implemented in this milestone: `@media`/at-rules in the cascade, and
/// `:is()`, `:where()`, `:not()`, `:has()`, which currently invalidate the selector list.
pub fn parse_selectors(input: &str) -> SelectorList {
    if input.trim().is_empty() {
        return SelectorList::default();
    }
    // Functional pseudo-class arguments recurse through this same entry point; past the cap
    // the list is dropped as invalid, which also invalidates the declaration that uses it.
    let Some(_nesting) = ParserNestingGuard::enter() else {
        return SelectorList::default();
    };
    let parts = split_top_level_commas(input);
    let mut selectors = Vec::with_capacity(parts.len());
    for part in parts {
        let Some(complex) = parse_complex_selector(part.trim()) else {
            return SelectorList::default();
        };
        selectors.push(complex);
    }
    SelectorList { selectors }
}

/// Decodes one CSS escape whose introducing `\` has already been consumed
/// (CSS Syntax §4.3.7). Returns `U+FFFD` for an invalid or out-of-range escape.
fn consume_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<char> {
    match chars.peek().copied() {
        None => Some('\u{FFFD}'),
        // A newline after the backslash is not a valid escape: do not consume it.
        Some(c) if c == '\n' || c == '\r' || c == '\u{c}' => Some('\u{FFFD}'),
        Some(c) if c.is_ascii_hexdigit() => {
            let mut value: u32 = 0;
            let mut digits = 0;
            while digits < 6 {
                let Some(h) = chars.peek().copied().filter(|h| h.is_ascii_hexdigit()) else {
                    break;
                };
                chars.next();
                value = value * 16 + h.to_digit(16).unwrap_or(0);
                digits += 1;
            }
            // One whitespace character terminates the hex escape and is consumed.
            if chars.peek().is_some_and(|c| c.is_whitespace()) {
                chars.next();
            }
            if value == 0 || value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
                Some('\u{FFFD}')
            } else {
                char::from_u32(value)
            }
        }
        Some(c) => {
            chars.next();
            Some(c)
        }
    }
}

/// Reads a CSS identifier from the raw selector text, decoding escapes, so that an
/// author's `md\:flex` becomes the class name `md:flex` instead of `md` followed by a
/// `:flex` pseudo-class. Used for type names, ids, classes and pseudo-class names.
fn read_identifier(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut out = String::new();
    loop {
        match chars.peek().copied() {
            Some('\\') => {
                chars.next();
                if let Some(decoded) = consume_escape(chars) {
                    out.push(decoded);
                }
            }
            Some(c) if c.is_alphanumeric() || c == '-' || c == '_' => {
                chars.next();
                out.push(c);
            }
            _ => break,
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Splits a selector list on top-level commas, ignoring commas nested in blocks or strings.
fn split_top_level_commas(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            current.push(ch);
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            c => current.push(c),
        }
    }
    if !current.is_empty() || !parts.is_empty() {
        parts.push(current);
    }
    parts
}

/// Forward-only cursor over selector parts. `Vec::remove(0)` shifts every
/// remaining `String` header, making an n-part selector O(n²) (#1414).
struct SelectorTokens {
    parts: Vec<String>,
    pos: usize,
}

impl SelectorTokens {
    fn new(parts: Vec<String>) -> Self {
        Self { parts, pos: 0 }
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.parts.len()
    }

    fn next_part(&mut self) -> Option<String> {
        let part = self.parts.get(self.pos)?.clone();
        self.pos += 1;
        Some(part)
    }
}

/// Parses a single complex selector string.
fn parse_complex_selector(input: &str) -> Option<ComplexSelector> {
    let mut tokens = SelectorTokens::new(tokenize_selector_parts(input));
    let mut compounds = Vec::new();

    loop {
        let compound = parse_compound_selector(&mut tokens)?;
        if tokens.is_empty() {
            compounds.push((compound, None));
            break;
        }
        let combinator = match tokens.next_part()?.as_str() {
            ">" => Combinator::Child,
            "+" => Combinator::NextSibling,
            "~" => Combinator::SubsequentSibling,
            " " => Combinator::Descendant,
            _ => return None,
        };
        compounds.push((compound, Some(combinator)));
    }

    Some(ComplexSelector { compounds })
}

/// Appends the currently accumulated selector chunk to `parts`, if it is not blank.
fn push_selector_chunk(parts: &mut Vec<String>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
        current.clear();
    } else {
        current.clear();
    }
}

/// Splits a complex selector into compound-selector chunks and combinator tokens.
///
/// Whitespace becomes an explicit `" "` descendant-combinator token; `>`, `+` and `~` become
/// combinator tokens. Whitespace and combinators nested in `[...]`, `(...)` or strings stay
/// inside their chunk, so attribute values such as `[title="Hello, world"]` survive intact
/// (F15).
/// Anti-DoS caps for a single selector: beyond these the selector is rejected
/// instead of spending O(n²) time in the old `Vec::remove(0)` parser (#1414).
const MAX_SELECTOR_BYTES: usize = 8 * 1024;
const MAX_SELECTOR_PARTS: usize = 64;

fn tokenize_selector_parts(input: &str) -> Vec<String> {
    if input.len() > MAX_SELECTOR_BYTES {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // A CSS escape is part of the current compound; never a combinator or a
            // separator. Copy it verbatim so `read_identifier` decodes it later.
            current.push(ch);
            match chars.peek().copied() {
                Some(h) if h.is_ascii_hexdigit() => {
                    let mut digits = 0;
                    while digits < 6 {
                        let Some(h) = chars.peek().copied().filter(|h| h.is_ascii_hexdigit())
                        else {
                            break;
                        };
                        current.push(h);
                        chars.next();
                        digits += 1;
                    }
                    if chars.peek().is_some_and(|c| c.is_whitespace())
                        && let Some(ws) = chars.next()
                    {
                        current.push(ws);
                    }
                }
                Some(next) => {
                    current.push(next);
                    chars.next();
                }
                None => {}
            }
            continue;
        }
        match ch {
            '>' | '+' | '~' => {
                push_selector_chunk(&mut parts, &mut current);
                parts.push(ch.to_string());
            }
            c if c.is_whitespace() => {
                push_selector_chunk(&mut parts, &mut current);
                while chars.peek().copied().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                let next = chars.peek().copied();
                let prev_is_combinator =
                    matches!(parts.last().map(String::as_str), Some(">" | "+" | "~"));
                if next.is_some()
                    && !matches!(next, Some('>' | '+' | '~'))
                    && !parts.is_empty()
                    && !prev_is_combinator
                {
                    parts.push(" ".to_string());
                }
            }
            '[' | '(' => {
                let (open, close) = if ch == '[' { ('[', ']') } else { ('(', ')') };
                current.push(open);
                let mut depth = 1usize;
                let mut quote: Option<char> = None;
                let mut escaped = false;
                for inner in chars.by_ref() {
                    if escaped {
                        current.push(inner);
                        escaped = false;
                        continue;
                    }
                    if let Some(q) = quote {
                        current.push(inner);
                        if inner == '\\' {
                            escaped = true;
                        } else if inner == q {
                            quote = None;
                        }
                        continue;
                    }
                    match inner {
                        '"' | '\'' => {
                            quote = Some(inner);
                            current.push(inner);
                        }
                        c if c == open => {
                            depth += 1;
                            current.push(c);
                        }
                        c if c == close => {
                            depth -= 1;
                            current.push(c);
                            if depth == 0 {
                                break;
                            }
                        }
                        c => current.push(c),
                    }
                }
            }
            other => current.push(other),
        }
        if parts.len() > MAX_SELECTOR_PARTS {
            return Vec::new();
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Dynamic pseudo-classes whose state this engine does not track yet.
///
/// They are accepted by the parser but mapped to [`PseudoClass::Never`]: a rule such as
/// `a:hover { ... }` must not match every `a` element (CSS Selectors Level 4 §3).
const DYNAMIC_PSEUDO_CLASSES: &[&str] = &[
    "hover",
    "active",
    "focus",
    "focus-visible",
    "focus-within",
    "target",
    "target-within",
    "visited",
    "link",
    "any-link",
    "local-link",
    "checked",
    "indeterminate",
    "default",
    "disabled",
    "enabled",
    "required",
    "optional",
    "valid",
    "invalid",
    "in-range",
    "out-of-range",
    "read-only",
    "read-write",
    "placeholder-shown",
    "autofill",
    "fullscreen",
    "modal",
    "picture-in-picture",
    "playing",
    "paused",
];

fn parse_compound_selector(tokens: &mut SelectorTokens) -> Option<CompoundSelector> {
    if tokens.is_empty() {
        return None;
    }
    let s = tokens.next_part()?;
    if matches!(s.as_str(), " " | ">" | "+" | "~") {
        return None;
    }

    let mut compound = CompoundSelector::default();
    let mut chars = s.chars().peekable();

    // Optional type selector or universal selector prefix.
    if chars
        .peek()
        .copied()
        .is_some_and(|c| c.is_alphabetic() || c == '*' || c == '_' || c == '\\')
    {
        if chars.peek() == Some(&'*') {
            chars.next();
            compound.tag_name = Some("*".to_string());
        } else {
            compound.tag_name = Some(read_identifier(&mut chars)?);
        }
    }

    while let Some(ch) = chars.next() {
        match ch {
            '#' => {
                let id = read_identifier(&mut chars)?;
                compound.id_count = compound.id_count.saturating_add(1);
                if let Some(existing) = &compound.id {
                    // `#a#b` requires two different ids on one element, which is impossible.
                    if existing != &id {
                        compound.never_matches = true;
                    }
                } else {
                    compound.id = Some(id);
                }
            }
            '.' => {
                let class = read_identifier(&mut chars)?;
                compound.classes.push(class);
            }
            ':' => {
                let is_pseudo_element = chars.peek() == Some(&':');
                if is_pseudo_element {
                    chars.next();
                }
                let pseudo = read_identifier(&mut chars)?;

                let mut argument = None;
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1usize;
                    let mut arg = String::new();
                    for inner in chars.by_ref() {
                        match inner {
                            '(' => {
                                depth += 1;
                                arg.push(inner);
                            }
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                arg.push(inner);
                            }
                            c => arg.push(c),
                        }
                    }
                    if depth != 0 {
                        return None; // unbalanced functional pseudo-class
                    }
                    argument = Some(arg);
                }

                if is_pseudo_element {
                    if pseudo.eq_ignore_ascii_case("backdrop") && argument.is_none() {
                        compound.pseudo_classes.push(PseudoClass::Backdrop);
                    } else {
                        // Pseudo-element boxes are not generated yet: the selector stays valid
                        // but can never match an element (F13).
                        compound.pseudo_classes.push(PseudoClass::Never);
                    }
                    continue;
                }

                let name = pseudo.to_ascii_lowercase();
                let pseudo_class = match name.as_str() {
                    "first-child" if argument.is_none() => PseudoClass::FirstChild,
                    "last-child" if argument.is_none() => PseudoClass::LastChild,
                    "only-child" if argument.is_none() => PseudoClass::OnlyChild,
                    "empty" if argument.is_none() => PseudoClass::Empty,
                    "root" if argument.is_none() => PseudoClass::Root,
                    "first-of-type" if argument.is_none() => PseudoClass::FirstOfType,
                    "last-of-type" if argument.is_none() => PseudoClass::LastOfType,
                    "only-of-type" if argument.is_none() => PseudoClass::OnlyOfType,
                    "nth-child" => PseudoClass::NthChild(parse_an_plus_b(argument.as_deref()?)?),
                    "nth-last-child" => {
                        PseudoClass::NthLastChild(parse_an_plus_b(argument.as_deref()?)?)
                    }
                    "nth-of-type" => PseudoClass::NthOfType(parse_an_plus_b(argument.as_deref()?)?),
                    "nth-last-of-type" => {
                        PseudoClass::NthLastOfType(parse_an_plus_b(argument.as_deref()?)?)
                    }
                    "is" => PseudoClass::Is(parse_selector_argument(&argument)?),
                    "where" => PseudoClass::Where(parse_selector_argument(&argument)?),
                    "not" => PseudoClass::Not(parse_selector_argument(&argument)?),
                    "has" => PseudoClass::Has(parse_relative_selector_list(argument.as_deref()?)?),
                    "focus" if argument.is_none() => PseudoClass::Focus,
                    "focus-visible" if argument.is_none() => PseudoClass::FocusVisible,
                    "hover" if argument.is_none() => PseudoClass::Hover,
                    "active" if argument.is_none() => PseudoClass::Active,
                    "checked" if argument.is_none() => PseudoClass::Checked,
                    "disabled" if argument.is_none() => PseudoClass::Disabled,
                    "enabled" if argument.is_none() => PseudoClass::Enabled,
                    "read-only" if argument.is_none() => PseudoClass::ReadOnly,
                    "read-write" if argument.is_none() => PseudoClass::ReadWrite,
                    "required" if argument.is_none() => PseudoClass::Required,
                    "placeholder-shown" if argument.is_none() => PseudoClass::PlaceholderShown,
                    "modal" if argument.is_none() => PseudoClass::Modal,
                    // Legacy single-colon pseudo-element syntax (`:before`, `:first-line`, ...).
                    "before" | "after" | "first-line" | "first-letter" if argument.is_none() => {
                        PseudoClass::Never
                    }
                    _ if DYNAMIC_PSEUDO_CLASSES.contains(&name.as_str()) && argument.is_none() => {
                        PseudoClass::Never
                    }
                    // Unknown pseudo-classes invalidate the selector list instead of
                    // over-matching every element (F13).
                    _ => return None,
                };
                compound.pseudo_classes.push(pseudo_class);
            }
            '[' => {
                let mut attr_spec = String::new();
                let mut quote: Option<char> = None;
                let mut escaped = false;
                let mut closed = false;
                for inner in chars.by_ref() {
                    if escaped {
                        attr_spec.push(inner);
                        escaped = false;
                        continue;
                    }
                    if let Some(q) = quote {
                        attr_spec.push(inner);
                        if inner == '\\' {
                            escaped = true;
                        } else if inner == q {
                            quote = None;
                        }
                        continue;
                    }
                    match inner {
                        '"' | '\'' => {
                            quote = Some(inner);
                            attr_spec.push(inner);
                        }
                        ']' => {
                            closed = true;
                            break;
                        }
                        c => attr_spec.push(c),
                    }
                }
                if !closed {
                    return None;
                }
                compound
                    .attributes
                    .push(parse_attribute_selector(&attr_spec)?);
            }
            _ => return None,
        }
    }

    Some(compound)
}

/// Parses the argument of `:is()`, `:where()` or `:not()` into a selector list.
fn parse_selector_argument(argument: &Option<String>) -> Option<SelectorList> {
    let selectors = parse_selectors(argument.as_deref()?);
    if selectors.selectors.is_empty() {
        None
    } else {
        Some(selectors)
    }
}

/// Parses the argument of `:has()`: a comma-separated list of relative selectors.
///
/// Each relative selector may start with `>`, `+` or `~` (descendant when omitted), e.g.
/// `:has(> img)`, `:has(p)`, `:has(+ .note)`. Invalid or unsupported selectors make the whole
/// `:has()` pseudo-class invalid.
fn parse_relative_selector_list(input: &str) -> Option<Vec<RelativeSelector>> {
    let parts = split_top_level_commas(input);
    if parts.is_empty() {
        return None;
    }
    let mut relatives = Vec::with_capacity(parts.len());
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        let (leading, rest) = match part.chars().next()? {
            '>' => (Combinator::Child, part[1..].trim()),
            '+' => (Combinator::NextSibling, part[1..].trim()),
            '~' => (Combinator::SubsequentSibling, part[1..].trim()),
            _ => (Combinator::Descendant, part),
        };
        if rest.is_empty() {
            return None;
        }
        relatives.push(RelativeSelector {
            leading,
            selector: parse_complex_selector(rest)?,
        });
    }
    Some(relatives)
}

fn parse_attribute_selector(spec: &str) -> Option<AttributeSelector> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }

    // Find the operator outside quoted strings.
    let mut operator = None;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (byte_index, ch) in spec.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        let rest = &spec[byte_index + ch.len_utf8()..];
        match ch {
            '"' | '\'' => quote = Some(ch),
            '~' | '^' | '$' | '*' if rest.starts_with('=') => {
                let op = match ch {
                    '~' => AttrOperator::Includes,
                    '^' => AttrOperator::Prefix,
                    '$' => AttrOperator::Suffix,
                    _ => AttrOperator::Substring,
                };
                operator = Some((byte_index, op, 2usize));
                break;
            }
            '|' if rest.starts_with('=') => {
                operator = Some((byte_index, AttrOperator::DashMatch, 2usize));
                break;
            }
            '=' => {
                operator = Some((byte_index, AttrOperator::Exact, 1usize));
                break;
            }
            // Attribute namespaces (`[ns|attr]`) are not supported yet.
            '|' => return None,
            _ => {}
        }
    }

    let Some((op_index, op, op_len)) = operator else {
        // Presence selector `[attr]`.
        if !is_valid_attribute_name(spec) {
            return None;
        }
        return Some(AttributeSelector {
            name: spec.to_string(),
            operator: None,
            value: None,
            case_insensitive: false,
        });
    };

    let name = spec[..op_index].trim();
    if !is_valid_attribute_name(name) {
        return None;
    }

    let mut value_part = spec[op_index + op_len..].trim();
    let mut case_insensitive = false;
    if let Some((rest, flag)) = split_case_flag(value_part) {
        value_part = rest;
        case_insensitive = flag;
    }
    let value = strip_attribute_value_quotes(value_part)?;

    Some(AttributeSelector {
        name: name.to_string(),
        operator: Some(op),
        value: Some(value),
        case_insensitive,
    })
}

/// Returns whether `name` looks like a valid attribute local name for a selector.
fn is_valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().any(|c| {
            c.is_ascii_whitespace()
                || matches!(
                    c,
                    '"' | '\'' | '[' | ']' | '=' | '~' | '^' | '$' | '*' | '|'
                )
        })
}

/// Splits a trailing `i`/`s` case-sensitivity flag from an attribute selector value.
fn split_case_flag(value: &str) -> Option<(&str, bool)> {
    let (rest, flag) = value.trim_end().rsplit_once(char::is_whitespace)?;
    let case_insensitive = match flag {
        "i" | "I" => true,
        "s" | "S" => false,
        _ => return None,
    };
    Some((rest.trim_end(), case_insensitive))
}

/// Removes the surrounding quotes of an attribute selector value, validating unquoted values.
fn strip_attribute_value_quotes(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(first) = value.chars().next()
        && matches!(first, '"' | '\'')
        && value.len() >= 2
        && value.ends_with(first)
    {
        return Some(value[1..value.len() - 1].to_string());
    }
    if value.is_empty() || value.contains(char::is_whitespace) {
        return None;
    }
    Some(value.to_string())
}

#[cfg(test)]
mod id_list_tests {
    use super::*;
    use dom::Attribute;

    fn doc_with_id(id: &str) -> (Document, NodeId) {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", Vec::new());
        doc.append_child(doc.root(), html);
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("id", id)]);
        doc.append_child(html, div);
        (doc, div)
    }

    #[test]
    fn conflicting_ids_never_match() {
        let (doc, node) = doc_with_id("a");
        assert!(
            !parse_selectors("#a#b").matches(&doc, node),
            "an element has a single id attribute"
        );
        assert!(
            parse_selectors("#a#a").matches(&doc, node),
            "repeating the same id is redundant but matchable"
        );
        assert!(!parse_selectors("div#b#c").matches(&doc, node));
    }

    #[test]
    fn every_id_counts_for_specificity() {
        let list = parse_selectors("#a#b");
        assert_eq!(list.max_specificity(), Specificity::new(2, 0, 0));
        assert_eq!(
            parse_selectors("#a#a").max_specificity(),
            Specificity::new(2, 0, 0)
        );
        assert_eq!(
            parse_selectors("div#a").max_specificity(),
            Specificity::new(1, 0, 1)
        );
    }
}

#[cfg(test)]
mod sibling_cache_tests {
    use super::*;

    #[test]
    fn nth_of_type_reuses_one_sibling_table() {
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", Vec::new());
        doc.append_child(doc.root(), body);
        for _ in 0..500 {
            let item = doc.create_element(Namespace::Html, "li", Vec::new());
            doc.append_child(body, item);
        }
        let selector = parse_selectors("li:nth-of-type(odd)");
        let _scope = document_scope();
        let matched = doc
            .children(body)
            .filter(|node| selector.matches(&doc, *node))
            .count();
        assert_eq!(matched, 250);
        assert_eq!(
            SIBLING_TABLES_BUILT.with(|count| count.get()),
            1,
            "one table for the parent, reused by every child"
        );
    }
}

#[cfg(test)]
mod specificity_overflow_tests {
    use super::Specificity;

    #[test]
    fn addition_saturates_instead_of_overflowing() {
        let huge = Specificity::new(u32::MAX, u32::MAX, u32::MAX);
        let one = Specificity::new(1, 1, 1);
        assert_eq!(huge + one, huge);
        let mut sum = Specificity::new(u32::MAX - 1, 0, 0);
        sum += Specificity::new(2, 0, 0);
        assert_eq!(sum, Specificity::new(u32::MAX, 0, 0));
    }
}

#[cfg(test)]
mod anplusb_tests {
    use super::*;

    #[test]
    fn anplusb_matches_does_not_overflow() {
        let formula = AnPlusB { a: -1, b: i32::MIN };
        // Must not panic; treat as no match rather than aborting.
        let _ = formula.matches(1);
        let _ = formula.matches(i32::MAX);
        let formula = AnPlusB { a: 1, b: i32::MIN };
        let _ = formula.matches(i32::MIN);
    }
}

#[cfg(test)]
mod style_pass_budget_tests {
    use super::*;

    #[test]
    fn a_style_pass_shares_the_step_budget_across_elements() {
        let _pass = style_pass_scope();
        {
            let _element = match_scope();
            // Pretend this element burned the whole shared budget.
            MATCH_STEPS.with(|steps| steps.set(0));
            assert!(!spend_match_step());
        }
        let _next_element = match_scope();
        // The next element of the same pass must not get a fresh 200k budget: that reset is
        // what turned a pathological stylesheet into quadratic work (#1413).
        assert!(!spend_match_step());
    }

    #[test]
    fn a_standalone_match_scope_still_gets_its_own_budget() {
        {
            let _element = match_scope();
            MATCH_STEPS.with(|steps| steps.set(0));
            assert!(!spend_match_step());
        }
        // Outside a style pass each top-level match/resolution keeps the documented
        // per-element budget.
        let _next = match_scope();
        assert!(spend_match_step());
    }

    #[test]
    fn nested_style_passes_install_the_budget_only_once() {
        let _outer = style_pass_scope();
        MATCH_STEPS.with(|steps| steps.set(1));
        let _inner = style_pass_scope();
        assert_eq!(MATCH_STEPS.with(|steps| steps.get()), 1);
    }
}

#[cfg(test)]
mod selector_budget_tests {
    use super::*;

    #[test]
    fn oversized_selectors_are_rejected_before_tokenizing() {
        let huge = "a".repeat(MAX_SELECTOR_BYTES + 1);
        assert!(tokenize_selector_parts(&huge).is_empty());

        let many = "a ".repeat(MAX_SELECTOR_PARTS + 2);
        assert!(many.len() < MAX_SELECTOR_BYTES);
        assert!(tokenize_selector_parts(&many).is_empty());

        assert!(!tokenize_selector_parts("a>b").is_empty());
    }
}
