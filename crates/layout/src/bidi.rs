//! Pure-Rust Unicode Bidirectional Algorithm (UAX #9 subset) for inline layout.
//!
//! Handles strong (`L`, `R`, `AL`), weak (`EN`, `AN`, `ES`, `ET`, `CS`), neutral
//! (`WS`, `ON`) resolution, implicit embedding levels (I1–I2), line-level visual
//! reordering (L2), and bracket mirroring (L4) with `#![forbid(unsafe_code)]`.

use css::{Direction, UnicodeBidi};

/// Simplified UAX #9 directional character classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BidiClass {
    /// Left-to-Right (Latin, Greek, Cyrillic, CJK, LRM).
    L,
    /// Right-to-Left (Hebrew, NKo, RLM).
    R,
    /// Right-to-Left Arabic (Arabic, Syriac, Thaana, ALM).
    AL,
    /// European Number (ASCII digits `0..=9`).
    EN,
    /// Arabic Number (`U+0660..=U+0669`, `U+06F0..=U+06F9`).
    AN,
    /// European Number Separator (`+`, `-`).
    ES,
    /// European Number Terminator (`%`, `$`, `#`, currency).
    ET,
    /// Common Number Separator (`,`, `.`, `:`, `/`, NBSP).
    CS,
    /// Whitespace (` `, `\t`, etc.).
    WS,
    /// Other Neutrals (punctuation, brackets, symbols).
    ON,
}

/// Returns `true` if `ch` is an RTL or explicit bidi formatting character.
#[inline]
pub fn is_rtl_or_bidi_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{0590}'..='\u{08FF}'
            | '\u{200E}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FB1D}'..='\u{FEFF}'
    )
}

/// Returns `true` if any character in `text` requires bidi processing.
#[inline]
pub fn contains_rtl_or_bidi(text: &str) -> bool {
    text.chars().any(is_rtl_or_bidi_char)
}

/// Classifies a Unicode scalar value into its UAX #9 [`BidiClass`].
pub fn char_bidi_class(ch: char) -> BidiClass {
    match ch {
        '\u{200F}'
        | '\u{0590}'..='\u{05FF}'
        | '\u{07C0}'..='\u{085F}'
        | '\u{FB1D}'..='\u{FB4F}' => BidiClass::R,
        '\u{061C}'
        | '\u{0600}'..='\u{065F}'
        | '\u{066E}'..='\u{06EF}'
        | '\u{06FA}'..='\u{07BF}'
        | '\u{08A0}'..='\u{08FF}'
        | '\u{FB50}'..='\u{FDFF}'
        | '\u{FE70}'..='\u{FEFF}' => BidiClass::AL,
        '\u{0660}'..='\u{0669}' | '\u{06F0}'..='\u{06F9}' => BidiClass::AN,
        '0'..='9' | '\u{00B2}'..='\u{00B3}' | '\u{00B9}' => BidiClass::EN,
        '+' | '-' | '\u{2212}' => BidiClass::ES,
        '%' | '$' | '#' | '°' | '€' | '£' | '¥' | '¢' => BidiClass::ET,
        ',' | '.' | ':' | '/' | '\u{00A0}' => BidiClass::CS,
        ' ' | '\t' | '\n' | '\r' | '\u{000C}' => BidiClass::WS,
        ch if ch.is_ascii_punctuation() || matches!(ch, '«' | '»' | '‹' | '›' | '“' | '”') => {
            BidiClass::ON
        }
        _ => BidiClass::L,
    }
}

/// Resolves the paragraph base embedding level (`0` = LTR, `1` = RTL).
pub fn paragraph_base_level(
    direction: Direction,
    unicode_bidi: UnicodeBidi,
    first_strong: Option<BidiClass>,
) -> u8 {
    if unicode_bidi == UnicodeBidi::Plaintext {
        return match first_strong {
            Some(BidiClass::R | BidiClass::AL) => 1,
            _ => 0,
        };
    }
    match direction {
        Direction::Ltr => 0,
        Direction::Rtl => 1,
    }
}

