//! #2317: DOM attribute getters must see namespaced attributes by qualified name.

use megi_core::{Attribute, ElementData, Namespace};

#[test]
fn namespaced_attribute_is_visible_by_qualified_name() {
    let mut element = ElementData::new(Namespace::Svg, "use");
    assert!(element.set_attribute(
        Attribute::with_namespace(Namespace::XLink, Some("xlink".to_owned()), "href", "#icon",),
        true
    ));

    // WHATWG DOM §4.9.2: getAttribute/hasAttribute/removeAttribute match the
    // qualified name (`xlink:href`), regardless of the attribute's namespace.
    assert_eq!(element.get_attribute("xlink:href"), Some("#icon"));
    assert!(element.has_attribute("xlink:href"));
    assert_eq!(
        element.remove_attribute("xlink:href"),
        Some("#icon".to_owned())
    );
    assert_eq!(element.get_attribute("xlink:href"), None);
    assert!(!element.has_attribute("xlink:href"));
}

#[test]
fn namespaced_attribute_does_not_match_its_local_name() {
    let mut element = ElementData::new(Namespace::Svg, "use");
    assert!(element.set_attribute(
        Attribute::with_namespace(Namespace::XLink, Some("xlink".to_owned()), "href", "#icon",),
        true
    ));
    // The qualified name is `xlink:href`; `href` alone must not match it.
    assert_eq!(element.get_attribute("href"), None);
    assert!(!element.has_attribute("href"));
}

#[test]
fn html_attribute_case_insensitivity_is_preserved() {
    let mut element = ElementData::new(Namespace::Html, "div");
    assert!(element.set_attribute(Attribute::new("ID", "main"), true));
    // HTML elements compare attribute names ASCII case-insensitively (#686).
    assert_eq!(element.get_attribute("id"), Some("main"));
    assert!(element.has_attribute("Id"));
    assert_eq!(element.remove_attribute("ID"), Some("main".to_owned()));

    // A namespaced attribute on an HTML element is also matched case-insensitively.
    assert!(element.set_attribute(
        Attribute::with_namespace(Namespace::XLink, Some("xlink".to_owned()), "HREF", "#frag",),
        true
    ));
    assert_eq!(element.get_attribute("xlink:href"), Some("#frag"));
}
