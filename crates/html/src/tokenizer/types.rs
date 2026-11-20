//! Tokenizer data types: states, emitted tokens and pending-token buffers.

use super::*;
use std::collections::HashSet;

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
pub(super) struct PendingTag {
    pub(super) is_end_tag: bool,
    pub(super) name: String,
    pub(super) self_closing: bool,
    pub(super) attributes: Vec<(String, String)>,
    /// Lazily-created set of attribute names, only for tags with many attributes.
    ///
    /// Real pages have a handful of attributes per tag: a linear scan over the few
    /// already-seen names is cheaper than allocating/hashing a `HashSet` per tag. Tags
    /// with at least [`MANY_ATTRIBUTES`] attributes switch to the set so a hostile tag
    /// with tens of thousands of attributes stays amortized `O(1)` (anti-DoS).
    pub(super) attribute_names: Option<HashSet<String>>,
    pub(super) current_attr_name: String,
    pub(super) current_attr_value: String,
    pub(super) has_current_attr: bool,
    /// 1-based `(line, col)` where the current attribute name ended (used for the
    /// `duplicate-attribute` position, matching html5lib-tests expectations).
    pub(super) name_end_line: usize,
    pub(super) name_end_col: usize,
}

impl PendingTag {
    pub(super) fn new_start() -> Self {
        Self {
            is_end_tag: false,
            name: String::new(),
            self_closing: false,
            attributes: Vec::new(),
            attribute_names: None,
            current_attr_name: String::new(),
            current_attr_value: String::new(),
            has_current_attr: false,
            name_end_line: 1,
            name_end_col: 1,
        }
    }

    pub(super) fn new_end() -> Self {
        Self {
            is_end_tag: true,
            name: String::new(),
            self_closing: false,
            attributes: Vec::new(),
            attribute_names: None,
            current_attr_name: String::new(),
            current_attr_value: String::new(),
            has_current_attr: false,
            name_end_line: 1,
            name_end_col: 1,
        }
    }

    pub(super) fn start_new_attribute(&mut self, errors: &mut Vec<ParseError>) {
        self.finish_attribute(errors);
        self.current_attr_name.clear();
        self.current_attr_value.clear();
        self.has_current_attr = true;
    }

    pub(super) fn finish_attribute(&mut self, errors: &mut Vec<ParseError>) {
        if !self.has_current_attr {
            return;
        }
        self.has_current_attr = false;
        if self.current_attr_name.is_empty() {
            return;
        }
        let name = std::mem::take(&mut self.current_attr_name);
        let value = std::mem::take(&mut self.current_attr_value);
        // First occurrence wins; later duplicates are reported and dropped. Small tags
        // (the common case) use a linear scan; once a tag has `MANY_ATTRIBUTES` names a
        // `HashSet` takes over so a single tag with tens of thousands of attributes stays
        // amortized O(1) instead of quadratic (anti-DoS).
        let duplicate = if let Some(names) = self.attribute_names.as_mut() {
            !names.insert(name.clone())
        } else if self.attributes.len() >= MANY_ATTRIBUTES {
            let mut names: HashSet<String> = self
                .attributes
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            let duplicate = !names.insert(name.clone());
            self.attribute_names = Some(names);
            duplicate
        } else {
            self.attributes
                .iter()
                .any(|(existing, _)| *existing == name)
        };
        if duplicate {
            // #1734: honor the global parse-error cap; many duplicate attributes across a
            // document must not grow this shared vector without bound.
            if errors.len() < MAX_PARSE_ERRORS {
                errors.push(ParseError {
                    code: ParseErrorCode::DuplicateAttribute,
                    line: self.name_end_line,
                    col: self.name_end_col,
                });
            }
        } else if self.attributes.len() >= MAX_TOKENIZER_ATTRIBUTES {
            // Drop the attribute to keep peak memory bounded; the tree builder's
            // cap would reject it anyway (#560).
        } else {
            self.attributes.push((name, value));
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PendingDoctype {
    pub(super) name: Option<String>,
    pub(super) public_id: Option<String>,
    pub(super) system_id: Option<String>,
    pub(super) force_quirks: bool,
}
