//! html5lib fixture constants, escaping and optional-tag filter pipeline.

use super::*;

pub(super) const VOID_ELEMENTS: &[&str] = &[
    "area",
    "base",
    "br",
    "col",
    "command",
    "embed",
    "event-source",
    "hr",
    "img",
    "input",
    "link",
    "meta",
    "param",
    "source",
    "track",
    "wbr",
];

pub(super) const RCDATA_ELEMENTS: &[&str] = &[
    "style", "script", "xmp", "iframe", "noembed", "noframes", "noscript",
];

/// html5lib's `booleanAttributes` table (empty key = attributes boolean on any element).
pub(super) const BOOLEAN_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("", &["irrelevant", "itemscope"]),
    ("style", &["scoped"]),
    ("img", &["ismap"]),
    ("audio", &["autoplay", "controls"]),
    ("video", &["autoplay", "controls"]),
    ("script", &["defer", "async"]),
    ("details", &["open"]),
    ("datagrid", &["multiple", "disabled"]),
    ("command", &["hidden", "disabled", "checked", "default"]),
    ("hr", &["noshade"]),
    ("menu", &["autosubmit"]),
    ("fieldset", &["disabled", "readonly"]),
    ("option", &["disabled", "readonly", "selected"]),
    ("optgroup", &["disabled", "readonly"]),
    ("button", &["disabled", "autofocus"]),
    (
        "input",
        &[
            "disabled",
            "readonly",
            "required",
            "autofocus",
            "checked",
            "ismap",
        ],
    ),
    ("select", &["disabled", "readonly", "autofocus", "multiple"]),
    ("ol", &["reversed"]),
    ("output", &["disabled", "readonly"]),
    ("iframe", &["seamless"]),
];

pub(super) fn is_boolean_attribute(tag_name: &str, attr_name: &str) -> bool {
    BOOLEAN_ATTRIBUTES
        .iter()
        .any(|(tag, attrs)| (*tag == tag_name || tag.is_empty()) && attrs.contains(&attr_name))
}

pub(super) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Attribute values need quoting under the fixture dialect's "legacy" rule: ASCII whitespace,
/// `"`, `'`, `=`, `>` and `` ` ``. Note that `<` does not force quoting in this fixture dialect
/// (it is escaped instead), and neither does U+000B.
pub(super) fn needs_legacy_quotes(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '\t' | '\n' | '\u{0C}' | '\r' | ' ' | '"' | '\'' | '=' | '>' | '`'
        )
    })
}

pub(super) fn token_is_space_characters(token: &SerializerToken) -> bool {
    matches!(token, SerializerToken::Spaces(_))
}

pub(super) fn is_space_character(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\u{0C}' | '\r' | ' ')
}

/// Splits a `Characters` token the way html5lib's `TreeWalker.text()` does: a leading run and a
/// trailing run of ASCII whitespace become `SpaceCharacters` tokens, the middle stays `Characters`.
pub(super) fn split_characters_token(data: &str) -> Vec<SerializerToken> {
    let mut out = Vec::new();
    let left_len = data.len() - data.trim_start_matches(is_space_character).len();
    let (left, rest) = data.split_at(left_len);
    if !left.is_empty() {
        out.push(SerializerToken::Spaces(left.to_owned()));
    }
    let middle = rest.trim_end_matches(is_space_character);
    let right = &rest[middle.len()..];
    if !middle.is_empty() {
        out.push(SerializerToken::Characters(middle.to_owned()));
    }
    if !right.is_empty() {
        out.push(SerializerToken::Spaces(right.to_owned()));
    }
    out
}

