//! CSS cascade and `@layer` support.
//!
//! Cascade order (highest priority first): origin/importance, then cascade layer, then
//! specificity, then source/declaration order. Normal declarations follow layer declaration
//! order with unlayered rules as the implicit last (highest) layer; `!important` inverts the
//! layer order (unlayered important is the lowest). `revert-layer` rolls the winner back to
//! the previous layer (not the UA origin) in a single step; same-layer candidates are
//! skipped.
//!
//! MVP limits (documented in `lib.rs`): no `layer()` in `@import`/programmatic API, nested
//! `@layer` at-rules are supported but declarations directly inside a layer are approximated
//! as part of that layer (ordered before its sublayers), and `@container`/`@media` inside
//! layers only participate through the layer rank they are declared in.

//! # W3C CSS Cascading and Inheritance Level 4 (§6)
//!
//! Implements declaration origin and importance ranking, specificity calculation,
//! source-order tie breaking, and cascaded value resolution for DOM elements.
//!
//! ## Rule index
//!
//! [`StyleCollection`] indexes every selector by the key of its rightmost compound
//! (`#id` > `.class` > `tag`; universal and pseudo-class-only compounds go to a small "slow"
//! bucket evaluated for every element). Resolving an element only evaluates the selectors
//! whose key the element carries, instead of every rule in every stylesheet. The index is a
//! *superset* filter: matching and the cascade order (origin/importance/specificity/source
//! order) are unchanged, and the equivalence tests compare the indexed path against a
//! brute-force reference over several stylesheets/rule orders. Mutating the public `rules`
//! vector directly makes the index stale; resolution then falls back to the brute-force path.

use std::cmp::Ordering;
use std::collections::HashMap;

/// Fast non-cryptographic hasher for short CSS property names in the per-element cascade map.
#[derive(Default)]
struct FastHasher(u64);

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
struct FastBuildHasher;

impl std::hash::BuildHasher for FastBuildHasher {
    type Hasher = FastHasher;
    fn build_hasher(&self) -> FastHasher {
        FastHasher(0xcbf2_9ce4_8422_2325)
    }
}

use crate::rule::{Declaration, Rule, Stylesheet, serialize_component_values};
use crate::selector::{Combinator, ComplexSelector, SelectorList, Specificity, parse_selectors};
use crate::shorthand::expand_declarations;
use dom::{Document, NodeId};

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

/// Media environment used to evaluate `@media` rules and imported stylesheet media lists.
///
/// Re-exported from [`crate::media`], which owns the full media query parser/evaluator.
pub use crate::media::MediaContext;

/// Bucket key derived from the rightmost compound of a complex selector.
///
/// The rightmost compound is the one that must match the target element, so the key is a
/// safe candidate filter: any element matched by the selector is found through this bucket.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RuleKey {
    /// `#id` — the most selective key (ids are unique in a document).
    Id(String),
    /// `.class` — stored ASCII-lowercased because class matching is ASCII case-insensitive.
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
struct Candidate {
    rule: u32,
    selector: u32,
    /// Bloom mask of the classes the selector requires on *ancestor* compounds (only
    /// descendant/child combinators). A candidate whose mask is not covered by the target
    /// element's ancestor-class mask cannot match, so it is dropped before the (expensive)
    /// full selector match.
    ancestor_mask: u64,
}

