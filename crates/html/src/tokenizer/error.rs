//! Parse error codes, error positions and XML infoset coercion helpers (§13.2.2, §13.2.9).

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
pub(super) const fn xml_coerce_codepoint(cp: u32) -> u32 {
    if cp == 0x0C {
        0x20
    } else if is_valid_xml_character(cp) {
        cp
    } else {
        0xFFFD
    }
}

/// Applies the §13.2.9 character tweaks to a `String` used as character data or an attribute value.
pub(super) fn xml_coerce_text(text: &str) -> String {
    if text.chars().all(|ch| is_valid_xml_character(ch as u32)) {
        return text.to_owned();
    }
    text.chars()
        .map(|ch| char::from_u32(xml_coerce_codepoint(ch as u32)).unwrap_or('\u{FFFD}'))
        .collect()
}

/// Applies the §13.2.9 comment tweaks: non-XML characters/FF are coerced like character data and
/// a U+0020 SPACE is inserted between two consecutive U+002D HYPHEN-MINUS characters.
pub(super) fn xml_coerce_comment(text: &str) -> String {
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
pub(super) enum ErrorPositionMode {
    /// Position of the most recently consumed character (Megi's historical behaviour).
    Current,
    /// Position of the next input character (EOF / character-reference / lookahead families,
    /// which html5lib records after advancing past the character that triggered the error).
    Next,
}

/// Position convention per parse error code, derived from the html5lib-tests fixtures.
///
/// All codes not listed here use [`ErrorPositionMode::Current`]. The residue of codes that still
/// differ after this split is tracked explicitly by `megi_test_harness::KNOWN_POSITION_GAPS`.
pub(super) const fn error_position_mode(code: ParseErrorCode) -> ErrorPositionMode {
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