/// Port of html5lib's `optionaltags.Filter.is_optional_start`.
pub(super) fn is_optional_start(
    tag_name: &str,
    previous: Option<&SerializerToken>,
    next: Option<&SerializerToken>,
) -> bool {
    let next_type = next.map(SerializerToken::kind);
    match tag_name {
        "html" => {
            !matches!(next_type, Some("Comment")) && !next.is_some_and(token_is_space_characters)
        }
        "head" => match next_type {
            Some("StartTag" | "EmptyTag") => true,
            Some("EndTag") => next.and_then(SerializerToken::tag_name) == Some("head"),
            _ => false,
        },
        "body" => match next {
            Some(SerializerToken::Comment(_)) => false,
            Some(SerializerToken::Spaces(_)) => false,
            Some(SerializerToken::StartTag { name, .. }) => name != "script" && name != "style",
            Some(_) => true,
            None => true,
        },
        "colgroup" => match next {
            Some(
                SerializerToken::StartTag { name, .. } | SerializerToken::EmptyTag { name, .. },
            ) => name == "col",
            _ => false,
        },
        "tbody" => {
            if let Some(SerializerToken::StartTag { name, .. }) = next {
                if let Some(SerializerToken::EndTag {
                    name: previous_name,
                    ..
                }) = previous
                    && matches!(previous_name.as_str(), "tbody" | "thead" | "tfoot")
                {
                    return false;
                }
                name == "tr"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Port of html5lib's `optionaltags.Filter.is_optional_end`.
pub(super) fn is_optional_end(tag_name: &str, next: Option<&SerializerToken>) -> bool {
    let next_type = next.map(SerializerToken::kind);
    let next_name = next.and_then(SerializerToken::tag_name);
    let next_is_end_or_eof = matches!(next_type, Some("EndTag") | None);
    match tag_name {
        "html" | "head" | "body" => {
            !matches!(next_type, Some("Comment")) && !next.is_some_and(token_is_space_characters)
        }
        "li" | "optgroup" | "tr" => match next_type {
            Some("StartTag" | "EmptyTag") => next_name == Some(tag_name),
            _ => next_is_end_or_eof,
        },
        "dt" | "dd" => match next_type {
            Some("StartTag" | "EmptyTag") => matches!(next_name, Some("dt" | "dd")),
            _ => tag_name == "dd" && next_is_end_or_eof,
        },
        "p" => match next_type {
            Some("StartTag" | "EmptyTag") => matches!(
                next_name,
                Some(
                    "address"
                        | "article"
                        | "aside"
                        | "blockquote"
                        | "datagrid"
                        | "dialog"
                        | "dir"
                        | "div"
                        | "dl"
                        | "fieldset"
                        | "footer"
                        | "form"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "header"
                        | "hr"
                        | "menu"
                        | "nav"
                        | "ol"
                        | "p"
                        | "pre"
                        | "section"
                        | "table"
                        | "ul"
                )
            ),
            _ => next_is_end_or_eof,
        },
        "option" => match next_type {
            Some("StartTag" | "EmptyTag") => matches!(next_name, Some("option" | "optgroup")),
            _ => next_is_end_or_eof,
        },
        "rt" | "rp" => match next_type {
            Some("StartTag" | "EmptyTag") => matches!(next_name, Some("rt" | "rp")),
            _ => next_is_end_or_eof,
        },
        "colgroup" => match next {
            Some(SerializerToken::Comment(_)) => false,
            Some(token) if token_is_space_characters(token) => false,
            Some(SerializerToken::StartTag { name, .. }) => name != "colgroup",
            _ => true,
        },
        "thead" | "tbody" => match next_type {
            Some("StartTag" | "EmptyTag") => matches!(next_name, Some("tbody" | "tfoot")),
            _ => tag_name == "tbody" && next_is_end_or_eof,
        },
        "tfoot" => match next_type {
            Some("StartTag" | "EmptyTag") => next_name == Some("tbody"),
            _ => next_is_end_or_eof,
        },
        "td" | "th" => match next_type {
            Some("StartTag" | "EmptyTag") => matches!(next_name, Some("td" | "th")),
            _ => next_is_end_or_eof,
        },
        _ => false,
    }
}

/// Port of html5lib's `optionaltags.Filter`.
pub(super) fn omit_optional_tags(tokens: Vec<SerializerToken>) -> Vec<SerializerToken> {
    let mut out = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|i| &tokens[i]);
        let next = tokens.get(index + 1);
        let keep = match token {
            SerializerToken::StartTag { name, attrs, .. }
            | SerializerToken::EmptyTag { name, attrs } => {
                !attrs.is_empty() || !is_optional_start(name, previous, next)
            }
            SerializerToken::EndTag { name, .. } => !is_optional_end(name, next),
            _ => true,
        };
        if keep {
            out.push(token.clone());
        }
    }
    out
}

/// Port of html5lib's `whitespace.Filter`: collapse runs of ASCII whitespace outside
/// pre/textarea/rcdata elements.
pub(super) fn strip_whitespace(tokens: Vec<SerializerToken>) -> Vec<SerializerToken> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut preserve_depth = 0usize;
    for mut token in tokens {
        match &token {
            SerializerToken::StartTag { name, .. }
                if preserve_depth > 0
                    || matches!(name.as_str(), "pre" | "textarea")
                    || RCDATA_ELEMENTS.contains(&name.as_str()) =>
            {
                preserve_depth += 1;
            }
            SerializerToken::EndTag { .. } if preserve_depth > 0 => {
                preserve_depth -= 1;
            }
            SerializerToken::Characters(data) if preserve_depth == 0 => {
                token = SerializerToken::Characters(collapse_spaces(data));
            }
            SerializerToken::Spaces(data) if preserve_depth == 0 && !data.is_empty() => {
                token = SerializerToken::Spaces(" ".to_owned());
            }
            _ => {}
        }
        out.push(token);
    }
    out
}

pub(super) fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space_run = false;
    for ch in text.chars() {
        if is_space_character(ch) {
            if !in_space_run {
                out.push(' ');
                in_space_run = true;
            }
        } else {
            out.push(ch);
            in_space_run = false;
        }
    }
    out
}

