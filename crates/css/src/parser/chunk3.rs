//! Sized submodule of the parent facade (verbatim move).

#[allow(unused_imports)]
use super::*;

#[cfg(test)]
pub(crate) mod serialization_tests {
    use super::*;

    #[test]
    fn string_serialization_keeps_escapes_inside_the_string() {
        let declarations = parse_list_of_declarations(r#"content: "a\"b";"#);
        assert_eq!(declarations[0].raw_value, r#""a\"b""#);
    }
}

#[cfg(test)]
pub(crate) mod nesting_depth_tests {
    use super::*;
    use crate::rule::Rule;

    /// Returns the maximum depth of nested child style rules.
    fn max_rule_depth(rules: &[Rule]) -> usize {
        rules
            .iter()
            .map(|rule| match rule {
                Rule::Style(style) => 1 + max_rule_depth(&style.child_rules),
                Rule::At(_) => 1,
            })
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn excessive_rule_nesting_is_capped() {
        // Unbounded `a{a{a{…` recursion (#1412) used to blow the 8 MB stack with ~26 k levels.
        // A moderate 200 levels is safe to parse, but the resulting tree must be capped; before
        // the fix max_rule_depth was 200, so this assertion failed.
        let input = format!("{}color:red{}", "a{".repeat(200), "}".repeat(200));
        let sheet = parse_stylesheet(&input);
        assert!(
            max_rule_depth(&sheet.rules) <= MAX_RULE_NESTING,
            "nesting must be capped at {MAX_RULE_NESTING}, got {}",
            max_rule_depth(&sheet.rules)
        );

        // The issue's exact shape: ~200 KB of `a{` used to abort the process with a stack
        // overflow. With the cap it parses to completion (and stays bounded).
        let huge = format!("{}color:red{}", "a{".repeat(100_000), "}".repeat(100_000));
        let huge_sheet = parse_stylesheet(&huge);
        assert!(max_rule_depth(&huge_sheet.rules) <= MAX_RULE_NESTING);
    }

    #[test]
    fn nesting_cap_keeps_sibling_rules_and_declarations() {
        // Past the cap the deep block is dropped, but outer declarations and later siblings
        // must still parse — the cap is a bound, not a whole-sheet abort.
        let deep = format!("{}color:blue{}", "a{".repeat(100), "}".repeat(100));
        let input = format!("a{{color:red}}{deep} b{{color:green}}");
        let sheet = parse_stylesheet(&input);
        let rules = &sheet.rules;
        assert!(
            rules.len() >= 2,
            "outer siblings must survive the nesting cap, got {} rules",
            rules.len()
        );
        let first = match &rules[0] {
            Rule::Style(style) => style,
            other => panic!("expected style rule, got {other:?}"),
        };
        assert_eq!(first.declarations[0].name, "color");
    }
}

#[cfg(test)]
pub(crate) mod error_recovery_tests {
    use super::*;
    use crate::rule::Rule;

    #[test]
    fn invalid_star_hack_declaration_keeps_following_top_level_rules() {
        // The legacy IE star hack `*zoom:1` is an invalid declaration that browsers
        // silently ignore. It must not make the parser treat following top-level rules
        // as nested children of the current rule (CSS Syntax §5.5.1 error recovery).
        let sheet = parse_stylesheet(".cf{*zoom:1} .list{list-style-type:none}");
        assert_eq!(
            sheet.rules.len(),
            2,
            "star hack must not swallow the following top-level rule; got {:?}",
            sheet.rules
        );
        let second = match &sheet.rules[1] {
            Rule::Style(style) => style,
            other => panic!("expected style rule, got {other:?}"),
        };
        assert!(
            second
                .declarations
                .iter()
                .any(|decl| decl.name == "list-style-type"),
            ".list must survive with its list-style-type declaration"
        );
        assert_eq!(
            second.child_rules.len(),
            0,
            ".list must be parsed as a top-level rule, not a nested child of .cf"
        );
    }
}

#[cfg(test)]
pub(crate) mod keyframes_tests {
    use super::*;

    #[test]
    fn parses_keyframes_frames_and_offsets() {
        let sheet = parse_stylesheet(
            "@keyframes fade { from { opacity: 0 } 50%, 75% { opacity: 0.5 } to { opacity: 1 } }",
        );
        assert_eq!(sheet.keyframes.len(), 1);
        let rule = &sheet.keyframes[0];
        assert_eq!(rule.name, "fade");
        assert_eq!(rule.frames.len(), 3);
        assert_eq!(rule.frames[0].offsets, vec![0.0]);
        assert_eq!(rule.frames[1].offsets, vec![0.5, 0.75]);
        assert_eq!(rule.frames[2].offsets, vec![1.0]);
        assert_eq!(rule.frames[0].declarations[0].name, "opacity");
    }

    #[test]
    fn keyframes_are_forgiving_and_webkit_prefixed() {
        let sheet = parse_stylesheet(
            "@-webkit-keyframes spin { bogus { opacity: 1 } 0% { opacity: 0 } to { opacity: 1 } }",
        );
        assert_eq!(sheet.keyframes.len(), 1);
        assert_eq!(sheet.keyframes[0].frames.len(), 2);
        assert_eq!(sheet.keyframes[0].frames[0].offsets, vec![0.0]);
        // A rule without a block or name is not a keyframes rule.
        assert!(parse_stylesheet("@keyframes none").keyframes.is_empty());
        assert!(
            parse_stylesheet("@keyframes { from { opacity: 0 } }")
                .keyframes
                .is_empty()
        );
        // Out-of-range percentages are invalid selectors and are skipped.
        assert!(
            parse_stylesheet("@keyframes bad { 120% { opacity: 0 } }")
                .keyframes
                .is_empty()
        );
    }
}
