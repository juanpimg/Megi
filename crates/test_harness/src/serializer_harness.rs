//! # `html5lib-tests` HTML Serializer Conformance Harness (`serializer_harness`)
//!
//! Runs the five vendored `tests/html5lib-tests/serializer/*.test` suites
//! (`core`, `injectmeta`, `optionaltags`, `options`, `whitespace`).
//!
//! These fixtures are **not** the WHATWG HTML §13.3 (`innerHTML`) serialization: they encode the
//! legacy html5lib serializer and its options (`quote_attr_values`, `omit_optional_tags`,
//! `inject_meta_charset`, `strip_whitespace`, ...). The fixture input is a flat token stream
//! (`StartTag`/`EndTag`/`EmptyTag`/`Characters`/`Comment`/`Doctype`) and the expected value is one
//! of possibly several acceptable serialized strings. This module implements that legacy
//! option-driven serializer faithfully so the suites become part of the conformance gate; the
//! WHATWG §13.3 implementation used by `innerHTML`/`outerHTML` lives in `html::serializer`
//! (`serialize_html`, `serialize_fragment`, `serialize_outer_html`).
//!
//! The XHTML alternatives (`test.xhtml`) are not exercised because Megi implements the HTML
//! serialization, not the XHTML one; tests still have to match an `expected` (HTML) string.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

/// One attribute in a fixture token. `namespace` is always `null` in the vendored fixtures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializerAttr {
    pub namespace: Option<String>,
    pub name: String,
    pub value: String,
}

/// Token kinds used by the html5lib serializer fixtures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SerializerToken {
    Doctype {
        name: String,
        public_id: Option<String>,
        system_id: Option<String>,
    },
    StartTag {
        namespace: Option<String>,
        name: String,
        attrs: Vec<SerializerAttr>,
    },
    EndTag {
        namespace: Option<String>,
        name: String,
    },
    EmptyTag {
        name: String,
        attrs: Vec<SerializerAttr>,
    },
    Characters(String),
    /// html5lib `SpaceCharacters` token: a leading/trailing run of ASCII whitespace that the
    /// `JsonWalker`/`TreeWalker.text` loader splits out of a `Characters` token. Serialized raw.
    Spaces(String),
    Comment(String),
}

impl SerializerToken {
    /// Token type name as used by html5lib options/filters.
    fn kind(&self) -> &'static str {
        match self {
            Self::Doctype { .. } => "Doctype",
            Self::StartTag { .. } => "StartTag",
            Self::EndTag { .. } => "EndTag",
            Self::EmptyTag { .. } => "EmptyTag",
            Self::Characters(_) => "Characters",
            Self::Spaces(_) => "SpaceCharacters",
            Self::Comment(_) => "Comment",
        }
    }

    fn tag_name(&self) -> Option<&str> {
        match self {
            Self::StartTag { name, .. }
            | Self::EndTag { name, .. }
            | Self::EmptyTag { name, .. } => Some(name),
            _ => None,
        }
    }
}

/// Raw serializer fixture case (`serializer/*.test`).
#[derive(Clone, Debug, Deserialize)]
pub struct RawSerializerCase {
    pub description: String,
    pub input: Vec<Value>,
    /// Acceptable HTML serializations (any one passing means the case passes).
    pub expected: Vec<String>,
    #[serde(default)]
    pub options: SerializerFixtureOptions,
    /// XHTML serialization alternatives; recorded but not exercised (HTML serialization only).
    #[serde(default)]
    pub xhtml: Option<Vec<String>>,
}

/// Options present in the serializer fixtures (subset of html5lib's serializer options).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SerializerFixtureOptions {
    pub quote_char: Option<String>,
    pub quote_attr_values: Option<bool>,
    pub use_best_quote_char: Option<bool>,
    pub omit_optional_tags: Option<bool>,
    pub minimize_boolean_attributes: Option<bool>,
    pub use_trailing_solidus: Option<bool>,
    pub space_before_trailing_solidus: Option<bool>,
    pub escape_lt_in_attrs: Option<bool>,
    pub escape_rcdata: Option<bool>,
    pub inject_meta_charset: Option<bool>,
    pub encoding: Option<String>,
    pub strip_whitespace: Option<bool>,
}

