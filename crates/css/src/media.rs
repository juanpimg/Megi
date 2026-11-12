//! # CSS Media Queries Level 3/4 (subset)
//!
//! Parser and evaluator for media query lists, shared by the cascade (`@media` rules),
//! `@import` gating and consumers such as the `render` crate for `<link media="...">`.
//!
//! Supported grammar: comma-separated query lists; `[not | only]? <media-type>` with
//! `and <condition>`; standalone conditions with nested parentheses, `and`, `or` and `not`;
//! media types `all`, `screen` and `print` (any other type never matches); and the features
//! `min-width`, `max-width`, `width`, `min-height`, `max-height`, `height` (px/em/rem/pt/pc/
//! in/cm/mm; `em`/`rem` are resolved against the 16px initial font size) and `orientation`
//! (`portrait`/`landscape`). Features that are syntactically valid but not modelled
//! (`resolution`, `prefers-color-scheme`, `hover`, `aspect-ratio`, boolean features, ...)
//! evaluate to `false`; this is the conservative choice used by the cascade.
//!
//! Evaluation rules (CSS Media Queries 4):
//! - An empty media query list matches every context (this is what `@import "x.css";` means).
//! - A list matches when at least one of its queries matches.
//! - Width/height/orientation features need a known viewport in [`MediaContext`]; when the
//!   viewport is unknown (`None`) they evaluate to `false`.
//! - `not` negates the whole query (media type included); `only` has no effect on modern user
//!   agents (it only hides the query from legacy parsers).
//! - Invalid queries inside a list are forgivingly replaced by a never-matching query
//!   (Media Queries 4 §3.2); a malformed single query therefore matches nothing.

use crate::token::CssToken;

/// Environment used to evaluate media queries.
///
/// `viewport` is `None` by default, which makes width/height/orientation features evaluate to
/// `false` (their value is unknown). `screen` selects the target medium: `false` means the
/// `print` medium.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaContext {
    /// Viewport size in CSS pixels, if known.
    pub viewport: Option<(f32, f32)>,
    /// `true` for the `screen` medium, `false` for `print`.
    pub screen: bool,
}

impl Default for MediaContext {
    fn default() -> Self {
        Self {
            viewport: None,
            screen: true,
        }
    }
}

impl MediaContext {
    /// Creates a screen medium with a known viewport size.
    pub fn with_viewport(width: f32, height: f32) -> Self {
        Self {
            viewport: Some((width, height)),
            screen: true,
        }
    }
}

/// Media type targeted by a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaType {
    /// `all`
    All,
    /// `screen`
    Screen,
    /// `print`
    Print,
    /// Any other syntactically valid media type; it never matches in this engine.
    Other(String),
}

/// Qualifier preceding the media type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaQualifier {
    /// `not`: negates the whole query.
    Not,
    /// `only`: no effect on modern user agents.
    Only,
}

/// Orientation values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// A single media feature (`(min-width: 600px)`, `(orientation: landscape)`, ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaFeature {
    MinWidth(f32),
    MaxWidth(f32),
    Width(f32),
    MinHeight(f32),
    MaxHeight(f32),
    Height(f32),
    Orientation(Orientation),
    /// A valid feature that this engine does not model (`resolution`, `prefers-*`, ...);
    /// always evaluates to `false`.
    Unsupported,
}

impl MediaFeature {
    /// Evaluates this feature against `context` (viewport in CSS px).
    pub fn evaluate(&self, context: &MediaContext) -> bool {
        let Some((width, height)) = context.viewport else {
            return false;
        };
        match self {
            Self::MinWidth(value) => width >= *value,
            Self::MaxWidth(value) => width <= *value,
            Self::Width(value) => (width - *value).abs() < 0.5,
            Self::MinHeight(value) => height >= *value,
            Self::MaxHeight(value) => height <= *value,
            Self::Height(value) => (height - *value).abs() < 0.5,
            Self::Orientation(Orientation::Portrait) => height >= width,
            Self::Orientation(Orientation::Landscape) => width > height,
            Self::Unsupported => false,
        }
    }
}

/// Boolean combination of media conditions.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaCondition {
    Feature(MediaFeature),
    Not(Box<MediaCondition>),
    And(Box<MediaCondition>, Box<MediaCondition>),
    Or(Box<MediaCondition>, Box<MediaCondition>),
}