/// One bit of the ancestor-class Bloom filter for `class`.
///
/// Class matching is ASCII case-insensitive, so the hash folds ASCII case; false positives
/// are harmless because the mask only prunes candidates that would fail to match anyway.
fn class_hash_bit(class: &str) -> u64 {
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
fn ancestor_class_mask(selector: &ComplexSelector) -> u64 {
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
fn ancestor_classes_mask(doc: &Document, node_id: NodeId) -> u64 {
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
struct RuleIndex {
    by_id: HashMap<String, Vec<Candidate>>,
    by_class: HashMap<String, Vec<Candidate>>,
    by_tag: HashMap<String, Vec<Candidate>>,
    slow: Vec<Candidate>,
    /// Number of rules covered by the index; compared against `rules.len()` so a collection
    /// whose public `rules` vector was mutated directly falls back to the brute-force path.
    indexed_rules: usize,
}

impl RuleIndex {
    /// Adds the selectors of `rule` to the buckets for their rightmost-compound key.
    fn insert(&mut self, rule: u32, selectors: &SelectorList) {
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
                Some(RuleKey::Tag(tag)) => self.by_tag.entry(tag).or_default().push(candidate),
                // No usable key (universal or pseudo-class-only rightmost compound): the
                // selector is evaluated for every element.
                None => self.slow.push(candidate),
            }
        }
        self.indexed_rules += 1;
    }

    /// Collects and sorts the candidate selectors that may match `node_id`.
    fn collect_candidates(&self, doc: &Document, node_id: NodeId, out: &mut Vec<Candidate>) {
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
            for class in class_attribute.split_ascii_whitespace() {
                if let Some(rules) = self.by_class.get(class) {
                    out.extend_from_slice(rules);
                } else if class.bytes().any(|byte| byte.is_ascii_uppercase()) {
                    // Bucket keys are ASCII-lowercased; plain class names (the common case)
                    // avoid this allocation.
                    let folded = class.to_ascii_lowercase();
                    if let Some(rules) = self.by_class.get(&folded) {
                        out.extend_from_slice(rules);
                    }
                }
            }
        }

        let local_name = element.local_name.as_str();
        if let Some(rules) = self.by_tag.get(local_name) {
            out.extend_from_slice(rules);
        } else if local_name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            let folded = local_name.to_ascii_lowercase();
            if let Some(rules) = self.by_tag.get(&folded) {
                out.extend_from_slice(rules);
            }
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
fn selector_key(selector: &ComplexSelector) -> Option<RuleKey> {
    let (compound, _) = selector.compounds.last()?;
    if let Some(id) = &compound.id {
        return Some(RuleKey::Id(id.clone()));
    }
    // Any required class is a safe key; use the last one (typically the modifier class and
    // the most selective in author stylesheets).
    if let Some(class) = compound.classes.last() {
        return Some(RuleKey::Class(class.to_ascii_lowercase()));
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
    layer_index: HashMap<Vec<String>, u32>,
    /// Rule index by rightmost-compound key. Maintained by the `add_*` methods; if the public
    /// `rules` vector is mutated directly, resolution falls back to evaluating every rule.
    index: RuleIndex,
    /// Whether any compiled declaration uses the `revert-layer` value. When false (the
    /// common case) the per-element revert-layer scan is skipped entirely.
    has_revert_layer: bool,
}

/// Compiles one style rule (selectors + expanded declarations) without assigning a source
/// order; callers set `source_order` before inserting it.
fn compile_style_rule(style_rule: &crate::rule::StyleRule, origin: Origin) -> Option<CompiledRule> {
    let selector_str = serialize_component_values(&style_rule.prelude);
    let selectors = parse_selectors(&selector_str);
    if selectors.selectors.is_empty() {
        return None;
    }
    let declarations = expand_declarations(&style_rule.declarations);
    if declarations.is_empty() {
        return None;
    }
    Some(CompiledRule {
        selectors,
        declarations,
        origin,
        source_order: 0,
        layer_rank: UNLAYERED_LAYER,
    })
}

impl StyleCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a stylesheet to the collection with the given origin.
    ///
    /// Declarations are shorthand-expanded before they are stored, so every rule contributes
    /// longhand declarations to the cascade (see [`crate::shorthand::expand_declarations`]).
    /// `@media` rules are evaluated against [`StyleCollection::media`]; other at-rules
    /// (`@import`, `@supports`, `@font-face`, ...) are still ignored.
    pub fn add_stylesheet(&mut self, stylesheet: &Stylesheet, origin: Origin) {
        self.add_rules(&stylesheet.rules, origin, &[], UNLAYERED_LAYER);
        for rule in &stylesheet.keyframes {
            match self
                .keyframes
                .iter_mut()
                .find(|existing| existing.name == rule.name)
            {
                // Later definitions of the same name replace earlier ones (source order).
                Some(existing) => *existing = rule.clone(),
                None => self.keyframes.push(rule.clone()),
            }
        }
    }

    /// Returns the `@keyframes` rule for `name` (CSS animation names are case-sensitive).
    pub fn keyframes(&self, name: &str) -> Option<&crate::rule::KeyframesRule> {
        self.keyframes.iter().find(|rule| rule.name == name)
    }

    /// Registers a layer path (first appearance wins the rank) and returns its rank.
    fn register_layer(&mut self, path: Vec<String>) -> u32 {
        if let Some(&rank) = self.layer_index.get(&path) {
            return rank;
        }
        let rank = self.layers.len() as u32;
        self.layers.push(path.clone());
        self.layer_index.insert(path, rank);
        rank
    }

    /// Composes a possibly dotted layer name relative to the current layer path.
    fn compose_layer_path(current: &[String], name: &str) -> Vec<String> {
        let mut path = current.to_vec();
        for segment in name.split('.') {
            let segment = segment.trim();
            if !segment.is_empty() {
                path.push(segment.to_ascii_lowercase());
            }
        }
        path
    }

    /// Returns the global layer rank for a path (0-based, first appearance order).
    pub fn layer_rank(&self, path: &[String]) -> Option<u32> {
        self.layer_index.get(path).copied()
    }

    fn add_rules(
        &mut self,
        rules: &[Rule],
        origin: Origin,
        layer_path: &[String],
        layer_rank: u32,
    ) {
        for rule in rules {
            match rule {
                Rule::Style(style_rule) => {
                    self.add_style_rule(style_rule, origin, layer_rank);
                }
                Rule::At(at_rule) if at_rule.name.eq_ignore_ascii_case("media") => {
                    let prelude = serialize_component_values(&at_rule.prelude);
                    let query_list = crate::media::parse_media_query_list(&prelude);
                    if crate::media::evaluate(&query_list, &self.media)
                        && let Some(block) = &at_rule.block
                    {
                        let nested_text = serialize_component_values(&block.values);
                        let nested = crate::parser::parse_stylesheet(&nested_text);
                        self.add_rules(&nested.rules, origin, layer_path, layer_rank);
                    }
                }
                Rule::At(at_rule) if at_rule.name.eq_ignore_ascii_case("container") => {
                    let prelude = serialize_component_values(&at_rule.prelude);
                    let Some(query) = crate::container::parse_query(&prelude) else {
                        continue;
                    };
                    let Some(block) = &at_rule.block else {
                        continue;
                    };
                    let nested_text = serialize_component_values(&block.values);
                    let nested = crate::parser::parse_stylesheet(&nested_text);
                    self.add_container_rules(&nested.rules, origin, &query, layer_rank);
                }
                Rule::At(at_rule) if at_rule.name.eq_ignore_ascii_case("layer") => {
                    let prelude = serialize_component_values(&at_rule.prelude);
                    let prelude = prelude.trim();
                    match &at_rule.block {
                        // `@layer name;` / `@layer a, b;`: declaration only.
                        None => {
                            for name in prelude.split(',') {
                                let name = name.trim();
                                if !name.is_empty() {
                                    let path = Self::compose_layer_path(layer_path, name);
                                    self.register_layer(path);
                                }
                            }
                        }
                        Some(block) => {
                            let name = if prelude.is_empty() {
                                format!("<anonymous-{}>", self.layers.len())
                            } else {
                                prelude.to_owned()
                            };
                            let path = Self::compose_layer_path(layer_path, &name);
                            let rank = self.register_layer(path.clone());
                            let nested_text = serialize_component_values(&block.values);
                            let nested = crate::parser::parse_stylesheet(&nested_text);
                            self.add_rules(&nested.rules, origin, &path, rank);
                        }
                    }
                }
                Rule::At(_) => {}
            }
        }
    }

    /// Compiles the style rules inside an `@container` block (nested `@container`/`@layer`
    /// at-rules are not supported; `@media` inside is evaluated immediately).
    fn add_container_rules(
        &mut self,
        rules: &[Rule],
        origin: Origin,
        query: &crate::container::ContainerQuery,
        layer_rank: u32,
    ) {
        for rule in rules {
            match rule {
                Rule::Style(style_rule) => {
                    if let Some(compiled) = compile_style_rule(style_rule, origin) {
                        let compiled = CompiledRule {
                            source_order: self.next_source_order,
                            layer_rank,
                            ..compiled
                        };
                        self.next_source_order += 1;
                        if compiled.declarations.iter().any(is_revert_layer) {
                            self.has_revert_layer = true;
                        }
                        self.container_rules.push(ContainerRule {
                            query: query.clone(),
                            rule: compiled,
                        });
                    }
                }
                Rule::At(at_rule) if at_rule.name.eq_ignore_ascii_case("media") => {
                    let prelude = serialize_component_values(&at_rule.prelude);
                    let query_list = crate::media::parse_media_query_list(&prelude);
                    if crate::media::evaluate(&query_list, &self.media)
                        && let Some(block) = &at_rule.block
                    {
                        let nested_text = serialize_component_values(&block.values);
                        let nested = crate::parser::parse_stylesheet(&nested_text);
                        self.add_container_rules(&nested.rules, origin, query, layer_rank);
                    }
                }
                _ => {}
            }
        }
    }

    fn add_style_rule(
        &mut self,
        style_rule: &crate::rule::StyleRule,
        origin: Origin,
        layer_rank: u32,
    ) {
        let Some(mut compiled) = compile_style_rule(style_rule, origin) else {
            return;
        };
        compiled.source_order = self.next_source_order;
        compiled.layer_rank = layer_rank;
        self.next_source_order += 1;
        let rule_index = self.rules.len() as u32;
        self.index.insert(rule_index, &compiled.selectors);
        if compiled.declarations.iter().any(is_revert_layer) {
            self.has_revert_layer = true;
        }
        self.rules.push(compiled);
    }

    /// Returns the candidate selectors that can possibly match `node_id`, in ascending
    /// (source) order.
    ///
    /// The result is a superset of the matching selectors: rules keyed by id/class/tag whose
    /// key the element does not carry are skipped, while universal/slow selectors are always
    /// included.
    fn candidate_selectors(&self, doc: &Document, node_id: NodeId, out: &mut Vec<Candidate>) {
        if self.index.indexed_rules != self.rules.len() {
            // `rules` was mutated outside the `add_*` API: brute-force to stay correct.
            out.clear();
            for (rule_index, rule) in self.rules.iter().enumerate() {
                for selector_index in 0..rule.selectors.selectors.len() {
                    out.push(Candidate {
                        rule: rule_index as u32,
                        selector: selector_index as u32,
                        ancestor_mask: 0,
                    });
                }
            }
            return;
        }
        self.index.collect_candidates(doc, node_id, out);
    }

    /// Parses and adds CSS text to the collection with the given origin.
    pub fn add_css(&mut self, css_text: &str, origin: Origin) {
        let stylesheet = crate::parser::parse_stylesheet(css_text);
        self.add_stylesheet(&stylesheet, origin);
    }

    /// Adds the stylesheet fetched for an [`crate::rule::ImportRule`] if its media list matches
    /// the collection's [`StyleCollection::media`] environment.
    ///
    /// Contract for consumers (e.g. the `render` crate): resolve [`crate::rule::ImportRule::url`] against
    /// the base URL, fetch/decode the stylesheet, parse it with
    /// [`crate::parser::parse_stylesheet`] and call this method for each import **in document
    /// order before the importing sheet's own rules**, so origin/source order match CSS.
    /// Recursive imports are the consumer's responsibility (guard cycles). Returns `true`
    /// when the stylesheet was added (media matched).
    pub fn add_import_stylesheet(
        &mut self,
        import: &crate::rule::ImportRule,
        stylesheet: &Stylesheet,
        origin: Origin,
    ) -> bool {
        if !crate::media::evaluate(&import.media, &self.media) {
            return false;
        }
        self.add_stylesheet(stylesheet, origin);
        true
    }
}

/// Resolves the cascaded property declarations for a DOM element node.
///
/// Evaluates:
/// 1. Matched rules in [`StyleCollection`] (User-Agent, User, Author).
/// 2. Inline declarations from the element's `style="..."` attribute (if present).
/// 3. Normalizes priority via `(Origin & Importance, Specificity, Source Order)`.
///
/// Returns a map of property name (lowercase) to the winning [`Declaration`]. Shorthands in
/// the rules are already expanded; callers passing inline `style="..."` declarations should
/// expand them with [`crate::shorthand::expand_declarations`] first (as
/// [`resolve_document_styles`] does).
pub fn resolve_cascaded_properties<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
) -> HashMap<String, &'a Declaration> {
    let mut candidates = Vec::new();
    collection.candidate_selectors(doc, node_id, &mut candidates);
    let mut best = resolve_base_cascaded_properties(
        doc,
        node_id,
        collection,
        inline_declarations,
        &candidates,
    );
    merge_container_rules(doc, node_id, collection, &mut best);
    apply_revert_layer(doc, node_id, collection, &candidates, &mut best);
    owned_declarations(best)
}

