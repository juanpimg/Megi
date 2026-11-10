//! # W3C CSS Cascading and Inheritance Level 4 (§6)
//!
//! Implements declaration origin and importance ranking, specificity calculation,
//! source-order tie breaking, and cascaded value resolution for DOM elements.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::properties::Length;
use crate::rule::{Declaration, Rule, Stylesheet, serialize_component_values};
use crate::selector::{SelectorList, Specificity, parse_selectors};
use crate::shorthand::expand_declarations;
use dom::{Document, NodeId};

/// Origin of a CSS declaration per W3C Cascading and Inheritance Level 4 §6.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Browser default User-Agent stylesheet.
    UserAgent,
    /// User personal stylesheet or accessibility overrides.
    User,
    /// Author stylesheet (`<style>`, `<link>`, or `style="..."` attribute).
    Author,
}

/// Cascade precedence rank per W3C Cascading and Inheritance Level 4 §6.2:
///
/// 1. User-Agent normal
/// 2. User normal
/// 3. Author normal
/// 4. Author `!important`
/// 5. User `!important`
/// 6. User-Agent `!important`
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CascadeOriginImportance {
    UserAgentNormal = 0,
    UserNormal = 1,
    AuthorNormal = 2,
    AuthorImportant = 3,
    UserImportant = 4,
    UserAgentImportant = 5,
}

impl CascadeOriginImportance {
    #[inline]
    pub fn new(origin: Origin, important: bool) -> Self {
        match (origin, important) {
            (Origin::UserAgent, false) => Self::UserAgentNormal,
            (Origin::User, false) => Self::UserNormal,
            (Origin::Author, false) => Self::AuthorNormal,
            (Origin::Author, true) => Self::AuthorImportant,
            (Origin::User, true) => Self::UserImportant,
            (Origin::UserAgent, true) => Self::UserAgentImportant,
        }
    }
}

/// A matched declaration along with its cascade priority metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchedDeclaration<'a> {
    pub declaration: &'a Declaration,
    pub origin_importance: CascadeOriginImportance,
    pub specificity: Specificity,
    pub source_order: u32,
    /// Position of the declaration inside its rule (or inline block). Later declarations win
    /// over earlier ones with the same origin, importance, specificity and source order.
    pub declaration_order: u32,
}

impl<'a> Eq for MatchedDeclaration<'a> {}

impl<'a> PartialOrd for MatchedDeclaration<'a> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for MatchedDeclaration<'a> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.origin_importance
            .cmp(&other.origin_importance)
            .then_with(|| self.specificity.cmp(&other.specificity))
            .then_with(|| self.source_order.cmp(&other.source_order))
            .then_with(|| self.declaration_order.cmp(&other.declaration_order))
    }
}

/// A compiled CSS rule with pre-parsed selectors and declarations for fast matching.
#[derive(Clone, Debug)]
pub struct CompiledRule {
    pub selectors: SelectorList,
    pub declarations: Vec<Declaration>,
    pub origin: Origin,
    pub source_order: u32,
}

/// Environment used to evaluate `@media` queries while stylesheets are compiled.
///
/// `viewport` is `None` by default, which makes width/height media features evaluate to
/// `false` (their value is unknown). `screen` selects the target medium: `false` means the
/// `print` medium. `@media` support is intentionally basic: media types (`all`, `screen`,
/// `print`), `min-width`/`max-width`/`width` and `min-height`/`max-height`/`height`, plus `and`
/// combinations and comma-separated query lists. `not`/`only` negation, `orientation`,
/// `resolution`, `prefers-*` and other features are not supported and evaluate to `false`.
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

/// A collection of compiled stylesheets ready for cascade resolution.
#[derive(Clone, Debug, Default)]
pub struct StyleCollection {
    pub rules: Vec<CompiledRule>,
    pub next_source_order: u32,
    /// Media environment used when compiling `@media` rules; set it before adding stylesheets.
    pub media: MediaContext,
}

