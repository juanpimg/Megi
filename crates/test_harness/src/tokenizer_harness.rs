//! # `html5lib-tests` Tokenizer JSON Conformance Harness (`tokenizer_harness`)
//!
//! Parses `tests/html5lib-tests/tokenizer/*.test` JSON files, handles `doubleEscaped`
//! `\uHHHH` hex unescaping (preserving lone UTF-16 surrogates via `Vec<u32>` code-point
//! buffers), runs [`html::tokenizer::Tokenizer`] across every state in `initialStates`,
//! coalesces adjacent `Character` tokens, and compares actual vs expected output.

use std::collections::BTreeMap;
use std::path::Path;

use html::tokenizer::{Token, Tokenizer, TokenizerConfig, TokenizerState};
use serde::Deserialize;
use serde_json::Value;

/// Raw JSON schema for a `tokenizer/*.test` file.
#[derive(Clone, Debug, Deserialize)]
pub struct RawTokenizerFile {
    #[serde(default)]
    pub tests: Vec<RawTokenizerCase>,
    #[serde(default, rename = "xmlViolationTests")]
    pub xml_violation_tests: Vec<RawTokenizerCase>,
}

/// Raw JSON schema for a single test case inside a `.test` file.
#[derive(Clone, Debug, Deserialize)]
pub struct RawTokenizerCase {
    pub description: String,
    pub input: String,
    pub output: Vec<Value>,
    #[serde(default = "default_initial_states", rename = "initialStates")]
    pub initial_states: Vec<String>,
    #[serde(default, rename = "lastStartTag")]
    pub last_start_tag: Option<String>,
    #[serde(default, rename = "doubleEscaped")]
    pub double_escaped: bool,
    #[serde(default)]
    pub errors: Vec<RawTokenizerError>,
}

fn default_initial_states() -> Vec<String> {
    vec!["Data state".to_string()]
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RawTokenizerError {
    pub code: String,
    pub line: usize,
    pub col: usize,
}

/// Normalized, canonical representation of an `html5lib-tests` output token.
/// Uses `Vec<u32>` for character/string payloads so lone surrogates in `doubleEscaped`
/// tests (`unicodeCharsProblematic.test`, `domjs.test`) can be compared accurately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NormalizedToken {
    Doctype {
        name: Option<Vec<u32>>,
        public_id: Option<Vec<u32>>,
        system_id: Option<Vec<u32>>,
        correctness: bool,
    },
    StartTag {
        name: Vec<u32>,
        attributes: BTreeMap<Vec<u32>, Vec<u32>>,
        self_closing: bool,
    },
    EndTag {
        name: Vec<u32>,
    },
    Comment(Vec<u32>),
    ProcessingInstruction {
        target: Vec<u32>,
        data: Vec<u32>,
    },
    Character(Vec<u32>),
}

/// Result of running a single `(test_case, initial_state)` execution.
#[derive(Clone, Debug)]
pub struct TokenizerRunResult {
    pub file_name: String,
    pub index: usize,
    pub description: String,
    pub initial_state: String,
    pub passed: bool,
    pub expected: Vec<NormalizedToken>,
    pub actual: Vec<NormalizedToken>,
}

/// Unescapes `\uHHHH` sequences when `double_escaped == true`, returning raw Unicode
/// code points (`u32`, including lone surrogates `0xD800..=0xDFFF`).
pub fn decode_test_string(s: &str, double_escaped: bool) -> Vec<u32> {
    if !double_escaped {
        return s.chars().map(|c| c as u32).collect();
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'u' if i + 5 < bytes.len() => {
                    if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 2..i + 6])
                        && let Ok(val) = u32::from_str_radix(hex_str, 16)
                    {
                        out.push(val);
                        i += 6;
                        continue;
                    }
                }
                b'n' => {
                    out.push(0x0A);
                    i += 2;
                    continue;
                }
                b'r' => {
                    out.push(0x0D);
                    i += 2;
                    continue;
                }
                b't' => {
                    out.push(0x09);
                    i += 2;
                    continue;
                }
                b'\\' => {
                    out.push(b'\\' as u32);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch as u32);
        i += ch.len_utf8();
    }
    out
}

