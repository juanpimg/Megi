use super::*;

/// Visual state of a native widget (UA-style colors used by the shell overlay/tests).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WidgetVisual {
    /// Background RGB.
    pub background: [u8; 3],
    /// Border/focus-ring RGB.
    pub border: [u8; 3],
    /// Whether a focus ring is drawn.
    pub focus_ring: bool,
}

/// Deterministic UA-style widget colors for hover/active/focus/disabled states.
pub fn widget_visual(
    kind: FormControlKind,
    checked: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    disabled: bool,
) -> WidgetVisual {
    let (background, border) = if disabled {
        ([0xE9, 0xEC, 0xEF], [0xB8, 0xBF, 0xC7])
    } else if pressed {
        ([0xD8, 0xDE, 0xE6], [0x70, 0x7A, 0x87])
    } else if hovered {
        ([0xF3, 0xF6, 0xF9], [0x70, 0x7A, 0x87])
    } else {
        ([0xFF, 0xFF, 0xFF], [0x8C, 0x95, 0x9F])
    };
    let background = if kind.is_checkable() && checked {
        if disabled {
            [0xBC, 0xCF, 0xEA]
        } else {
            [0x1F, 0x6F, 0xEB]
        }
    } else {
        background
    };
    WidgetVisual {
        background,
        border,
        focus_ring: focused && !disabled,
    }
}

/// Deterministic caret visibility for text inputs: visible for the first half of a
/// 1060 ms blink period (logical ticks, no wall clock). `tick` counts 50 ms units.
pub fn caret_visible(tick: u64) -> bool {
    (tick % 21) < 11
}