impl StyleCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a stylesheet to the collection with the given origin.
    ///
    /// Declarations are shorthand-expanded before they are stored, so every rule contributes
    /// longhand declarations to the cascade (see [`crate::shorthand::expand_declarations`]).
    /// `@media` rules are evaluated against [`StyleCollection::media`]; other at-rules
    /// (`@import`, `@supports`, `@font-face`, ...) are still ignored.
    pub fn add_stylesheet(&mut self, stylesheet: &Stylesheet, origin: Origin) {
        self.add_rules(&stylesheet.rules, origin);
    }

    fn add_rules(&mut self, rules: &[Rule], origin: Origin) {
        for rule in rules {
            match rule {
                Rule::Style(style_rule) => self.add_style_rule(style_rule, origin),
                Rule::At(at_rule) if at_rule.name.eq_ignore_ascii_case("media") => {
                    let query = serialize_component_values(&at_rule.prelude);
                    if media_query_matches(&query, &self.media)
                        && let Some(block) = &at_rule.block
                    {
                        let nested_text = serialize_component_values(&block.values);
                        let nested = crate::parser::parse_stylesheet(&nested_text);
                        self.add_rules(&nested.rules, origin);
                    }
                }
                Rule::At(_) => {}
            }
        }
    }

    fn add_style_rule(&mut self, style_rule: &crate::rule::StyleRule, origin: Origin) {
        let selector_str = serialize_component_values(&style_rule.prelude);
        let selectors = parse_selectors(&selector_str);
        if selectors.selectors.is_empty() {
            return;
        }
        let declarations = expand_declarations(&style_rule.declarations);
        if declarations.is_empty() {
            return;
        }
        self.rules.push(CompiledRule {
            selectors,
            declarations,
            origin,
            source_order: self.next_source_order,
        });
        self.next_source_order += 1;
    }

    /// Parses and adds CSS text to the collection with the given origin.
    pub fn add_css(&mut self, css_text: &str, origin: Origin) {
        let stylesheet = crate::parser::parse_stylesheet(css_text);
        self.add_stylesheet(&stylesheet, origin);
    }
}

/// Returns whether a serialized `@media` prelude matches the given media environment.
///
/// A comma-separated query list matches when at least one query matches (CSS Media Queries 4).
fn media_query_matches(query: &str, media: &MediaContext) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let mut list = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in query.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => list.push(std::mem::take(&mut current)),
            c => current.push(c),
        }
    }
    list.push(current);
    list.iter()
        .any(|item| media_query_item_matches(item, media))
}

/// Returns whether a single media query (no top-level commas) matches.
fn media_query_item_matches(query: &str, media: &MediaContext) -> bool {
    let lower = query.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return true;
    }
    // `not` negation is not implemented: never apply those rules (conservative).
    if lower == "not" || lower.starts_with("not ") || lower.starts_with("not(") {
        return false;
    }
    for part in split_media_and(&lower) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part.starts_with('(') {
            if !media_feature_matches(part, media) {
                return false;
            }
            continue;
        }
        match part {
            // `only` only hides the query from legacy user agents; ignore it here.
            "only" => {}
            "all" | "screen" => {
                if !media.screen {
                    return false;
                }
            }
            "print" => {
                if media.screen {
                    return false;
                }
            }
            // Unknown media types never match (`tv`, `speech`, ...).
            _ => return false,
        }
    }
    true
}

/// Splits a media query on `and` outside parentheses.
fn split_media_and(query: &str) -> Vec<String> {
    let bytes = query.as_bytes();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index];
        match ch {
            b'(' => {
                depth += 1;
                current.push('(');
                index += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                current.push(')');
                index += 1;
            }
            _ if depth == 0
                && bytes[index..].starts_with(b"and")
                && (index == 0 || bytes[index - 1] == b' ')
                && bytes.get(index + 3).is_none_or(|next| *next == b' ') =>
            {
                parts.push(std::mem::take(&mut current));
                index += 3;
            }
            _ => {
                current.push(ch as char);
                index += 1;
            }
        }
    }
    parts.push(current);
    parts
}

/// Evaluates a parenthesized media feature such as `(min-width: 600px)`.
fn media_feature_matches(feature: &str, media: &MediaContext) -> bool {
    let inner = feature.trim().trim_start_matches('(').trim_end_matches(')');
    let Some((name, value)) = inner.split_once(':') else {
        // Boolean features (`(color)`, `(hover)`, ...) are not supported.
        return false;
    };
    let name = name.trim();
    let value = value.trim();
    let Some((width, height)) = media.viewport else {
        return false;
    };
    let px = match Length::parse(value, 16.0, 16.0) {
        Some(Length::Px(px)) => px,
        _ => return false,
    };
    match name {
        "min-width" | "width" => width >= px && (name == "min-width" || (width - px).abs() < 0.5),
        "max-width" => width <= px,
        "min-height" | "height" => {
            height >= px && (name == "min-height" || (height - px).abs() < 0.5)
        }
        "max-height" => height <= px,
        _ => false,
    }
}

