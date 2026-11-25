//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;
#[path = "chunk2/supports.rs"]
mod supports;

/// Compiles one style rule (selectors + expanded declarations) without assigning a source
/// order; callers set `source_order` before inserting it.
pub(crate) fn compile_style_rule(
    style_rule: &crate::rule::StyleRule,
    origin: Origin,
) -> Option<CompiledRule> {
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
    /// `@media` rules are evaluated against [`StyleCollection::media`] and `@supports`
    /// conditions against the declarations this engine parses; other at-rules
    /// (`@import`, `@font-face`, ...) are still ignored.
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
    pub(crate) fn register_layer(&mut self, path: Vec<String>) -> u32 {
        if let Some(&rank) = self.layer_index.get(&path) {
            return rank;
        }
        let rank = self.layers.len() as u32;
        self.layers.push(path.clone());
        self.layer_index.insert(path, rank);
        rank
    }

    /// Composes a possibly dotted layer name relative to the current layer path.
    pub(crate) fn compose_layer_path(current: &[String], name: &str) -> Vec<String> {
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

    pub(crate) fn add_rules(
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
                Rule::At(at_rule) if at_rule.name.eq_ignore_ascii_case("supports") => {
                    if supports::condition(&at_rule.prelude)
                        && let Some(block) = &at_rule.block
                    {
                        let nested_text = serialize_component_values(&block.values);
                        let nested = crate::parser::parse_stylesheet(&nested_text);
                        self.add_rules(&nested.rules, origin, layer_path, layer_rank);
                    }
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

    fn record_rule_flags(&mut self, compiled: &CompiledRule) {
        for decl in &compiled.declarations {
            if is_revert_layer(decl) {
                self.has_revert_layer = true;
            }
            if decl.name == "animation" || decl.name.starts_with("animation-") {
                self.has_animations = true;
            } else if decl.name == "transition" || decl.name.starts_with("transition-") {
                self.has_transitions = true;
            }
        }
        if compiled
            .selectors
            .selectors
            .iter()
            .any(|s| s.pseudo_element().is_some())
        {
            self.has_pseudo_rules = true;
        }
    }

    /// Compiles the style rules inside an `@container` block (nested `@container`/`@layer`
    /// at-rules are not supported; `@media` inside is evaluated immediately).
    pub(crate) fn add_container_rules(
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
                        self.record_rule_flags(&compiled);
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

    pub(crate) fn add_style_rule(
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
        if compiled
            .selectors
            .selectors
            .iter()
            .any(ComplexSelector::has_backdrop)
        {
            let mut backdrop_selectors = Vec::new();
            let mut normal_selectors = Vec::new();
            for selector in compiled.selectors.selectors.drain(..) {
                if selector.has_backdrop() {
                    backdrop_selectors.push(selector);
                } else {
                    normal_selectors.push(selector);
                }
            }
            self.backdrop_rules.push(CompiledRule {
                selectors: SelectorList {
                    selectors: backdrop_selectors,
                },
                declarations: compiled.declarations.clone(),
                origin: compiled.origin,
                source_order: compiled.source_order,
                layer_rank: compiled.layer_rank,
            });
            if normal_selectors.is_empty() {
                return;
            }
            compiled.selectors.selectors = normal_selectors;
        }
        let rule_index = self.rules.len() as u32;
        self.index.insert(rule_index, &compiled.selectors);
        self.record_rule_flags(&compiled);
        self.rules.push(compiled);
    }

    /// Returns true if pseudo-element rules may be present (or `rules` was mutated directly).
    pub fn may_have_pseudo_rules(&self) -> bool {
        self.has_pseudo_rules || self.index.indexed_rules != self.rules.len()
    }

    /// Returns true if any rule may declare `animation*` properties (or `rules` was mutated directly).
    pub fn may_have_animations(&self) -> bool {
        self.has_animations || self.index.indexed_rules != self.rules.len()
    }

    /// Returns true if any rule may declare `transition*` properties (or `rules` was mutated directly).
    pub fn may_have_transitions(&self) -> bool {
        self.has_transitions || self.index.indexed_rules != self.rules.len()
    }

    /// Returns the candidate selectors that can possibly match `node_id`, in ascending
    /// (source) order.
    ///
    /// The result is a superset of the matching selectors: rules keyed by id/class/tag whose
    /// key the element does not carry are skipped, while universal/slow selectors are always
    /// included.
    pub(crate) fn candidate_selectors(
        &self,
        doc: &Document,
        node_id: NodeId,
        out: &mut Vec<Candidate>,
    ) {
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
#[inline]
pub(crate) fn selector_matches(
    selector: &ComplexSelector,
    doc: &Document,
    node_id: NodeId,
    pseudo: Option<PseudoElement>,
) -> bool {
    match pseudo {
        Some(p) => selector.matches_for_pseudo(doc, node_id, p),
        None => selector.matches(doc, node_id),
    }
}

pub fn resolve_cascaded_properties<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
) -> HashMap<String, &'a Declaration> {
    let mut candidates = Vec::new();
    collection.candidate_selectors(doc, node_id, &mut candidates);
    let mut best = resolve_base_cascaded_properties_for(
        doc,
        node_id,
        None,
        collection,
        inline_declarations,
        &candidates,
    );
    merge_container_rules_for(doc, node_id, None, collection, &mut best);
    apply_revert_for(doc, node_id, None, collection, &candidates, &mut best);
    apply_revert_layer_for(doc, node_id, None, collection, &candidates, &mut best);
    owned_declarations(best)
}

/// Resolves the cascaded property declarations for a pseudo-element (`::before`/`::after`)
/// targeting originating element `node_id`.
pub fn resolve_pseudo_cascaded_properties<'a>(
    doc: &Document,
    node_id: NodeId,
    pseudo: PseudoElement,
    collection: &'a StyleCollection,
) -> HashMap<String, &'a Declaration> {
    if !collection.may_have_pseudo_rules() {
        return HashMap::new();
    }
    let mut candidates = Vec::new();
    collection.candidate_selectors(doc, node_id, &mut candidates);
    resolve_pseudo_cascaded_with_candidates(doc, node_id, pseudo, collection, &candidates)
}
