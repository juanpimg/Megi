//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

// ---------------------------------------------------------------------------
// List-style shorthand
// ---------------------------------------------------------------------------

/// Expands `list-style` to the modelled `list-style-type` and `list-style-image` longhands.
///
/// `none` resets both longhands to `none`; omitted components reset to their initial values
/// (`disc` / `none`). `list-style-position` is not modelled and is ignored.
pub(crate) fn expand_list_style(value: &str) -> Option<Vec<(String, String)>> {
    let components = split_components(value);
    if components.is_empty() {
        return None;
    }
    let mut list_type = None;
    let mut image = None;
    for component in &components {
        let lower = component.trim().to_ascii_lowercase();
        if is_css_wide_keyword(component) {
            return Some(Vec::new());
        }
        if is_image_component(component) {
            if image.replace(component.clone()).is_some() {
                return Some(Vec::new());
            }
        } else if ListStyleType::parse(component).is_some() {
            if list_type.replace(component.clone()).is_some() {
                return Some(Vec::new());
            }
        } else if lower == "inside" || lower == "outside" {
            // `list-style-position` is not modelled yet.
        } else {
            return Some(Vec::new());
        }
    }
    Some(vec![
        (
            "list-style-type".to_string(),
            list_type.unwrap_or_else(|| "disc".to_string()),
        ),
        (
            "list-style-image".to_string(),
            image.unwrap_or_else(|| "none".to_string()),
        ),
    ])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::parser::parse_list_of_declarations;

    fn expand(css: &str) -> Vec<Declaration> {
        expand_declarations(&parse_list_of_declarations(css))
    }

    fn get<'a>(declarations: &'a [Declaration], name: &str) -> Option<&'a str> {
        declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .map(|declaration| declaration.raw_value.as_str())
    }

    #[test]
    fn test_expand_border_shorthand_to_twelve_longhands() {
        let expanded = expand("border: 2px solid red;");
        assert_eq!(expanded.len(), 12);
        for side in SIDES {
            assert_eq!(get(&expanded, &format!("border-{side}-width")), Some("2px"));
            assert_eq!(
                get(&expanded, &format!("border-{side}-style")),
                Some("solid")
            );
            assert_eq!(get(&expanded, &format!("border-{side}-color")), Some("red"));
        }
    }

    #[test]
    fn test_font_shorthand_rejects_a_dangling_slash() {
        assert!(expand("font: 16px /").is_empty());
        assert!(expand("font: 16px /;").is_empty());
        assert!(!expand("font: bold 16px/1.5 sans-serif;").is_empty());
    }

    #[test]
    fn test_expand_border_defaults_and_invalid() {
        // Omitted components reset to their initial values (medium / none / currentColor).
        let expanded = expand("border: solid;");
        assert_eq!(get(&expanded, "border-top-width"), Some("medium"));
        assert_eq!(get(&expanded, "border-top-style"), Some("solid"));
        assert_eq!(get(&expanded, "border-top-color"), Some("currentColor"));

        // Duplicated slot invalidates the shorthand: every longhand is dropped.
        assert!(expand("border: 1px solid red green;").is_empty());
    }

    #[test]
    fn test_expand_margin_and_logical_edges() {
        let expanded = expand("margin: 10px 20px;");
        assert_eq!(get(&expanded, "margin-top"), Some("10px"));
        assert_eq!(get(&expanded, "margin-right"), Some("20px"));
        assert_eq!(get(&expanded, "margin-bottom"), Some("10px"));
        assert_eq!(get(&expanded, "margin-left"), Some("20px"));

        let expanded = expand("padding-block: 1em 2em;");
        assert_eq!(get(&expanded, "padding-top"), Some("1em"));
        assert_eq!(get(&expanded, "padding-bottom"), Some("2em"));
        assert_eq!(get(&expanded, "padding-left"), None);

        // `padding: auto` / negative padding are invalid and drop the declaration.
        assert!(expand("padding: auto;").is_empty());
        assert!(expand("padding: -2px;").is_empty());
    }

    #[test]
    fn test_expand_font_shorthand() {
        let expanded = expand("font: italic bold 20px/1.5 \"Helvetica Neue\", Arial, sans-serif;");
        assert_eq!(get(&expanded, "font-style"), Some("italic"));
        assert_eq!(get(&expanded, "font-weight"), Some("bold"));
        assert_eq!(get(&expanded, "font-size"), Some("20px"));
        assert_eq!(get(&expanded, "line-height"), Some("1.5"));
        assert_eq!(
            get(&expanded, "font-family"),
            Some("\"Helvetica Neue\", Arial, sans-serif")
        );

        // Reset semantics: omitted slots become their initial value.
        let expanded = expand("font: 12px serif;");
        assert_eq!(get(&expanded, "font-style"), Some("normal"));
        assert_eq!(get(&expanded, "font-weight"), Some("normal"));
        assert_eq!(get(&expanded, "line-height"), Some("normal"));
        assert_eq!(get(&expanded, "font-family"), Some("serif"));

        // Invalid shorthands (missing family, system fonts) are dropped.
        assert!(expand("font: 12px;").is_empty());
        assert!(expand("font: menu;").is_empty());
    }

    #[test]
    fn test_expand_background_and_list_style() {
        let expanded = expand("background: red none;");
        assert_eq!(get(&expanded, "background-color"), Some("red"));
        assert_eq!(get(&expanded, "background-image"), Some("none"));

        let expanded = expand("background: url(bg.png) no-repeat #0f0;");
        assert_eq!(get(&expanded, "background-color"), Some("#0f0"));
        assert_eq!(get(&expanded, "background-image"), Some("url(bg.png)"));

        let expanded = expand("background: none;");
        assert_eq!(get(&expanded, "background-color"), Some("transparent"));
        assert_eq!(get(&expanded, "background-image"), Some("none"));

        let expanded = expand("list-style: none inside;");
        assert_eq!(get(&expanded, "list-style-type"), Some("none"));
        assert_eq!(get(&expanded, "list-style-image"), Some("none"));
        let expanded = expand("list-style: square;");
        assert_eq!(get(&expanded, "list-style-type"), Some("square"));
        assert_eq!(get(&expanded, "list-style-image"), Some("none"));
        let expanded = expand("list-style: url(bullet.png) square;");
        assert_eq!(get(&expanded, "list-style-type"), Some("square"));
        assert_eq!(get(&expanded, "list-style-image"), Some("url(bullet.png)"));
    }
    #[test]
    fn animation_shorthand_expands_to_cycles_and_defaults() {
        let expanded = expand("animation: 2s linear 1s infinite alternate fade, 1s ease-in slide;");
        assert_eq!(get(&expanded, "animation-name"), Some("fade, slide"));
        assert_eq!(get(&expanded, "animation-duration"), Some("2s, 1s"));
        assert_eq!(
            get(&expanded, "animation-timing-function"),
            Some("linear, ease-in")
        );
        assert_eq!(get(&expanded, "animation-delay"), Some("1s, 0s"));
        assert_eq!(
            get(&expanded, "animation-iteration-count"),
            Some("infinite, 1")
        );
        assert_eq!(
            get(&expanded, "animation-direction"),
            Some("alternate, normal")
        );
        assert_eq!(get(&expanded, "animation-fill-mode"), Some("none, none"));
        assert_eq!(
            get(&expanded, "animation-play-state"),
            Some("running, running")
        );
    }

    #[test]
    fn transition_shorthand_expands_with_defaults() {
        let expanded = expand("transition: opacity 300ms ease-out 50ms, transform 200ms;");
        assert_eq!(
            get(&expanded, "transition-property"),
            Some("opacity, transform")
        );
        assert_eq!(get(&expanded, "transition-duration"), Some("300ms, 200ms"));
        assert_eq!(
            get(&expanded, "transition-timing-function"),
            Some("ease-out, ease")
        );
        assert_eq!(get(&expanded, "transition-delay"), Some("50ms, 0s"));
    }

    #[test]
    fn invalid_timeline_shorthands_are_dropped() {
        // Three times in one entry is invalid grammar.
        assert!(expand("animation: 1s 2s 3s fade;").is_empty());
        // An unknown component cannot be both name and property.
        assert!(expand("transition: opacity transform 1s;").is_empty());
    }
}