/// Resolves the cascaded property declarations for a DOM element node.
///
/// Evaluates:
/// 1. Matched rules in [`StyleCollection`] (User-Agent, User, Author).
/// 2. Inline declarations from the element's `style="..."` attribute (if present).
/// 3. Normalizes priority via `(Origin & Importance, Specificity, Source Order)`.
///
/// Returns a map of property name (lowercase) to the winning [`Declaration`]. Shorthands in
/// the rules are already expanded; callers passing inline `style="..."` declarations should
/// expand them with [`crate::shorthand::expand_declarations`] first (as
/// [`resolve_document_styles`] does).
pub fn resolve_cascaded_properties<'a>(
    doc: &Document,
    node_id: NodeId,
    collection: &'a StyleCollection,
    inline_declarations: Option<&'a [Declaration]>,
) -> HashMap<String, &'a Declaration> {
    let mut best_candidates: HashMap<String, MatchedDeclaration<'a>> = HashMap::new();

    // 1. Match stylesheet rules
    for rule in &collection.rules {
        if let Some(spec) = rule.selectors.matching_specificity(doc, node_id) {
            for (declaration_order, decl) in rule.declarations.iter().enumerate() {
                let origin_importance = CascadeOriginImportance::new(rule.origin, decl.important);
                let candidate = MatchedDeclaration {
                    declaration: decl,
                    origin_importance,
                    specificity: spec,
                    source_order: rule.source_order,
                    declaration_order: declaration_order as u32,
                };

                match best_candidates.get_mut(&decl.name) {
                    Some(existing) => {
                        if candidate > *existing {
                            *existing = candidate;
                        }
                    }
                    None => {
                        best_candidates.insert(decl.name.clone(), candidate);
                    }
                }
            }
        }
    }

    // 2. Process inline styles from style attribute (Author origin with high specificity)
    if let Some(inline_decls) = inline_declarations {
        let inline_specificity = Specificity::new(100_000, 0, 0);
        let inline_source_order = collection.next_source_order + 10_000;
        for (declaration_order, decl) in inline_decls.iter().enumerate() {
            let origin_importance = CascadeOriginImportance::new(Origin::Author, decl.important);
            let candidate = MatchedDeclaration {
                declaration: decl,
                origin_importance,
                specificity: inline_specificity,
                source_order: inline_source_order,
                declaration_order: declaration_order as u32,
            };

            match best_candidates.get_mut(&decl.name) {
                Some(existing) => {
                    if candidate > *existing {
                        *existing = candidate;
                    }
                }
                None => {
                    best_candidates.insert(decl.name.clone(), candidate);
                }
            }
        }
    }

    best_candidates
        .into_iter()
        .map(|(k, v)| (k, v.declaration))
        .collect()
}

/// Resolves computed styles for all elements in the DOM document.
///
/// Pre-loads the default WHATWG HTML User-Agent stylesheet, then compiles and cascades
/// any author CSS supplied (e.g. from `<style>` tags or external stylesheets). `@media`
/// rules are evaluated for the `screen` medium with an unknown viewport, so width/height
/// features never match; use [`resolve_document_styles_with_viewport`] to provide one.
pub fn resolve_document_styles(
    doc: &Document,
    author_css: Option<&str>,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    resolve_document_styles_with_media(doc, author_css, MediaContext::default())
}

/// Resolves computed styles with `@media` width/height features evaluated for a viewport.
pub fn resolve_document_styles_with_viewport(
    doc: &Document,
    author_css: Option<&str>,
    width: f32,
    height: f32,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    resolve_document_styles_with_media(doc, author_css, MediaContext::with_viewport(width, height))
}

/// Resolves computed styles with a fully specified [`MediaContext`] for `@media` evaluation.
pub fn resolve_document_styles_with_media(
    doc: &Document,
    author_css: Option<&str>,
    media: MediaContext,
) -> HashMap<NodeId, crate::properties::ComputedStyle> {
    let mut collection = crate::user_agent::create_user_agent_style_collection();
    collection.media = media;
    if let Some(css) = author_css {
        collection.add_css(css, Origin::Author);
    }

    let mut styles: HashMap<NodeId, crate::properties::ComputedStyle> = HashMap::new();
    let root = doc.root();

    // Collect elements in top-down pre-order traversal
    let mut stack = Vec::new();
    let mut curr = doc.node(root).first_child;
    while let Some(child_id) = curr {
        stack.push(child_id);
        curr = doc.node(child_id).next_sibling;
    }

    while let Some(node_id) = stack.pop() {
        if let Some(el) = doc.node(node_id).as_element() {
            let inline_decls = el.get_attribute("style").map(|style| {
                let parsed = crate::parser::parse_list_of_declarations(style);
                expand_declarations(&parsed)
            });

            let cascaded =
                resolve_cascaded_properties(doc, node_id, &collection, inline_decls.as_deref());

            let parent_style = doc.node(node_id).parent.and_then(|p| styles.get(&p));
            let root_style = styles.get(&root);

            let computed =
                crate::properties::ComputedStyle::compute(&cascaded, parent_style, root_style);
            styles.insert(node_id, computed);
        }

        // Push children in reverse order so first child is popped first
        let mut children = Vec::new();
        let mut child = doc.node(node_id).first_child;
        while let Some(c) = child {
            children.push(c);
            child = doc.node(c).next_sibling;
        }
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }

    styles
}
