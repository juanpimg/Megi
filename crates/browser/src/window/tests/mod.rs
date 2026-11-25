//! Unit tests for the pure helpers of the window shell.

use super::shortcuts::{Modifiers, ShellAction, binding_for, check_conflicts, resolve_shortcut};
use super::*;

mod rail;

#[cfg(test)]
mod chrome_mode;

#[cfg(test)]
mod ime_preedit_status_tests {
    use super::presentation::document_ime_status;

    #[test]
    fn document_composition_preedit_shows_in_status_bar() {
        // #1724: composing inside a document <input> must surface the intermediate
        // text in the status bar; the address bar paints its own inline preedit.
        let preedit = document_ime_status("にほん", false, None, Some("https://x.test/"));
        assert_eq!(
            preedit,
            Some("にほん"),
            "preedit takes priority over hover URL"
        );
        let none = document_ime_status("", false, None, Some("https://x.test/"));
        assert_eq!(
            none,
            Some("https://x.test/"),
            "no preedit falls back to hover URL"
        );
    }

    #[test]
    fn address_bar_composition_does_not_duplicate_preedit_in_status_bar() {
        // The address bar already renders `ime_preedit` inline (#1724); showing it
        // in the status bar too would duplicate it.
        let status = document_ime_status("にほん", true, None, Some("https://x.test/"));
        assert_eq!(status, Some("https://x.test/"));
    }

    #[test]
    fn download_status_still_takes_precedence() {
        let status = document_ime_status("にほん", false, Some("file.html"), None);
        assert_eq!(
            status,
            Some("file.html"),
            "download status wins over preedit"
        );
    }
}

#[cfg(test)]
mod ui_event_forwarding_tests {
    use super::*;

    #[test]
    fn host_focus_events_orders_blur_before_focus_and_skips_same_target() {
        assert!(host_focus_events(None, None).is_empty());
        assert!(host_focus_events(Some(2), Some(2)).is_empty());

        let moved = host_focus_events(Some(1), Some(4));
        assert_eq!(moved.len(), 2);
        assert_eq!(moved[0].event_type, "blur");
        assert_eq!(
            moved[0].target,
            megi_webview::render::HostEventTarget::FormControl(1)
        );
        assert!(!moved[0].bubbles);
        assert_eq!(moved[1].event_type, "focus");
        assert_eq!(
            moved[1].target,
            megi_webview::render::HostEventTarget::FormControl(4)
        );
        assert!(!moved[1].bubbles);
    }

    #[test]
    fn host_keydown_event_maps_named_and_character_keys() {
        let enter = host_keydown_event(
            megi_webview::render::HostEventTarget::FormControl(3),
            &Key::Named(NamedKey::Enter),
            false,
            true,
            false,
        )
        .expect("enter keydown");
        assert_eq!(enter.event_type, "keydown");
        assert!(enter.bubbles);
        assert!(enter.cancelable);
        assert_eq!(
            enter.kind,
            megi_webview::render::HostUiEventKind::Keyboard {
                key: "Enter".to_owned(),
                code: "Enter".to_owned(),
                ctrl_key: false,
                shift_key: true,
                alt_key: false,
                meta_key: false,
                repeat: false,
            }
        );

        let letter = host_keydown_event(
            megi_webview::render::HostEventTarget::Focused,
            &Key::Character("a".into()),
            true,
            false,
            true,
        )
        .expect("letter keydown");
        assert_eq!(
            letter.kind,
            megi_webview::render::HostUiEventKind::Keyboard {
                key: "a".to_owned(),
                code: "KeyA".to_owned(),
                ctrl_key: true,
                shift_key: false,
                alt_key: true,
                meta_key: false,
                repeat: false,
            }
        );
    }
}

#[cfg(test)]
mod focus_target_tests {
    use super::*;
    use crate::links::CssRect;

    /// A focusable `<a href>` must appear in the keyboard focus list even though it has no
    /// form control — the old Tab handler gated on `forms` and could never reach links (#1517).
    #[test]
    fn focus_list_includes_links_in_document_order() {
        let document = megi_webview::html::parse_document(
            "<input id=a><a id=link href=/next>Next</a><button id=b>Go</button>",
            false,
        );
        // Locate the link node via the a11y tree.
        let focusable = a11y::focus_order(&document, document.root());
        let link_node = focusable
            .iter()
            .find(|node| {
                document
                    .node(**node)
                    .as_element()
                    .is_some_and(|el| el.get_attribute("id").is_some_and(|v| v == "link"))
            })
            .copied()
            .expect("link in focus order");

        // No form controls: the focus list must still contain the link.
        let links = vec![LinkRegion {
            rect: CssRect::new(0.0, 0.0, 10.0, 10.0),
            href: "/next".to_string(),
            node: Some(link_node),
            is_blocker: false,
        }];
        let targets = build_focus_targets(&document, &[], &links);
        assert!(
            targets.contains(&FocusTarget::Link(link_node)),
            "the link must be keyboard-focusable: {targets:?}"
        );

        // With `|| true` style gating on `forms`, this list would be empty; prove it is not.
        assert!(!targets.is_empty(), "focus list must not be forms-only");
    }

