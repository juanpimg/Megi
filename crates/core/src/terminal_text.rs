//! Safe text for terminal output (issue #2309).
//!
//! Console messages, `<title>` elements and URLs are controlled by the page. Writing
//! them verbatim to stdout/stderr lets a page forge log lines, ring the bell or
//! reconfigure the operator's terminal. This module provides one small filter used
//! by every crate that prints such text; it lives in `core` so both `render` and
//! `browser` reach it through their existing DAG edges (`browser` via the
//! `megi_webview::core` facade re-export).
//!
//! Normative references: ECMA-48 §8.3 (CSI), ISO/IEC 2022 (OSC/ST), ITU-T T.416.

/// Replaces C0 (including `\\t`, `\\n` and `\\r`), DEL, C1 and every escape sequence
/// with harmless characters before the text is written to a terminal.
///
/// `\\n`/`\\r` are folded too: a page must not be able to forge an extra log line by
/// embedding a newline in a warning. Each dropped control byte becomes one ASCII
/// space, so word boundaries and UTF-8 are preserved. CSI (`ESC [`), OSC/string
/// sequences (`ESC ]`, `ESC P`, `ESC X`, `ESC ^`, `ESC _`) are consumed in full up to
/// their final byte / ST (`ESC \\` or BEL), so no sequence payload survives.
///
/// Complexity is linear in the number of characters; no allocation happens beyond
/// the returned `String`.
pub fn sanitize_terminal_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut mode = Mode::Text;
    for ch in input.chars() {
        let cp = ch as u32;
        mode = match mode {
            Mode::Text => {
                if ch == '\u{1b}' {
                    Mode::Escape
                } else if is_terminal_control(cp) {
                    out.push(' ');
                    Mode::Text
                } else {
                    out.push(ch);
                    Mode::Text
                }
            }
            Mode::Escape => {
                if ch == '[' {
                    Mode::Csi
                } else if matches!(ch, ']' | 'P' | 'X' | '^' | '_') {
                    Mode::String
                } else if ch == '\u{1b}' {
                    Mode::Escape
                } else if ch == '\\' {
                    // ST without an OSC/string opener: consume it silently.
                    Mode::Text
                } else if (0x20..=0x2f).contains(&cp) {
                    // ISO-2022 intermediate byte; the final byte is still pending.
                    Mode::Escape
                } else if (0x30..=0x7e).contains(&cp) {
                    // Final byte of a two-character escape (`ESC 7`, `ESC c`, ...).
                    Mode::Text
                } else {
                    out.push(' ');
                    Mode::Text
                }
            }
            Mode::Csi => {
                if (0x20..=0x3f).contains(&cp) {
                    // Parameter and intermediate bytes; never echoed.
                    Mode::Csi
                } else if (0x40..=0x7e).contains(&cp) {
                    Mode::Text // final byte of the CSI sequence
                } else if ch == '\u{1b}' {
                    Mode::Escape // malformed sequence, resynchronise on ESC
                } else {
                    out.push(' ');
                    Mode::Text
                }
            }
            Mode::String => {
                if ch == '\u{7}' {
                    Mode::Text // BEL terminates OSC
                } else if ch == '\u{1b}' {
                    Mode::StEscape
                } else {
                    Mode::String
                }
            }
            Mode::StEscape => {
                if ch == '\\' {
                    Mode::Text // ST ends the string sequence
                } else if ch == '\u{1b}' {
                    Mode::StEscape
                } else {
                    out.push(' ');
                    Mode::Text
                }
            }
        };
    }
    out
}

/// True for the C0 controls, DEL and the C1 controls (Unicode `Cc`).
fn is_terminal_control(cp: u32) -> bool {
    cp <= 0x1f || (0x7f..=0x9f).contains(&cp)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Escape,
    Csi,
    String,
    StEscape,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_c0_del_and_c1_without_touching_utf8() {
        assert_eq!(
            sanitize_terminal_text("a\u{0000}b\u{0007}c\u{007f}d\u{0085}é"),
            "a b c d é"
        );
        assert_eq!(sanitize_terminal_text("a\nb\rc\td"), "a b c d");
        assert_eq!(sanitize_terminal_text("€ 漢字"), "€ 漢字");
    }

    #[test]
    fn consumes_csi_osc_and_simple_escapes() {
        assert_eq!(sanitize_terminal_text("x\u{1b}[31mred\u{1b}[0m!"), "xred!");
        assert_eq!(sanitize_terminal_text("x\u{1b}]0;title\u{0007}y"), "xy");
        assert_eq!(sanitize_terminal_text("x\u{1b}]0;title\u{1b}\\y"), "xy");
        assert_eq!(sanitize_terminal_text("x\u{1b}7y"), "xy");
        // `ESC y` is a two-byte escape: its final byte is consumed as well.
        assert_eq!(sanitize_terminal_text("x\u{1b}y"), "x");
    }

    #[test]
    fn never_emits_a_control_character() {
        let input = "\u{1b}[1;31mBOOM\u{0007}\u{1b}]0;\u{0007}\u{009b}2K\r\n\u{1b}\\end";
        let clean = sanitize_terminal_text(input);
        assert!(clean.chars().all(|c| !c.is_control()), "{clean:?}");
        assert!(clean.contains("BOOM") && clean.contains("end"), "{clean:?}");
    }
}