/// Resolves UAX #9 embedding levels for a sequence of [`BidiClass`] values
/// given a paragraph `base_level` (`0` for LTR, `1` for RTL).
pub fn resolve_levels(input: &[BidiClass], base_level: u8) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut classes = input.to_vec();
    let base_dir = if base_level.is_multiple_of(2) {
        BidiClass::L
    } else {
        BidiClass::R
    };

    // W2 & W3: AL -> R, and EN preceded by AL -> AN.
    let mut last_strong = base_dir;
    for cls in &mut classes {
        match *cls {
            BidiClass::L => last_strong = BidiClass::L,
            BidiClass::R => last_strong = BidiClass::R,
            BidiClass::AL => {
                last_strong = BidiClass::AL;
                *cls = BidiClass::R;
            }
            BidiClass::EN if last_strong == BidiClass::AL => {
                *cls = BidiClass::AN;
            }
            _ => {}
        }
    }

    // W4: single ES between ENs -> EN; single CS between same number types -> that type.
    for i in 1..classes.len().saturating_sub(1) {
        if classes[i] == BidiClass::ES
            && classes[i - 1] == BidiClass::EN
            && classes[i + 1] == BidiClass::EN
        {
            classes[i] = BidiClass::EN;
        } else if classes[i] == BidiClass::CS
            && matches!(classes[i - 1], BidiClass::EN | BidiClass::AN)
            && classes[i - 1] == classes[i + 1]
        {
            classes[i] = classes[i - 1];
        }
    }

    // W5: sequences of ET adjacent to EN -> EN.
    let mut i = 0;
    while i < classes.len() {
        if classes[i] == BidiClass::ET {
            let start = i;
            while i < classes.len() && classes[i] == BidiClass::ET {
                i += 1;
            }
            let adj_en = (start > 0 && classes[start - 1] == BidiClass::EN)
                || (i < classes.len() && classes[i] == BidiClass::EN);
            if adj_en {
                for cls in &mut classes[start..i] {
                    *cls = BidiClass::EN;
                }
            }
        } else {
            i += 1;
        }
    }

    // W6: remaining separators/terminators become ON.
    for cls in &mut classes {
        if matches!(*cls, BidiClass::ES | BidiClass::ET | BidiClass::CS) {
            *cls = BidiClass::ON;
        }
    }

    // W7: EN preceded by strong L -> L.
    let mut strong = base_dir;
    for cls in &mut classes {
        match *cls {
            BidiClass::L => strong = BidiClass::L,
            BidiClass::R => strong = BidiClass::R,
            BidiClass::EN if strong == BidiClass::L => *cls = BidiClass::L,
            _ => {}
        }
    }

    // N1 & N2: resolve neutrals (WS / ON) between identical strong directions,
    // otherwise fall back to the paragraph embedding direction.
    let mut i = 0;
    while i < classes.len() {
        if matches!(classes[i], BidiClass::WS | BidiClass::ON) {
            let start = i;
            while i < classes.len() && matches!(classes[i], BidiClass::WS | BidiClass::ON) {
                i += 1;
            }
            let prev = if start == 0 {
                base_dir
            } else {
                match classes[start - 1] {
                    BidiClass::EN | BidiClass::AN => BidiClass::R,
                    other => other,
                }
            };
            let next = if i == classes.len() {
                base_dir
            } else {
                match classes[i] {
                    BidiClass::EN | BidiClass::AN => BidiClass::R,
                    other => other,
                }
            };
            let resolved = if prev == next && matches!(prev, BidiClass::L | BidiClass::R) {
                prev
            } else {
                base_dir
            };
            for cls in &mut classes[start..i] {
                *cls = resolved;
            }
        } else {
            i += 1;
        }
    }

    // I1 & I2: implicit levels.
    let mut levels = Vec::with_capacity(classes.len());
    let even_base = base_level.is_multiple_of(2);
    for cls in classes {
        let lvl = if even_base {
            match cls {
                BidiClass::R => base_level + 1,
                BidiClass::AN | BidiClass::EN => base_level + 2,
                _ => base_level,
            }
        } else {
            match cls {
                BidiClass::L | BidiClass::AN | BidiClass::EN => base_level + 1,
                _ => base_level,
            }
        };
        levels.push(lvl);
    }
    levels
}