/// Shared cascade resolution over the candidate rules selected by the index.
///
/// `candidates` is a superset of the selectors that can match `node_id`, in ascending source
/// order (the index only prunes; matching still decides). Passing every `(rule, selector)`
/// pair yields the exact unindexed behaviour, which the equivalence tests use as reference.
fn resolve_base_cascaded_properties<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
    candidates: &[Candidate],
) -> HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher> {
    // One selector-matching budget for this element: every candidate selector shares it.
    let _match_scope = crate::selector::match_scope();
    let mut best_candidates: HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher> =
        HashMap::with_hasher(FastBuildHasher);

    // 1. Match stylesheet rules (indexed candidates only; the index guarantees a superset).
    //    A rule with several matching selectors contributes each of them; the cascade compares
    //    their specificities with the same total order, so the winner equals the rule-level
    //    "maximum matching specificity" semantics.
    for candidate in candidates {
        let rule = &collection.rules[candidate.rule as usize];
        let selector = &rule.selectors.selectors[candidate.selector as usize];
        if selector.matches(doc, node_id) {
            let spec = selector.specificity();
            for (declaration_order, decl) in rule.declarations.iter().enumerate() {
                let origin_importance = CascadeOriginImportance::new(rule.origin, decl.important);
                let candidate = MatchedDeclaration {
                    declaration: decl,
                    origin_importance,
                    layer_rank: rule.layer_rank,
                    specificity: spec,
                    source_order: rule.source_order,
                    declaration_order: declaration_order as u32,
                };

                match best_candidates.entry(decl.name.as_str()) {
                    std::collections::hash_map::Entry::Occupied(mut occupied) => {
                        if candidate > *occupied.get() {
                            occupied.insert(candidate);
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(vacant) => {
                        vacant.insert(candidate);
                    }
                }
            }
        }
    }

    // 2. Process inline styles from style attribute (Author origin with high specificity)
    if let Some(inline_decls) = inline_declarations {
        let inline_specificity = Specificity::new(100_000, 0, 0);
        let inline_source_order = collection.next_source_order + 10_000;
        for (declaration_order, decl) in inline_decls.iter().enumerate() {
            let origin_importance = CascadeOriginImportance::new(Origin::Author, decl.important);
            let candidate = MatchedDeclaration {
                declaration: decl,
                origin_importance,
                layer_rank: UNLAYERED_LAYER,
                specificity: inline_specificity,
                source_order: inline_source_order,
                declaration_order: declaration_order as u32,
            };

            match best_candidates.entry(decl.name.as_str()) {
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    if candidate > *occupied.get() {
                        occupied.insert(candidate);
                    }
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(candidate);
                }
            }
        }
    }

    best_candidates
}

/// Reference implementation used by cascade-equivalence tests (no `@container` rules).
#[cfg(test)]
fn resolve_cascaded_properties_from_candidates<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
    candidates: &[Candidate],
) -> HashMap<String, &'a Declaration> {
    owned_declarations(resolve_base_cascaded_properties(
        doc,
        node_id,
        collection,
        inline_declarations,
        candidates,
    ))
}

