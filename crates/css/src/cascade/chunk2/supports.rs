//! CSS Conditional Rules Level 4 §3: evaluation of `@supports` conditions.
//!
//! The grammar is parsed from component values (so `not`, `and`, `or`, nested conditions
//! and declaration conditions keep their authored grouping). Declaration conditions are
//! checked with the same declaration parser and value parsers used by the cascade.
//! Conditional functions such as `selector()` are reported unsupported, which makes
//! `@supports not selector(...)` fallbacks work without pretending to implement them.

use crate::component_value::ComponentValue;
use crate::rule::Declaration;
use crate::token::CssToken;

/// Evaluates an `@supports` prelude; a malformed or partially consumed condition is false.
pub(crate) fn condition(values: &[ComponentValue]) -> bool {
    eval_or(values).is_some_and(|(result, used)| {
        result && values[used..].iter().all(ComponentValue::is_whitespace)
    })
}

fn skip_ws(values: &[ComponentValue], mut index: usize) -> usize {
    while values.get(index).is_some_and(ComponentValue::is_whitespace) {
        index += 1;
    }
    index
}

fn is_ident(value: Option<&ComponentValue>, name: &str) -> bool {
    matches!(value, Some(ComponentValue::PreservedToken(CssToken::Ident(word))) if word.eq_ignore_ascii_case(name))
}

fn eval_or(values: &[ComponentValue]) -> Option<(bool, usize)> {
    let (mut result, mut used) = eval_and(values)?;
    loop {
        let at = skip_ws(values, used);
        if !is_ident(values.get(at), "or") {
            break;
        }
        let (rhs, next) = eval_and(&values[at + 1..])?;
        result |= rhs;
        used = at + 1 + next;
    }
    Some((result, used))
}

fn eval_and(values: &[ComponentValue]) -> Option<(bool, usize)> {
    let (mut result, mut used) = eval_atom(values)?;
    loop {
        let at = skip_ws(values, used);
        if !is_ident(values.get(at), "and") {
            break;
        }
        let (rhs, next) = eval_atom(&values[at + 1..])?;
        result &= rhs;
        used = at + 1 + next;
    }
    Some((result, used))
}

fn eval_atom(values: &[ComponentValue]) -> Option<(bool, usize)> {
    let at = skip_ws(values, 0);
    if is_ident(values.get(at), "not") {
        let (result, used) = eval_atom(&values[at + 1..])?;
        return Some((!result, at + 1 + used));
    }
    match values.get(at)? {
        ComponentValue::Block(block) if block.associated_token == CssToken::LeftParen => {
            let text = crate::rule::serialize_component_values(&block.values);
            let declarations = crate::parser::parse_list_of_declarations(&text);
            if declarations.len() == 1 {
                return Some((declaration_supported(&declarations[0]), at + 1));
            }
            let (result, _used) = eval_or(&block.values)?;
            Some((result, at + 1))
        }
        ComponentValue::Function(_) => Some((false, at + 1)),
        _ => None,
    }
}

/// Unknown property names are unsupported (CSS Conditional Rules Level 4 §3.1.1); known
/// properties whose value syntax this engine does not model are accepted syntactically.
fn declaration_supported(declaration: &Declaration) -> bool {
    const COLOR: &[&str] = &["color", "background-color", "border-color"];
    const LENGTH: &[&str] = &[
        "width",
        "height",
        "min-width",
        "min-height",
        "max-width",
        "max-height",
    ];
    const KEYWORD: &[&str] = &[
        "position",
        "float",
        "clear",
        "visibility",
        "overflow",
        "white-space",
        "text-align",
        "text-transform",
        "font-style",
        "direction",
        "unicode-bidi",
        "vertical-align",
    ];
    const KNOWN: &[&str] = &[
        "align-items",
        "align-self",
        "aspect-ratio",
        "background",
        "background-attachment",
        "background-clip",
        "background-image",
        "background-origin",
        "background-position",
        "background-repeat",
        "background-size",
        "border",
        "border-collapse",
        "border-radius",
        "border-spacing",
        "box-shadow",
        "box-sizing",
        "caption-side",
        "column-count",
        "container-name",
        "container-type",
        "content",
        "cursor",
        "flex",
        "flex-direction",
        "flex-wrap",
        "font",
        "font-family",
        "font-size",
        "font-weight",
        "gap",
        "grid",
        "grid-area",
        "grid-auto-flow",
        "grid-column",
        "grid-gap",
        "grid-row",
        "grid-template",
        "grid-template-areas",
        "grid-template-columns",
        "grid-template-rows",
        "justify-content",
        "letter-spacing",
        "line-height",
        "list-style",
        "list-style-type",
        "margin",
        "margin-block",
        "margin-inline",
        "object-fit",
        "opacity",
        "outline",
        "padding",
        "padding-block",
        "padding-inline",
        "text-overflow",
        "text-shadow",
        "transform",
        "transform-origin",
        "transition",
        "z-index",
    ];
    let name = declaration.name.as_str();
    let value = declaration.raw_value.trim();
    if name.starts_with("--") || matches!(value, "inherit" | "initial" | "unset" | "revert") {
        return true;
    }
    match name {
        "display" => crate::Display::parse(value).is_some(),
        "text-decoration" | "text-decoration-line" => crate::TextDecoration::parse(value).is_some(),
        _ if COLOR.contains(&name) => crate::Color::parse(value).is_some(),
        _ if LENGTH.contains(&name) => {
            crate::Length::parse(value, 16.0, 16.0).is_some()
                || matches!(
                    value,
                    "auto" | "min-content" | "max-content" | "fit-content"
                )
        }
        // These properties are accepted by the declaration parser with arbitrary values.
        _ if KEYWORD.contains(&name) => true,
        // Both spellings are supported at the syntax level, as in current browsers.
        "mask-image" | "-webkit-mask-image" => {
            value.eq_ignore_ascii_case("none")
                || value.contains("url(")
                || value.contains("gradient(")
        }
        _ => KNOWN.contains(&name),
    }
}
