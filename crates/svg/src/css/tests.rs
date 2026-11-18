use super::*;
use crate::xml::{XmlElement, parse_document};

fn element(source: &str) -> XmlElement {
    parse_document(source.as_bytes(), &crate::Limits::default()).expect("xml")
}

#[test]
fn test_parse_rules_selectors_and_declarations() {
    let rules =
        parse_stylesheet("rect { fill: red } .card, #hero text { stroke: blue; stroke-width: 2 }");
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].declarations[0].name, "fill");
    assert_eq!(rules[1].selectors.len(), 2);
    assert_eq!(rules[1].declarations.len(), 2);
}

#[test]
fn test_unsupported_and_invalid_rules_are_skipped() {
    let rules = parse_stylesheet("@media screen { rect { fill: red } } rect:hover { fill: blue }");
    assert!(rules.is_empty());
    let rules = parse_stylesheet("/* comment */ rect { fill: green }");
    assert_eq!(rules.len(), 1);
}

#[test]
fn test_specificity_ordering() {
    let rules = parse_stylesheet("rect { fill: red } .a { fill: green } #x { fill: blue }");
    let root = element("<svg><rect id='x' class='a'/></svg>");
    let rect = match &root.children[0] {
        crate::xml::XmlNode::Element(element) => element,
        _ => unreachable!(),
    };
    let ancestors = vec![&root];
    let matched = matching_declarations(
        &rules,
        rect,
        &ancestors,
        &mut SelectorBudget {
            used: 0,
            max: usize::MAX,
        },
    )
    .expect("cascade");
    let colors: Vec<&str> = matched
        .iter()
        .map(|declaration| declaration.value)
        .collect();
    assert_eq!(colors, vec!["red", "green", "blue"]);
}

#[test]
fn test_child_combinator_and_selector_part_cap() {
    let rules = parse_stylesheet("svg > g > rect { fill: red }");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].selectors[0].parts.len(), 3);

    // Over-long selector (> MAX_SELECTOR_PARTS) is rejected without truncating.
    let long_selector = vec!["g"; MAX_SELECTOR_PARTS + 1].join(" > ");
    let css = format!("{long_selector} {{ fill: blue }}");
    assert!(parse_stylesheet(&css).is_empty());
}

#[test]
fn test_descendant_combinator_requires_ancestor() {
    let rules = parse_stylesheet("g rect { fill: red }");
    let root = element("<svg><g><rect class='a'/></g></svg>");
    let group = match &root.children[0] {
        crate::xml::XmlNode::Element(element) => element,
        _ => unreachable!(),
    };
    let rect = match &group.children[0] {
        crate::xml::XmlNode::Element(element) => element,
        _ => unreachable!(),
    };
    let ancestors = vec![&root, group];
    assert!(
        !matching_declarations(
            &rules,
            rect,
            &ancestors,
            &mut SelectorBudget {
                used: 0,
                max: usize::MAX,
            },
        )
        .expect("cascade")
        .is_empty()
    );
    // Without the `g` ancestor the selector must not match.
    let root2 = element("<svg><rect class='a'/></svg>");
    let rect2 = match &root2.children[0] {
        crate::xml::XmlNode::Element(element) => element,
        _ => unreachable!(),
    };
    assert!(
        matching_declarations(
            &rules,
            rect2,
            &[&root2],
            &mut SelectorBudget {
                used: 0,
                max: usize::MAX,
            },
        )
        .expect("cascade")
        .is_empty()
    );
}