impl MediaCondition {
    /// Evaluates this condition against `context`.
    pub fn evaluate(&self, context: &MediaContext) -> bool {
        match self {
            Self::Feature(feature) => feature.evaluate(context),
            Self::Not(condition) => !condition.evaluate(context),
            Self::And(left, right) => left.evaluate(context) && right.evaluate(context),
            Self::Or(left, right) => left.evaluate(context) || right.evaluate(context),
        }
    }
}

/// One media query: `[not | only]? <media-type> [and <condition>]?` or a condition alone.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaQuery {
    /// `not` / `only`, if any.
    pub qualifier: Option<MediaQualifier>,
    /// Media type; `None` for a condition-only query (implicit `all`).
    pub media_type: Option<MediaType>,
    /// The condition part (`and <condition>` after a type, or the whole condition).
    pub condition: Option<MediaCondition>,
}

impl MediaQuery {
    /// Returns a query that never matches (used for invalid entries in a forgiving list).
    pub fn never() -> Self {
        Self {
            qualifier: Some(MediaQualifier::Not),
            media_type: Some(MediaType::All),
            condition: None,
        }
    }

    /// Evaluates this query against `context`.
    pub fn matches(&self, context: &MediaContext) -> bool {
        let media_type_matches = match &self.media_type {
            None | Some(MediaType::All) => true,
            Some(MediaType::Screen) => context.screen,
            Some(MediaType::Print) => !context.screen,
            Some(MediaType::Other(_)) => false,
        };
        let matches = media_type_matches
            && self
                .condition
                .as_ref()
                .is_none_or(|condition| condition.evaluate(context));
        match self.qualifier {
            Some(MediaQualifier::Not) => !matches,
            _ => matches,
        }
    }
}

/// A comma-separated media query list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaQueryList {
    pub queries: Vec<MediaQuery>,
}

impl MediaQueryList {
    /// Returns whether the list is empty (an empty list matches every context).
    pub fn is_empty(&self) -> bool {
        self.queries.is_empty()
    }
}

/// Evaluates a media query list against a media context.
///
/// An empty list matches; a list matches when any of its queries matches.
pub fn evaluate(query_list: &MediaQueryList, context: &MediaContext) -> bool {
    if query_list.queries.is_empty() {
        return true;
    }
    query_list
        .queries
        .iter()
        .any(|query| query.matches(context))
}

/// Parses a comma-separated media query list (forgiving: invalid entries never match).
pub fn parse_media_query_list(input: &str) -> MediaQueryList {
    let mut list = MediaQueryList::default();
    for fragment in split_top_level_commas(input) {
        let fragment = fragment.trim();
        if fragment.is_empty() {
            continue;
        }
        let tokens: Vec<CssToken> = crate::tokenizer::tokenize(fragment)
            .into_iter()
            .filter(|token| !matches!(token, CssToken::Whitespace | CssToken::Eof))
            .collect();
        match parse_media_query(&tokens) {
            Some(query) => list.queries.push(query),
            None => list.queries.push(MediaQuery::never()),
        }
    }
    list
}

/// Splits a media query list on commas that are not inside parentheses.
fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

/// Token cursor used by the media query parser.
struct MediaParser<'a> {
    tokens: &'a [CssToken],
    pos: usize,
}

