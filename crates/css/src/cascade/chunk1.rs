//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

/// Fast non-cryptographic hasher for short CSS property names in the per-element cascade map.
#[derive(Default)]
pub(crate) struct FastHasher(u64);

impl std::hash::Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = self.0;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        self.0 = hash;
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct FastBuildHasher;

impl std::hash::BuildHasher for FastBuildHasher {
    type Hasher = FastHasher;
    fn build_hasher(&self) -> FastHasher {
        FastHasher(0xcbf2_9ce4_8422_2325)
    }
}

/// Computed styles for generated pseudo-elements (`::before` / `::after`), keyed by
/// `(originating_element, pseudo_element)`.
pub type PseudoStyles = HashMap<(NodeId, PseudoElement), crate::properties::ComputedStyle>;

/// Layer rank for declarations that do not belong to any `@layer`. Normal unlayered
/// declarations outrank every layer; important unlayered declarations are the lowest
/// (the order is inverted for `!important`).
pub const UNLAYERED_LAYER: u32 = u32::MAX;

/// Origin of a CSS declaration per W3C Cascading and Inheritance Level 4 §6.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Browser default User-Agent stylesheet.
    UserAgent,
    /// User personal stylesheet or accessibility overrides.
    User,
    /// Author stylesheet (`<style>`, `<link>`, or `style="..."` attribute).
    Author,
}

/// Cascade precedence rank per W3C Cascading and Inheritance Level 4 §6.2:
///
/// 1. User-Agent normal
/// 2. User normal
/// 3. Author normal
/// 4. Author `!important`
/// 5. User `!important`
/// 6. User-Agent `!important`
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CascadeOriginImportance {
    UserAgentNormal = 0,
    UserNormal = 1,
    AuthorNormal = 2,
    AuthorImportant = 3,
    UserImportant = 4,
    UserAgentImportant = 5,
}

impl CascadeOriginImportance {
    #[inline]
    pub fn new(origin: Origin, important: bool) -> Self {
        match (origin, important) {
            (Origin::UserAgent, false) => Self::UserAgentNormal,
            (Origin::User, false) => Self::UserNormal,
            (Origin::Author, false) => Self::AuthorNormal,
            (Origin::Author, true) => Self::AuthorImportant,
            (Origin::User, true) => Self::UserImportant,
            (Origin::UserAgent, true) => Self::UserAgentImportant,
        }
    }
}

/// A matched declaration along with its cascade priority metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchedDeclaration<'a> {
    pub declaration: &'a Declaration,
    pub origin_importance: CascadeOriginImportance,
    /// Global cascade-layer rank (0 = first declared layer; [`UNLAYERED_LAYER`] = no layer).
    pub layer_rank: u32,
    pub specificity: Specificity,
    pub source_order: u32,
    /// Position of the declaration inside its rule (or inline block). Later declarations win
    /// over earlier ones with the same origin, importance, specificity and source order.
    pub declaration_order: u32,
}

impl<'a> Eq for MatchedDeclaration<'a> {}

impl<'a> PartialOrd for MatchedDeclaration<'a> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for MatchedDeclaration<'a> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        let important = matches!(
            self.origin_importance,
            CascadeOriginImportance::AuthorImportant
                | CascadeOriginImportance::UserImportant
                | CascadeOriginImportance::UserAgentImportant
        );
        // Normal declarations: later layers win (unlayered is the implicit last layer).
        // Important declarations: the layer order is inverted.
        let layer_order = if important {
            other.layer_rank.cmp(&self.layer_rank)
        } else {
            self.layer_rank.cmp(&other.layer_rank)
        };
        self.origin_importance
            .cmp(&other.origin_importance)
            .then(layer_order)
            .then_with(|| self.specificity.cmp(&other.specificity))
            .then_with(|| self.source_order.cmp(&other.source_order))
            .then_with(|| self.declaration_order.cmp(&other.declaration_order))
    }
}

/// A compiled CSS rule with pre-parsed selectors and declarations for fast matching.
#[derive(Clone, Debug)]
pub struct CompiledRule {
    pub selectors: SelectorList,
    pub declarations: Vec<Declaration>,
    pub origin: Origin,
    pub source_order: u32,
    /// Cascade-layer rank (see [`UNLAYERED_LAYER`]).
    pub layer_rank: u32,
}

/// One compiled `@container` rule: query + nested style rule (selectors/declarations).
#[derive(Clone, Debug)]
pub struct ContainerRule {
    pub query: crate::container::ContainerQuery,
    pub rule: CompiledRule,
}