fn str_to_cps(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

fn missing_token_field(kind: &str, slot: usize) -> String {
    format!("malformed expected {kind} token: missing/non-string slot {slot}")
}

/// Parses an expected token array from `test.output` JSON into [`NormalizedToken`].
///
/// Returns an error (instead of silently dropping the token) for unknown token kinds or malformed
/// shapes: an unparsed expectation must fail the suite, never shrink the denominator.
pub fn parse_expected_token(val: &Value, double_escaped: bool) -> Result<NormalizedToken, String> {
    let arr = val
        .as_array()
        .ok_or_else(|| format!("expected a token array, got {val}"))?;
    let kind = arr
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| format!("expected a token array starting with a type string, got {val}"))?;
    match kind {
        "DOCTYPE" => {
            let name = arr
                .get(1)
                .ok_or_else(|| missing_token_field(kind, 1))?
                .as_str()
                .map(|s| decode_test_string(s, double_escaped));
            let public_id = arr
                .get(2)
                .ok_or_else(|| missing_token_field(kind, 2))?
                .as_str()
                .map(|s| decode_test_string(s, double_escaped));
            let system_id = arr
                .get(3)
                .ok_or_else(|| missing_token_field(kind, 3))?
                .as_str()
                .map(|s| decode_test_string(s, double_escaped));
            let correctness = arr
                .get(4)
                .and_then(Value::as_bool)
                .ok_or_else(|| missing_token_field(kind, 4))?;
            Ok(NormalizedToken::Doctype {
                name,
                public_id,
                system_id,
                correctness,
            })
        }
        "StartTag" => {
            let name = decode_test_string(
                arr.get(1)
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing_token_field(kind, 1))?,
                double_escaped,
            );
            let attrs_obj = arr
                .get(2)
                .and_then(Value::as_object)
                .ok_or_else(|| missing_token_field(kind, 2))?;
            let mut attributes = BTreeMap::new();
            for (k, v) in attrs_obj {
                let k_cps = decode_test_string(k, double_escaped);
                let v_cps = decode_test_string(
                    v.as_str().ok_or_else(|| {
                        format!(
                            "malformed expected StartTag attribute {k:?}: value is not a string"
                        )
                    })?,
                    double_escaped,
                );
                attributes.insert(k_cps, v_cps);
            }
            // The html5lib-tests format makes `self_closing` optional (absent == false).
            let self_closing = arr.get(3).and_then(Value::as_bool).unwrap_or(false);
            Ok(NormalizedToken::StartTag {
                name,
                attributes,
                self_closing,
            })
        }
        "EndTag" => {
            let name = decode_test_string(
                arr.get(1)
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing_token_field(kind, 1))?,
                double_escaped,
            );
            Ok(NormalizedToken::EndTag { name })
        }
        "Comment" => {
            let data = decode_test_string(
                arr.get(1)
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing_token_field(kind, 1))?,
                double_escaped,
            );
            Ok(NormalizedToken::Comment(data))
        }
        "ProcessingInstruction" => {
            let target = decode_test_string(
                arr.get(1)
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing_token_field(kind, 1))?,
                double_escaped,
            );
            let data = arr
                .get(2)
                .and_then(Value::as_str)
                .map(|s| decode_test_string(s, double_escaped))
                .unwrap_or_default();
            Ok(NormalizedToken::ProcessingInstruction { target, data })
        }
        "Character" => {
            let data = decode_test_string(
                arr.get(1)
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing_token_field(kind, 1))?,
                double_escaped,
            );
            Ok(NormalizedToken::Character(data))
        }
        other => Err(format!("unknown expected token type `{other}`")),
    }
}

/// Coalesces adjacent `Character` tokens in a [` NormalizedToken `] list.
pub fn coalesce_normalized_tokens(tokens: Vec<NormalizedToken>) -> Vec<NormalizedToken> {
    let mut out: Vec<NormalizedToken> = Vec::with_capacity(tokens.len());
    for tok in tokens {
        if let NormalizedToken::Character(cps) = tok {
            if let Some(NormalizedToken::Character(prev)) = out.last_mut() {
                prev.extend(cps);
            } else if !cps.is_empty() {
                out.push(NormalizedToken::Character(cps));
            }
        } else {
            out.push(tok);
        }
    }
    out
}

