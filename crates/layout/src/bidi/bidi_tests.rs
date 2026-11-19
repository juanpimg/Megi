//! Tests for the UAX #9 subset in [`super`].

#[cfg(test)]
mod tests {
    use crate::bidi::*;

    #[test]
    fn rtl_word_keeps_digits_in_ltr_order() {
        // "שלום123": the Hebrew letters reverse visually, the Latin digits must stay "123".
        assert_eq!(word_to_visual("שלום123", 1, None), "123םולש");
        // Arabic-Indic digits (AN) behave the same way.
        let arabic = word_to_visual("مرحبا١٢٣", 1, None);
        assert!(
            arabic.starts_with("١٢٣"),
            "AN digits must keep their order: {arabic:?}"
        );
    }

    #[test]
    fn nonspacing_marks_stay_within_the_rtl_run() {
        // W1: niqqud inherits the preceding letter's class instead of becoming L,
        // so it stays inside the reversed RTL run adjacent to its base (#1447).
        let visual = word_to_visual("שָלום", 1, None);
        let chars: Vec<char> = visual.chars().collect();
        assert_eq!(chars.len(), 5);
        let base = chars.iter().position(|&ch| ch == 'ש').expect("base letter");
        assert!(
            chars.get(base.wrapping_sub(1)) == Some(&'\u{05B8}')
                || chars.get(base + 1) == Some(&'\u{05B8}'),
            "the niqqud must remain adjacent to its base: {visual:?}"
        );
    }

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
        assert_eq!(word_to_visual("שלום", 1, None), "םולש");
        assert_eq!(
            word_to_visual("(a)", 1, None),
            "(a)".chars().rev().map(mirror_char).collect::<String>()
        );
        assert_eq!(word_to_visual("hello", 0, None), "hello");
    }
}

#[cfg(test)]
mod bidi_control_tests {
    use crate::bidi::*;

    #[test]
    fn lro_rlo_controls_are_recognized_and_strippable() {
        assert!(is_bidi_control('\u{202E}'));
        assert!(is_bidi_control('\u{202D}'));
        assert!(is_bidi_control('\u{2066}'));
        assert!(!is_bidi_control('a'));
        assert_eq!(
            forced_direction_from_controls("ab\u{202E}cd"),
            Some(Direction::Rtl)
        );
        assert_eq!(
            forced_direction_from_controls("ab\u{202D}cd"),
            Some(Direction::Ltr)
        );
        assert_eq!(forced_direction_from_controls("abcd"), None);
    }
}

#[cfg(test)]
mod bidi_ltr_inside_rtl_tests {
    use crate::bidi::word_to_visual;

    #[test]
    fn ltr_word_inside_rtl_paragraph_keeps_character_order() {
        // #1978: an LTR run in an RTL paragraph changes position, not its letter order.
        assert_eq!(word_to_visual("RTL AR:", 1, None), "RTL AR:");
        assert_eq!(word_to_visual("ABC123", 1, None), "ABC123");
    }
}
