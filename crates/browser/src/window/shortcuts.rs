//! Central shell shortcut registry and conflict detection.
//!
//! `resolve_shortcut` is the single place that maps a modifier mask plus logical
//! key to a [`ShellAction`]. The event loop uses it for the bindings listed here;
//! actions introduced before their shell behaviour exists resolve but are
//! deliberately ignored by the handler; all workspace actions are dispatched.

use winit::keyboard::{Key, NamedKey};

/// Modifier mask used by the registry (no `Meta` binding exists in this shell).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Modifiers {
    pub(super) ctrl: bool,
    pub(super) alt: bool,
    pub(super) shift: bool,
}

impl Modifiers {
    pub(super) const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
    };
}

/// A shell command bound to a keyboard shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShellAction {
    ToggleReader,
    ToggleHistory,
    SaveFrame,
    SaveAs,
    FocusAddressBar,
    OpenFind,
    PrintDocumentInfo,
    NewTab,
    CloseTab,
    CycleTab { next: bool },
    ActivateTab(usize),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    NavigateHome,
    Quit,
    Reload,
    GoBack,
    GoForward,
    ToggleRail,
    ToggleCompact,
    CycleWorkspace,
    NewWorkspace,
    CloseWorkspace,
}

/// One registered binding: a modifier mask and a logical key.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ShortcutBinding {
    pub(super) mods: Modifiers,
    pub(super) key: Key,
}

/// Case-insensitive character key, matching the existing `eq_ignore_ascii_case` checks.
#[cfg(test)]
fn letter(text: &str) -> Key {
    Key::Character(text.to_lowercase().into())
}

/// Returns the action bound to `mods` + `key`, preserving all historical bindings.
pub(super) fn resolve_shortcut(mods: Modifiers, key: &Key) -> Option<ShellAction> {
    let key = key.as_ref();
    let lower = match key {
        Key::Character(text) => Some(text.to_lowercase()),
        _ => None,
    };
    let character = lower.as_deref();

    // The original Alt handlers did not exclude Ctrl/Shift, so keep that order.
    if mods.alt {
        if let Some(digit) = character.and_then(|text| text.chars().next())
            && let Some(index) = digit.to_digit(10)
            && (1..=9).contains(&index)
        {
            return Some(ShellAction::ActivateTab(index as usize - 1));
        }
        if key == Key::Named(NamedKey::Home) {
            return Some(ShellAction::NavigateHome);
        }
        if key == Key::Named(NamedKey::ArrowLeft) {
            return Some(ShellAction::GoBack);
        }
        if key == Key::Named(NamedKey::ArrowRight) {
            return Some(ShellAction::GoForward);
        }
    }
    if mods.ctrl && !mods.alt && !mods.shift {
        return match character {
            Some("h") => Some(ShellAction::ToggleHistory),
            Some("l") => Some(ShellAction::FocusAddressBar),
            Some("f") => Some(ShellAction::OpenFind),
            Some("d") => Some(ShellAction::PrintDocumentInfo),
            Some("t") => Some(ShellAction::NewTab),
            Some("w") => Some(ShellAction::CloseTab),
            Some("s") => Some(ShellAction::SaveFrame),
            Some("b") => Some(ShellAction::ToggleRail),
            Some("=" | "+") => Some(ShellAction::ZoomIn),
            Some("-" | "_") => Some(ShellAction::ZoomOut),
            Some("0") => Some(ShellAction::ZoomReset),
            _ if key == Key::Named(NamedKey::Tab) => Some(ShellAction::CycleTab { next: true }),
            _ => None,
        };
    }
    if mods.ctrl && mods.shift && !mods.alt {
        return match character {
            Some("r") => Some(ShellAction::ToggleReader),
            Some("s") => Some(ShellAction::SaveAs),
            Some("b") => Some(ShellAction::ToggleCompact),
            Some("w") => Some(ShellAction::NewWorkspace),
            _ if key == Key::Named(NamedKey::Tab) => Some(ShellAction::CycleTab { next: false }),
            _ => None,
        };
    }
    if mods.ctrl && mods.alt && mods.shift {
        return match character {
            Some("w") => Some(ShellAction::CloseWorkspace),
            _ => None,
        };
    }
    if mods.ctrl && mods.alt && !mods.shift {
        return match character {
            Some("w") => Some(ShellAction::CycleWorkspace),
            _ => None,
        };
    }
    if mods == Modifiers::NONE {
        return match key {
            Key::Named(NamedKey::Escape) => Some(ShellAction::Quit),
            Key::Named(NamedKey::F5) => Some(ShellAction::Reload),
            Key::Named(NamedKey::Backspace) => Some(ShellAction::GoBack),
            _ => match character {
                Some("q") => Some(ShellAction::Quit),
                Some("r" | "g") => Some(ShellAction::Reload),
                Some("b") => Some(ShellAction::GoBack),
                Some("n") => Some(ShellAction::GoForward),
                _ => None,
            },
        };
    }
    None
}

