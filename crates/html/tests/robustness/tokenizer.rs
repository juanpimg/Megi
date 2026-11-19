use super::*;

#[test]
fn tokenizer_handles_many_attributes_in_linear_time() {
    // Audit F3: `PendingTag::finish_attribute` used a linear `.any()` scan per attribute,
    // making a single tag with 50k attributes quadratic (~seconds of CPU).
    let mut input = String::from("<a");
    for i in 0..50_000 {
        write!(input, " b{i}").unwrap();
    }
    input.push('>');

    let start = Instant::now();
    let mut tok = Tokenizer::from_str_input(&input, TokenizerConfig::default());
    tok.run();
    let elapsed = start.elapsed();

    let attribute_count = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::StartTag { attributes, .. } => Some(attributes.len()),
            _ => None,
        })
        .expect("start tag must be emitted");
    // The tokenizer caps attributes at MAX_TOKENIZER_ATTRIBUTES (4096) to bound
    // peak memory (#560); the performance regression test still covers the
    // linear-time path up to the cap.
    assert_eq!(attribute_count, 4096);
    assert!(tok.errors.is_empty());
    let budget = scaled_budget(Duration::from_secs(10));
    assert!(
        elapsed < budget,
        "tokenizing 50k attributes took {elapsed:?} (budget {budget:?}, scale {}); duplicate detection may be quadratic again",
        budget_scale()
    );
}

#[test]
fn tokenizer_still_reports_duplicate_attributes_and_keeps_the_first() {
    let mut tok = Tokenizer::from_str_input("<a x=1 x=2 y=3>", TokenizerConfig::default());
    tok.run();
    let attributes = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::StartTag { attributes, .. } => Some(attributes.clone()),
            _ => None,
        })
        .expect("start tag must be emitted");
    assert_eq!(
        attributes,
        vec![
            ("x".to_string(), "1".to_string()),
            ("y".to_string(), "3".to_string())
        ]
    );
    assert!(
        tok.errors
            .iter()
            .any(|e| e.code == ParseErrorCode::DuplicateAttribute)
    );
}

#[test]
fn huge_numeric_character_reference_runs_stay_linear_and_out_of_range() {
    // #638: every digit of a numeric character reference ran a saturating multiply/add, so
    // `&#` + millions of digits burned one arithmetic step per digit. The accumulator now stops
    // after eight significant digits (still consuming the run to find the terminator) and
    // reports the out-of-range error once.
    use megi_html::{ParseOptions, parse_document_with_options};

    let run = "9".repeat(200_000);
    let input = format!("<p>&#{run};</p>");
    let started = std::time::Instant::now();
    let outcome = parse_document_with_options(&input, false, &ParseOptions::default());
    let elapsed = started.elapsed();
    let budget = scaled_budget(Duration::from_secs(2));
    assert!(
        elapsed < budget,
        "a 200k-digit character reference took {elapsed:?} (budget {budget:?}, scale {})",
        budget_scale()
    );
    let text = paragraph_text(&outcome.document);
    assert_eq!(text, "\u{FFFD}", "an out-of-range reference becomes U+FFFD");

    // Leading zeros do not consume the significant-digit budget.
    let outcome = parse_document_with_options(
        &format!("<p>&#x{}41;</p>", "0".repeat(50)),
        false,
        &ParseOptions::default(),
    );
    let text = paragraph_text(&outcome.document);
    assert_eq!(
        text, "A",
        "leading zeros must not make `&#x0…041;` overflow"
    );
}

