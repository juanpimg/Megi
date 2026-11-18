//! Sized submodule of the parent facade (verbatim move).

use super::*;


/// Parses a parenthesized condition group or feature (`(...)`, `((...) and (...))`).
pub(crate) fn parse_condition_group(parser: &mut MediaParser<'_>, depth: usize) -> Option<MediaCondition> {
    if !matches!(parser.peek(), Some(CssToken::LeftParen)) {
        return None;
    }
    parser.pos += 1;
    let inner = if matches!(parser.peek(), Some(CssToken::LeftParen)) || parser.peek_ident_is("not")
    {
        parse_condition(parser, depth + 1)?
    } else {
        parse_feature(parser)?
    };
    if !matches!(parser.peek(), Some(CssToken::RightParen)) {
        return None;
    }
    parser.pos += 1;
    Some(inner)
}


/// Parses a single feature inside parentheses, e.g. `min-width: 600px`.
pub(crate) fn parse_feature(parser: &mut MediaParser<'_>) -> Option<MediaCondition> {
    let name = parser.peek_ident()?.to_ascii_lowercase();
    parser.pos += 1;
    let mut value_tokens: Vec<CssToken> = Vec::new();
    if matches!(parser.peek(), Some(CssToken::Colon)) {
        parser.pos += 1;
        while let Some(token) = parser.peek() {
            if matches!(token, CssToken::RightParen) {
                break;
            }
            value_tokens.push(token.clone());
            parser.pos += 1;
        }
    }

    let feature = match name.as_str() {
        "min-width" => MediaFeature::MinWidth(parse_length_value(&value_tokens)?),
        "max-width" => MediaFeature::MaxWidth(parse_length_value(&value_tokens)?),
        "width" => MediaFeature::Width(parse_length_value(&value_tokens)?),
        "min-height" => MediaFeature::MinHeight(parse_length_value(&value_tokens)?),
        "max-height" => MediaFeature::MaxHeight(parse_length_value(&value_tokens)?),
        "height" => MediaFeature::Height(parse_length_value(&value_tokens)?),
        "orientation" => {
            let value = single_ident(&value_tokens)?.to_ascii_lowercase();
            MediaFeature::Orientation(match value.as_str() {
                "portrait" => Orientation::Portrait,
                "landscape" => Orientation::Landscape,
                _ => return None,
            })
        }
        "prefers-color-scheme" => {
            let value = single_ident(&value_tokens)?.to_ascii_lowercase();
            MediaFeature::PrefersColorScheme(match value.as_str() {
                "dark" => ColorScheme::Dark,
                _ => ColorScheme::Light,
            })
        }
        "prefers-reduced-motion" => {
            let value = single_ident(&value_tokens)?.to_ascii_lowercase();
            MediaFeature::PrefersReducedMotion(value == "reduce")
        }
        // Syntactically valid but not modelled: evaluate to false (documented).
        _ => MediaFeature::Unsupported,
    };
    Some(MediaCondition::Feature(feature))
}


/// Extracts the single identifier of a feature value (`(orientation: landscape)`).
pub(crate) fn single_ident(tokens: &[CssToken]) -> Option<&str> {
    match tokens {
        [CssToken::Ident(name)] => Some(name.as_str()),
        _ => None,
    }
}


/// Converts a media feature length token to CSS pixels.
///
/// Media query `em`/`rem` are relative to the initial font size (16px), not the document
/// font size (Media Queries 4 §1.3).
pub(crate) fn parse_length_value(tokens: &[CssToken]) -> Option<f32> {
    match tokens {
        [CssToken::Dimension { value, unit, .. }] => {
            let value = *value as f32;
            match unit.to_ascii_lowercase().as_str() {
                "px" => Some(value),
                "em" | "rem" => Some(value * 16.0),
                "pt" => Some(value * 96.0 / 72.0),
                "pc" => Some(value * 16.0),
                "in" => Some(value * 96.0),
                "cm" => Some(value * 96.0 / 2.54),
                "mm" => Some(value * 96.0 / 25.4),
                _ => None,
            }
        }
        [CssToken::Number { value, .. }] if *value == 0.0 => Some(0.0),
        _ => None,
    }
}