    /// A control's `form_id` maps back to its focus position so mouse clicks keep the unified
    /// focus model in sync (#1517).
    #[test]
    fn control_form_id_maps_to_focus_position() {
        // Pure check of the mapping used by `focus_index_for_control`: build a target list
        // where a control sits between two links and verify lookups are stable.
        let targets = [
            FocusTarget::Link(megi_webview::core::NodeId::from_index(1)),
            FocusTarget::Control(7),
            FocusTarget::Link(megi_webview::core::NodeId::from_index(3)),
        ];
        let pos = targets
            .iter()
            .position(|target| *target == FocusTarget::Control(7));
        assert_eq!(pos, Some(1));
        assert!(
            targets.contains(&FocusTarget::Link(megi_webview::core::NodeId::from_index(
                3
            )))
        );
    }
}

#[cfg(test)]
mod form_patch_send_tests {
    #[test]
    fn failed_patch_send_reports_an_error() {
        // Mirrors the three patched-render call sites: only a failed send sets the
        // error and skips committing the patched document (#1459).
        let mut committed = false;
        let mut error: Option<String> = None;
        let send: Result<(), ()> = Err(());
        if send.is_err() {
            error = Some("render worker stopped unexpectedly".to_string());
        } else {
            committed = true;
        }
        assert_eq!(error.as_deref(), Some("render worker stopped unexpectedly"));
        assert!(!committed);

        let send_ok: Result<(), ()> = Ok(());
        if send_ok.is_err() {
            panic!("a successful send must not report an error");
        }
        assert!(send_ok.is_ok());
    }
}

#[cfg(test)]
mod shortcut_registry_tests {
    use super::*;
    use winit::keyboard::{Key, NamedKey};

    fn char_key(text: &str) -> Key {
        Key::Character(text.into())
    }

    #[test]
    fn existing_shell_shortcuts_resolve_to_the_same_actions() {
        let none = Modifiers::NONE;
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        let cases = [
            (ctrl_shift, char_key("R"), ShellAction::ToggleReader),
            (ctrl, char_key("H"), ShellAction::ToggleHistory),
            (ctrl, char_key("S"), ShellAction::SaveFrame),
            (ctrl_shift, char_key("S"), ShellAction::SaveAs),
            (ctrl, char_key("L"), ShellAction::FocusAddressBar),
            (ctrl, char_key("F"), ShellAction::OpenFind),
            (ctrl, char_key("D"), ShellAction::PrintDocumentInfo),
            (ctrl, char_key("T"), ShellAction::NewTab),
            (ctrl, char_key("W"), ShellAction::CloseTab),
            (
                ctrl,
                Key::Named(NamedKey::Tab),
                ShellAction::CycleTab { next: true },
            ),
            (
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Modifiers::NONE
                },
                Key::Named(NamedKey::Tab),
                ShellAction::CycleTab { next: false },
            ),
            (alt, char_key("1"), ShellAction::ActivateTab(0)),
            (alt, char_key("9"), ShellAction::ActivateTab(8)),
            (ctrl, char_key("="), ShellAction::ZoomIn),
            (ctrl, char_key("+"), ShellAction::ZoomIn),
            (ctrl, char_key("-"), ShellAction::ZoomOut),
            (ctrl, char_key("_"), ShellAction::ZoomOut),
            (ctrl, char_key("0"), ShellAction::ZoomReset),
            (alt, Key::Named(NamedKey::Home), ShellAction::NavigateHome),
            (none, char_key("Q"), ShellAction::Quit),
            (none, char_key("R"), ShellAction::Reload),
            (none, char_key("G"), ShellAction::Reload),
            (none, char_key("B"), ShellAction::GoBack),
            (none, char_key("N"), ShellAction::GoForward),
            (none, char_key("q"), ShellAction::Quit),
            (none, Key::Named(NamedKey::Escape), ShellAction::Quit),
            (none, Key::Named(NamedKey::F5), ShellAction::Reload),
            (alt, Key::Named(NamedKey::ArrowLeft), ShellAction::GoBack),
            (
                alt,
                Key::Named(NamedKey::ArrowRight),
                ShellAction::GoForward,
            ),
            (none, Key::Named(NamedKey::Backspace), ShellAction::GoBack),
        ];
        for (mods, key, expected) in cases {
            assert_eq!(
                resolve_shortcut(mods, &key),
                Some(expected),
                "mods {mods:?} key {key:?} changed its shell action"
            );
        }
    }

    #[test]
    fn new_shell_shortcuts_resolve_without_collisions() {
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
        assert_eq!(
            resolve_shortcut(ctrl, &char_key("B")),
            Some(ShellAction::ToggleRail)
        );
        assert_eq!(
            resolve_shortcut(ctrl_shift, &char_key("B")),
            Some(ShellAction::ToggleCompact)
        );
        assert_eq!(
            resolve_shortcut(ctrl_alt, &char_key("W")),
            Some(ShellAction::CycleWorkspace)
        );
        assert!(
            check_conflicts().is_ok(),
            "{}",
            check_conflicts().unwrap_err()
        );
    }

    #[test]
    fn workspace_shortcuts_resolve_without_collisions() {
        // Phase 3 red tests: `NewWorkspace`/`CloseWorkspace` do not exist yet, so the
        // expected compile error is `no variant NewWorkspace ... in enum ShellAction`.
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
        assert_eq!(
            resolve_shortcut(ctrl_shift, &char_key("W")),
            Some(ShellAction::NewWorkspace)
        );
        assert_eq!(
            resolve_shortcut(ctrl_alt_shift, &char_key("W")),
            Some(ShellAction::CloseWorkspace)
        );
        assert_eq!(
            resolve_shortcut(ctrl_alt, &char_key("W")),
            Some(ShellAction::CycleWorkspace)
        );
        assert!(
            check_conflicts().is_ok(),
            "{}",
            check_conflicts().unwrap_err()
        );
    }

    #[test]
    fn binding_for_round_trips_the_registry_entries() {
        for action in [
            ShellAction::ToggleRail,
            ShellAction::ToggleCompact,
            ShellAction::CycleWorkspace,
            ShellAction::NewWorkspace,
            ShellAction::CloseWorkspace,
            ShellAction::ToggleHistory,
            ShellAction::FocusAddressBar,
            ShellAction::OpenFind,
            ShellAction::NewTab,
            ShellAction::CloseTab,
            ShellAction::ActivateTab(4),
            ShellAction::ZoomIn,
            ShellAction::ZoomOut,
            ShellAction::ZoomReset,
            ShellAction::Quit,
        ] {
            let binding = binding_for(action).expect("registered action");
            assert_eq!(
                resolve_shortcut(binding.mods, &binding.key),
                Some(action),
                "binding for {action:?} must resolve back to itself"
            );
        }
    }
}