/// Computes visual index order via UAX #9 Rule L2 from resolved embedding `levels`.
pub fn reorder_visual_indices(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let Some(&max_level) = levels.iter().max() else {
        return order;
    };
    let min_odd = levels
        .iter()
        .copied()
        .filter(|&l| l % 2 == 1)
        .min()
        .unwrap_or(max_level.saturating_add(1));
    if min_odd > max_level {
        return order;
    }

    for level in (min_odd..=max_level).rev() {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let start = i;
                while i < order.len() && levels[order[i]] >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
    order
}

/// Mirrors paired characters when rendered at an odd (RTL) embedding level (UAX #9 L4).
#[inline]
pub fn mirror_char(ch: char) -> char {
    match ch {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => ch,
    }
}

/// Reorders a single word's characters into left-to-right visual glyph order
/// when its resolved embedding level is odd (RTL).
pub fn word_to_visual(text: &str, level: u8) -> String {
    if level.is_multiple_of(2) {
        return text.to_string();
    }
    text.chars().rev().map(mirror_char).collect()
}

/// Returns the dominant [`BidiClass`] for a word token (first strong character, or `ON`).
pub fn word_bidi_class(text: &str) -> BidiClass {
    for ch in text.chars() {
        let cls = char_bidi_class(ch);
        if matches!(cls, BidiClass::L | BidiClass::R | BidiClass::AL) {
            return cls;
        }
    }
    for ch in text.chars() {
        let cls = char_bidi_class(ch);
        if matches!(cls, BidiClass::EN | BidiClass::AN) {
            return cls;
        }
    }
    BidiClass::ON
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_mixed_ltr_rtl_and_numbers() {
        // LTR paragraph: "Hello שלום 123 world"
        let classes = [
            BidiClass::L,  // Hello
            BidiClass::WS, // ' '
            BidiClass::R,  // שלום
            BidiClass::WS, // ' '
            BidiClass::EN, // 123
            BidiClass::WS, // ' '
            BidiClass::L,  // world
        ];
        let levels = resolve_levels(&classes, 0);
        // "שלום" gets level 1; "123" after R gets level 2, neutral space between R and EN resolves to R (level 1).
        assert_eq!(levels[0], 0);
        assert_eq!(levels[2], 1);
        assert_eq!(levels[4], 2);
        assert_eq!(levels[6], 0);
        let order = reorder_visual_indices(&levels);
        // Visual order: Hello (0), ' '(1), 123(4), ' '(3), שלום(2), ' '(5), world(6)
        assert_eq!(order, vec![0, 1, 4, 3, 2, 5, 6]);
    }

    #[test]
    fn resolves_rtl_paragraph_with_embedded_ltr() {
        // RTL paragraph: "שלום hello עולם"
        let classes = [
            BidiClass::R,
            BidiClass::WS,
            BidiClass::L,
            BidiClass::WS,
            BidiClass::R,
        ];
        let levels = resolve_levels(&classes, 1);
        assert_eq!(levels, vec![1, 1, 2, 1, 1]);
        let order = reorder_visual_indices(&levels);
        assert_eq!(order, vec![4, 3, 2, 1, 0]);
    }

    #[test]
    fn mirrors_brackets_and_reverses_rtl_word() {
        assert_eq!(word_to_visual("שלום", 1), "םולש");
        assert_eq!(
            word_to_visual("(a)", 1),
            "(a)".chars().rev().map(mirror_char).collect::<String>()
        );
        assert_eq!(word_to_visual("hello", 0), "hello");
    }
}