/// Applies the collection's `@container` rules to `best` (already resolved base cascade).
fn merge_container_rules<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    best: &mut HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher>,
) {
    // Documents without `@container` rules (the vast majority) skip the loop entirely.
    if collection.container_rules.is_empty() {
        return;
    }
    for container_rule in &collection.container_rules {
        let rule = &container_rule.rule;
        for selector in &rule.selectors.selectors {
            if !selector.matches(doc, node_id) {
                continue;
            }
            let Some((size, container_style)) =
                crate::container::matching_size(doc, node_id, &container_rule.query, collection)
            else {
                continue;
            };
            if !crate::container::evaluate(&container_rule.query, &size, &container_style) {
                continue;
            }
            let specificity = selector.specificity();
            for (declaration_order, decl) in rule.declarations.iter().enumerate() {
                let origin_importance = CascadeOriginImportance::new(rule.origin, decl.important);
                let candidate = MatchedDeclaration {
                    declaration: decl,
                    origin_importance,
                    layer_rank: rule.layer_rank,
                    specificity,
                    source_order: rule.source_order,
                    declaration_order: declaration_order as u32,
                };
                match best.entry(decl.name.as_str()) {
                    std::collections::hash_map::Entry::Occupied(mut occupied) => {
                        if candidate > *occupied.get() {
                            occupied.insert(candidate);
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(vacant) => {
                        vacant.insert(candidate);
                    }
                }
            }
        }
    }
}

fn is_revert_layer(declaration: &Declaration) -> bool {
    declaration
        .raw_value
        .trim()
        .eq_ignore_ascii_case("revert-layer")
}

fn is_important(origin_importance: CascadeOriginImportance) -> bool {
    matches!(
        origin_importance,
        CascadeOriginImportance::AuthorImportant
            | CascadeOriginImportance::UserImportant
            | CascadeOriginImportance::UserAgentImportant
    )
}

/// Applies `revert-layer`: the winner rolls back to the previous cascade layer (not the UA
/// origin). Candidates from the same layer are skipped; a missing previous layer removes the
/// property (initial/inherited value).
fn apply_revert_layer<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    candidates: &[Candidate],
    best: &mut HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher>,
) {
    // `revert-layer` is rare: without any such declaration the per-element scan (which
    // allocates a Vec and string-compares every winning declaration) is pure overhead.
    // The flag is only trusted when the index is in sync; a `rules` vector mutated
    // directly (documented fallback path) keeps the slow, always-correct scan.
    if !collection.has_revert_layer && collection.index.indexed_rules == collection.rules.len() {
        return;
    }
    let losers: Vec<&'a str> = best
        .iter()
        .filter(|(_, matched)| is_revert_layer(matched.declaration))
        .map(|(name, _)| *name)
        .collect();
    for property in losers {
        let Some(winner) = best.get(property).cloned() else {
            continue;
        };
        let important = is_important(winner.origin_importance);
        let mut matched: Vec<MatchedDeclaration<'a>> = Vec::new();
        // Re-evaluate only the index's candidates (a superset of the matching selectors)
        // instead of scanning every rule × selector of the collection: `revert-layer` used
        // to disable the rule index for each affected property.
        for candidate in candidates {
            let Some(rule) = collection.rules.get(candidate.rule as usize) else {
                continue;
            };
            let Some(selector) = rule.selectors.selectors.get(candidate.selector as usize) else {
                continue;
            };
            if !selector.matches(doc, node_id) {
                continue;
            }
            let specificity = selector.specificity();
            for (declaration_order, declaration) in rule.declarations.iter().enumerate() {
                if declaration.name != property {
                    continue;
                }
                let origin_importance =
                    CascadeOriginImportance::new(rule.origin, declaration.important);
                if origin_importance != winner.origin_importance {
                    continue;
                }
                matched.push(MatchedDeclaration {
                    declaration,
                    origin_importance,
                    layer_rank: rule.layer_rank,
                    specificity,
                    source_order: rule.source_order,
                    declaration_order: declaration_order as u32,
                });
            }
        }
        for container_rule in &collection.container_rules {
            let rule = &container_rule.rule;
            let Some((size, container_style)) =
                crate::container::matching_size(doc, node_id, &container_rule.query, collection)
            else {
                continue;
            };
            if !crate::container::evaluate(&container_rule.query, &size, &container_style) {
                continue;
            }
            for selector in &rule.selectors.selectors {
                if !selector.matches(doc, node_id) {
                    continue;
                }
                let specificity = selector.specificity();
                for (declaration_order, declaration) in rule.declarations.iter().enumerate() {
                    if declaration.name != property {
                        continue;
                    }
                    let origin_importance =
                        CascadeOriginImportance::new(rule.origin, declaration.important);
                    if origin_importance != winner.origin_importance {
                        continue;
                    }
                    matched.push(MatchedDeclaration {
                        declaration,
                        origin_importance,
                        layer_rank: rule.layer_rank,
                        specificity,
                        source_order: rule.source_order,
                        declaration_order: declaration_order as u32,
                    });
                }
            }
        }
        matched.sort_by(|a, b| b.cmp(a));
        let mut replacement = None;
        for candidate in matched {
            if candidate.layer_rank == winner.layer_rank {
                continue;
            }
            let lower_priority = if important {
                candidate.layer_rank > winner.layer_rank
            } else {
                candidate.layer_rank < winner.layer_rank
            };
            if lower_priority {
                replacement = Some(candidate);
                break;
            }
        }
        match replacement {
            Some(candidate) if !is_revert_layer(candidate.declaration) => {
                best.insert(property, candidate);
            }
            _ => {
                best.remove(property);
            }
        }
    }
}

fn owned_declarations<'a>(
    best: HashMap<&'a str, MatchedDeclaration<'a>, FastBuildHasher>,
) -> HashMap<String, &'a Declaration> {
    best.into_iter()
        .map(|(name, matched)| (name.to_string(), matched.declaration))
        .collect()
}