impl<'a> MediaParser<'a> {
    fn new(tokens: &'a [CssToken]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&'a CssToken> {
        self.tokens.get(self.pos)
    }

    fn peek_ident(&self) -> Option<&'a str> {
        match self.peek() {
            Some(CssToken::Ident(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    fn peek_ident_is(&self, keyword: &str) -> bool {
        self.peek_ident()
            .is_some_and(|ident| ident.eq_ignore_ascii_case(keyword))
    }
}

/// Parses one media query from its token slice (must consume all tokens).
fn parse_media_query(tokens: &[CssToken]) -> Option<MediaQuery> {
    let mut parser = MediaParser::new(tokens);

    // Optional `not` / `only` qualifier. `not` is only a qualifier when a media type follows;
    // otherwise it starts a condition (`not (min-width: 600px)`).
    let mut qualifier = None;
    if parser.peek_ident_is("not") || parser.peek_ident_is("only") {
        let qualifier_keyword = parser.peek_ident()?.to_ascii_lowercase();
        let next_is_media_type = matches!(parser.tokens.get(1), Some(CssToken::Ident(_)));
        if next_is_media_type {
            qualifier = Some(if qualifier_keyword == "not" {
                MediaQualifier::Not
            } else {
                MediaQualifier::Only
            });
            parser.pos += 1;
        }
    }

    // Optional media type. A leading `not` starts a condition (`not (feature)`), and a
    // leading `only` without a media type is invalid.
    let media_type = match parser.peek_ident() {
        Some(ident) => {
            let lower = ident.to_ascii_lowercase();
            match lower.as_str() {
                "and" | "or" | "only" => return None,
                "not" => None,
                _ => {
                    parser.pos += 1;
                    Some(match lower.as_str() {
                        "all" => MediaType::All,
                        "screen" => MediaType::Screen,
                        "print" => MediaType::Print,
                        _ => MediaType::Other(lower),
                    })
                }
            }
        }
        None => None,
    };
    if qualifier.is_some() && media_type.is_none() {
        return None;
    }

    // Optional condition.
    let condition = if media_type.is_some() {
        if parser.peek().is_none() {
            None
        } else {
            if !parser.peek_ident_is("and") {
                return None;
            }
            parser.pos += 1;
            Some(parse_condition(&mut parser, 0)?)
        }
    } else if parser.peek().is_some() {
        Some(parse_condition(&mut parser, 0)?)
    } else {
        return None;
    };

    if parser.peek().is_some() {
        return None; // trailing garbage invalidates the query
    }

    Some(MediaQuery {
        qualifier,
        media_type,
        condition,
    })
}

/// Maximum nesting of parenthesized media conditions accepted from a raw string entry
/// point (`<style media>`/`<link media>`/`matchMedia`), which bypasses the parser's
/// component-nesting cap.
pub const MAX_MEDIA_NESTING: usize = 32;

/// Parses a `<media-condition>`: `not <group>` or `<group> (('and'|'or') <group>)*`.
fn parse_condition(parser: &mut MediaParser<'_>, depth: usize) -> Option<MediaCondition> {
    if depth > MAX_MEDIA_NESTING {
        return None;
    }
    if parser.peek_ident_is("not") {
        parser.pos += 1;
        let inner = parse_condition_group(parser, depth)?;
        return Some(MediaCondition::Not(Box::new(inner)));
    }
    let first = parse_condition_group(parser, depth)?;
    let using_and = parser.peek_ident_is("and");
    let using_or = parser.peek_ident_is("or");
    if using_and {
        let mut condition = first;
        while parser.peek_ident_is("and") {
            parser.pos += 1;
            let right = parse_condition_group(parser, depth)?;
            condition = MediaCondition::And(Box::new(condition), Box::new(right));
        }
        Some(condition)
    } else if using_or {
        let mut condition = first;
        while parser.peek_ident_is("or") {
            parser.pos += 1;
            let right = parse_condition_group(parser, depth)?;
            condition = MediaCondition::Or(Box::new(condition), Box::new(right));
        }
        Some(condition)
    } else {
        Some(first)
    }
}

/// Parses a parenthesized condition group or feature (`(...)`, `((...) and (...))`).
fn parse_condition_group(parser: &mut MediaParser<'_>, depth: usize) -> Option<MediaCondition> {
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
fn parse_feature(parser: &mut MediaParser<'_>) -> Option<MediaCondition> {
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
        // Syntactically valid but not modelled: evaluate to false (documented).
        _ => MediaFeature::Unsupported,
    };
    Some(MediaCondition::Feature(feature))
}

/// Extracts the single identifier of a feature value (`(orientation: landscape)`).
fn single_ident(tokens: &[CssToken]) -> Option<&str> {
    match tokens {
        [CssToken::Ident(name)] => Some(name.as_str()),
        _ => None,
    }
}

/// Converts a media feature length token to CSS pixels.
///
/// Media query `em`/`rem` are relative to the initial font size (16px), not the document
/// font size (Media Queries 4 §1.3).
fn parse_length_value(tokens: &[CssToken]) -> Option<f32> {
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
mod tests {
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