/// Result of running one serializer fixture case.
#[derive(Clone, Debug)]
pub struct SerializerRunResult {
    pub file_name: String,
    pub index: usize,
    pub description: String,
    pub passed: bool,
    pub expected: Vec<String>,
    pub actual: String,
    /// Whether the fixture case also ships an `xhtml` alternative (not exercised).
    pub has_xhtml: bool,
}

/// Resolved options with html5lib's defaults.
struct EffectiveOptions {
    quote_char: char,
    quote_attr_values: bool,
    use_best_quote_char: bool,
    omit_optional_tags: bool,
    minimize_boolean_attributes: bool,
    use_trailing_solidus: bool,
    space_before_trailing_solidus: bool,
    escape_lt_in_attrs: bool,
    escape_rcdata: bool,
    inject_meta_charset: bool,
    encoding: Option<String>,
    strip_whitespace: bool,
}

impl EffectiveOptions {
    fn from_fixture(fixture: &SerializerFixtureOptions) -> Self {
        Self {
            quote_char: fixture
                .quote_char
                .as_deref()
                .and_then(|s| s.chars().next())
                .unwrap_or('"'),
            quote_attr_values: fixture.quote_attr_values.unwrap_or(false),
            use_best_quote_char: fixture
                .use_best_quote_char
                .unwrap_or(fixture.quote_char.is_none()),
            omit_optional_tags: fixture.omit_optional_tags.unwrap_or(true),
            minimize_boolean_attributes: fixture.minimize_boolean_attributes.unwrap_or(true),
            use_trailing_solidus: fixture.use_trailing_solidus.unwrap_or(false),
            space_before_trailing_solidus: fixture.space_before_trailing_solidus.unwrap_or(true),
            escape_lt_in_attrs: fixture.escape_lt_in_attrs.unwrap_or(false),
            escape_rcdata: fixture.escape_rcdata.unwrap_or(false),
            inject_meta_charset: fixture.inject_meta_charset.unwrap_or(true),
            encoding: fixture.encoding.clone(),
            strip_whitespace: fixture.strip_whitespace.unwrap_or(false),
        }
    }
}

fn optional_string(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("expected a string or null, got {other}")),
    }
}

fn parse_attrs(value: Option<&Value>) -> Result<Vec<SerializerAttr>, String> {
    let items = match value {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(items)) => items,
        // Legacy html5lib fixtures sometimes use an empty object for "no attributes".
        Some(Value::Object(map)) if map.is_empty() => return Ok(Vec::new()),
        Some(other) => {
            return Err(format!(
                "expected an attribute array or empty object, got {other}"
            ));
        }
    };
    let mut attrs = Vec::with_capacity(items.len());
    for item in items {
        let Value::Object(map) = item else {
            return Err(format!("expected an attribute object, got {item}"));
        };
        let namespace = optional_string(map.get("namespace"))?;
        let name = map
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("attribute without a string name: {item}"))?
            .to_owned();
        let value = map
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("attribute {name:?} without a string value"))?
            .to_owned();
        attrs.push(SerializerAttr {
            namespace,
            name,
            value,
        });
    }
    // The html5lib serializer test loader always constructs
    // `HTMLSerializer(alphabetical_attributes=True)`, so fixture expectations are ordered by
    // attribute name. Keep the order deterministic here.
    attrs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(attrs)
}

/// Parses one fixture input token array into a [`SerializerToken`].
pub fn parse_serializer_token(value: &Value) -> Result<SerializerToken, String> {
    let array = value
        .as_array()
        .ok_or_else(|| format!("expected a token array, got {value}"))?;
    let kind = array
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected a token type string, got {value}"))?;
    match kind {
        "Doctype" => Ok(SerializerToken::Doctype {
            name: array
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| "Doctype token without a name".to_string())?
                .to_owned(),
            public_id: optional_string(array.get(2))?,
            system_id: optional_string(array.get(3))?,
        }),
        "StartTag" => Ok(SerializerToken::StartTag {
            namespace: optional_string(array.get(1))?,
            name: array
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| "StartTag token without a name".to_string())?
                .to_owned(),
            attrs: parse_attrs(array.get(3))?,
        }),
        "EndTag" => Ok(SerializerToken::EndTag {
            namespace: optional_string(array.get(1))?,
            name: array
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| "EndTag token without a name".to_string())?
                .to_owned(),
        }),
        "EmptyTag" => Ok(SerializerToken::EmptyTag {
            name: array
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| "EmptyTag token without a name".to_string())?
                .to_owned(),
            attrs: parse_attrs(array.get(2))?,
        }),
        "Characters" => Ok(SerializerToken::Characters(
            array
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| "Characters token without data".to_string())?
                .to_owned(),
        )),
        "Comment" => Ok(SerializerToken::Comment(
            array
                .get(1)
                .and_then(Value::as_str)
                .ok_or_else(|| "Comment token without data".to_string())?
                .to_owned(),
        )),
        other => Err(format!("unknown serializer token type `{other}`")),
    }
}