/// Resolves only the unconditional cascade (no `@container` rules); used by container size
/// prediction so it cannot recurse through itself.
pub(crate) fn resolve_cascaded_properties_base<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
) -> HashMap<String, &'a Declaration> {
    let mut candidates = Vec::new();
    collection.candidate_selectors(doc, node_id, &mut candidates);
    owned_declarations(resolve_base_cascaded_properties(
        doc,
        node_id,
        collection,
        inline_declarations,
        &candidates,
    ))
}

/// Resolves computed styles with an explicit interaction [`crate::StateContext`] for dynamic
/// pseudo-classes (`:focus`, `:hover`, `:checked`, ...). `render`/shell should call this so a
/// focused/edited form re-resolves with the host state; static documents keep using
/// [`resolve_document_styles_with_media`].
pub fn resolve_document_styles_with_media_and_state(
    doc: &Document,
    author_css: Option<&str>,
    media: MediaContext,
    state: &crate::StateContext,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    crate::state::with_state(*state, || {
        resolve_document_styles_with_media(doc, author_css, media)
    })
}

/// Applies embedded CSS animations to already-resolved computed styles at `time_ms`.
///
/// The base styles are used as the underlying values; animated styles are not
/// re-inherited into descendants in this pass (animating an inherited property such as
/// `font-size` affects the element's own box but not its children's inherited value).
/// Callers that need live animation advance `time_ms` and re-run the pass.
pub fn apply_animations_to_styles(
    doc: &Document,
    styles: &mut HashMap<NodeId, crate::properties::ComputedStyle>,
    collection: &StyleCollection,
    time_ms: f32,
) {
    if collection.keyframes.is_empty() || !time_ms.is_finite() {
        return;
    }
    let mut animated: Vec<(NodeId, crate::properties::ComputedStyle)> = Vec::new();
    for node_id in doc.descendants(doc.root()) {
        let Some(base) = styles.get(&node_id) else {
            continue;
        };
        if base.animations.is_empty() {
            continue;
        }
        let style = crate::animation_runtime::apply_animations(
            base,
            &base.animations,
            &collection.keyframes,
            time_ms,
        );
        animated.push((node_id, style));
    }
    for (node_id, style) in animated {
        styles.insert(node_id, style);
    }
}

/// Resolves computed styles for all elements in the DOM document.
///
/// Pre-loads the default WHATWG HTML User-Agent stylesheet, then compiles and cascades
/// any author CSS supplied (e.g. from `<style>` tags or external stylesheets). `@media`
/// rules are evaluated for the `screen` medium with an unknown viewport, so width/height
/// features never match; use [`resolve_document_styles_with_viewport`] to provide one.
pub fn resolve_document_styles(
    doc: &Document,
    author_css: Option<&str>,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    resolve_document_styles_with_media(doc, author_css, MediaContext::default())
}

