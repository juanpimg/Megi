//! Sized submodule of the parent facade (verbatim move).

use super::*;


/// Environment used to evaluate media queries.
///
/// `viewport` is `None` by default, which makes width/height/orientation features evaluate to
/// `false` (their value is unknown). `screen` selects the target medium: `false` means the
/// `print` medium.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorScheme {
    #[default]
    Light,
    Dark,
}


#[derive(Clone, Debug, PartialEq)]
pub struct MediaContext {
    /// Viewport size in CSS pixels, if known.
    pub viewport: Option<(f32, f32)>,
    /// `true` for the `screen` medium, `false` for `print`.
    pub screen: bool,
    /// Preferred color scheme.
    pub color_scheme: ColorScheme,
    /// Whether the user prefers reduced motion.
    pub reduced_motion: bool,
}


impl Default for MediaContext {
    fn default() -> Self {
        Self {
            viewport: None,
            screen: true,
            color_scheme: ColorScheme::Light,
            reduced_motion: false,
        }
    }
}


impl MediaContext {
    /// Creates a screen medium with a known viewport size.
    pub fn with_viewport(width: f32, height: f32) -> Self {
        Self {
            viewport: Some((width, height)),
            screen: true,
            color_scheme: ColorScheme::Light,
            reduced_motion: false,
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
    PrefersColorScheme(ColorScheme),
    PrefersReducedMotion(bool),
    /// A valid feature that this engine does not model (`resolution`, `hover`, ...);
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
            Self::PrefersColorScheme(scheme) => *scheme == context.color_scheme,
            Self::PrefersReducedMotion(value) => *value == context.reduced_motion,
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
pub(crate) fn split_top_level_commas(input: &str) -> Vec<&str> {
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
pub(crate) struct MediaParser<'a> {
    pub(crate) tokens: &'a [CssToken],
    pub(crate) pos: usize,
}


impl<'a> MediaParser<'a> {
    pub(crate) fn new(tokens: &'a [CssToken]) -> Self {
        Self { tokens, pos: 0 }
    }

    pub(crate) fn peek(&self) -> Option<&'a CssToken> {
        self.tokens.get(self.pos)
    }

    pub(crate) fn peek_ident(&self) -> Option<&'a str> {
        match self.peek() {
            Some(CssToken::Ident(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    pub(crate) fn peek_ident_is(&self, keyword: &str) -> bool {
        self.peek_ident()
            .is_some_and(|ident| ident.eq_ignore_ascii_case(keyword))
    }
}


/// Parses one media query from its token slice (must consume all tokens).
pub(crate) fn parse_media_query(tokens: &[CssToken]) -> Option<MediaQuery> {
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
pub(crate) fn parse_condition(parser: &mut MediaParser<'_>, depth: usize) -> Option<MediaCondition> {
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
