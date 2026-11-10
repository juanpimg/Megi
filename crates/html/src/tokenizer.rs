//! # WHATWG HTML §13.2.5 Tokenization
//!
//! Implements the 84-state HTML Tokenizer state machine (§13.2.5.1 through §13.2.5.84),
//! all 52 normative parse error codes (§13.2.2), appropriate end tag checking via
//! `last_start_tag`, Processing Instruction states (§13.2.5.72–76), numeric/named
//! character references (§13.2.5.77–84), and all 7 emitted token variants.

use crate::input_stream::{
    is_ascii_whitespace, is_control_character_in_stream, is_noncharacter, is_surrogate,
    preprocess_str_with_line_starts, preprocess_stream,
};
use std::collections::HashSet;

/// All 52 normative parse error codes defined in WHATWG HTML §13.2.2 (`#parse-errors`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParseErrorCode {
    AbruptClosingOfEmptyComment,
    AbruptDoctypePublicIdentifier,
    AbruptDoctypeSystemIdentifier,
    AbsenceOfDigitsInNumericCharacterReference,
    CdataInHtmlContent,
    CharacterReferenceOutsideUnicodeRange,
    ControlCharacterInInputStream,
    ControlCharacterReference,
    DisallowedProcessingInstructionTarget,
    DuplicateAttribute,
    EndTagWithAttributes,
    EndTagWithTrailingSolidus,
    EofBeforeTagName,
    EofInCdata,
    EofInComment,
    EofInDoctype,
    EofInProcessingInstruction,
    EofInScriptHtmlCommentLikeText,
    EofInTag,
    IncorrectlyClosedComment,
    IncorrectlyOpenedComment,
    InvalidCharacterSequenceAfterDoctypeName,
    InvalidFirstCharacterOfProcessingInstructionTarget,
    InvalidFirstCharacterOfTagName,
    InvalidProcessingInstructionTarget,
    MissingAttributeValue,
    MissingDoctypeName,
    MissingDoctypePublicIdentifier,
    MissingDoctypeSystemIdentifier,
    MissingEndTagName,
    MissingQuoteBeforeDoctypePublicIdentifier,
    MissingQuoteBeforeDoctypeSystemIdentifier,
    MissingSemicolonAfterCharacterReference,
    MissingWhitespaceAfterDoctypePublicKeyword,
    MissingWhitespaceAfterDoctypeSystemKeyword,
    MissingWhitespaceBeforeDoctypeName,
    MissingWhitespaceBetweenAttributes,
    MissingWhitespaceBetweenDoctypePublicAndSystemIdentifiers,
    NestedComment,
    NoncharacterCharacterReference,
    NoncharacterInInputStream,
    NonVoidHtmlElementStartTagWithTrailingSolidus,
    NullCharacterReference,
    SurrogateCharacterReference,
    SurrogateInInputStream,
    UnexpectedCharacterAfterDoctypeSystemIdentifier,
    UnexpectedCharacterInAttributeName,
    UnexpectedCharacterInUnquotedAttributeValue,
    UnexpectedEqualsSignBeforeAttributeName,
    UnexpectedNullCharacter,
    UnexpectedSolidusInTag,
    UnknownNamedCharacterReference,
}

impl ParseErrorCode {
    /// Returns the exact kebab-case normative code string from WHATWG HTML §13.2.2.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AbruptClosingOfEmptyComment => "abrupt-closing-of-empty-comment",
            Self::AbruptDoctypePublicIdentifier => "abrupt-doctype-public-identifier",
            Self::AbruptDoctypeSystemIdentifier => "abrupt-doctype-system-identifier",
            Self::AbsenceOfDigitsInNumericCharacterReference => {
                "absence-of-digits-in-numeric-character-reference"
            }
            Self::CdataInHtmlContent => "cdata-in-html-content",
            Self::CharacterReferenceOutsideUnicodeRange => {
                "character-reference-outside-unicode-range"
            }
            Self::ControlCharacterInInputStream => "control-character-in-input-stream",
            Self::ControlCharacterReference => "control-character-reference",
            Self::DisallowedProcessingInstructionTarget => {
                "disallowed-processing-instruction-target"
            }
            Self::DuplicateAttribute => "duplicate-attribute",
            Self::EndTagWithAttributes => "end-tag-with-attributes",
            Self::EndTagWithTrailingSolidus => "end-tag-with-trailing-solidus",
            Self::EofBeforeTagName => "eof-before-tag-name",
            Self::EofInCdata => "eof-in-cdata",
            Self::EofInComment => "eof-in-comment",
            Self::EofInDoctype => "eof-in-doctype",
            Self::EofInProcessingInstruction => "eof-in-processing-instruction",
            Self::EofInScriptHtmlCommentLikeText => "eof-in-script-html-comment-like-text",
            Self::EofInTag => "eof-in-tag",
            Self::IncorrectlyClosedComment => "incorrectly-closed-comment",
            Self::IncorrectlyOpenedComment => "incorrectly-opened-comment",
            Self::InvalidCharacterSequenceAfterDoctypeName => {
                "invalid-character-sequence-after-doctype-name"
            }
            Self::InvalidFirstCharacterOfProcessingInstructionTarget => {
                "invalid-first-character-of-processing-instruction-target"
            }
            Self::InvalidFirstCharacterOfTagName => "invalid-first-character-of-tag-name",
            Self::InvalidProcessingInstructionTarget => "invalid-processing-instruction-target",
            Self::MissingAttributeValue => "missing-attribute-value",
            Self::MissingDoctypeName => "missing-doctype-name",
            Self::MissingDoctypePublicIdentifier => "missing-doctype-public-identifier",
            Self::MissingDoctypeSystemIdentifier => "missing-doctype-system-identifier",
            Self::MissingEndTagName => "missing-end-tag-name",
            Self::MissingQuoteBeforeDoctypePublicIdentifier => {
                "missing-quote-before-doctype-public-identifier"
            }
            Self::MissingQuoteBeforeDoctypeSystemIdentifier => {
                "missing-quote-before-doctype-system-identifier"
            }
            Self::MissingSemicolonAfterCharacterReference => {
                "missing-semicolon-after-character-reference"
            }
            Self::MissingWhitespaceAfterDoctypePublicKeyword => {
                "missing-whitespace-after-doctype-public-keyword"
            }
            Self::MissingWhitespaceAfterDoctypeSystemKeyword => {
                "missing-whitespace-after-doctype-system-keyword"
            }
            Self::MissingWhitespaceBeforeDoctypeName => "missing-whitespace-before-doctype-name",
            Self::MissingWhitespaceBetweenAttributes => "missing-whitespace-between-attributes",
            Self::MissingWhitespaceBetweenDoctypePublicAndSystemIdentifiers => {
                "missing-whitespace-between-doctype-public-and-system-identifiers"
            }
            Self::NestedComment => "nested-comment",
            Self::NoncharacterCharacterReference => "noncharacter-character-reference",
            Self::NoncharacterInInputStream => "noncharacter-in-input-stream",
            Self::NonVoidHtmlElementStartTagWithTrailingSolidus => {
                "non-void-html-element-start-tag-with-trailing-solidus"
            }
            Self::NullCharacterReference => "null-character-reference",
            Self::SurrogateCharacterReference => "surrogate-character-reference",
            Self::SurrogateInInputStream => "surrogate-in-input-stream",
            Self::UnexpectedCharacterAfterDoctypeSystemIdentifier => {
                "unexpected-character-after-doctype-system-identifier"
            }
            Self::UnexpectedCharacterInAttributeName => "unexpected-character-in-attribute-name",
            Self::UnexpectedCharacterInUnquotedAttributeValue => {
                "unexpected-character-in-unquoted-attribute-value"
            }
            Self::UnexpectedEqualsSignBeforeAttributeName => {
                "unexpected-equals-sign-before-attribute-name"
            }
            Self::UnexpectedNullCharacter => "unexpected-null-character",
            Self::UnexpectedSolidusInTag => "unexpected-solidus-in-tag",
            Self::UnknownNamedCharacterReference => "unknown-named-character-reference",
        }
    }
}

/// Recorded parse error with 1-based line and column numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub code: ParseErrorCode,
    pub line: usize,
    pub col: usize,
}