/// Resolves computed styles with `@media` width/height features evaluated for a viewport.
pub fn resolve_document_styles_with_viewport(
    doc: &Document,
    author_css: Option<&str>,
    width: f32,
    height: f32,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    resolve_document_styles_with_media(doc, author_css, MediaContext::with_viewport(width, height))
}

/// Hashes an element's winning declarations (and its parent identity) into a 128-bit
/// style-cache key.
///
/// Winning declarations live in the immutable `StyleCollection` (or in the element's local
/// inline-declaration buffer), so their addresses identify the values cheaply: hashing
/// pointers avoids re-hashing every property name/value string per element. Map iteration
/// order is unspecified, so per-declaration hashes are XOR-ed together. Two independent
/// 64-bit mixes (different constants/rotations) make accidental collisions negligible.
fn style_cache_key(parent_key: u128, cascaded: &HashMap<String, &Declaration>) -> u128 {
    let mut low = parent_key as u64 ^ (parent_key >> 64) as u64;
    let mut high = low ^ 0xa5a5_5a5a_0123_4567;
    for declaration in cascaded.values() {
        let pointer = std::ptr::from_ref(*declaration) as usize as u64;
        low ^= pointer
            .wrapping_add(0x9e37_79b9_7f4a_7c15)
            .rotate_left(17)
            .wrapping_mul(0x100_0000_01b3);
        high ^= pointer
            .wrapping_add(0xd6e8_feb8_6659_fd93)
            .rotate_left(29)
            .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    }
    u128::from(low) << 64 | u128::from(high)
}

