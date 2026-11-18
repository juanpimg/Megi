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
//! WHATWG §13.3 implementation used by `innerHTML`/`outerHTML` lives in `megi_html::serializer`
//! (`serialize_html`, `serialize_fragment`, `serialize_outer_html`).
//!
//! The XHTML alternatives (`test.xhtml`) are not exercised because Megi implements the HTML
//! serialization, not the XHTML one; tests still have to match an `expected` (HTML) string.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

mod optional;
mod serialize;

use optional::{
    RCDATA_ELEMENTS, VOID_ELEMENTS, html_escape, inject_meta_charset, is_boolean_attribute,
    needs_legacy_quotes, omit_optional_tags, split_characters_token, strip_whitespace,
};
use serialize::serialize_tokens;

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
