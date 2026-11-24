//! Sized submodule of the parent facade (verbatim move).

use super::*;

/// Parses a parenthesized condition group or feature (`(...)`, `((...) and (...))`).
pub(crate) fn parse_condition_group(
    parser: &mut MediaParser<'_>,
    depth: usize,
) -> Option<MediaCondition> {
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

/// Comparison operator of a media feature, from range syntax or a colon.
#[derive(Clone, Copy, PartialEq)]
enum RangeOp {
    Exact,
    Min,
    Max,
    Gt,
    Lt,
}

/// Parses a single feature inside parentheses, e.g. `min-width: 600px` or
/// `width >= calc(10rem + 2px)` (Media Queries 4 §4.2, §8).
pub(crate) fn parse_feature(parser: &mut MediaParser<'_>) -> Option<MediaCondition> {
    let name = parser.peek_ident()?.to_ascii_lowercase();
    parser.pos += 1;

    // Collect every token up to the feature's closing ')'. Nested calc()/group
    // parentheses must be consumed too, so track depth: a Function token or a
    // LeftParen opens a group, a RightParen closes one. Whitespace was already
    // filtered by parse_media_query_list.
    let mut value_tokens: Vec<CssToken> = Vec::new();
    let mut depth = 0usize;
    while let Some(token) = parser.peek() {
        match token {
            CssToken::RightParen if depth == 0 => break,
            CssToken::RightParen => {
                depth -= 1;
                value_tokens.push(token.clone());
                parser.pos += 1;
            }
            CssToken::Function(_) | CssToken::LeftParen => {
                depth += 1;
                value_tokens.push(token.clone());
                parser.pos += 1;
            }
            _ => {
                value_tokens.push(token.clone());
                parser.pos += 1;
            }
        }
    }

    // Determine operator and the value slice that follows it.
    let (value, op): (&[CssToken], RangeOp) = match value_tokens.first() {
        Some(CssToken::Colon) => (&value_tokens[1..], RangeOp::Exact),
        Some(CssToken::Delim('>')) => {
            if matches!(value_tokens.get(1), Some(CssToken::Delim('='))) {
                (&value_tokens[2..], RangeOp::Min)
            } else {
                (&value_tokens[1..], RangeOp::Gt)
            }
        }
        Some(CssToken::Delim('<')) => {
            if matches!(value_tokens.get(1), Some(CssToken::Delim('='))) {
                (&value_tokens[2..], RangeOp::Max)
            } else {
                (&value_tokens[1..], RangeOp::Lt)
            }
        }
        _ => (&value_tokens[..], RangeOp::Exact),
    };

    // Range syntax only applies to the `width`/`height` features.
    if op != RangeOp::Exact && name != "width" && name != "height" {
        return None;
    }

    let feature = match name.as_str() {
        "min-width" => MediaFeature::MinWidth(parse_length_value(value)?),
        "max-width" => MediaFeature::MaxWidth(parse_length_value(value)?),
        "width" => match op {
            RangeOp::Exact => MediaFeature::Width(parse_length_value(value)?),
            RangeOp::Min => MediaFeature::MinWidth(parse_length_value(value)?),
            RangeOp::Max => MediaFeature::MaxWidth(parse_length_value(value)?),
            RangeOp::Gt => MediaFeature::WidthGreaterThan(parse_length_value(value)?),
            RangeOp::Lt => MediaFeature::WidthLessThan(parse_length_value(value)?),
        },
        "min-height" => MediaFeature::MinHeight(parse_length_value(value)?),
        "max-height" => MediaFeature::MaxHeight(parse_length_value(value)?),
        "height" => match op {
            RangeOp::Exact => MediaFeature::Height(parse_length_value(value)?),
            RangeOp::Min => MediaFeature::MinHeight(parse_length_value(value)?),
            RangeOp::Max => MediaFeature::MaxHeight(parse_length_value(value)?),
            RangeOp::Gt => MediaFeature::HeightGreaterThan(parse_length_value(value)?),
            RangeOp::Lt => MediaFeature::HeightLessThan(parse_length_value(value)?),
        },
        "orientation" => {
            let value = single_ident(value)?.to_ascii_lowercase();
            MediaFeature::Orientation(match value.as_str() {
                "portrait" => Orientation::Portrait,
                "landscape" => Orientation::Landscape,
                _ => return None,
            })
        }
        "prefers-color-scheme" => {
            let value = single_ident(value)?.to_ascii_lowercase();
            MediaFeature::PrefersColorScheme(match value.as_str() {
                "light" => ColorScheme::Light,
                "dark" => ColorScheme::Dark,
                // #1842: an invalid value makes the feature evaluate to false.
                _ => return None,
            })
        }
        "prefers-reduced-motion" => {
            let value = single_ident(value)?.to_ascii_lowercase();
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
/// font size (Media Queries 4 §1.3). `calc()`/math functions (Media Queries 4 §8) are
/// evaluated with a 16px font size and the current viewport; a percentage result is
/// invalid in a media feature length.
pub(crate) fn parse_length_value(tokens: &[CssToken]) -> Option<f32> {
    // `calc()`, `min()`, `max()` and `clamp()` arrive as a Function token followed
    // by its (nested) argument tokens. Re-serialize and evaluate them.
    if matches!(tokens.first(), Some(CssToken::Function(_))) {
        let text = serialize_tokens(tokens);
        let (viewport_width, viewport_height) = crate::calc::viewport();
        let context = crate::CalcContext {
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width,
            viewport_height,
        };
        let value = crate::evaluate_calc(&text, &context)?;
        return (value.percent == 0.0).then_some(value.px);
    }
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

/// Re-serializes a token slice back into a CSS string suitable for the `calc()`
/// evaluator. Operators get surrounding spaces so the byte-based calc parser
/// never mistakes a following `+`/`-` for a numeric sign.
fn serialize_tokens(tokens: &[CssToken]) -> String {
    let mut out = String::new();
    for token in tokens {
        match token {
            CssToken::Ident(name) => out.push_str(name),
            CssToken::Function(name) => {
                out.push_str(name);
                out.push('(');
            }
            CssToken::Dimension { value, unit, .. } => {
                out.push_str(&value.to_string());
                out.push_str(unit);
            }
            CssToken::Number { value, .. } => out.push_str(&value.to_string()),
            CssToken::Percentage { value, .. } => {
                out.push_str(&value.to_string());
                out.push('%');
            }
            CssToken::Delim(c) => {
                out.push(' ');
                out.push(*c);
                out.push(' ');
            }
            CssToken::LeftParen => out.push('('),
            CssToken::RightParen => out.push(')'),
            CssToken::Whitespace => out.push(' '),
            _ => {}
        }
    }
    out
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
    fn invalid_prefers_color_scheme_value_is_never_true() {
        // #1842: an invalid value makes the media feature evaluate to false,
        // regardless of the user's color scheme (Media Queries 4 §5.10).
        use crate::media::ColorScheme;
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            let ctx = MediaContext {
                color_scheme: scheme,
                ..MediaContext::default()
            };
            assert!(!evaluate_str("(prefers-color-scheme: bogus)", &ctx));
            assert!(!evaluate_str("(prefers-color-scheme: )", &ctx));
            // Valid values keep working under either scheme.
            assert_eq!(
                evaluate_str("(prefers-color-scheme: light)", &ctx),
                scheme == ColorScheme::Light
            );
            assert_eq!(
                evaluate_str("(prefers-color-scheme: dark)", &ctx),
                scheme == ColorScheme::Dark
            );
        }
    }

    #[test]
    fn preference_media_features_work_without_a_viewport() {
        // #1756: user preferences do not depend on viewport dimensions.
        use crate::media::ColorScheme;
        let ctx = MediaContext {
            color_scheme: ColorScheme::Dark,
            reduced_motion: true,
            ..MediaContext::default()
        };
        assert!(evaluate_str("(prefers-color-scheme: dark)", &ctx));
        assert!(evaluate_str("(prefers-reduced-motion: reduce)", &ctx));
        assert!(!evaluate_str("(prefers-color-scheme: light)", &ctx));
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

    #[test]
    fn media_range_syntax_and_calc_are_supported() {
        // Media Queries 4 §4.2: range syntax `(width >= <length>)` etc.; §8:
        // `calc()` is allowed in media feature values. Real sites (MDN) gate
        // their two-column layout on queries like
        // `(width >= calc(1rem * 2 + (15rem + 2rem) * 2 + 31rem))`.
        let ctx = screen(1024.0, 900.0);
        assert!(evaluate_str("(width >= 600px)", &ctx));
        assert!(!evaluate_str("(width >= 1200px)", &ctx));
        assert!(evaluate_str("(width <= 1024px)", &ctx));
        assert!(!evaluate_str("(width <= 1000px)", &ctx));
        assert!(evaluate_str("(width > 1000px)", &ctx));
        assert!(!evaluate_str("(width > 1024px)", &ctx));
        assert!(evaluate_str("(width < 2000px)", &ctx));
        assert!(!evaluate_str("(width < 1024px)", &ctx));

        // calc() with rem (16px initial font size) and nested parens.
        // 1rem * 2 + (15rem + 2rem) * 2 + 31rem = 2 + 34 + 31 = 67rem = 1072px.
        let calc = "calc(1rem * 2 + (15rem + 2rem) * 2 + 31rem)";
        assert!(!evaluate_str(&format!("(width >= {calc})"), &ctx)); // 1024 < 1072
        assert!(evaluate_str(
            &format!("(width >= {calc})"),
            &screen(1200.0, 900.0)
        ));
        assert!(evaluate_str(&format!("(width < {calc})"), &ctx));
        assert!(!evaluate_str(
            &format!("(width < {calc})"),
            &screen(1200.0, 900.0)
        ));

        // Real-world negation form used by MDN.
        assert!(!evaluate_str("not (width <= 769px)", &screen(600.0, 900.0)));
        assert!(evaluate_str("not (width <= 769px)", &ctx));
    }
}