/// Port of html5lib's `inject_meta_charset.Filter`.
pub(super) fn inject_meta_charset(
    tokens: Vec<SerializerToken>,
    encoding: &str,
) -> Vec<SerializerToken> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut state = "pre_head";
    let mut meta_found = false;
    let mut pending: Vec<SerializerToken> = Vec::new();

    for mut token in tokens {
        match &mut token {
            SerializerToken::StartTag { name, .. } if name.eq_ignore_ascii_case("head") => {
                state = "in_head";
            }
            SerializerToken::EmptyTag { name, attrs } if name.eq_ignore_ascii_case("meta") => {
                let mut has_http_equiv_content_type = false;
                let mut replaced = false;
                for attr in attrs.iter_mut() {
                    if attr.namespace.is_some() {
                        continue;
                    }
                    if attr.name.eq_ignore_ascii_case("charset") {
                        attr.value = encoding.to_owned();
                        meta_found = true;
                        replaced = true;
                        break;
                    } else if attr.name == "http-equiv"
                        && attr.value.eq_ignore_ascii_case("content-type")
                    {
                        has_http_equiv_content_type = true;
                    }
                }
                if !replaced
                    && has_http_equiv_content_type
                    && let Some(content) = attrs
                        .iter_mut()
                        .find(|attr| attr.namespace.is_none() && attr.name == "content")
                {
                    content.value = format!("text/html; charset={encoding}");
                    meta_found = true;
                }
            }
            SerializerToken::EndTag { name, .. }
                if name.eq_ignore_ascii_case("head") && !pending.is_empty() =>
            {
                // Insert the missing meta element first, then flush the buffered head.
                if !meta_found {
                    out.push(SerializerToken::EmptyTag {
                        name: "meta".to_string(),
                        attrs: vec![SerializerAttr {
                            namespace: None,
                            name: "charset".to_string(),
                            value: encoding.to_owned(),
                        }],
                    });
                    meta_found = true;
                }
                out.append(&mut pending);
                out.push(token);
                state = "post_head";
                continue;
            }
            _ => {}
        }
        if state == "in_head" {
            pending.push(token);
        } else {
            out.push(token);
        }
    }
    out
}
