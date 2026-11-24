//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::properties::{Color, ComputedStyle};
    use crate::rule::Declaration;
    use megi_core::{Attribute, Namespace};

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
        element.get_attribute("style").map(parse_inline_style)
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

    #[test]
    fn type_selectors_match_case_insensitively_through_rule_index() {
        // HTML type selectors are ASCII case-insensitive (Selectors §4). The index must
        // not silently drop `DIV { … }` for a lowercase <div> (issue #1127), or `div` for
        // an uppercase <DIV>. Compare against the unindexed reference path as well.
        for (selector, tag) in [("DIV", "div"), ("div", "DIV")] {
            let mut doc = Document::new();
            let html = doc.create_element(Namespace::Html, "html", vec![]);
            let target = doc.create_element(Namespace::Html, tag, vec![]);
            doc.append_child(doc.root(), html);
            doc.append_child(html, target);

            let collection = collection_with(&format!("{selector} {{ color: rgb(1, 2, 3); }}"));
            let indexed = resolve_indexed(&doc, &collection);
            let bruteforce = resolve_bruteforce(&doc, &collection);
            let expected = Color::new(1, 2, 3, 255);

            assert_eq!(
                indexed.get(&target).expect("indexed style").color,
                expected,
                "indexed path: `{selector}` should match <{tag}>"
            );
            assert_eq!(
                bruteforce.get(&target).expect("bruteforce style").color,
                expected,
                "bruteforce path: `{selector}` should match <{tag}>"
            );
        }
    }

    #[test]
    fn oversized_inline_style_with_multibyte_char_at_cap_does_not_panic() {
        // Regression test for issue #1126: truncating an inline `style` attribute at byte
        // 64*1024 used to slice through a multi-byte code point and panic (remote DoS).
        let prefix = "color: red; ";
        let mut style = String::from(prefix);
        // Pad so the two-byte 'é' starts at byte 65535 and straddles the 65536 cap.
        style.push_str(&" ".repeat(64 * 1024 - prefix.len() - 1));
        style.push('é');
        assert_eq!(style.len(), 64 * 1024 + 1);
        assert!(!style.is_char_boundary(64 * 1024));

        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let target =
            doc.create_element(Namespace::Html, "div", vec![Attribute::new("style", style)]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, target);

        // Before the fix this call panicked with "end byte index 65536 is not a char boundary".
        let styles = resolve_document_styles(&doc, None);
        let computed = styles.get(&target).expect("computed style for target");
        assert_eq!(
            computed.color,
            Color::new(255, 0, 0, 255),
            "the declaration before the cap must survive the boundary-safe truncation"
        );
    }

    #[test]
    fn parse_inline_style_handles_four_byte_char_at_cap_and_container_base() {
        let prefix = "container-type: inline-size; width: 320px; color: rgb(12, 34, 56); ";
        let mut style = String::from(prefix);
        style.push_str(&" ".repeat(MAX_INLINE_STYLE_BYTES - prefix.len() - 2));
        style.push('🦀');
        style.push_str("; color: rgb(255, 0, 0);");
        assert!(!style.is_char_boundary(MAX_INLINE_STYLE_BYTES));

        let decls = parse_inline_declarations(&style);
        assert!(
            decls
                .iter()
                .any(|d| d.name == "color" && d.raw_value == "rgb(12, 34, 56)")
        );
        assert!(!decls.iter().any(|d| d.raw_value == "rgb(255, 0, 0)"));

        let mut doc = Document::new();
        let html = doc.create_element(Namespace::Html, "html", vec![]);
        let container =
            doc.create_element(Namespace::Html, "div", vec![Attribute::new("style", style)]);
        let child = doc.create_element(Namespace::Html, "span", vec![]);
        doc.append_child(doc.root(), html);
        doc.append_child(html, container);
        doc.append_child(container, child);

        let css = "@container (min-width: 200px) { span { color: rgb(7, 89, 10); } }";
        let styles = resolve_document_styles_with_viewport(&doc, Some(css), 800.0, 600.0);
        assert_eq!(
            styles.get(&container).expect("container style").color,
            Color::new(12, 34, 56, 255)
        );
        assert_eq!(
            styles.get(&child).expect("child @container style").color,
            Color::new(7, 89, 10, 255)
        );
    }

    /// CSS 2.2 §11.2: `visibility` is inherited, and the style-resolution cache key
    /// must preserve all 128 bits of the parent key. Folding the two halves with XOR
    /// (and deriving `high` from `low`) let two distinct parents with the same
    /// `lo ^ hi` alias to one cached `ComputedStyle`, which is why Wikipedia rendered
    /// blank: a hidden dropdown's style leaked into unrelated article containers.
    #[test]
    fn test_style_cache_key_must_not_alias_distinct_parent_keys() {
        // Same XOR of halves, different 128-bit parent keys: these currently alias.
        let p1 = 0x1111_1111_1111_1111_2222_2222_2222_2222u128;
        let p2 = 0x2222_2222_2222_2222_1111_1111_1111_1111u128;
        assert_ne!(p1, p2);
        assert_eq!(p1 as u64 ^ (p1 >> 64) as u64, p2 as u64 ^ (p2 >> 64) as u64);

        let decls = parse_inline_style("display:block");
        let cascaded: std::collections::HashMap<String, &crate::rule::Declaration> =
            decls.iter().map(|d| (d.name.clone(), d)).collect();

        assert_ne!(
            style_cache_key(p1, &cascaded, false),
            style_cache_key(p2, &cascaded, false),
            "distinct parent keys must never produce the same style-cache key"
        );
    }
}