/// Loads all cases from a `tests/html5lib-tests/serializer/*.test` file.
pub fn load_serializer_suite_file(path: &Path) -> Result<Vec<RawSerializerCase>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    #[derive(Deserialize)]
    struct File {
        tests: Vec<RawSerializerCase>,
    }
    let file: File = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON in {}: {e}", path.display()))?;
    Ok(file.tests)
}

const VOID_ELEMENTS: &[&str] = &[
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

const RCDATA_ELEMENTS: &[&str] = &[
    "style", "script", "xmp", "iframe", "noembed", "noframes", "noscript",
];

/// html5lib's `booleanAttributes` table (empty key = attributes boolean on any element).
const BOOLEAN_ATTRIBUTES: &[(&str, &[&str])] = &[
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

fn is_boolean_attribute(tag_name: &str, attr_name: &str) -> bool {
    BOOLEAN_ATTRIBUTES
        .iter()
        .any(|(tag, attrs)| (*tag == tag_name || tag.is_empty()) && attrs.contains(&attr_name))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Attribute values need quoting under the fixture dialect's "legacy" rule: ASCII whitespace,
/// `"`, `'`, `=`, `>` and `` ` ``. Note that `<` does not force quoting in this fixture dialect
/// (it is escaped instead), and neither does U+000B.
fn needs_legacy_quotes(value: &str) -> bool {
    value.chars().any(|ch| {
        matches!(
            ch,
            '\t' | '\n' | '\u{0C}' | '\r' | ' ' | '"' | '\'' | '=' | '>' | '`'
        )
    })
}

fn token_is_space_characters(token: &SerializerToken) -> bool {
    matches!(token, SerializerToken::Spaces(_))
}

fn is_space_character(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\u{0C}' | '\r' | ' ')
}

/// Splits a `Characters` token the way html5lib's `TreeWalker.text()` does: a leading run and a
/// trailing run of ASCII whitespace become `SpaceCharacters` tokens, the middle stays `Characters`.
fn split_characters_token(data: &str) -> Vec<SerializerToken> {
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
fn is_optional_start(
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
fn is_optional_end(tag_name: &str, next: Option<&SerializerToken>) -> bool {
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
fn omit_optional_tags(tokens: Vec<SerializerToken>) -> Vec<SerializerToken> {
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
fn strip_whitespace(tokens: Vec<SerializerToken>) -> Vec<SerializerToken> {
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

fn collapse_spaces(text: &str) -> String {
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
fn inject_meta_charset(tokens: Vec<SerializerToken>, encoding: &str) -> Vec<SerializerToken> {
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

/// Serializes a token stream using html5lib's serializer semantics and the resolved options.
fn serialize_tokens(tokens: &[SerializerToken], options: &EffectiveOptions) -> String {
    let mut out = String::new();
    let mut in_cdata = false;
    for token in tokens {
        match token {
            SerializerToken::Doctype {
                name,
                public_id,
                system_id,
            } => {
                out.push_str("<!DOCTYPE ");
                out.push_str(name);
                let public_id = public_id.as_deref().filter(|id| !id.is_empty());
                let system_id = system_id.as_deref().filter(|id| !id.is_empty());
                if let Some(public_id) = public_id {
                    out.push_str(&format!(" PUBLIC \"{public_id}\""));
                    if let Some(system_id) = system_id {
                        out.push_str(&format!(" \"{system_id}\""));
                    }
                } else if let Some(system_id) = system_id {
                    out.push_str(&format!(" SYSTEM \"{system_id}\""));
                }
                out.push('>');
            }
            SerializerToken::Characters(data) => {
                if in_cdata {
                    out.push_str(data);
                } else {
                    out.push_str(&html_escape(data));
                }
            }
            SerializerToken::Spaces(data) => out.push_str(data),
            SerializerToken::StartTag { name, attrs, .. }
            | SerializerToken::EmptyTag { name, attrs } => {
                out.push('<');
                out.push_str(name);
                if RCDATA_ELEMENTS.contains(&name.as_str()) && !options.escape_rcdata {
                    in_cdata = true;
                }
                for attr in attrs {
                    serialize_attribute(&mut out, name, attr, options);
                }
                if VOID_ELEMENTS.contains(&name.as_str()) && options.use_trailing_solidus {
                    if options.space_before_trailing_solidus {
                        out.push_str(" /");
                    } else {
                        out.push('/');
                    }
                }
                out.push('>');
            }
            SerializerToken::EndTag { name, .. } => {
                if RCDATA_ELEMENTS.contains(&name.as_str()) {
                    in_cdata = false;
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
            SerializerToken::Comment(data) => {
                out.push_str("<!--");
                out.push_str(data);
                out.push_str("-->");
            }
        }
    }
    out
}

fn serialize_attribute(
    out: &mut String,
    tag_name: &str,
    attr: &SerializerAttr,
    options: &EffectiveOptions,
) {
    out.push(' ');
    out.push_str(&attr.name);
    let minimized =
        options.minimize_boolean_attributes && is_boolean_attribute(tag_name, &attr.name);
    if minimized {
        return;
    }
    let mut value = html_escape(&attr.value);
    if options.escape_lt_in_attrs && !attr.value.contains('<') {
        // html_escape already escapes `<` in this fixture dialect; keep the option explicit.
    }
    let needs_quotes =
        options.quote_attr_values || value.is_empty() || needs_legacy_quotes(&attr.value);
    if !needs_quotes {
        out.push('=');
        out.push_str(&value);
        return;
    }
    let mut quote_char = options.quote_char;
    if options.use_best_quote_char {
        if value.contains('\'') && !value.contains('"') {
            quote_char = '"';
        } else if value.contains('"') && !value.contains('\'') {
            quote_char = '\'';
        }
    }
    if quote_char == '\'' {
        value = value.replace('\'', "&#39;");
    } else {
        value = value.replace('"', "&quot;");
    }
    out.push('=');
    out.push(quote_char);
    out.push_str(&value);
    out.push(quote_char);
}

/// Runs one fixture case: parse tokens, apply html5lib filters in the canonical order and
/// serialize; the case passes when the output matches any `expected` alternative.
pub fn run_serializer_case(
    file_name: &str,
    index: usize,
    case: &RawSerializerCase,
) -> Result<SerializerRunResult, String> {
    let options = EffectiveOptions::from_fixture(&case.options);
    let mut tokens: Vec<SerializerToken> = case
        .input
        .iter()
        .enumerate()
        .map(|(token_index, value)| {
            parse_serializer_token(value)
                .map_err(|e| format!("{file_name} case {index} input[{token_index}]: {e}"))
        })
        .collect::<Result<_, _>>()?;

    // Mimic `JsonWalker`/`TreeWalker.text()`: split `Characters` tokens into the
    // `SpaceCharacters`/`Characters` pair html5lib's tree walker would emit.
    tokens = tokens
        .into_iter()
        .flat_map(|token| match token {
            SerializerToken::Characters(data) => split_characters_token(&data),
            other => vec![other],
        })
        .collect();

    // Filter order matches html5lib's `HTMLSerializer.serialize`: inject meta charset, then
    // strip whitespace, then omit optional tags.
    if options.inject_meta_charset
        && let Some(encoding) = options.encoding.as_deref()
    {
        tokens = inject_meta_charset(tokens, encoding);
    }
    if options.strip_whitespace {
        tokens = strip_whitespace(tokens);
    }
    if options.omit_optional_tags {
        tokens = omit_optional_tags(tokens);
    }

    let actual = serialize_tokens(&tokens, &options);
    let passed = case.expected.contains(&actual);
    Ok(SerializerRunResult {
        file_name: file_name.to_owned(),
        index,
        description: case.description.clone(),
        passed,
        expected: case.expected.clone(),
        actual,
        has_xhtml: case
            .xhtml
            .as_ref()
            .is_some_and(|alternatives| !alternatives.is_empty()),
    })
}

/// Runs every case in a `serializer/*.test` file.
pub fn run_serializer_suite_file(path: &Path) -> Result<Vec<SerializerRunResult>, String> {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.test")
        .to_owned();
    let cases = load_serializer_suite_file(path)?;
    cases
        .iter()
        .enumerate()
        .map(|(index, case)| run_serializer_case(&file_name, index, case))
        .collect()
}