/// Returns `true` if `cp` is a valid XML 1.0 `Char` production
/// (`#x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`).
///
/// Used by the optional WHATWG HTML §13.2.9 "Coercing an HTML DOM into an infoset" mode.
pub const fn is_valid_xml_character(cp: u32) -> bool {
    matches!(cp, 0x9 | 0xA | 0xD)
        || matches!(cp, 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// Applies the §13.2.9 character tweaks: U+000C FORM FEED becomes U+0020 SPACE and any other
/// literal non-XML character becomes U+FFFD REPLACEMENT CHARACTER.
const fn xml_coerce_codepoint(cp: u32) -> u32 {
    if cp == 0x0C {
        0x20
    } else if is_valid_xml_character(cp) {
        cp
    } else {
        0xFFFD
    }
}

/// Applies the §13.2.9 character tweaks to a `String` used as character data or an attribute value.
fn xml_coerce_text(text: &str) -> String {
    if text.chars().all(|ch| is_valid_xml_character(ch as u32)) {
        return text.to_owned();
    }
    text.chars()
        .map(|ch| char::from_u32(xml_coerce_codepoint(ch as u32)).unwrap_or('\u{FFFD}'))
        .collect()
}

/// Applies the §13.2.9 comment tweaks: non-XML characters/FF are coerced like character data and
/// a U+0020 SPACE is inserted between two consecutive U+002D HYPHEN-MINUS characters.
fn xml_coerce_comment(text: &str) -> String {
    let coerced = xml_coerce_text(text);
    let mut out = String::with_capacity(coerced.len());
    let mut previous = None;
    for ch in coerced.chars() {
        if previous == Some('-') && ch == '-' {
            out.push(' ');
        }
        out.push(ch);
        previous = Some(ch);
    }
    out
}

/// How html5lib-tests expects the position of a given parse error to be encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrorPositionMode {
    /// Position of the most recently consumed character (Megi's historical behaviour).
    Current,
    /// Position of the next input character (EOF / character-reference / lookahead families,
    /// which html5lib records after advancing past the character that triggered the error).
    Next,
}

/// Position convention per parse error code, derived from the html5lib-tests fixtures.
///
/// All codes not listed here use [`ErrorPositionMode::Current`]. The residue of codes that still
/// differ after this split is tracked explicitly by `test_harness::KNOWN_POSITION_GAPS`.
const fn error_position_mode(code: ParseErrorCode) -> ErrorPositionMode {
    use ParseErrorCode::*;
    match code {
        // End-of-file errors are reported at the position the parser would read next.
        EofBeforeTagName
        | EofInCdata
        | EofInComment
        | EofInDoctype
        | EofInProcessingInstruction
        | EofInScriptHtmlCommentLikeText
        | EofInTag => ErrorPositionMode::Next,
        // Character-reference errors raised by the numeric-reference end state are reported after
        // the reference's terminator was read; the numeric parsing states report at the offending
        // character and override the mode with `record_error_at_current`.
        ControlCharacterReference
        | MissingSemicolonAfterCharacterReference
        | NoncharacterCharacterReference
        | NullCharacterReference
        | SurrogateCharacterReference => ErrorPositionMode::Next,
        // Raised while leaving the markup-declaration-open state, after the lookahead char.
        IncorrectlyOpenedComment => ErrorPositionMode::Next,
        _ => ErrorPositionMode::Current,
    }
}

/// All 84 states of the WHATWG HTML Living Standard Tokenizer (§13.2.5.1 – §13.2.5.84).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TokenizerState {
    /// 13.2.5.1 Data state (`#data-state`)
    #[default]
    Data,
    /// 13.2.5.2 RCDATA state (`#rcdata-state`)
    Rcdata,
    /// 13.2.5.3 RAWTEXT state (`#rawtext-state`)
    Rawtext,
    /// 13.2.5.4 Script data state (`#script-data-state`)
    ScriptData,
    /// 13.2.5.5 PLAINTEXT state (`#plaintext-state`)
    Plaintext,
    /// 13.2.5.6 Tag open state (`#tag-open-state`)
    TagOpen,
    /// 13.2.5.7 End tag open state (`#end-tag-open-state`)
    EndTagOpen,
    /// 13.2.5.8 Tag name state (`#tag-name-state`)
    TagName,
    /// 13.2.5.9 RCDATA less-than sign state (`#rcdata-less-than-sign-state`)
    RcdataLessThanSign,
    /// 13.2.5.10 RCDATA end tag open state (`#rcdata-end-tag-open-state`)
    RcdataEndTagOpen,
    /// 13.2.5.11 RCDATA end tag name state (`#rcdata-end-tag-name-state`)
    RcdataEndTagName,
    /// 13.2.5.12 RAWTEXT less-than sign state (`#rawtext-less-than-sign-state`)
    RawtextLessThanSign,
    /// 13.2.5.13 RAWTEXT end tag open state (`#rawtext-end-tag-open-state`)
    RawtextEndTagOpen,
    /// 13.2.5.14 RAWTEXT end tag name state (`#rawtext-end-tag-name-state`)
    RawtextEndTagName,
    /// 13.2.5.15 Script data less-than sign state (`#script-data-less-than-sign-state`)
    ScriptDataLessThanSign,
    /// 13.2.5.16 Script data end tag open state (`#script-data-end-tag-open-state`)
    ScriptDataEndTagOpen,
    /// 13.2.5.17 Script data end tag name state (`#script-data-end-tag-name-state`)
    ScriptDataEndTagName,
    /// 13.2.5.18 Script data escape start state (`#script-data-escape-start-state`)
    ScriptDataEscapeStart,
    /// 13.2.5.19 Script data escape start dash state (`#script-data-escape-start-dash-state`)
    ScriptDataEscapeStartDash,
    /// 13.2.5.20 Script data escaped state (`#script-data-escaped-state`)
    ScriptDataEscaped,
    /// 13.2.5.21 Script data escaped dash state (`#script-data-escaped-dash-state`)
    ScriptDataEscapedDash,
    /// 13.2.5.22 Script data escaped dash dash state (`#script-data-escaped-dash-dash-state`)
    ScriptDataEscapedDashDash,
    /// 13.2.5.23 Script data escaped less-than sign state (`#script-data-escaped-less-than-sign-state`)
    ScriptDataEscapedLessThanSign,
    /// 13.2.5.24 Script data escaped end tag open state (`#script-data-escaped-end-tag-open-state`)
    ScriptDataEscapedEndTagOpen,
    /// 13.2.5.25 Script data escaped end tag name state (`#script-data-escaped-end-tag-name-state`)
    ScriptDataEscapedEndTagName,
    /// 13.2.5.26 Script data double escape start state (`#script-data-double-escape-start-state`)
    ScriptDataDoubleEscapeStart,
    /// 13.2.5.27 Script data double escaped state (`#script-data-double-escaped-state`)
    ScriptDataDoubleEscaped,
    /// 13.2.5.28 Script data double escaped dash state (`#script-data-double-escaped-dash-state`)
    ScriptDataDoubleEscapedDash,
    /// 13.2.5.29 Script data double escaped dash dash state (`#script-data-double-escaped-dash-dash-state`)
    ScriptDataDoubleEscapedDashDash,
    /// 13.2.5.30 Script data double escaped less-than sign state (`#script-data-double-escaped-less-than-sign-state`)
    ScriptDataDoubleEscapedLessThanSign,
    /// 13.2.5.31 Script data double escape end state (`#script-data-double-escape-end-state`)
    ScriptDataDoubleEscapeEnd,
    /// 13.2.5.32 Before attribute name state (`#before-attribute-name-state`)
    BeforeAttributeName,
    /// 13.2.5.33 Attribute name state (`#attribute-name-state`)
    AttributeName,
    /// 13.2.5.34 After attribute name state (`#after-attribute-name-state`)
    AfterAttributeName,
    /// 13.2.5.35 Before attribute value state (`#before-attribute-value-state`)
    BeforeAttributeValue,
    /// 13.2.5.36 Attribute value (double-quoted) state (`#attribute-value-(double-quoted)-state`)
    AttributeValueDoubleQuoted,
    /// 13.2.5.37 Attribute value (single-quoted) state (`#attribute-value-(single-quoted)-state`)
    AttributeValueSingleQuoted,
    /// 13.2.5.38 Attribute value (unquoted) state (`#attribute-value-(unquoted)-state`)
    AttributeValueUnquoted,
    /// 13.2.5.39 After attribute value (quoted) state (`#after-attribute-value-(quoted)-state`)
    AfterAttributeValueQuoted,
    /// 13.2.5.40 Self-closing start tag state (`#self-closing-start-tag-state`)
    SelfClosingStartTag,
    /// 13.2.5.41 Bogus comment state (`#bogus-comment-state`)
    BogusComment,
    /// 13.2.5.42 Markup declaration open state (`#markup-declaration-open-state`)
    MarkupDeclarationOpen,
    /// 13.2.5.43 Comment start state (`#comment-start-state`)
    CommentStart,
    /// 13.2.5.44 Comment start dash state (`#comment-start-dash-state`)
    CommentStartDash,
    /// 13.2.5.45 Comment state (`#comment-state`)
    Comment,
    /// 13.2.5.46 Comment less-than sign state (`#comment-less-than-sign-state`)
    CommentLessThanSign,
    /// 13.2.5.47 Comment less-than sign bang state (`#comment-less-than-sign-bang-state`)
    CommentLessThanSignBang,
    /// 13.2.5.48 Comment less-than sign bang dash state (`#comment-less-than-sign-bang-dash-state`)
    CommentLessThanSignBangDash,
    /// 13.2.5.49 Comment less-than sign bang dash dash state (`#comment-less-than-sign-bang-dash-dash-state`)
    CommentLessThanSignBangDashDash,
    /// 13.2.5.50 Comment end dash state (`#comment-end-dash-state`)
    CommentEndDash,
    /// 13.2.5.51 Comment end state (`#comment-end-state`)
    CommentEnd,
    /// 13.2.5.52 Comment end bang state (`#comment-end-bang-state`)
    CommentEndBang,
    /// 13.2.5.53 DOCTYPE state (`#doctype-state`)
    Doctype,
    /// 13.2.5.54 Before DOCTYPE name state (`#before-doctype-name-state`)
    BeforeDoctypeName,
    /// 13.2.5.55 DOCTYPE name state (`#doctype-name-state`)
    DoctypeName,
    /// 13.2.5.56 After DOCTYPE name state (`#after-doctype-name-state`)
    AfterDoctypeName,
    /// 13.2.5.57 After DOCTYPE public keyword state (`#after-doctype-public-keyword-state`)
    AfterDoctypePublicKeyword,
    /// 13.2.5.58 Before DOCTYPE public identifier state (`#before-doctype-public-identifier-state`)
    BeforeDoctypePublicIdentifier,
    /// 13.2.5.59 DOCTYPE public identifier (double-quoted) state (`#doctype-public-identifier-(double-quoted)-state`)
    DoctypePublicIdentifierDoubleQuoted,
    /// 13.2.5.60 DOCTYPE public identifier (single-quoted) state (`#doctype-public-identifier-(single-quoted)-state`)
    DoctypePublicIdentifierSingleQuoted,
    /// 13.2.5.61 After DOCTYPE public identifier state (`#after-doctype-public-identifier-state`)
    AfterDoctypePublicIdentifier,
    /// 13.2.5.62 Between DOCTYPE public and system identifiers state (`#between-doctype-public-and-system-identifiers-state`)
    BetweenDoctypePublicAndSystemIdentifiers,
    /// 13.2.5.63 After DOCTYPE system keyword state (`#after-doctype-system-keyword-state`)
    AfterDoctypeSystemKeyword,
    /// 13.2.5.64 Before DOCTYPE system identifier state (`#before-doctype-system-identifier-state`)
    BeforeDoctypeSystemIdentifier,
    /// 13.2.5.65 DOCTYPE system identifier (double-quoted) state (`#doctype-system-identifier-(double-quoted)-state`)
    DoctypeSystemIdentifierDoubleQuoted,
    /// 13.2.5.66 DOCTYPE system identifier (single-quoted) state (`#doctype-system-identifier-(single-quoted)-state`)
    DoctypeSystemIdentifierSingleQuoted,
    /// 13.2.5.67 After DOCTYPE system identifier state (`#after-doctype-system-identifier-state`)
    AfterDoctypeSystemIdentifier,
    /// 13.2.5.68 Bogus DOCTYPE state (`#bogus-doctype-state`)
    BogusDoctype,
    /// 13.2.5.69 CDATA section state (`#cdata-section-state`)
    CdataSection,
    /// 13.2.5.70 CDATA section bracket state (`#cdata-section-bracket-state`)
    CdataSectionBracket,
    /// 13.2.5.71 CDATA section end state (`#cdata-section-end-state`)
    CdataSectionEnd,
    /// 13.2.5.72 Processing instruction open state (`#processing-instruction-open-state`)
    ProcessingInstructionOpen,
    /// 13.2.5.73 Processing instruction target state (`#processing-instruction-target-state`)
    ProcessingInstructionTarget,
    /// 13.2.5.74 After processing instruction target state (`#after-processing-instruction-target-state`)
    AfterProcessingInstructionTarget,
    /// 13.2.5.75 Processing instruction data state (`#processing-instruction-data-state`)
    ProcessingInstructionData,
    /// 13.2.5.76 Processing instruction questionable state (`#processing-instruction-questionable-state`)
    ProcessingInstructionQuestionable,
    /// 13.2.5.77 Character reference state (`#character-reference-state`)
    CharacterReference,
    /// 13.2.5.78 Named character reference state (`#named-character-reference-state`)
    NamedCharacterReference,
    /// 13.2.5.79 Ambiguous ampersand state (`#ambiguous-ampersand-state`)
    AmbiguousAmpersand,
    /// 13.2.5.80 Numeric character reference state (`#numeric-character-reference-state`)
    NumericCharacterReference,
    /// 13.2.5.81 Hexadecimal character reference start state (`#hexadecimal-character-reference-start-state`)
    HexadecimalCharacterReferenceStart,
    /// 13.2.5.82 Hexadecimal character reference state (`#hexadecimal-character-reference-state`)
    HexadecimalCharacterReference,
    /// 13.2.5.83 Decimal character reference state (`#decimal-character-reference-state`)
    DecimalCharacterReference,
    /// 13.2.5.84 Numeric character reference end state (`#numeric-character-reference-end-state`)
    NumericCharacterReferenceEnd,
}