/// Bucket key derived from the rightmost compound of a complex selector.
///
/// The rightmost compound is the one that must match the target element, so the key is a
/// safe candidate filter: any element matched by the selector is found through this bucket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RuleKey {
    /// `#id` — the most selective key (ids are unique in a document).
    Id(String),
    /// `.class` — stored verbatim: class matching is case-sensitive (#682).
    Class(String),
    /// `tag` — stored ASCII-lowercased; element lookups try the exact name first and the
    /// folded name second, keeping foreign (SVG/MathML) case-sensitive matching a superset.
    Tag(String),
}

/// One indexed candidate: a specific selector inside a rule.
///
/// The index stores selectors rather than rules so grouped rules such as the UA
/// `html, body, div, p, ... { display: block }` only evaluate the selector whose key matched
/// (a rule-level index would test every alternative in the list for every candidate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Candidate {
    pub(crate) rule: u32,
    pub(crate) selector: u32,
    /// Bloom mask of the classes the selector requires on *ancestor* compounds (only
    /// descendant/child combinators). A candidate whose mask is not covered by the target
    /// element's ancestor-class mask cannot match, so it is dropped before the (expensive)
    /// full selector match.
    pub(crate) ancestor_mask: u64,
}

/// One bit of the ancestor-class Bloom filter for `class`.
///
/// The hash deliberately folds ASCII case even though class matching is case-sensitive
/// (#682): both sides of the comparison fold the same way, so the mask can only produce a
/// false positive, never prune a selector that would have matched.
pub(crate) fn class_hash_bit(class: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in class.bytes() {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    1u64 << (hash & 63)
}

/// Classes that `selector` requires on ancestors of the target element.
///
/// Only compounds followed by a descendant or child combinator are guaranteed to be on the
/// ancestor chain; compounds followed by a sibling combinator are not, so their classes are
/// left out (the mask must never reject a selector that could match).
pub(crate) fn ancestor_class_mask(selector: &ComplexSelector) -> u64 {
    let mut mask = 0u64;
    for (compound, combinator) in &selector.compounds {
        if matches!(combinator, Some(Combinator::Descendant | Combinator::Child)) {
            for class in &compound.classes {
                mask |= class_hash_bit(class);
            }
        }
    }
    mask
}

/// Bloom mask of every class on the ancestor chain of `node_id`.
pub(crate) fn ancestor_classes_mask(doc: &Document, node_id: NodeId) -> u64 {
    let mut mask = 0u64;
    let mut current = doc.node(node_id).parent;
    while let Some(ancestor) = current {
        if let Some(element) = doc.node(ancestor).as_element()
            && let Some(classes) = element.get_attribute("class")
        {
            for class in classes.split_ascii_whitespace() {
                mask |= class_hash_bit(class);
            }
        }
        current = doc.node(ancestor).parent;
    }
    mask
}

/// Index of selectors by the key of their rightmost compound.
///
/// Rules whose rightmost compound has no id/class/tag key (universal `*`, standalone
/// functional pseudo-classes such as `:is(...)`/`:not(...)`/`:has(...)`, or any compound whose
/// only simple selectors are pseudo-classes/attributes) are kept in [`RuleIndex::slow`] and
/// evaluated for every element: the index can only prune rules, never change matching.
#[derive(Clone, Debug, Default)]
pub(crate) struct RuleIndex {
    pub(crate) by_id: HashMap<String, Vec<Candidate>>,
    pub(crate) by_class: HashMap<String, Vec<Candidate>>,
    pub(crate) by_tag: HashMap<String, Vec<Candidate>>,
    pub(crate) slow: Vec<Candidate>,
    /// Number of rules covered by the index; compared against `rules.len()` so a collection
    /// whose public `rules` vector was mutated directly falls back to the brute-force path.
    pub(crate) indexed_rules: usize,
}

impl RuleIndex {
    /// Adds the selectors of `rule` to the buckets for their rightmost-compound key.
    pub(crate) fn insert(&mut self, rule: u32, selectors: &SelectorList) {
        for (selector_index, selector) in selectors.selectors.iter().enumerate() {
            let candidate = Candidate {
                rule,
                selector: selector_index as u32,
                ancestor_mask: ancestor_class_mask(selector),
            };
            match selector_key(selector) {
                Some(RuleKey::Id(id)) => self.by_id.entry(id).or_default().push(candidate),
                Some(RuleKey::Class(class)) => {
                    self.by_class.entry(class).or_default().push(candidate);
                }
                Some(RuleKey::Tag(tag)) => self
                    .by_tag
                    .entry(tag.to_ascii_lowercase())
                    .or_default()
                    .push(candidate),
                // No usable key (universal or pseudo-class-only rightmost compound): the
                // selector is evaluated for every element.
                None => self.slow.push(candidate),
            }
        }
        self.indexed_rules += 1;
    }

    /// Collects and sorts the candidate selectors that may match `node_id`.
    pub(crate) fn collect_candidates(
        &self,
        doc: &Document,
        node_id: NodeId,
        out: &mut Vec<Candidate>,
    ) {
        out.clear();
        out.extend_from_slice(&self.slow);
        let Some(element) = doc.node(node_id).as_element() else {
            out.sort_unstable();
            return;
        };
        let ancestor_mask = ancestor_classes_mask(doc, node_id);

        if let Some(id) = element.get_attribute("id")
            && let Some(rules) = self.by_id.get(id)
        {
            out.extend_from_slice(rules);
        }

        if let Some(class_attribute) = element.get_attribute("class") {
            // Bucket keys keep the selector's own spelling, so the lookup is exact: folding
            // here would route `.Foo` to elements whose class is `foo` (#682).
            for class in class_attribute.split_ascii_whitespace() {
                if let Some(rules) = self.by_class.get(class) {
                    out.extend_from_slice(rules);
                }
            }
        }

        // `by_tag` keys are ASCII-lowercased (see `selector_key`). An element name that is
        // already lowercase hits the bucket directly; otherwise fold it (allocating only
        // for the rarer uppercase-element case) so `<DIV>` still finds a `div` rule.
        let local_name = element.local_name.as_str();
        let folded = local_name.to_ascii_lowercase();
        if let Some(rules) = self.by_tag.get(&folded) {
            out.extend_from_slice(rules);
        }

        // Prune descendant-selector candidates whose required ancestor classes are not on
        // this element's ancestor chain (a cheap Bloom test before the full match).
        if ancestor_mask != 0 {
            out.retain(|candidate| candidate.ancestor_mask & !ancestor_mask == 0);
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// Returns the candidate key of the rightmost compound, or `None` when the compound has no
/// id/class/tag (universal or pseudo-class-only rightmost compound).
///
/// Complex selectors (descendant/child/sibling combinators) are keyed by their rightmost
/// compound only: the index never needs to understand the rest of the chain, because any
/// element the selector matches must match that compound.
pub(crate) fn selector_key(selector: &ComplexSelector) -> Option<RuleKey> {
    let (compound, _) = selector.compounds.last()?;
    if let Some(id) = &compound.id {
        return Some(RuleKey::Id(id.clone()));
    }
    // Any required class is a safe key; use the last one (typically the modifier class and
    // the most selective in author stylesheets).
    if let Some(class) = compound.classes.last() {
        return Some(RuleKey::Class(class.clone()));
    }
    if let Some(tag) = &compound.tag_name
        && tag != "*"
    {
        return Some(RuleKey::Tag(tag.to_ascii_lowercase()));
    }
    None
}

/// A collection of compiled stylesheets ready for cascade resolution.
#[derive(Clone, Debug, Default)]
pub struct StyleCollection {
    /// Compiled rules in source order. Mutate only through `add_stylesheet`/`add_css`; a direct
    /// mutation makes `index` stale, in which case resolution falls back to evaluating every
    /// rule (correct, just slower).
    pub rules: Vec<CompiledRule>,
    pub next_source_order: u32,
    /// Media environment used when compiling `@media` rules; set it before adding stylesheets.
    pub media: MediaContext,
    /// Compiled `@container` rules (added in source order; evaluated per node during the
    /// cascade against the nearest ancestor query container).
    pub container_rules: Vec<ContainerRule>,
    /// `@keyframes` rules in source order (the animation engine resolves them by name;
    /// later definitions win, as in the cascade).
    pub keyframes: Vec<crate::rule::KeyframesRule>,
    /// Declared cascade layers in first-appearance order (each path is relative: `["a","b"]`
    /// for `@layer a.b`). Anonymous layers get a unique `"<anonymous N>"` path segment.
    pub layers: Vec<Vec<String>>,
    /// Map from layer path to its global rank; used by `@layer` registration.
    pub(crate) layer_index: HashMap<Vec<String>, u32>,
    /// `::backdrop` rules compiled from stylesheets (applied to top-layer modal elements).
    pub backdrop_rules: Vec<CompiledRule>,
    /// Rule index by rightmost-compound key. Maintained by the `add_*` methods; if the public
    /// `rules` vector is mutated directly, resolution falls back to evaluating every rule.
    pub(crate) index: RuleIndex,
    /// Whether any compiled declaration uses the `revert-layer` value. When false (the
    /// common case) the per-element revert-layer scan is skipped entirely.
    pub(crate) has_revert_layer: bool,
    /// Whether any compiled selector targets a pseudo-element (`::before`/`::after`).
    pub(crate) has_pseudo_rules: bool,
    /// Whether any compiled rule declares an `animation*` property.
    pub(crate) has_animations: bool,
    /// Whether any compiled rule declares a `transition*` property.
    pub(crate) has_transitions: bool,
}