#[test]
fn token_values_are_capped_to_the_per_token_budget() {
    // #636: `max_input_bytes` bounds the document, not a single value. One unterminated
    // comment, doctype id, quoted attribute value or script/RCDATA run used to grow a single
    // buffer to input size, which the tree builder/DOM/serializer then copied.
    use megi_html::tokenizer::{MAX_TOKEN_BYTES, Token, Tokenizer, TokenizerConfig};

    let big = "x".repeat(MAX_TOKEN_BYTES + 4096);

    let mut tok = Tokenizer::from_str_input(&format!("<!--{big}-->"), TokenizerConfig::default());
    tok.run();
    let comment = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::Comment(data) => Some(data.clone()),
            _ => None,
        })
        .expect("a comment token");
    assert_eq!(comment.len(), MAX_TOKEN_BYTES, "comment must be capped");
    assert!(
        tok.token_bytes_dropped > 0,
        "the truncation must be counted"
    );

    let mut tok =
        Tokenizer::from_str_input(&format!("<a href=\"{big}\">"), TokenizerConfig::default());
    tok.run();
    let value_len = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::StartTag { attributes, .. } => attributes
                .iter()
                .find(|(name, _)| name == "href")
                .map(|(_, value)| value.len()),
            _ => None,
        })
        .expect("an href attribute");
    assert_eq!(value_len, MAX_TOKEN_BYTES, "attribute value must be capped");

    // A processing instruction target accumulates while the tokenizer scans for the target
    // delimiter, so an overlong target must be capped and still produce a PI token (#1362).
    let mut tok = Tokenizer::from_str_input(
        &format!("<?{}?>", "x".repeat(MAX_TOKEN_BYTES + 4096)),
        TokenizerConfig::default(),
    );
    tok.run();
    let target_len = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::ProcessingInstruction { target, .. } => Some(target.len()),
            _ => None,
        })
        .expect("a processing instruction token must be emitted");
    // #1736: temp_buffer stores u32 code points, so the byte cap allows MAX/4 characters.
    assert_eq!(
        target_len,
        MAX_TOKEN_BYTES / std::mem::size_of::<u32>(),
        "PI target must be capped by bytes"
    );
    assert!(
        tok.token_bytes_dropped > 0,
        "the truncation must be counted"
    );
}

#[test]
fn active_formatting_list_is_bounded() {
    // #643: the Noah's Ark clause only removes a fourth *identical* entry, so distinct
    // `(tag, attributes)` pairs grew the list without bound and every character run then
    // cloned the whole thing in `reconstruct_active_formatting_elements`.
    use megi_html::{ParseOptions, parse_document_with_options};

    let mut input = String::new();
    for index in 0..20_000 {
        write!(input, "<b id={index}>").unwrap();
    }
    input.push_str("text");

    let started = std::time::Instant::now();
    let outcome = parse_document_with_options(&input, false, &ParseOptions::DEFAULT);
    let elapsed = started.elapsed();
    let budget = scaled_budget(Duration::from_secs(3));
    assert!(
        elapsed < budget,
        "20k distinct formatting elements took {elapsed:?} (budget {budget:?}, scale {})",
        budget_scale()
    );
    assert!(
        outcome.limit_violations.formatting_dropped > 0,
        "the overflow must be counted, got {:?}",
        outcome.limit_violations
    );
}

#[test]
fn pi_data_byte_cap_holds_with_multibyte_characters() {
    // #1738: a 4-byte code point must not push `pending_pi_data` past the byte cap.
    let cap = 1usize << 20;
    let source = format!("<?target {}?>", "é".repeat(cap));
    let mut tok = Tokenizer::from_str_input(&source, TokenizerConfig::default());
    tok.run();
    let data_len = tok
        .tokens
        .iter()
        .find_map(|token| match token {
            Token::ProcessingInstruction { data, .. } => Some(data.len()),
            _ => None,
        })
        .expect("a PI token must be emitted");
    assert!(data_len <= cap, "PI data exceeded the byte cap: {data_len}");
}

#[test]
fn non_void_self_closing_tag_reports_a_parse_error() {
    // #1735: `<div/>` must report the trailing-solidus parse error.
    let mut tok = Tokenizer::from_str_input("<div/><span></span>", TokenizerConfig::default());
    tok.run();
    assert!(
        tok.errors
            .iter()
            .any(|e| { e.code == ParseErrorCode::NonVoidHtmlElementStartTagWithTrailingSolidus })
    );
}