impl TokenizerState {
    /// Parses one of the 6 `initialStates` names used in `html5lib-tests/tokenizer/*.test`.
    pub fn from_test_name(name: &str) -> Option<Self> {
        match name {
            "Data state" => Some(Self::Data),
            "PLAINTEXT state" => Some(Self::Plaintext),
            "RCDATA state" => Some(Self::Rcdata),
            "RAWTEXT state" => Some(Self::Rawtext),
            "Script data state" => Some(Self::ScriptData),
            "CDATA section state" => Some(Self::CdataSection),
            _ => None,
        }
    }

    pub const fn is_attribute_value_state(self) -> bool {
        matches!(
            self,
            Self::AttributeValueDoubleQuoted
                | Self::AttributeValueSingleQuoted
                | Self::AttributeValueUnquoted
        )
    }
}

/// Token emitted by the HTML Tokenizer (§13.2.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Doctype {
        name: Option<String>,
        public_id: Option<String>,
        system_id: Option<String>,
        force_quirks: bool,
    },
    StartTag {
        name: String,
        self_closing: bool,
        attributes: Vec<(String, String)>,
    },
    EndTag {
        name: String,
        self_closing: bool,
        attributes: Vec<(String, String)>,
    },
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
    /// Emitted code point (uses `u32` so lone surrogates in `doubleEscaped` tests survive intact).
    Character(u32),
    Eof,
}

/// Configuration for initializing a [`Tokenizer`].
#[derive(Clone, Debug, Default)]
pub struct TokenizerConfig {
    pub initial_state: TokenizerState,
    pub last_start_tag: Option<String>,
}

#[derive(Clone, Debug)]
struct PendingTag {
    is_end_tag: bool,
    name: String,
    self_closing: bool,
    attributes: Vec<(String, String)>,
    /// Set of attribute names already present in `attributes`, used to detect
    /// duplicates in amortized `O(1)` instead of a linear scan (which made a tag with
    /// many attributes quadratic per tag; see the tokenizer anti-DoS notes).
    attribute_names: HashSet<String>,
    current_attr_name: String,
    current_attr_value: String,
    has_current_attr: bool,
    /// 1-based `(line, col)` where the current attribute name ended (used for the
    /// `duplicate-attribute` position, matching html5lib-tests expectations).
    name_end_line: usize,
    name_end_col: usize,
}

impl PendingTag {
    fn new_start() -> Self {
        Self {
            is_end_tag: false,
            name: String::new(),
            self_closing: false,
            attributes: Vec::new(),
            attribute_names: HashSet::new(),
            current_attr_name: String::new(),
            current_attr_value: String::new(),
            has_current_attr: false,
            name_end_line: 1,
            name_end_col: 1,
        }
    }

    fn new_end() -> Self {
        Self {
            is_end_tag: true,
            name: String::new(),
            self_closing: false,
            attributes: Vec::new(),
            attribute_names: HashSet::new(),
            current_attr_name: String::new(),
            current_attr_value: String::new(),
            has_current_attr: false,
            name_end_line: 1,
            name_end_col: 1,
        }
    }

    fn start_new_attribute(&mut self, errors: &mut Vec<ParseError>) {
        self.finish_attribute(errors);
        self.current_attr_name.clear();
        self.current_attr_value.clear();
        self.has_current_attr = true;
    }

    fn finish_attribute(&mut self, errors: &mut Vec<ParseError>) {
        if !self.has_current_attr {
            return;
        }
        self.has_current_attr = false;
        if self.current_attr_name.is_empty() {
            return;
        }
        let name = std::mem::take(&mut self.current_attr_name);
        let value = std::mem::take(&mut self.current_attr_value);
        // First occurrence wins; later duplicates are reported and dropped. The
        // The set keeps this amortized O(1) per attribute (a quadratic `.any()` scan
        // here was an algorithmic DoS: a single tag with tens of thousands of
        // attributes could burn seconds of CPU).
        if self.attribute_names.insert(name.clone()) {
            self.attributes.push((name, value));
        } else {
            errors.push(ParseError {
                code: ParseErrorCode::DuplicateAttribute,
                line: self.name_end_line,
                col: self.name_end_col,
            });
        }
    }
}

#[derive(Clone, Debug, Default)]
struct PendingDoctype {
    name: Option<String>,
    public_id: Option<String>,
    system_id: Option<String>,
    force_quirks: bool,
}

use crate::entities::NAMED_ENTITIES;

/// Windows-1252 replacement table for numeric character references `0x80..=0x9F` (§13.2.5.84).
const NUMERIC_REPLACEMENT_TABLE: [(u32, u32); 27] = [
    (0x80, 0x20AC),
    (0x82, 0x201A),
    (0x83, 0x0192),
    (0x84, 0x201E),
    (0x85, 0x2026),
    (0x86, 0x2020),
    (0x87, 0x2021),
    (0x88, 0x02C6),
    (0x89, 0x2030),
    (0x8A, 0x0160),
    (0x8B, 0x2039),
    (0x8C, 0x0152),
    (0x8E, 0x017D),
    (0x91, 0x2018),
    (0x92, 0x2019),
    (0x93, 0x201C),
    (0x94, 0x201D),
    (0x95, 0x2022),
    (0x96, 0x2013),
    (0x97, 0x2014),
    (0x98, 0x02DC),
    (0x99, 0x2122),
    (0x9A, 0x0161),
    (0x9B, 0x203A),
    (0x9C, 0x0153),
    (0x9E, 0x017E),
    (0x9F, 0x0178),
];

/// WHATWG HTML §13.2.5 Tokenizer state machine.
pub struct Tokenizer {
    input: Vec<u32>,
    pos: usize,
    reconsume: bool,
    current_cp: Option<u32>,
    pub state: TokenizerState,
    return_state: TokenizerState,
    last_start_tag: Option<String>,
    temp_buffer: Vec<u32>,
    pending_tag: Option<PendingTag>,
    pending_comment: String,
    pending_doctype: Option<PendingDoctype>,
    pending_pi_target: String,
    pending_pi_data: String,
    char_ref_code: u32,
    pub allow_cdata: bool,
    pub tokens: Vec<Token>,
    pub errors: Vec<ParseError>,
    /// 0-based indices of the first code point of each line (used for error positions).
    line_starts: Vec<usize>,
    /// Prefix sums of non-BMP code points (UTF-16 surrogate-pair columns in html5lib-tests
    /// positions). `None` when the input is entirely BMP, keeping the common path allocation-free.
    wide_prefix: Option<Vec<u32>>,
    /// Position of the whitespace that ended a processing-instruction target (`None` before it).
    /// html5lib-tests reports `eof-in-processing-instruction` at that position.
    pi_data_start: Option<(usize, usize)>,
    /// Position convention to use for errors raised in the numeric-character-reference end state.
    numeric_end_mode: ErrorPositionMode,
    /// When enabled, emitted tokens are coerced per WHATWG HTML §13.2.9 so they match the
    /// html5lib-tests `xmlViolationTests` expectations (XML infoset compatibility).
    xml_coercion: bool,
}

impl Tokenizer {
    /// Creates a new `Tokenizer` from raw code points (which are preprocessed via §13.2.3.5).
    pub fn new(raw_code_points: &[u32], config: TokenizerConfig) -> Self {
        let (input, errors, line_starts) = preprocess_stream(raw_code_points.iter().copied());
        Self::from_parts(input, errors, line_starts, config)
    }

