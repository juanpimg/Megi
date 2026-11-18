//! UAX #9 bidirectional reordering of inline token runs.
use super::*;

/// IFC layout with the inherited positioning context (needed to lay out atomic
/// inline-level boxes and their absolutely positioned descendants).
/// Finds an LRO/RLO control anywhere in this formatting context's text (#1448).
pub(crate) fn bidi_control_direction(box_node: &LayoutBox) -> Option<megi_css::Direction> {
    match &box_node.box_type {
        BoxType::TextNode(text) => crate::bidi::forced_direction_from_controls(text),
        _ => box_node.children.iter().find_map(bidi_control_direction),
    }
}

/// Applies UAX #9 bidirectional reordering to a single line's tokens.
pub(crate) fn reorder_line_tokens_bidi(
    tokens: &mut Vec<InlineToken>,
    text_pool: &mut String,
    styles: &[Arc<ComputedStyle>],
    container_style: &ComputedStyle,
    control_direction: Option<megi_css::Direction>,
) {
    use crate::bidi::{
        BidiClass, paragraph_base_level, reorder_visual_indices, resolve_levels, word_bidi_class,
        word_to_visual,
    };

    if tokens.is_empty() {
        return;
    }
    // A paragraph-level bidi-override forces every character, including Latin text,
    // into the container direction; per-word style indices are not reliable here.
    let forced_all = match container_style.unicode_bidi {
        megi_css::UnicodeBidi::BidiOverride | megi_css::UnicodeBidi::IsolateOverride => {
            Some(match container_style.direction {
                megi_css::Direction::Ltr => BidiClass::L,
                megi_css::Direction::Rtl => BidiClass::R,
            })
        }
        // LRO/RLO embedded in the text also force the run (#1448).
        _ => control_direction.map(|direction| match direction {
            megi_css::Direction::Ltr => BidiClass::L,
            megi_css::Direction::Rtl => BidiClass::R,
        }),
    };

    // Group balanced `BoxStart .. BoxEnd` spans as single reorderable units so
    // inline box nesting stays well-formed; flat tokens are singleton units.
    let mut groups: Vec<Vec<InlineToken>> = Vec::with_capacity(tokens.len());
    let mut depth = 0usize;
    for tok in tokens.drain(..) {
        match &tok {
            InlineToken::BoxStart { .. } => {
                if depth == 0 {
                    groups.push(vec![tok]);
                } else if let Some(last) = groups.last_mut() {
                    last.push(tok);
                }
                depth += 1;
            }
            InlineToken::BoxEnd { .. } => {
                if depth > 0 {
                    depth -= 1;
                    if let Some(last) = groups.last_mut() {
                        last.push(tok);
                    }
                } else {
                    groups.push(vec![tok]);
                }
            }
            _ => {
                if depth > 0 {
                    if let Some(last) = groups.last_mut() {
                        last.push(tok);
                    }
                } else {
                    groups.push(vec![tok]);
                }
            }
        }
    }

    let mut classes = Vec::with_capacity(groups.len());
    let mut first_strong = None;
    let override_dir = match container_style.unicode_bidi {
        megi_css::UnicodeBidi::BidiOverride | megi_css::UnicodeBidi::IsolateOverride => {
            Some(match container_style.direction {
                megi_css::Direction::Ltr => BidiClass::L,
                megi_css::Direction::Rtl => BidiClass::R,
            })
        }
        _ => None,
    };

    for group in &groups {
        let cls = if let Some(forced) = override_dir {
            forced
        } else {
            let mut found = None;
            for tok in group {
                match tok {
                    InlineToken::Word {
                        start, end, style, ..
                    } => {
                        let st = &styles[*style as usize];
                        if matches!(
                            st.unicode_bidi,
                            megi_css::UnicodeBidi::BidiOverride
                                | megi_css::UnicodeBidi::IsolateOverride
                        ) {
                            found = Some(match st.direction {
                                megi_css::Direction::Ltr => BidiClass::L,
                                megi_css::Direction::Rtl => BidiClass::R,
                            });
                        } else {
                            found = Some(word_bidi_class(&text_pool[*start..*end]));
                        }
                        break;
                    }
                    InlineToken::Space { .. } => {
                        if found.is_none() {
                            found = Some(BidiClass::WS);
                        }
                    }
                    InlineToken::Atomic { .. } => {
                        found = Some(BidiClass::ON);
                        break;
                    }
                    _ => {}
                }
            }
            found.unwrap_or(BidiClass::ON)
        };
        if first_strong.is_none() && matches!(cls, BidiClass::L | BidiClass::R | BidiClass::AL) {
            first_strong = Some(cls);
        }
        classes.push(cls);
    }

    let mut base_level = paragraph_base_level(
        container_style.direction,
        container_style.unicode_bidi,
        first_strong,
    );
    if let Some(class) = forced_all {
        classes.fill(class);
        base_level = match class {
            BidiClass::R | BidiClass::AL => 1,
            _ => 0,
        };
    }
    let levels = resolve_levels(&classes, base_level);
    let order = reorder_visual_indices(&levels);

    for idx in order {
        let level = levels[idx];
        for mut tok in std::mem::take(&mut groups[idx]) {
            if let InlineToken::Word {
                ref mut start,
                ref mut end,
                ..
            } = tok
                && level % 2 == 1
            {
                let visual = word_to_visual(&text_pool[*start..*end], level, forced_all);
                let new_start = text_pool.len();
                text_pool.push_str(&visual);
                *start = new_start;
                *end = text_pool.len();
            }
            tokens.push(tok);
        }
    }
}
