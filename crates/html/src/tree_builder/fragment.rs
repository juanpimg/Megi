//! Fragment parser core and tokenizer driving loop.

use crate::tokenizer::{Token, Tokenizer, TokenizerConfig, TokenizerState};
use megi_core::{Namespace, NodeId};

use super::*;

/// Shared fragment parser: tokenizer setup, §13.4 synthetic `<html>` root, token loop and
/// finalization (move children to the document root, drop the synthetic root/context node).
pub(crate) fn run_fragment_parser(
    mut tb: TreeBuilder,
    input: &str,
    context_namespace: Namespace,
    context_local_name: &str,
    scripting_enabled: bool,
    options: &ParseOptions,
) -> TreeBuilder {
    // Cap and validate the context tag name (issue #558): reject empty/whitespace/control
    // names and bound length to prevent unbounded allocation from a huge name.
    const MAX_CONTEXT_NAME_BYTES: usize = 256;
    let trimmed = context_local_name.trim();
    let context_local_name =
        if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
            "div"
        } else if trimmed.len() > MAX_CONTEXT_NAME_BYTES {
            // Clip on a UTF-8 boundary: a multibyte context name used to panic here
            // and the catch_unwind in parse_fragment_into wiped the caller's document (#1444).
            let mut end = MAX_CONTEXT_NAME_BYTES;
            while end > 0 && !trimmed.is_char_boundary(end) {
                end -= 1;
            }
            &trimmed[..end]
        } else {
            trimmed
        };
    let context_local_name = if context_namespace == Namespace::Html {
        context_local_name.to_ascii_lowercase()
    } else {
        context_local_name.to_owned()
    };
    let context_local_name = context_local_name.as_str();
    let (input, input_truncated) = truncate_input(input, options.max_input_bytes);
    tb.limit_violations.input_truncated = input_truncated;
    let initial_tok_state = if context_namespace == Namespace::Html {
        match context_local_name {
            "title" | "textarea" => TokenizerState::Rcdata,
            "style" | "xmp" | "iframe" | "noembed" | "noframes" => TokenizerState::Rawtext,
            "script" => TokenizerState::ScriptData,
            "noscript" if scripting_enabled => TokenizerState::Rawtext,
            "plaintext" => TokenizerState::Plaintext,
            _ => TokenizerState::Data,
        }
    } else {
        TokenizerState::Data
    };

    // Per WHATWG §13.4 there is no appropriate end tag in the fragment case, so the
    // tokenizer's last start tag stays unset (issue #413 was resolved as invalid).
    let mut tok = Tokenizer::from_str_input(
        input,
        TokenizerConfig {
            initial_state: initial_tok_state,
            last_start_tag: None,
        },
    );

    tb.fragment_context = Some((context_namespace, context_local_name.to_owned()));
    let ctx_node = tb
        .doc
        .create_element(context_namespace, context_local_name, vec![]);
    tb.fragment_context_node = Some(ctx_node);

    let html_root = tb.doc.create_element(Namespace::Html, "html", vec![]);
    tb.doc.append_child(tb.doc.root(), html_root);
    tb.push_open_element(html_root);

    if context_namespace == Namespace::Html && context_local_name == "template" {
        tb.template_modes.push(InsertionMode::InTemplate);
    }
    tb.reset_insertion_mode_appropriately();

    drive_tokenizer(&mut tok, &mut tb);

    // Move children of the synthetic `<html>` root directly under `doc.root()` so callers
    // can pick them up as "the new children appended after the previous last child".
    let root = tb.doc.root();
    let moved = tb.doc.reparent_children(html_root, root);
    debug_assert!(moved, "root reparent must not create a cycle");
    tb.doc.detach(html_root);
    // The synthetic `<html>` root and the fragment context element are parser scaffolding:
    // free their slots so repeated in-place fragment parses (`innerHTML`) do not leak arena
    // nodes. Neither is reachable from the returned document tree.
    tb.doc.remove_subtree(html_root);
    if let Some(ctx_id) = tb.fragment_context_node.take() {
        tb.doc.remove_subtree(ctx_id);
    }
    tb.update_selectedcontent_elements();
    tb
}