/// Resolves computed styles with a fully specified [`MediaContext`] for `@media` evaluation.
pub fn resolve_document_styles_with_media(
    doc: &Document,
    author_css: Option<&str>,
    media: MediaContext,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    // One sibling-index cache for the whole pass: `:nth-*` matching becomes O(1) per element
    // instead of scanning the sibling chain.
    let _sibling_scope = crate::selector::document_scope();
    let mut collection = crate::user_agent::create_user_agent_style_collection();
    collection.media = media;
    if let Some(css) = author_css {
        collection.add_css(css, Origin::Author);
    }

    let mut styles: HashMap<NodeId, crate::properties::ComputedStyle> = HashMap::new();
    let mut style_cache: HashMap<u128, crate::properties::ComputedStyle, FastBuildHasher> =
        HashMap::with_hasher(FastBuildHasher);
    let mut style_seen: HashMap<u128, u8, FastBuildHasher> = HashMap::with_hasher(FastBuildHasher);
    let mut style_keys: HashMap<NodeId, u128, FastBuildHasher> =
        HashMap::with_hasher(FastBuildHasher);
    let mut candidates: Vec<Candidate> = Vec::new();
    let root = doc.root();

    // Collect elements in top-down pre-order traversal
    let mut stack = Vec::new();
    let mut curr = doc.node(root).first_child;
    while let Some(child_id) = curr {
        stack.push(child_id);
        curr = doc.node(child_id).next_sibling;
    }

    while let Some(node_id) = stack.pop() {
        if let Some(el) = doc.node(node_id).as_element() {
            let inline_decls = el.get_attribute("style").map(|style| {
                // Cap inline style length to prevent DoS.
                let style = if style.len() > 64 * 1024 {
                    &style[..64 * 1024]
                } else {
                    style
                };
                let parsed = crate::parser::parse_list_of_declarations(style);
                expand_declarations(&parsed)
            });

            collection.candidate_selectors(doc, node_id, &mut candidates);
            let mut best = resolve_base_cascaded_properties(
                doc,
                node_id,
                &collection,
                inline_decls.as_deref(),
                &candidates,
            );
            merge_container_rules(doc, node_id, &collection, &mut best);
            apply_revert_layer(doc, node_id, &collection, &candidates, &mut best);
            let cascaded = owned_declarations(best);

            let parent_style = doc.node(node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&root);
            // Inline `style="..."` declarations live in a per-element buffer whose memory
            // could be reused by the next element, so they are excluded from the
            // pointer-keyed cache (they are also rare); everything else lives in the
            // immutable StyleCollection, where declaration addresses are stable.
            let cacheable = inline_decls.is_none();
            let parent_key = doc
                .node(node_id)
                .parent
                .and_then(|parent| style_keys.get(&parent).copied())
                .unwrap_or(0x9e37_79b9_7f4a_7c15);
            let cache_key = style_cache_key(parent_key, &cascaded);
            if cacheable && let Some(cached) = style_cache.get(&cache_key) {
                styles.insert(node_id, cached.clone());
                style_keys.insert(node_id, cache_key);
            } else {
                let computed =
                    crate::properties::ComputedStyle::compute(&cascaded, parent_style, root_style);
                if cacheable {
                    // Only repeated declaration sets are cached; unique elements pay a
                    // cheap hash/counter instead of a full ComputedStyle clone, and the
                    // miss path never clones (the computed value moves into `styles`).
                    let seen = style_seen.entry(cache_key).or_insert(0);
                    *seen = seen.saturating_add(1);
                    if *seen >= 2 && style_cache.len() < 1024 {
                        style_cache.insert(cache_key, computed.clone());
                    }
                }
                styles.insert(node_id, computed);
                if cacheable {
                    style_keys.insert(node_id, cache_key);
                } else {
                    let mut hash = 0xcbf2_9ce4_8422_2325u64;
                    for &byte in node_id.get().to_le_bytes().iter() {
                        hash ^= u64::from(byte);
                        hash = hash.wrapping_mul(0x100_0000_01b3);
                    }
                    style_keys.insert(node_id, u128::from(hash) << 64 | u128::from(hash ^ 0xa5a5));
                }
            }
        }

        // Push children in reverse order (last child first) so the first child is popped
        // first; this avoids a temporary allocation per node on the hot resolution path.
        let mut child = doc.node(node_id).last_child;
        while let Some(c) = child {
            stack.push(c);
            child = doc.node(c).prev_sibling;
        }
    }

    styles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::{Color, ComputedStyle};
    use crate::rule::Declaration;
    use dom::{Attribute, Namespace};

    fn collection_with(author_css: &str) -> StyleCollection {
        let mut collection = crate::user_agent::create_user_agent_style_collection();
        collection.add_css(author_css, Origin::Author);
        collection
    }

    fn element_ids(doc: &Document) -> Vec<NodeId> {
        let mut ids = Vec::new();
        let mut stack = Vec::new();
        let mut child = doc.node(doc.root()).first_child;
        while let Some(child_id) = child {
            stack.push(child_id);
            child = doc.node(child_id).next_sibling;
        }
        while let Some(node_id) = stack.pop() {
            if doc.node(node_id).as_element().is_some() {
                ids.push(node_id);
            }
            let mut child = doc.node(node_id).last_child;
            while let Some(child_id) = child {
                stack.push(child_id);
                child = doc.node(child_id).prev_sibling;
            }
        }
        ids
    }

    fn expanded_inline(doc: &Document, node_id: NodeId) -> Option<Vec<Declaration>> {
        let element = doc.node(node_id).as_element()?;
        element
            .get_attribute("style")
            .map(|style| expand_declarations(&crate::parser::parse_list_of_declarations(style)))
    }

    /// Every `(rule, selector)` pair: the unindexed reference behaviour.
    fn all_candidates(collection: &StyleCollection) -> Vec<Candidate> {
        let mut candidates = Vec::new();
        for (rule_index, rule) in collection.rules.iter().enumerate() {
            for selector_index in 0..rule.selectors.selectors.len() {
                candidates.push(Candidate {
                    rule: rule_index as u32,
                    selector: selector_index as u32,
                    ancestor_mask: 0,
                });
            }
        }
        candidates
    }

    fn resolve_with_candidates(
        doc: &Document,
        collection: &StyleCollection,
        all: &[Candidate],
    ) -> HashMap<NodeId, ComputedStyle> {
        let mut styles = HashMap::new();
        for node_id in element_ids(doc) {
            let inline_declarations = expanded_inline(doc, node_id);
            let cascaded = resolve_cascaded_properties_from_candidates(
                doc,
                node_id,
                collection,
                inline_declarations.as_deref(),
                all,
            );
            let parent_style = doc.node(node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&doc.root());
            let computed = ComputedStyle::compute(&cascaded, parent_style, root_style);
            styles.insert(node_id, computed);
        }
        styles
    }

    fn resolve_indexed(
        doc: &Document,
        collection: &StyleCollection,
    ) -> HashMap<NodeId, ComputedStyle> {
        let mut styles = HashMap::new();
        let mut candidates = Vec::new();
        for node_id in element_ids(doc) {
            let inline_declarations = expanded_inline(doc, node_id);
            collection.candidate_selectors(doc, node_id, &mut candidates);
            let cascaded = resolve_cascaded_properties_from_candidates(
                doc,
                node_id,
                collection,
                inline_declarations.as_deref(),
                &candidates,
            );
            let parent_style = doc.node(node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&doc.root());
            let computed = ComputedStyle::compute(&cascaded, parent_style, root_style);
            styles.insert(node_id, computed);
        }
        styles
    }

    fn resolve_bruteforce(
        doc: &Document,
        collection: &StyleCollection,
    ) -> HashMap<NodeId, ComputedStyle> {
        resolve_with_candidates(doc, collection, &all_candidates(collection))
    }

    /// Document with ids, classes, child/sibling structure and inline styles.
    fn build_document() -> Document {
        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let section = doc.create_element(Namespace::Html, "section", vec![]);
        let target = doc.create_element(
            Namespace::Html,
            "div",
            vec![
                Attribute::new("id", "target"),
                Attribute::new("class", "a b"),
                Attribute::new("data-role", "note"),
            ],
        );
        let span1 = doc.create_element(Namespace::Html, "span", vec![Attribute::new("class", "a")]);
        let span2 = doc.create_element(
            Namespace::Html,
            "span",
            vec![
                Attribute::new("class", "b"),
                Attribute::new("style", "color: red !important; margin-block: 3px;"),
            ],
        );
        let p = doc.create_element(Namespace::Html, "p", vec![Attribute::new("class", "a")]);
        let list = doc.create_element(Namespace::Html, "ul", vec![]);
        let item = doc.create_element(Namespace::Html, "li", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, body);
        doc.append_child(body, section);
        doc.append_child(body, target);
        doc.append_child(target, span1);
        doc.append_child(target, span2);
        doc.append_child(target, p);
        doc.append_child(body, list);
        doc.append_child(list, item);
        doc
    }

    #[test]
    fn test_rule_index_matches_unindexed_cascade() {
        // Several rule orders and selector shapes: id/class/tag/universal, combinators,
        // selector lists, functional pseudo-classes (slow bucket) and shorthand expansion.
        let sheets = [
            "div { color: red; } .a { color: green; } #target { color: blue; }",
            "#target { color: blue; } div { color: red; } .a { color: green; }",
            ".a { color: green; } #target { color: blue; } div { color: red; }",
            "span, .a { font-weight: bold; } div > span { color: teal; } * { font-style: italic; }",
            "div:is(.a, #target) { color: purple; } div:not(.a) { opacity: 0.5; } \
             section:has(> p) { color: orange; }",
            "p ~ span { color: maroon; } div span { padding-left: 1px; } \
             [data-role=\"note\"] { margin-top: 2px; }",
            "div.a { border: 1px solid red; } div { border-top: none; } \
             span.a { margin: 1px 2px; } li { color: rgb(1, 2, 3); }",
            "body * { outline: 0; } html body section > div { font-size: 20px; }",
        ];
        for css in sheets {
            let doc = build_document();
            let collection = collection_with(css);
            let indexed = resolve_indexed(&doc, &collection);
            let bruteforce = resolve_bruteforce(&doc, &collection);
            assert_eq!(
                indexed.len(),
                bruteforce.len(),
                "element count differs for css: {css}"
            );
            for (node_id, expected) in &bruteforce {
                let actual = indexed.get(node_id).expect("indexed style");
                assert_eq!(
                    actual, expected,
                    "indexed cascade differs for node {node_id:?} with css: {css}"
                );
            }
            // The public end-to-end API must also agree with the brute-force reference.
            let public = resolve_document_styles(&doc, Some(css));
            assert_eq!(
                public.len(),
                bruteforce.len(),
                "public element count differs for css: {css}"
            );
            for (node_id, expected) in &bruteforce {
                assert_eq!(
                    public.get(node_id).expect("public style"),
                    expected,
                    "public cascade differs for node {node_id:?} with css: {css}"
                );
            }
        }
    }

    #[test]
    fn test_rule_index_source_order_and_specificity_preserved() {
        let doc = build_document();
        let target = doc
            .node(doc.root())
            .first_child
            .and_then(|html| doc.node(html).first_child)
            .and_then(|body| doc.node(body).first_child)
            .and_then(|section| doc.node(section).next_sibling)
            .expect("target div");

        // Lower specificity tag rule after a higher specificity class rule must lose.
        let collection = collection_with(".a { color: green; } div { color: red; }");
        let styles = resolve_indexed(&doc, &collection);
        assert_eq!(styles.get(&target).expect("style").color, Color::GREEN);

        // Later rule with equal specificity wins.
        let collection = collection_with("div { color: red; } div { color: blue; }");
        let styles = resolve_indexed(&doc, &collection);
        assert_eq!(styles.get(&target).expect("style").color, Color::BLUE);
    }

    #[test]
    fn test_rule_index_prunes_and_keeps_slow_selectors() {
        let mut collection = StyleCollection::new();
        collection.add_css(
            "div.hit { color: red; } span.miss { color: blue; } unknown { color: green; } \
             * { margin: 0; } div:is(.hit) { padding: 0; }",
            Origin::Author,
        );
        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![Attribute::new("class", "hit")]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, div);

        let mut candidates = Vec::new();
        collection.candidate_selectors(&doc, div, &mut candidates);
        let total: usize = collection
            .rules
            .iter()
            .map(|rule| rule.selectors.selectors.len())
            .sum();
        assert!(candidates.len() < total, "the index must prune candidates");
        // div.hit (class bucket), the slow `*` selector and `div:is(.hit)` (tag bucket).
        assert_eq!(candidates.len(), 3);
        let names: Vec<&str> = candidates
            .iter()
            .map(|candidate| {
                collection.rules[candidate.rule as usize]
                    .selectors
                    .selectors[candidate.selector as usize]
                    .compounds
                    .last()
                    .and_then(|(compound, _)| compound.classes.first())
                    .map_or("*", String::as_str)
            })
            .collect();
        assert!(names.contains(&"hit"));
        assert!(!names.contains(&"miss"));

        // The indexed and brute-force results still agree.
        assert_eq!(
            resolve_indexed(&doc, &collection),
            resolve_bruteforce(&doc, &collection)
        );
    }

    #[test]
    fn test_stale_index_falls_back_to_brute_force() {
        let mut collection = StyleCollection::new();
        collection.add_css("div { color: red; }", Origin::Author);
        // Mutate the public `rules` vector directly, bypassing index maintenance.
        collection.rules.push(CompiledRule {
            selectors: parse_selectors("div"),
            declarations: vec![Declaration {
                name: "color".to_string(),
                value: Vec::new(),
                important: false,
                raw_value: "blue".to_string(),
            }],
            origin: Origin::Author,
            source_order: 99,
            layer_rank: UNLAYERED_LAYER,
        });

        let mut doc = Document::new();
        let body = doc.create_element(Namespace::Html, "body", vec![]);
        let div = doc.create_element(Namespace::Html, "div", vec![]);
        doc.append_child(doc.root(), body);
        doc.append_child(body, div);

        let indexed = resolve_indexed(&doc, &collection);
        let bruteforce = resolve_bruteforce(&doc, &collection);
        assert_eq!(indexed, bruteforce);
        // The directly-appended rule has the highest source order and must win.
        assert_eq!(indexed.get(&div).expect("style").color, Color::BLUE);
    }
    #[test]
    fn uppercase_inline_property_names_apply() {
        let mut doc = build_document();
        let target = doc.get_element_by_id("target").expect("target");
        doc.node_mut(target)
            .as_element_mut()
            .expect("element")
            .set_attribute(Attribute::new("style", "COLOR: rgb(255, 0, 0)"), true);
        let collection = collection_with("");
        let styles = resolve_indexed(&doc, &collection);
        let style = styles.get(&target).expect("computed style");
        assert_eq!(
            style.color,
            Color::new(255, 0, 0, 255),
            "COLOR is the color property"
        );
    }
    #[test]
    fn keyframes_are_collected_and_last_definition_wins() {
        let collection = collection_with(
            "@keyframes pulse { from { opacity: 0 } to { opacity: 1 } } \
             @keyframes pulse { from { opacity: 0.2 } to { opacity: 1 } } \
             @keyframes other { from { opacity: 0 } to { opacity: 1 } }",
        );
        let rule = collection.keyframes("pulse").expect("pulse keyframes");
        assert_eq!(rule.frames.len(), 2);
        assert_eq!(rule.frames[0].declarations[0].raw_value, "0.2");
        assert!(
            collection.keyframes("Pulse").is_none(),
            "animation names are case-sensitive"
        );
        assert!(collection.keyframes("other").is_some());
    }
}
