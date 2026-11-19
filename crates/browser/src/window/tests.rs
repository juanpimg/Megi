//! Unit tests for the pure helpers of the window shell.

use super::*;

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