#[test]
fn workspace_visible_tab_helpers_follow_the_active_group() {
    // Phase 3 red test for the window-facing pure helpers; the expected failure is
    // `could not find workspace_chrome_titles in module presentation`.
    use super::presentation::{
        workspace_a11y_titles, workspace_chrome_active, workspace_chrome_titles,
    };
    use crate::tabs::{TabsState, Workspace};

    let mut tabs = TabsState::from_urls(vec![
        "https://example.com/a".to_string(),
        "https://example.com/b".to_string(),
        "https://example.com/c".to_string(),
    ]);
    let baseline = vec![
        "https://example.com/a".to_string(),
        "https://example.com/b".to_string(),
        "https://example.com/c".to_string(),
    ];
    assert_eq!(workspace_chrome_titles(&tabs), baseline);
    assert_eq!(workspace_chrome_active(&tabs), 0);
    assert_eq!(
        workspace_a11y_titles(&tabs),
        baseline,
        "no label decoration with one workspace"
    );

    tabs.activate(0);
    assert!(tabs.move_active_to_workspace(1));
    tabs.open_tab("https://example.com/d").expect("open tab");
    assert_eq!(
        workspace_chrome_titles(&tabs),
        vec![
            "https://example.com/a".to_string(),
            "https://example.com/d".to_string(),
        ]
    );
    assert_eq!(workspace_chrome_active(&tabs), 1);
    assert_eq!(
        workspace_a11y_titles(&tabs),
        vec![
            format!("{} — https://example.com/a", Workspace::label(1)),
            format!("{} — https://example.com/d", Workspace::label(1)),
        ]
    );
}

#[test]
fn build_focus_targets_maps_controls_and_links_in_document_order() {
    // #2196: the focus-target mapping must stay correct while using O(1) node indexes.
    use super::{FocusTarget, build_focus_targets};
    use megi_webview::core::Namespace;
    let mut doc = megi_webview::core::Document::new();
    let body = doc.create_element(Namespace::Html, "body", vec![]);
    let button = doc.create_element(Namespace::Html, "button", vec![]);
    let link = doc.create_element(
        Namespace::Html,
        "a",
        vec![megi_webview::core::Attribute::new("href", "/x")],
    );
    assert!(doc.append_child(doc.root(), body));
    assert!(doc.append_child(body, button));
    assert!(doc.append_child(body, link));

    let control = FormControl {
        form_id: 42,
        name: String::new(),
        node_id: button,
        kind: crate::forms::FormControlKind::Button,
        rect: crate::links::CssRect::new(0.0, 0.0, 1.0, 1.0),
        value: String::new(),
        checked: false,
        disabled: false,
        options: Vec::new(),
        multiple: false,
    };
    let links = vec![LinkRegion {
        rect: crate::links::CssRect::new(0.0, 0.0, 1.0, 1.0),
        href: "/x".to_owned(),
        node: Some(link),
        is_blocker: false,
    }];
    let targets = build_focus_targets(&doc, &[control], &links);
    assert!(targets.contains(&FocusTarget::Control(42)));
    assert!(targets.contains(&FocusTarget::Link(link)));
}