    /// Builds a tokenizer from already-preprocessed code points and line starts.
    fn from_parts(
        input: Vec<u32>,
        errors: Vec<ParseError>,
        line_starts: Vec<usize>,
        config: TokenizerConfig,
    ) -> Self {
        let wide_prefix = input.iter().any(|cp| *cp > 0xFFFF).then(|| {
            let mut prefix = Vec::with_capacity(input.len() + 1);
            prefix.push(0u32);
            for cp in &input {
                let previous = *prefix.last().expect("prefix starts with 0");
                prefix.push(previous + u32::from(*cp > 0xFFFF));
            }
            prefix
        });
        Self {
            input,
            pos: 0,
            reconsume: false,
            current_cp: None,
            state: config.initial_state,
            return_state: TokenizerState::Data,
            last_start_tag: config.last_start_tag.map(|s| s.to_ascii_lowercase()),
            temp_buffer: Vec::new(),
            pending_tag: None,
            pending_comment: String::new(),
            pending_doctype: None,
            pending_pi_target: String::new(),
            pending_pi_data: String::new(),
            char_ref_code: 0,
            allow_cdata: false,
            tokens: Vec::new(),
            errors,
            line_starts,
            wide_prefix,
            pi_data_start: None,
            numeric_end_mode: ErrorPositionMode::Next,
            xml_coercion: false,
        }
    }

    /// Creates a `Tokenizer` from a UTF-8 `&str` in a single preprocessing pass (no
    /// intermediate `Vec<u32>` for the raw code points).
    pub fn from_str_input(input: &str, config: TokenizerConfig) -> Self {
        let (input, errors, line_starts) = preprocess_str_with_line_starts(input);
        Self::from_parts(input, errors, line_starts, config)
    }

    /// Enables WHATWG HTML §13.2.9 "Coercing an HTML DOM into an infoset" tweaks on the emitted
    /// tokens: non-XML characters (including U+FFFF) become U+FFFD, U+000C becomes a space, and
    /// `--` inside comments becomes `- -`. Only the html5lib-tests `xmlViolationTests` need this.
    pub fn enable_xml_coercion(&mut self) {
        self.xml_coercion = true;
    }

    /// Returns whether §13.2.9 token coercion is enabled.
    pub const fn xml_coercion_enabled(&self) -> bool {
        self.xml_coercion
    }

    /// Runs the tokenizer until `Token::Eof` is emitted.
    pub fn run(&mut self) {
        while !matches!(self.tokens.last(), Some(Token::Eof)) {
            self.step();
        }
    }

    /// Fast path for the "data" state: consumes consecutive ordinary characters (anything
    /// but `&`, `<` and NUL) and appends their code points to `out`, returning how many were
    /// consumed. The caller (the tree builder) batches those characters exactly like the
    /// individual `Token::Character` emissions they would otherwise produce, so the
    /// observable token/tree output is unchanged; only the per-character token plumbing is
    /// skipped for plain text runs.
    pub(crate) fn consume_data_run(&mut self, out: &mut Vec<u32>) -> usize {
        if self.state != TokenizerState::Data || self.reconsume {
            return 0;
        }
        let mut consumed = 0usize;
        while self.pos < self.input.len() {
            let cp = self.input[self.pos];
            if matches!(cp, 0x26 | 0x3C | 0x00) {
                break;
            }
            self.pos += 1;
            self.current_cp = Some(cp);
            out.push(cp);
            consumed += 1;
        }
        consumed
    }

    /// WP-6.4: inserts `html` into the input stream at the tokenizer's current position.
    ///
    /// The tree builder calls this with the HTML written by `document.write` from a
    /// parser-inserted `<script>`; the written markup must be tokenized **immediately after**
    /// the `</script>` token (WHATWG HTML §13.2.6.4.7), before the remaining document input.
    /// The string is preprocessed like the document input (§13.2.3.5: CRLF/CR normalization
    /// plus input-stream errors) and the line-start table is shifted so later parse errors
    /// keep their `(line, column)` positions.
    ///
    /// Cost: this method is only called when a script actually wrote with `document.write`;
    /// documents without scripts never reach it.
    pub(crate) fn insert_input(&mut self, html: &str) {
        if html.is_empty() {
            return;
        }
        let (inserted, errors, inserted_line_starts) = preprocess_str_with_line_starts(html);
        self.errors.extend(errors);
        if inserted.is_empty() {
            return;
        }
        // A reconsume request means the next character to process is already at `pos - 1`.
        let insert_at = if self.reconsume {
            self.pos.saturating_sub(1)
        } else {
            self.pos
        }
        .min(self.input.len());
        let inserted_len = inserted.len();

        // Shift the remaining line starts, then merge the inserted text's own line starts.
        for start in &mut self.line_starts {
            if *start > insert_at {
                *start += inserted_len;
            }
        }
        for start in inserted_line_starts.iter().skip(1) {
            let absolute = insert_at + start;
            let index = self
                .line_starts
                .partition_point(|&existing| existing < absolute);
            self.line_starts.insert(index, absolute);
        }

        self.input.splice(insert_at..insert_at, inserted);

        // UTF-16 column prefix sums (html5lib `(line, UTF-16 column)` positions) only need
        // rebuilding when astral code points are involved.
        if self.wide_prefix.is_some() || self.input.iter().any(|cp| *cp > 0xFFFF) {
            let mut prefix = Vec::with_capacity(self.input.len() + 1);
            prefix.push(0u32);
            for cp in &self.input {
                let previous = *prefix.last().expect("prefix starts with 0");
                prefix.push(previous + u32::from(*cp > 0xFFFF));
            }
            self.wide_prefix = Some(prefix);
        }
    }

    fn consume_next(&mut self) -> Option<u32> {
        if self.reconsume {
            self.reconsume = false;
            return self.current_cp;
        }
        if self.pos < self.input.len() {
            let cp = self.input[self.pos];
            self.pos += 1;
            self.current_cp = Some(cp);
            Some(cp)
        } else {
            self.current_cp = None;
            None
        }
    }

    fn peek_slice_ascii_eq(&self, target: &str, ignore_case: bool) -> bool {
        let bytes = target.as_bytes();
        if self.pos + bytes.len() > self.input.len() {
            return false;
        }
        for (i, &b) in bytes.iter().enumerate() {
            let cp = self.input[self.pos + i];
            if cp > 0x7F {
                return false;
            }
            let ch = cp as u8;
            if ignore_case {
                if !ch.eq_ignore_ascii_case(&b) {
                    return false;
                }
            } else if ch != b {
                return false;
            }
        }
        true
    }

    /// Returns the 1-based `(line, col)` of the code point at 0-based `index`; `input.len()` maps
    /// to the EOF position (one past the last code point, or the start of the line after a
    /// trailing newline).
    fn line_col_at(&self, index: usize) -> (usize, usize) {
        if self.input.is_empty() {
            return (1, 1);
        }
        if index < self.input.len() {
            let line = self.line_starts.partition_point(|start| *start <= index);
            // html5lib-tests columns count UTF-16 code units, so a non-BMP code point occupies two.
            let line_start = self.line_starts[line - 1];
            let wide_units = self
                .wide_prefix
                .as_ref()
                .map_or(0, |prefix| prefix[index] - prefix[line_start]);
            (line, index - line_start + 1 + wide_units as usize)
        } else if self.input[self.input.len() - 1] == 0x0A {
            (self.line_starts.len(), 1)
        } else {
            let last = self.input[self.input.len() - 1];
            let (line, col) = self.line_col_at(self.input.len() - 1);
            (line, col + if last > 0xFFFF { 2 } else { 1 })
        }
    }

    /// Position of the most recently consumed code point (1-based).
    fn current_position(&self) -> (usize, usize) {
        if self.pos == 0 {
            (1, 1)
        } else {
            self.line_col_at(self.pos - 1)
        }
    }

    /// Position of the next code point to consume (1-based), i.e. one past the last consumed one.
    fn next_position(&self) -> (usize, usize) {
        self.line_col_at(self.pos)
    }

    /// Records a parse error at the position of the most recently consumed character, overriding
    /// the code's default position mode (used by the numeric-character-reference parsing states).
    fn record_error_at_current(&mut self, code: ParseErrorCode) {
        let (line, col) = self.current_position();
        self.errors.push(ParseError { code, line, col });
    }

    /// Records a parse error at the position of the next input character (EOF position at EOF).
    fn record_error_at_next(&mut self, code: ParseErrorCode) {
        let (line, col) = self.next_position();
        self.errors.push(ParseError { code, line, col });
    }

    /// Records an error raised in the numeric-character-reference end state, whose html5lib-tests
    /// position depends on how the state was entered (`;`/EOF vs. a non-`;` terminating character).
    fn record_error_in_numeric_end(&mut self, code: ParseErrorCode) {
        let (line, col) = match self.numeric_end_mode {
            ErrorPositionMode::Current => self.current_position(),
            ErrorPositionMode::Next => self.next_position(),
        };
        self.errors.push(ParseError { code, line, col });
    }

    fn record_error(&mut self, code: ParseErrorCode) {
        let (line, col) = if code == ParseErrorCode::EofInProcessingInstruction {
            // html5lib-tests pins this error to the whitespace that ended the PI target, if any.
            self.pi_data_start.unwrap_or_else(|| self.next_position())
        } else {
            match error_position_mode(code) {
                ErrorPositionMode::Current => self.current_position(),
                ErrorPositionMode::Next => self.next_position(),
            }
        };
        self.errors.push(ParseError { code, line, col });
    }

    fn emit_char(&mut self, cp: u32) {
        let cp = if self.xml_coercion {
            xml_coerce_codepoint(cp)
        } else {
            cp
        };
        self.tokens.push(Token::Character(cp));
    }

    fn push_cp_to_string(buf: &mut String, cp: u32) {
        if let Some(ch) = char::from_u32(cp) {
            buf.push(ch);
        } else {
            buf.push('\u{FFFD}');
        }
    }

    fn emit_current_tag(&mut self) {
        if let Some(mut tag) = self.pending_tag.take() {
            tag.finish_attribute(&mut self.errors);
            if tag.is_end_tag {
                if !tag.attributes.is_empty() {
                    self.record_error(ParseErrorCode::EndTagWithAttributes);
                }
                if tag.self_closing {
                    self.record_error(ParseErrorCode::EndTagWithTrailingSolidus);
                }
                self.tokens.push(Token::EndTag {
                    name: tag.name,
                    self_closing: tag.self_closing,
                    attributes: Vec::new(),
                });
            } else {
                self.last_start_tag = Some(tag.name.clone());
                let attributes = if self.xml_coercion {
                    tag.attributes
                        .into_iter()
                        .map(|(name, value)| (name, xml_coerce_text(&value)))
                        .collect()
                } else {
                    tag.attributes
                };
                self.tokens.push(Token::StartTag {
                    name: tag.name,
                    self_closing: tag.self_closing,
                    attributes,
                });
            }
        }
    }