/// Converts raw [`html::tokenizer::Token`] stream emitted by the engine into
/// coalesced [`NormalizedToken`]s matching `html5lib-tests` format.
pub fn normalize_engine_tokens(tokens: &[Token]) -> Vec<NormalizedToken> {
    let mut raw = Vec::new();
    for tok in tokens {
        match tok {
            Token::Doctype {
                name,
                public_id,
                system_id,
                force_quirks,
            } => {
                raw.push(NormalizedToken::Doctype {
                    name: name.as_deref().map(str_to_cps),
                    public_id: public_id.as_deref().map(str_to_cps),
                    system_id: system_id.as_deref().map(str_to_cps),
                    correctness: !*force_quirks,
                });
            }
            Token::StartTag {
                name,
                self_closing,
                attributes,
            } => {
                let mut attr_map = BTreeMap::new();
                for (k, v) in attributes {
                    attr_map.insert(str_to_cps(k), str_to_cps(v));
                }
                raw.push(NormalizedToken::StartTag {
                    name: str_to_cps(name),
                    attributes: attr_map,
                    self_closing: *self_closing,
                });
            }
            Token::EndTag { name, .. } => {
                raw.push(NormalizedToken::EndTag {
                    name: str_to_cps(name),
                });
            }
            Token::Comment(data) => {
                raw.push(NormalizedToken::Comment(str_to_cps(data)));
            }
            Token::ProcessingInstruction { target, data } => {
                raw.push(NormalizedToken::ProcessingInstruction {
                    target: str_to_cps(target),
                    data: str_to_cps(data),
                });
            }
            Token::Character(cp) => {
                raw.push(NormalizedToken::Character(vec![*cp]));
            }
            Token::Eof => {}
        }
    }
    coalesce_normalized_tokens(raw)
}

/// Loads all test cases from a `tokenizer/*.test` JSON file.
///
/// Refuses fixtures that define both `tests` and `xmlViolationTests`: the old implementation
/// silently dropped `tests` in that case.
pub fn load_tokenizer_test_file(path: &Path) -> Result<(bool, Vec<RawTokenizerCase>), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let parsed: RawTokenizerFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON in {}: {e}", path.display()))?;
    if !parsed.xml_violation_tests.is_empty() && !parsed.tests.is_empty() {
        return Err(format!(
            "{} defines both `tests` and `xmlViolationTests`; refusing to silently drop one of them",
            path.display()
        ));
    }
    if !parsed.xml_violation_tests.is_empty() {
        Ok((true, parsed.xml_violation_tests))
    } else {
        Ok((false, parsed.tests))
    }
}

/// Runs all test cases in a `tokenizer/*.test` file and returns detailed results.
pub fn run_tokenizer_suite_file(path: &Path) -> Result<Vec<TokenizerRunResult>, String> {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.test")
        .to_owned();
    let (_is_xml_violation, cases) = load_tokenizer_test_file(path)?;
    let mut results = Vec::new();

    for (idx, case) in cases.iter().enumerate() {
        if case.initial_states.is_empty() {
            return Err(format!(
                "{file_name} #{idx} has an empty `initialStates` array; expected at least one state"
            ));
        }
        let input_cps = decode_test_string(&case.input, case.double_escaped);
        let expected_raw: Vec<NormalizedToken> = case
            .output
            .iter()
            .enumerate()
            .map(|(token_idx, value)| {
                parse_expected_token(value, case.double_escaped)
                    .map_err(|e| format!("{file_name} #{idx} output[{token_idx}]: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let expected = coalesce_normalized_tokens(expected_raw);

        for state_name in &case.initial_states {
            let initial_state = TokenizerState::from_test_name(state_name).ok_or_else(|| {
                format!(
                    "{file_name} #{idx} requests unsupported initial state {state_name:?}; \
                     refusing to silently run it as Data state"
                )
            })?;
            let mut tok = Tokenizer::new(
                &input_cps,
                TokenizerConfig {
                    initial_state,
                    last_start_tag: case.last_start_tag.clone(),
                },
            );
            tok.run();
            let actual = normalize_engine_tokens(&tok.tokens);
            let passed = actual == expected;
            results.push(TokenizerRunResult {
                file_name: file_name.clone(),
                index: idx,
                description: case.description.clone(),
                initial_state: state_name.clone(),
                passed,
                expected: expected.clone(),
                actual,
            });
        }
    }

    Ok(results)
}