/// Runs the tokenizer → tree-builder loop until EOF.
/// Appends `cp` to a UTF-8 byte run (NUL/invalid code points are dropped, matching
/// [`TreeBuilder::process_character_run`]).
pub(crate) fn push_codepoint_utf8(out: &mut Vec<u8>, cp: u32) {
    if cp == 0 {
        return;
    }
    if cp < 0x80 {
        out.push(cp as u8);
    } else if let Some(ch) = char::from_u32(cp) {
        let mut buffer = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
    }
}

pub(crate) fn drive_tokenizer(tok: &mut Tokenizer, tb: &mut TreeBuilder) {
    // Reused token batch buffer: the old code allocated a `Vec` for every emitted token.
    let mut emitted: Vec<Token> = Vec::with_capacity(4);
    // The CDATA allowance only depends on the adjusted current node, which is determined by
    // `(open_elements.len(), current_node)` (plus the fixed fragment context). Caching the
    // key avoids two arena lookups per input character.
    let mut cdata_key: Option<(usize, Option<NodeId>)> = None;
    // Consecutive character tokens of the "in body" mode are processed as one run.
    let mut char_run: Vec<u8> = Vec::new();
    let refresh_cdata =
        |tok: &mut Tokenizer, tb: &TreeBuilder, key: &mut Option<(usize, Option<NodeId>)>| {
            let current_key = (tb.open_elements.len(), tb.current_node());
            if *key != Some(current_key) {
                tok.allow_cdata = tb.is_adjusted_current_node_foreign();
                *key = Some(current_key);
            }
        };
    loop {
        // Fast path: plain text in the "data" state can be consumed straight into the
        // character run without emitting/handling one `Token::Character` per code point.
        if tok.state == TokenizerState::Data
            && tb.mode == InsertionMode::InBody
            && tb.doc.len() < tb.parse_options.max_nodes
            && !tb.may_have_foreign_content()
        {
            let consumed = tok.consume_data_run(&mut char_run);
            if consumed > 0 {
                continue;
            }
        }
        refresh_cdata(tok, tb, &mut cdata_key);
        let prev_len = tok.tokens.len();
        tok.step();
        if tok.tokens.len() > prev_len {
            emitted.clear();
            emitted.extend(tok.tokens.drain(prev_len..));
            let mut saw_eof = false;
            for token in emitted.drain(..) {
                if let Token::Character(cp) = token {
                    if tb.mode == InsertionMode::InBody
                        && tb.doc.len() < tb.parse_options.max_nodes
                        && (!char_run.is_empty() || !tb.may_have_foreign_content())
                    {
                        push_codepoint_utf8(&mut char_run, cp);
                        continue;
                    }
                    if !char_run.is_empty() {
                        tb.process_character_run(&char_run);
                        char_run.clear();
                    }
                    if let Some(new_state) = tb.process_token(Token::Character(cp)) {
                        tok.state = new_state;
                    }
                    refresh_cdata(tok, tb, &mut cdata_key);
                    continue;
                }
                if !char_run.is_empty() {
                    tb.process_character_run(&char_run);
                    char_run.clear();
                }
                if token == Token::Eof {
                    saw_eof = true;
                }
                if let Some(new_state) = tb.process_token(token) {
                    tok.state = new_state;
                }
                // WP-6.4: a parser-inserted script may have written HTML with
                // `document.write`; re-tokenize it at the current position (immediately
                // after the token that closed the script, before the remaining input).
                if let Some(written) = tb.take_pending_written_html() {
                    tok.insert_input(&written);
                    // The inserted bytes must be tokenized before we stop, even if
                    // the triggering token was EOF.
                    saw_eof = false;
                }
                refresh_cdata(tok, tb, &mut cdata_key);
            }
            if saw_eof {
                break;
            }
        }
    }
}

/// Truncates `input` to at most `max_bytes` bytes on a UTF-8 character boundary.
/// Returns the prefix and whether any truncation happened.
pub(crate) fn truncate_input(input: &str, max_bytes: usize) -> (&str, bool) {
    if input.len() <= max_bytes {
        return (input, false);
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    (&input[..end], true)
}