    fn emit_current_comment(&mut self) {
        let data = std::mem::take(&mut self.pending_comment);
        let data = if self.xml_coercion {
            xml_coerce_comment(&data)
        } else {
            data
        };
        self.tokens.push(Token::Comment(data));
    }

    fn emit_current_doctype(&mut self) {
        if let Some(dt) = self.pending_doctype.take() {
            self.tokens.push(Token::Doctype {
                name: dt.name,
                public_id: dt.public_id,
                system_id: dt.system_id,
                force_quirks: dt.force_quirks,
            });
        }
    }

    fn emit_current_pi(&mut self) {
        let target = std::mem::take(&mut self.pending_pi_target);
        let data = std::mem::take(&mut self.pending_pi_data);
        self.tokens
            .push(Token::ProcessingInstruction { target, data });
    }

    fn convert_temp_buffer_to_comment(&mut self) {
        let mut s = String::from("?");
        for &cp in &self.temp_buffer {
            Self::push_cp_to_string(&mut s, cp);
        }
        self.pending_comment = s;
    }

    fn is_appropriate_end_tag(&self) -> bool {
        match (&self.pending_tag, &self.last_start_tag) {
            (Some(tag), Some(last)) => tag.name == *last,
            _ => false,
        }
    }

    fn flush_temp_buffer_as_end_tag_fallback(&mut self, return_to: TokenizerState) {
        self.emit_char(0x3C); // '<'
        self.emit_char(0x2F); // '/'
        let buf = self.temp_buffer.clone();
        for cp in buf {
            self.emit_char(cp);
        }
        self.pending_tag = None;
        self.reconsume = true;
        self.state = return_to;
    }

    fn flush_code_points_consumed_as_char_ref(&mut self) {
        let buf = self.temp_buffer.clone();
        if self.return_state.is_attribute_value_state() {
            if let Some(ref mut tag) = self.pending_tag {
                for cp in buf {
                    Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                }
            }
        } else {
            for cp in buf {
                self.emit_char(cp);
            }
        }
    }