#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn evaluate_str(query: &str, context: &MediaContext) -> bool {
        evaluate(&parse_media_query_list(query), context)
    }

    fn screen(width: f32, height: f32) -> MediaContext {
        MediaContext::with_viewport(width, height)
    }

    #[test]
    fn deeply_nested_string_media_queries_do_not_overflow() {
        // `<style media>`/`matchMedia` parse raw strings without the parser's nesting cap.
        let nested = "(".repeat(100_000);
        let list = parse_media_query_list(&nested);
        assert!(!evaluate(&list, &screen(800.0, 600.0)));
    }

    #[test]
    fn test_parse_media_query_list_ast() {
        let list = parse_media_query_list("screen and (min-width: 600px), print");
        assert_eq!(list.queries.len(), 2);
        assert_eq!(list.queries[0].media_type, Some(MediaType::Screen));
        assert_eq!(
            list.queries[0].condition,
            Some(MediaCondition::Feature(MediaFeature::MinWidth(600.0)))
        );
        assert_eq!(list.queries[1].media_type, Some(MediaType::Print));
        assert_eq!(list.queries[1].condition, None);

        let not_only = parse_media_query_list("not print, only screen");
        assert_eq!(not_only.queries[0].qualifier, Some(MediaQualifier::Not));
        assert_eq!(not_only.queries[1].qualifier, Some(MediaQualifier::Only));
    }

    #[test]
    fn test_media_types_and_qualifiers() {
        let screen_ctx = screen(800.0, 600.0);
        let print_ctx = MediaContext {
            viewport: Some((800.0, 600.0)),
            screen: false,
            color_scheme: ColorScheme::Light,
            reduced_motion: false,
        };
        assert!(evaluate_str("screen", &screen_ctx));
        assert!(!evaluate_str("print", &screen_ctx));
        assert!(evaluate_str("print", &print_ctx));
        assert!(evaluate_str("all", &screen_ctx));
        assert!(!evaluate_str("speech", &screen_ctx));
        assert!(!evaluate_str("screen", &print_ctx));
        assert!(evaluate_str("not screen", &print_ctx));
        assert!(!evaluate_str("not screen", &screen_ctx));
        assert!(evaluate_str("only screen", &screen_ctx));
        assert!(!evaluate_str("only print", &screen_ctx));
        // Empty list matches everything, invalid queries never match.
        assert!(evaluate_str("", &screen_ctx));
        assert!(!evaluate_str("garbage(", &screen_ctx));
        assert!(evaluate_str("garbage, screen", &screen_ctx));
    }

    #[test]
    fn test_media_features() {
        let ctx = screen(800.0, 600.0);
        assert!(evaluate_str("(min-width: 600px)", &ctx));
        assert!(!evaluate_str("(min-width: 900px)", &ctx));
        assert!(evaluate_str("(max-width: 50em)", &ctx)); // 50em = 800px
        assert!(!evaluate_str("(max-width: 40em)", &ctx)); // 40em = 640px
        assert!(evaluate_str("(max-width: 40em)", &screen(600.0, 800.0)));
        assert!(evaluate_str("(min-height: 500px)", &ctx));
        assert!(evaluate_str("(max-height: 600px)", &ctx));
        assert!(!evaluate_str("(height: 500px)", &ctx));
        assert!(evaluate_str("(orientation: landscape)", &ctx));
        assert!(!evaluate_str("(orientation: portrait)", &ctx));
        assert!(evaluate_str(
            "(orientation: portrait)",
            &screen(600.0, 800.0)
        ));

        // Unmodelled features are valid syntax but never match.
        assert!(!evaluate_str("(resolution: 2dppx)", &ctx));
        assert!(!evaluate_str("(prefers-color-scheme: dark)", &ctx));
        assert!(!evaluate_str("(hover)", &ctx));

        // Features need a known viewport.
        let unknown = MediaContext::default();
        assert!(!evaluate_str("(min-width: 0px)", &unknown));
        assert!(!evaluate_str("(orientation: landscape)", &unknown));
        // ... but media types still work without a viewport.
        assert!(evaluate_str("screen", &unknown));
    }

    #[test]
    fn test_media_conditions_and_or_not() {
        let ctx = screen(800.0, 600.0);
        assert!(evaluate_str(
            "screen and (min-width: 600px) and (max-width: 900px)",
            &ctx
        ));
        assert!(!evaluate_str(
            "screen and (min-width: 600px) and (max-width: 700px)",
            &ctx
        ));
        assert!(evaluate_str(
            "screen and ((min-width: 600px) or (max-width: 700px))",
            &ctx
        ));
        assert!(evaluate_str(
            "(min-width: 600px) or (max-width: 100px)",
            &ctx
        ));
        assert!(!evaluate_str(
            "(min-width: 900px) or (max-width: 100px)",
            &ctx
        ));
        assert!(evaluate_str("not (min-width: 900px)", &ctx));
        assert!(!evaluate_str("not (min-width: 600px)", &ctx));
        assert!(evaluate_str(
            "not ((min-width: 900px) and (orientation: portrait))",
            &ctx
        ));
        assert!(evaluate_str("screen and (min-width: 600px), print", &ctx));
        assert!(!evaluate_str("print and (min-width: 600px), print", &ctx));
    }
}