/// Returns the first registered binding for `action`, if it has one.
#[cfg(test)]
pub(super) fn binding_for(action: ShellAction) -> Option<ShortcutBinding> {
    all_bindings()
        .into_iter()
        .find(|binding| resolve_shortcut(binding.mods, &binding.key) == Some(action))
}

/// Every registry entry used for conflict detection.
#[cfg(test)]
fn all_bindings() -> Vec<ShortcutBinding> {
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    let ctrl_shift = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::NONE
    };
    let ctrl_alt = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::NONE
    };
    let ctrl_alt_shift = Modifiers {
        ctrl: true,
        alt: true,
        shift: true,
    };
    let alt = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    let named = |mods, key| ShortcutBinding { mods, key };
    let mut bindings = vec![
        named(ctrl_shift, letter("r")),
        named(ctrl, letter("h")),
        named(ctrl, letter("s")),
        named(ctrl_shift, letter("s")),
        named(ctrl, letter("l")),
        named(ctrl, letter("f")),
        named(ctrl, letter("d")),
        named(ctrl, letter("t")),
        named(ctrl, letter("w")),
        named(ctrl, Key::Named(NamedKey::Tab)),
        named(ctrl_shift, Key::Named(NamedKey::Tab)),
        named(ctrl, letter("=")),
        named(ctrl, letter("-")),
        named(ctrl, letter("0")),
        named(alt, Key::Named(NamedKey::Home)),
        named(Modifiers::NONE, letter("q")),
        named(Modifiers::NONE, letter("r")),
        named(Modifiers::NONE, letter("b")),
        named(Modifiers::NONE, letter("n")),
        named(Modifiers::NONE, Key::Named(NamedKey::Escape)),
        named(Modifiers::NONE, Key::Named(NamedKey::F5)),
        named(Modifiers::NONE, Key::Named(NamedKey::Backspace)),
        named(alt, Key::Named(NamedKey::ArrowLeft)),
        named(alt, Key::Named(NamedKey::ArrowRight)),
        named(ctrl, letter("b")),
        named(ctrl_shift, letter("b")),
        named(ctrl_shift, letter("w")),
        named(ctrl_alt, letter("w")),
        named(ctrl_alt_shift, letter("w")),
    ];
    for digit in '1'..='9' {
        bindings.push(named(alt, Key::Character(digit.to_string().into())));
    }
    bindings
}

/// Verifies that no two registry entries resolve to the same action.
#[cfg(test)]
pub(super) fn check_conflicts() -> Result<(), &'static str> {
    let bindings = all_bindings();
    for (index, first) in bindings.iter().enumerate() {
        for second in bindings.iter().skip(index + 1) {
            if first.mods == second.mods && first.key == second.key {
                return Err("duplicate shortcut binding");
            }
        }
    }
    Ok(())
}

/// Applies [`ShellAction::ToggleRail`] to the rail state. Opening starts expanded so
/// titles are visible in the first usable slice; closing keeps the expansion
/// preference for the next open. Other actions leave the state untouched.
pub(super) fn toggle_rail(open: bool, expanded: bool, action: ShellAction) -> (bool, bool) {
    if action != ShellAction::ToggleRail {
        return (open, expanded);
    }
    if open {
        (false, expanded)
    } else {
        (true, true)
    }
}