    /// Executes a single state transition of the 84-state HTML Tokenizer.
    pub fn step(&mut self) {
        match self.state {
            // 13.2.5.1 Data state
            TokenizerState::Data => match self.consume_next() {
                Some(0x26) => {
                    self.return_state = TokenizerState::Data;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::TagOpen;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0x00);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.2 RCDATA state
            TokenizerState::Rcdata => match self.consume_next() {
                Some(0x26) => {
                    self.return_state = TokenizerState::Rcdata;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::RcdataLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.3 RAWTEXT state
            TokenizerState::Rawtext => match self.consume_next() {
                Some(0x3C) => {
                    self.state = TokenizerState::RawtextLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.4 Script data state
            TokenizerState::ScriptData => match self.consume_next() {
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.5 PLAINTEXT state
            TokenizerState::Plaintext => match self.consume_next() {
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                Some(cp) => self.emit_char(cp),
                None => self.tokens.push(Token::Eof),
            },

            // 13.2.5.6 Tag open state
            TokenizerState::TagOpen => match self.consume_next() {
                Some(0x21) => {
                    self.state = TokenizerState::MarkupDeclarationOpen;
                }
                Some(0x2F) => {
                    self.state = TokenizerState::EndTagOpen;
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_start());
                    self.reconsume = true;
                    self.state = TokenizerState::TagName;
                }
                Some(0x3F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::ProcessingInstructionOpen;
                }
                None => {
                    self.record_error(ParseErrorCode::EofBeforeTagName);
                    self.emit_char(0x3C);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::InvalidFirstCharacterOfTagName);
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::Data;
                }
            },

            // 13.2.5.7 End tag open state
            TokenizerState::EndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::TagName;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingEndTagName);
                    self.state = TokenizerState::Data;
                }
                None => {
                    self.record_error(ParseErrorCode::EofBeforeTagName);
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::InvalidFirstCharacterOfTagName);
                    self.pending_comment.clear();
                    self.reconsume = true;
                    self.state = TokenizerState::BogusComment;
                }
            },

            // 13.2.5.8 Tag name state
            TokenizerState::TagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push((cp + 0x20) as u8 as char);
                    }
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push('\u{FFFD}');
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.name, cp);
                    }
                }
            },

            // 13.2.5.9 RCDATA less-than sign state
            TokenizerState::RcdataLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::RcdataEndTagOpen;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::Rcdata;
                }
            },

            // 13.2.5.10 RCDATA end tag open state
            TokenizerState::RcdataEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::RcdataEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::Rcdata;
                }
            },

            // 13.2.5.11 RCDATA end tag name state
            TokenizerState::RcdataEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.temp_buffer.push(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::Rcdata),
            },

            // 13.2.5.12 RAWTEXT less-than sign state
            TokenizerState::RawtextLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::RawtextEndTagOpen;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::Rawtext;
                }
            },

            // 13.2.5.13 RAWTEXT end tag open state
            TokenizerState::RawtextEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::RawtextEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::Rawtext;
                }
            },

            // 13.2.5.14 RAWTEXT end tag name state
            TokenizerState::RawtextEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.temp_buffer.push(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::Rawtext),
            },

            // 13.2.5.15 Script data less-than sign state
            TokenizerState::ScriptDataLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::ScriptDataEndTagOpen;
                }
                Some(0x21) => {
                    self.emit_char(0x3C);
                    self.emit_char(0x21);
                    self.state = TokenizerState::ScriptDataEscapeStart;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.16 Script data end tag open state
            TokenizerState::ScriptDataEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.17 Script data end tag name state
            TokenizerState::ScriptDataEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.temp_buffer.push(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::ScriptData),
            },

            // 13.2.5.18 Script data escape start state
            TokenizerState::ScriptDataEscapeStart => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapeStartDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.19 Script data escape start dash state
            TokenizerState::ScriptDataEscapeStartDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapedDashDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptData;
                }
            },

            // 13.2.5.20 Script data escaped state
            TokenizerState::ScriptDataEscaped => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapedDash;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => self.emit_char(cp),
            },

            // 13.2.5.21 Script data escaped dash state
            TokenizerState::ScriptDataEscapedDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataEscapedDashDash;
                }
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.22 Script data escaped dash dash state
            TokenizerState::ScriptDataEscapedDashDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                }
                Some(0x3C) => {
                    self.state = TokenizerState::ScriptDataEscapedLessThanSign;
                }
                Some(0x3E) => {
                    self.emit_char(0x3E);
                    self.state = TokenizerState::ScriptData;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.23 Script data escaped less-than sign state
            TokenizerState::ScriptDataEscapedLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.state = TokenizerState::ScriptDataEscapedEndTagOpen;
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.temp_buffer.clear();
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataDoubleEscapeStart;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.24 Script data escaped end tag open state
            TokenizerState::ScriptDataEscapedEndTagOpen => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.pending_tag = Some(PendingTag::new_end());
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscapedEndTagName;
                }
                _ => {
                    self.emit_char(0x3C);
                    self.emit_char(0x2F);
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.25 Script data escaped end tag name state
            TokenizerState::ScriptDataEscapedEndTagName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) && self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) if self.is_appropriate_end_tag() => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    let lower = (cp as u8).to_ascii_lowercase() as char;
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name.push(lower);
                    }
                    self.temp_buffer.push(cp);
                }
                _ => self.flush_temp_buffer_as_end_tag_fallback(TokenizerState::ScriptDataEscaped),
            },

            // 13.2.5.26 Script data double escape start state
            TokenizerState::ScriptDataDoubleEscapeStart => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) || matches!(cp, 0x2F | 0x3E) => {
                    let is_script = self.temp_buffer
                        == [
                            b's' as u32,
                            b'c' as u32,
                            b'r' as u32,
                            b'i' as u32,
                            b'p' as u32,
                            b't' as u32,
                        ];
                    self.emit_char(cp);
                    self.state = if is_script {
                        TokenizerState::ScriptDataDoubleEscaped
                    } else {
                        TokenizerState::ScriptDataEscaped
                    };
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.temp_buffer
                        .push((cp as u8).to_ascii_lowercase() as u32);
                    self.emit_char(cp);
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataEscaped;
                }
            },

            // 13.2.5.27 Script data double escaped state
            TokenizerState::ScriptDataDoubleEscaped => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataDoubleEscapedDash;
                }
                Some(0x3C) => {
                    self.emit_char(0x3C);
                    self.state = TokenizerState::ScriptDataDoubleEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => self.emit_char(cp),
            },

            // 13.2.5.28 Script data double escaped dash state
            TokenizerState::ScriptDataDoubleEscapedDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                    self.state = TokenizerState::ScriptDataDoubleEscapedDashDash;
                }
                Some(0x3C) => {
                    self.emit_char(0x3C);
                    self.state = TokenizerState::ScriptDataDoubleEscapedLessThanSign;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },

            // 13.2.5.29 Script data double escaped dash dash state
            TokenizerState::ScriptDataDoubleEscapedDashDash => match self.consume_next() {
                Some(0x2D) => {
                    self.emit_char(0x2D);
                }
                Some(0x3C) => {
                    self.emit_char(0x3C);
                    self.state = TokenizerState::ScriptDataDoubleEscapedLessThanSign;
                }
                Some(0x3E) => {
                    self.emit_char(0x3E);
                    self.state = TokenizerState::ScriptData;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.emit_char(0xFFFD);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInScriptHtmlCommentLikeText);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    self.emit_char(cp);
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },

            // 13.2.5.30 Script data double escaped less-than sign state
            TokenizerState::ScriptDataDoubleEscapedLessThanSign => match self.consume_next() {
                Some(0x2F) => {
                    self.temp_buffer.clear();
                    self.emit_char(0x2F);
                    self.state = TokenizerState::ScriptDataDoubleEscapeEnd;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },

            // 13.2.5.31 Script data double escape end state
            TokenizerState::ScriptDataDoubleEscapeEnd => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) || matches!(cp, 0x2F | 0x3E) => {
                    let is_script = self.temp_buffer
                        == [
                            b's' as u32,
                            b'c' as u32,
                            b'r' as u32,
                            b'i' as u32,
                            b'p' as u32,
                            b't' as u32,
                        ];
                    self.emit_char(cp);
                    self.state = if is_script {
                        TokenizerState::ScriptDataEscaped
                    } else {
                        TokenizerState::ScriptDataDoubleEscaped
                    };
                }
                Some(cp) if (cp as u8).is_ascii_alphabetic() && cp <= 0x7F => {
                    self.temp_buffer
                        .push((cp as u8).to_ascii_lowercase() as u32);
                    self.emit_char(cp);
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ScriptDataDoubleEscaped;
                }
            },

            // 13.2.5.32 Before attribute name state
            TokenizerState::BeforeAttributeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x2F | 0x3E) | None => {
                    self.reconsume = true;
                    self.state = TokenizerState::AfterAttributeName;
                }
                Some(0x3D) => {
                    self.record_error(ParseErrorCode::UnexpectedEqualsSignBeforeAttributeName);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.start_new_attribute(&mut self.errors);
                        tag.current_attr_name.push('=');
                    }
                    self.state = TokenizerState::AttributeName;
                }
                Some(_) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.start_new_attribute(&mut self.errors);
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::AttributeName;
                }
            },

            // 13.2.5.33 Attribute name state
            TokenizerState::AttributeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) || matches!(cp, 0x2F | 0x3E) => {
                    let (line, col) = self.current_position();
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name_end_line = line;
                        tag.name_end_col = col;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::AfterAttributeName;
                }
                None => {
                    self.reconsume = true;
                    self.state = TokenizerState::AfterAttributeName;
                }
                Some(0x3D) => {
                    let (line, col) = self.current_position();
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.name_end_line = line;
                        tag.name_end_col = col;
                    }
                    self.state = TokenizerState::BeforeAttributeValue;
                }
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_name.push((cp + 0x20) as u8 as char);
                    }
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_name.push('\u{FFFD}');
                    }
                }
                Some(cp @ (0x22 | 0x27 | 0x3C)) => {
                    self.record_error(ParseErrorCode::UnexpectedCharacterInAttributeName);
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_name, cp);
                    }
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_name, cp);
                    }
                }
            },

            // 13.2.5.34 After attribute name state
            TokenizerState::AfterAttributeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x2F) => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3D) => {
                    self.state = TokenizerState::BeforeAttributeValue;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.start_new_attribute(&mut self.errors);
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::AttributeName;
                }
            },

            // 13.2.5.35 Before attribute value state
            TokenizerState::BeforeAttributeValue => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x22) => {
                    self.state = TokenizerState::AttributeValueDoubleQuoted;
                }
                Some(0x27) => {
                    self.state = TokenizerState::AttributeValueSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingAttributeValue);
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::AttributeValueUnquoted;
                }
            },

            // 13.2.5.36 Attribute value (double-quoted) state
            TokenizerState::AttributeValueDoubleQuoted => match self.consume_next() {
                Some(0x22) => {
                    self.state = TokenizerState::AfterAttributeValueQuoted;
                }
                Some(0x26) => {
                    self.return_state = TokenizerState::AttributeValueDoubleQuoted;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_value.push('\u{FFFD}');
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
            },

            // 13.2.5.37 Attribute value (single-quoted) state
            TokenizerState::AttributeValueSingleQuoted => match self.consume_next() {
                Some(0x27) => {
                    self.state = TokenizerState::AfterAttributeValueQuoted;
                }
                Some(0x26) => {
                    self.return_state = TokenizerState::AttributeValueSingleQuoted;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_value.push('\u{FFFD}');
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
            },

            // 13.2.5.38 Attribute value (unquoted) state
            TokenizerState::AttributeValueUnquoted => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x26) => {
                    self.return_state = TokenizerState::AttributeValueUnquoted;
                    self.state = TokenizerState::CharacterReference;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.current_attr_value.push('\u{FFFD}');
                    }
                }
                Some(cp @ (0x22 | 0x27 | 0x3C | 0x3D | 0x60)) => {
                    self.record_error(ParseErrorCode::UnexpectedCharacterInUnquotedAttributeValue);
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                    }
                }
            },

            // 13.2.5.39 After attribute value (quoted) state
            TokenizerState::AfterAttributeValueQuoted => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeAttributeName;
                }
                Some(0x2F) => {
                    self.state = TokenizerState::SelfClosingStartTag;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceBetweenAttributes);
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeAttributeName;
                }
            },

            // 13.2.5.40 Self-closing start tag state
            TokenizerState::SelfClosingStartTag => match self.consume_next() {
                Some(0x3E) => {
                    if let Some(ref mut tag) = self.pending_tag {
                        tag.self_closing = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_tag();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInTag);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::UnexpectedSolidusInTag);
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeAttributeName;
                }
            },

            // 13.2.5.41 Bogus comment state
            TokenizerState::BogusComment => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                None => {
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.pending_comment.push('\u{FFFD}');
                }
                Some(cp) => {
                    Self::push_cp_to_string(&mut self.pending_comment, cp);
                }
            },

            // 13.2.5.42 Markup declaration open state
            TokenizerState::MarkupDeclarationOpen => {
                if self.peek_slice_ascii_eq("--", false) {
                    self.pos += 2;
                    self.pending_comment.clear();
                    self.state = TokenizerState::CommentStart;
                } else if self.peek_slice_ascii_eq("DOCTYPE", true) {
                    self.pos += 7;
                    self.state = TokenizerState::Doctype;
                } else if self.peek_slice_ascii_eq("[CDATA[", false) {
                    self.pos += 7;
                    if self.allow_cdata {
                        self.state = TokenizerState::CdataSection;
                    } else {
                        // In HTML content without foreign adjusted node, CDATA is a parse error
                        self.record_error(ParseErrorCode::CdataInHtmlContent);
                        self.pending_comment = String::from("[CDATA[");
                        self.state = TokenizerState::BogusComment;
                    }
                } else {
                    self.record_error(ParseErrorCode::IncorrectlyOpenedComment);
                    self.pending_comment.clear();
                    self.state = TokenizerState::BogusComment;
                }
            }

            // 13.2.5.43 Comment start state
            TokenizerState::CommentStart => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentStartDash;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptClosingOfEmptyComment);
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.44 Comment start dash state
            TokenizerState::CommentStartDash => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentEnd;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptClosingOfEmptyComment);
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push('-');
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.45 Comment state
            TokenizerState::Comment => match self.consume_next() {
                Some(0x3C) => {
                    self.pending_comment.push('<');
                    self.state = TokenizerState::CommentLessThanSign;
                }
                Some(0x2D) => {
                    self.state = TokenizerState::CommentEndDash;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.pending_comment.push('\u{FFFD}');
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    Self::push_cp_to_string(&mut self.pending_comment, cp);
                }
            },

            // 13.2.5.46 Comment less-than sign state
            TokenizerState::CommentLessThanSign => match self.consume_next() {
                Some(0x21) => {
                    self.pending_comment.push('!');
                    self.state = TokenizerState::CommentLessThanSignBang;
                }
                Some(0x3C) => {
                    self.pending_comment.push('<');
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.47 Comment less-than sign bang state
            TokenizerState::CommentLessThanSignBang => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentLessThanSignBangDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.48 Comment less-than sign bang dash state
            TokenizerState::CommentLessThanSignBangDash => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentLessThanSignBangDashDash;
                }
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::CommentEndDash;
                }
            },

            // 13.2.5.49 Comment less-than sign bang dash dash state
            TokenizerState::CommentLessThanSignBangDashDash => match self.consume_next() {
                Some(0x3E) | None => {
                    self.reconsume = true;
                    self.state = TokenizerState::CommentEnd;
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::NestedComment);
                    self.reconsume = true;
                    self.state = TokenizerState::CommentEnd;
                }
            },

            // 13.2.5.50 Comment end dash state
            TokenizerState::CommentEndDash => match self.consume_next() {
                Some(0x2D) => {
                    self.state = TokenizerState::CommentEnd;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push('-');
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.51 Comment end state
            TokenizerState::CommentEnd => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                Some(0x21) => {
                    self.state = TokenizerState::CommentEndBang;
                }
                Some(0x2D) => {
                    self.pending_comment.push('-');
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push_str("--");
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.52 Comment end bang state
            TokenizerState::CommentEndBang => match self.consume_next() {
                Some(0x2D) => {
                    self.pending_comment.push_str("--!");
                    self.state = TokenizerState::CommentEndDash;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::IncorrectlyClosedComment);
                    self.state = TokenizerState::Data;
                    self.emit_current_comment();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInComment);
                    self.emit_current_comment();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.pending_comment.push_str("--!");
                    self.reconsume = true;
                    self.state = TokenizerState::Comment;
                }
            },

            // 13.2.5.53 DOCTYPE state
            TokenizerState::Doctype => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeDoctypeName;
                }
                Some(0x3E) => {
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeDoctypeName;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    self.pending_doctype = Some(PendingDoctype {
                        force_quirks: true,
                        ..Default::default()
                    });
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceBeforeDoctypeName);
                    self.reconsume = true;
                    self.state = TokenizerState::BeforeDoctypeName;
                }
            },

            // 13.2.5.54 Before DOCTYPE name state
            TokenizerState::BeforeDoctypeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    self.pending_doctype = Some(PendingDoctype {
                        name: Some(String::from((cp + 0x20) as u8 as char)),
                        ..Default::default()
                    });
                    self.state = TokenizerState::DoctypeName;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    self.pending_doctype = Some(PendingDoctype {
                        name: Some(String::from('\u{FFFD}')),
                        ..Default::default()
                    });
                    self.state = TokenizerState::DoctypeName;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypeName);
                    self.pending_doctype = Some(PendingDoctype {
                        force_quirks: true,
                        ..Default::default()
                    });
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    self.pending_doctype = Some(PendingDoctype {
                        force_quirks: true,
                        ..Default::default()
                    });
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    let mut s = String::new();
                    Self::push_cp_to_string(&mut s, cp);
                    self.pending_doctype = Some(PendingDoctype {
                        name: Some(s),
                        ..Default::default()
                    });
                    self.state = TokenizerState::DoctypeName;
                }
            },

            // 13.2.5.55 DOCTYPE name state
            TokenizerState::DoctypeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::AfterDoctypeName;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(cp) if (0x41..=0x5A).contains(&cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut name) = dt.name
                    {
                        name.push((cp + 0x20) as u8 as char);
                    }
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut name) = dt.name
                    {
                        name.push('\u{FFFD}');
                    }
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut name) = dt.name
                    {
                        Self::push_cp_to_string(name, cp);
                    }
                }
            },

            // 13.2.5.56 After DOCTYPE name state
            TokenizerState::AfterDoctypeName => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    // Rewind 1 code point to check 6-char PUBLIC or SYSTEM
                    let saved_pos = self.pos - 1;
                    self.pos = saved_pos;
                    if self.peek_slice_ascii_eq("PUBLIC", true) {
                        self.pos += 6;
                        self.state = TokenizerState::AfterDoctypePublicKeyword;
                    } else if self.peek_slice_ascii_eq("SYSTEM", true) {
                        self.pos += 6;
                        self.state = TokenizerState::AfterDoctypeSystemKeyword;
                    } else {
                        self.pos = saved_pos + 1;
                        self.record_error(ParseErrorCode::InvalidCharacterSequenceAfterDoctypeName);
                        if let Some(ref mut dt) = self.pending_doctype {
                            dt.force_quirks = true;
                        }
                        self.reconsume = true;
                        self.state = TokenizerState::BogusDoctype;
                    }
                }
            },

            // 13.2.5.57 After DOCTYPE public keyword state
            TokenizerState::AfterDoctypePublicKeyword => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeDoctypePublicIdentifier;
                }
                Some(0x22) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypePublicKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypePublicKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.58 Before DOCTYPE public identifier state
            TokenizerState::BeforeDoctypePublicIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x22) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.public_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypePublicIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.59 DOCTYPE public identifier (double-quoted) state
            TokenizerState::DoctypePublicIdentifierDoubleQuoted => match self.consume_next() {
                Some(0x22) => {
                    self.state = TokenizerState::AfterDoctypePublicIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut pid) = dt.public_id
                    {
                        pid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut pid) = dt.public_id
                    {
                        Self::push_cp_to_string(pid, cp);
                    }
                }
            },

            // 13.2.5.60 DOCTYPE public identifier (single-quoted) state
            TokenizerState::DoctypePublicIdentifierSingleQuoted => match self.consume_next() {
                Some(0x27) => {
                    self.state = TokenizerState::AfterDoctypePublicIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut pid) = dt.public_id
                    {
                        pid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypePublicIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut pid) = dt.public_id
                    {
                        Self::push_cp_to_string(pid, cp);
                    }
                }
            },

            // 13.2.5.61 After DOCTYPE public identifier state
            TokenizerState::AfterDoctypePublicIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BetweenDoctypePublicAndSystemIdentifiers;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(0x22) => {
                    self.record_error(
                        ParseErrorCode::MissingWhitespaceBetweenDoctypePublicAndSystemIdentifiers,
                    );
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    self.record_error(
                        ParseErrorCode::MissingWhitespaceBetweenDoctypePublicAndSystemIdentifiers,
                    );
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.62 Between DOCTYPE public and system identifiers state
            TokenizerState::BetweenDoctypePublicAndSystemIdentifiers => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(0x22) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.63 After DOCTYPE system keyword state
            TokenizerState::AfterDoctypeSystemKeyword => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {
                    self.state = TokenizerState::BeforeDoctypeSystemIdentifier;
                }
                Some(0x22) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypeSystemKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    self.record_error(ParseErrorCode::MissingWhitespaceAfterDoctypeSystemKeyword);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.64 Before DOCTYPE system identifier state
            TokenizerState::BeforeDoctypeSystemIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x22) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierDoubleQuoted;
                }
                Some(0x27) => {
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.system_id = Some(String::new());
                    }
                    self.state = TokenizerState::DoctypeSystemIdentifierSingleQuoted;
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::MissingDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::MissingQuoteBeforeDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.65 DOCTYPE system identifier (double-quoted) state
            TokenizerState::DoctypeSystemIdentifierDoubleQuoted => match self.consume_next() {
                Some(0x22) => {
                    self.state = TokenizerState::AfterDoctypeSystemIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        sid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        Self::push_cp_to_string(sid, cp);
                    }
                }
            },

            // 13.2.5.66 DOCTYPE system identifier (single-quoted) state
            TokenizerState::DoctypeSystemIdentifierSingleQuoted => match self.consume_next() {
                Some(0x27) => {
                    self.state = TokenizerState::AfterDoctypeSystemIdentifier;
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        sid.push('\u{FFFD}');
                    }
                }
                Some(0x3E) => {
                    self.record_error(ParseErrorCode::AbruptDoctypeSystemIdentifier);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    if let Some(ref mut dt) = self.pending_doctype
                        && let Some(ref mut sid) = dt.system_id
                    {
                        Self::push_cp_to_string(sid, cp);
                    }
                }
            },

            // 13.2.5.67 After DOCTYPE system identifier state
            TokenizerState::AfterDoctypeSystemIdentifier => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInDoctype);
                    if let Some(ref mut dt) = self.pending_doctype {
                        dt.force_quirks = true;
                    }
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(
                        ParseErrorCode::UnexpectedCharacterAfterDoctypeSystemIdentifier,
                    );
                    self.reconsume = true;
                    self.state = TokenizerState::BogusDoctype;
                }
            },

            // 13.2.5.68 Bogus DOCTYPE state
            TokenizerState::BogusDoctype => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_doctype();
                }
                Some(0x00) => {
                    self.record_error(ParseErrorCode::UnexpectedNullCharacter);
                }
                None => {
                    self.emit_current_doctype();
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {}
            },

            // 13.2.5.69 CDATA section state
            TokenizerState::CdataSection => match self.consume_next() {
                Some(0x5D) => {
                    self.state = TokenizerState::CdataSectionBracket;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInCdata);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => self.emit_char(cp),
            },

            // 13.2.5.70 CDATA section bracket state
            TokenizerState::CdataSectionBracket => match self.consume_next() {
                Some(0x5D) => {
                    self.state = TokenizerState::CdataSectionEnd;
                }
                _ => {
                    self.emit_char(0x5D);
                    self.reconsume = true;
                    self.state = TokenizerState::CdataSection;
                }
            },

            // 13.2.5.71 CDATA section end state
            TokenizerState::CdataSectionEnd => match self.consume_next() {
                Some(0x5D) => {
                    self.emit_char(0x5D);
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                }
                _ => {
                    self.emit_char(0x5D);
                    self.emit_char(0x5D);
                    self.reconsume = true;
                    self.state = TokenizerState::CdataSection;
                }
            },

            // 13.2.5.72 Processing instruction open state
            TokenizerState::ProcessingInstructionOpen => match self.consume_next() {
                Some(cp) if ((cp as u8).is_ascii_alphabetic() && cp <= 0x7F) || cp == 0x5F => {
                    self.reconsume = true;
                    self.state = TokenizerState::ProcessingInstructionTarget;
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(
                        ParseErrorCode::InvalidFirstCharacterOfProcessingInstructionTarget,
                    );
                    self.convert_temp_buffer_to_comment();
                    self.reconsume = true;
                    self.state = TokenizerState::BogusComment;
                }
            },

            // 13.2.5.73 Processing instruction target state
            TokenizerState::ProcessingInstructionTarget => match self.consume_next() {
                Some(0x09 | 0x0A | 0x0C | 0x20 | 0x3F | 0x3E) => {
                    let mut target = String::new();
                    for &b in &self.temp_buffer {
                        Self::push_cp_to_string(&mut target, b);
                    }
                    if target.eq_ignore_ascii_case("xml")
                        || target.eq_ignore_ascii_case("xml-stylesheet")
                    {
                        self.record_error(ParseErrorCode::DisallowedProcessingInstructionTarget);
                        self.convert_temp_buffer_to_comment();
                        self.reconsume = true;
                        self.state = TokenizerState::BogusComment;
                    } else {
                        self.pending_pi_target = target;
                        self.pending_pi_data.clear();
                        self.pi_data_start = Some(self.current_position());
                        self.reconsume = true;
                        self.state = TokenizerState::AfterProcessingInstructionTarget;
                    }
                }
                Some(cp)
                    if ((cp as u8).is_ascii_alphanumeric() && cp <= 0x7F)
                        || cp == 0x2D
                        || cp == 0x5F =>
                {
                    self.temp_buffer.push(cp);
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                Some(_) => {
                    self.record_error(ParseErrorCode::InvalidProcessingInstructionTarget);
                    self.convert_temp_buffer_to_comment();
                    self.reconsume = true;
                    self.state = TokenizerState::BogusComment;
                }
            },

            // 13.2.5.74 After processing instruction target state
            TokenizerState::AfterProcessingInstructionTarget => match self.consume_next() {
                Some(cp) if is_ascii_whitespace(cp) => {}
                _ => {
                    self.reconsume = true;
                    self.state = TokenizerState::ProcessingInstructionData;
                }
            },

            // 13.2.5.75 Processing instruction data state
            TokenizerState::ProcessingInstructionData => match self.consume_next() {
                Some(0x3F) => {
                    self.state = TokenizerState::ProcessingInstructionQuestionable;
                }
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_pi();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                Some(cp) => {
                    Self::push_cp_to_string(&mut self.pending_pi_data, cp);
                }
            },

            // 13.2.5.76 Processing instruction questionable state
            TokenizerState::ProcessingInstructionQuestionable => match self.consume_next() {
                Some(0x3E) => {
                    self.state = TokenizerState::Data;
                    self.emit_current_pi();
                }
                None => {
                    self.record_error(ParseErrorCode::EofInProcessingInstruction);
                    self.tokens.push(Token::Eof);
                }
                _ => {
                    self.pending_pi_data.push('?');
                    self.reconsume = true;
                    self.state = TokenizerState::ProcessingInstructionData;
                }
            },

            // 13.2.5.77 Character reference state
            TokenizerState::CharacterReference => {
                self.temp_buffer.clear();
                self.temp_buffer.push(0x26); // '&'
                match self.consume_next() {
                    Some(cp) if (cp as u8).is_ascii_alphanumeric() && cp <= 0x7F => {
                        self.reconsume = false;
                        self.pos -= 1;
                        self.state = TokenizerState::NamedCharacterReference;
                    }
                    Some(0x23) => {
                        self.temp_buffer.push(0x23);
                        self.state = TokenizerState::NumericCharacterReference;
                    }
                    _ => {
                        self.flush_code_points_consumed_as_char_ref();
                        self.reconsume = true;
                        self.state = self.return_state;
                    }
                }
            }

            // 13.2.5.78 Named character reference state
            TokenizerState::NamedCharacterReference => {
                let start_pos = self.pos;
                let mut best_match: Option<(&str, &[u32])> = None;

                for &(name, cps) in NAMED_ENTITIES {
                    if self.peek_slice_ascii_eq(name, false)
                        && best_match.is_none_or(|(prev_name, _)| name.len() > prev_name.len())
                    {
                        best_match = Some((name, cps));
                    }
                }

                if let Some((matched_name, decoded_cps)) = best_match {
                    for &b in matched_name.as_bytes() {
                        self.temp_buffer.push(b as u32);
                    }
                    self.pos = start_pos + matched_name.len();
                    let ends_with_semi = matched_name.ends_with(';');
                    let next_cp = self.input.get(self.pos).copied();

                    if self.return_state.is_attribute_value_state()
                        && !ends_with_semi
                        && let Some(nc) = next_cp
                        && (nc == 0x3D || ((nc as u8).is_ascii_alphanumeric() && nc <= 0x7F))
                    {
                        self.flush_code_points_consumed_as_char_ref();
                        self.state = self.return_state;
                        return;
                    }

                    if !ends_with_semi {
                        self.record_error(ParseErrorCode::MissingSemicolonAfterCharacterReference);
                    }
                    self.temp_buffer.clear();
                    self.temp_buffer.extend_from_slice(decoded_cps);
                    self.flush_code_points_consumed_as_char_ref();
                    self.state = self.return_state;
                } else {
                    self.flush_code_points_consumed_as_char_ref();
                    self.state = TokenizerState::AmbiguousAmpersand;
                }
            }

            // 13.2.5.79 Ambiguous ampersand state
            TokenizerState::AmbiguousAmpersand => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_alphanumeric() && cp <= 0x7F => {
                    if self.return_state.is_attribute_value_state() {
                        if let Some(ref mut tag) = self.pending_tag {
                            Self::push_cp_to_string(&mut tag.current_attr_value, cp);
                        }
                    } else {
                        self.emit_char(cp);
                    }
                }
                Some(0x3B) => {
                    self.record_error(ParseErrorCode::UnknownNamedCharacterReference);
                    self.reconsume = true;
                    self.state = self.return_state;
                }
                _ => {
                    self.reconsume = true;
                    self.state = self.return_state;
                }
            },

            // 13.2.5.80 Numeric character reference state
            TokenizerState::NumericCharacterReference => {
                self.char_ref_code = 0;
                match self.consume_next() {
                    Some(cp @ (0x78 | 0x58)) => {
                        self.temp_buffer.push(cp);
                        self.state = TokenizerState::HexadecimalCharacterReferenceStart;
                    }
                    _ => {
                        self.reconsume = true;
                        self.state = TokenizerState::DecimalCharacterReference;
                    }
                }
            }

            // 13.2.5.81 Hexadecimal character reference start state
            TokenizerState::HexadecimalCharacterReferenceStart => match self.consume_next() {
                Some(cp) if (cp as u8).is_ascii_hexdigit() && cp <= 0x7F => {
                    self.reconsume = true;
                    self.state = TokenizerState::HexadecimalCharacterReference;
                }
                None => {
                    self.record_error_at_next(
                        ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                    );
                    self.flush_code_points_consumed_as_char_ref();
                    self.reconsume = true;
                    self.state = self.return_state;
                }
                _ => {
                    self.record_error_at_current(
                        ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                    );
                    self.flush_code_points_consumed_as_char_ref();
                    self.reconsume = true;
                    self.state = self.return_state;
                }
            },

            // 13.2.5.82 Hexadecimal character reference state
            TokenizerState::HexadecimalCharacterReference => match self.consume_next() {
                Some(cp @ 0x30..=0x39) => {
                    self.char_ref_code = self
                        .char_ref_code
                        .saturating_mul(16)
                        .saturating_add(cp - 0x30);
                }
                Some(cp @ 0x41..=0x46) => {
                    self.char_ref_code = self
                        .char_ref_code
                        .saturating_mul(16)
                        .saturating_add(cp - 0x37);
                }
                Some(cp @ 0x61..=0x66) => {
                    self.char_ref_code = self
                        .char_ref_code
                        .saturating_mul(16)
                        .saturating_add(cp - 0x57);
                }
                Some(0x3B) => {
                    self.numeric_end_mode = ErrorPositionMode::Next;
                    self.state = TokenizerState::NumericCharacterReferenceEnd;
                }
                None => {
                    self.numeric_end_mode = ErrorPositionMode::Next;
                    self.record_error_at_next(
                        ParseErrorCode::MissingSemicolonAfterCharacterReference,
                    );
                    self.reconsume = true;
                    self.state = TokenizerState::NumericCharacterReferenceEnd;
                }
                _ => {
                    self.numeric_end_mode = ErrorPositionMode::Current;
                    self.record_error_at_current(
                        ParseErrorCode::MissingSemicolonAfterCharacterReference,
                    );
                    self.reconsume = true;
                    self.state = TokenizerState::NumericCharacterReferenceEnd;
                }
            },

            // 13.2.5.83 Decimal character reference state
            TokenizerState::DecimalCharacterReference => {
                // Check if at least one digit was present if we entered directly from NumericCharacterReference
                if self.temp_buffer.len() == 2 && self.char_ref_code == 0 {
                    match self.consume_next() {
                        Some(cp @ 0x30..=0x39) => {
                            self.temp_buffer.push(cp);
                            self.char_ref_code = cp - 0x30;
                        }
                        None => {
                            self.record_error_at_next(
                                ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                            );
                            self.flush_code_points_consumed_as_char_ref();
                            self.reconsume = true;
                            self.state = self.return_state;
                        }
                        _ => {
                            self.record_error_at_current(
                                ParseErrorCode::AbsenceOfDigitsInNumericCharacterReference,
                            );
                            self.flush_code_points_consumed_as_char_ref();
                            self.reconsume = true;
                            self.state = self.return_state;
                        }
                    }
                    return;
                }

                match self.consume_next() {
                    Some(cp @ 0x30..=0x39) => {
                        self.char_ref_code = self
                            .char_ref_code
                            .saturating_mul(10)
                            .saturating_add(cp - 0x30);
                    }
                    Some(0x3B) => {
                        self.numeric_end_mode = ErrorPositionMode::Next;
                        self.state = TokenizerState::NumericCharacterReferenceEnd;
                    }
                    None => {
                        self.numeric_end_mode = ErrorPositionMode::Next;
                        self.record_error_at_next(
                            ParseErrorCode::MissingSemicolonAfterCharacterReference,
                        );
                        self.reconsume = true;
                        self.state = TokenizerState::NumericCharacterReferenceEnd;
                    }
                    _ => {
                        self.numeric_end_mode = ErrorPositionMode::Current;
                        self.record_error_at_current(
                            ParseErrorCode::MissingSemicolonAfterCharacterReference,
                        );
                        self.reconsume = true;
                        self.state = TokenizerState::NumericCharacterReferenceEnd;
                    }
                }
            }

            // 13.2.5.84 Numeric character reference end state
            TokenizerState::NumericCharacterReferenceEnd => {
                let mut code = self.char_ref_code;
                if code == 0x00 {
                    self.record_error_in_numeric_end(ParseErrorCode::NullCharacterReference);
                    code = 0xFFFD;
                } else if code > 0x10FFFF {
                    self.record_error_in_numeric_end(
                        ParseErrorCode::CharacterReferenceOutsideUnicodeRange,
                    );
                    code = 0xFFFD;
                } else if is_surrogate(code) {
                    self.record_error_in_numeric_end(ParseErrorCode::SurrogateCharacterReference);
                    code = 0xFFFD;
                } else if is_noncharacter(code) {
                    self.record_error_in_numeric_end(
                        ParseErrorCode::NoncharacterCharacterReference,
                    );
                } else if code == 0x0D || is_control_character_in_stream(code) {
                    self.record_error_in_numeric_end(ParseErrorCode::ControlCharacterReference);
                    if let Some(&(_, mapped)) =
                        NUMERIC_REPLACEMENT_TABLE.iter().find(|&&(k, _)| k == code)
                    {
                        code = mapped;
                    }
                }
                self.temp_buffer.clear();
                self.temp_buffer.push(code);
                self.flush_code_points_consumed_as_char_ref();
                self.state = self.return_state;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic_tags_and_attributes() {
        let mut tok = Tokenizer::from_str_input(
            r#"<!DOCTYPE html><div id="main" class='box' disabled><br/>Hi &amp; &#x41;</div>"#,
            TokenizerConfig::default(),
        );
        tok.run();
        assert_eq!(
            tok.tokens[0],
            Token::Doctype {
                name: Some("html".into()),
                public_id: None,
                system_id: None,
                force_quirks: false,
            }
        );
        assert_eq!(
            tok.tokens[1],
            Token::StartTag {
                name: "div".into(),
                self_closing: false,
                attributes: vec![
                    ("id".into(), "main".into()),
                    ("class".into(), "box".into()),
                    ("disabled".into(), "".into()),
                ],
            }
        );
        assert_eq!(
            tok.tokens[2],
            Token::StartTag {
                name: "br".into(),
                self_closing: true,
                attributes: vec![],
            }
        );
    }

    #[test]
    fn test_processing_instruction_states() {
        let mut tok =
            Tokenizer::from_str_input("<?foo bar?><?xml disallow?>", TokenizerConfig::default());
        tok.run();
        assert_eq!(
            tok.tokens[0],
            Token::ProcessingInstruction {
                target: "foo".into(),
                data: "bar".into(),
            }
        );
        assert_eq!(tok.tokens[1], Token::Comment("?xml disallow?".into()));
    }
}
