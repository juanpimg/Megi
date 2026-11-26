use super::*;

#[test]
fn context_menu_items_depend_on_targets() {
    let page = ContextMenu::new(None, None, 10.0, 10.0);
    assert_eq!(page.items.len(), 4, "page actions");
    let link = ContextMenu::new(Some("https://example.com/a".to_string()), None, 10.0, 10.0);
    assert_eq!(link.items.len(), 6);
    assert_eq!(link.items[0].action, MenuAction::OpenLinkInNewTab);
    assert_eq!(link.items[1].action, MenuAction::CopyLinkAddress);
    let image = crate::images::ImageRegion {
        node_id: megi_webview::core::NodeId::from_index(0),
        rect: crate::links::CssRect::new(0.0, 0.0, 10.0, 10.0),
        url: "https://example.com/i.png".to_string(),
    };
    let both = ContextMenu::new(
        Some("https://example.com/a".to_string()),
        Some(image),
        10.0,
        10.0,
    );
    assert_eq!(both.items.len(), 8);
    assert_eq!(both.items[2].action, MenuAction::SaveImage);
    assert_eq!(both.items[3].action, MenuAction::CopyImage);
    assert!(!MenuAction::CopyImage.is_implemented());
    assert!(MenuAction::SavePagePng.is_implemented());
}

#[test]
fn context_menu_navigation_layout_and_hit() {
    let mut menu = ContextMenu::new(None, None, 10.0, 10.0);
    assert!(menu.move_selection(true));
    assert_eq!(menu.selected, 1);
    assert!(menu.move_selection(false));
    menu.move_selection(false);
    assert_eq!(menu.selected, menu.items.len() - 1, "wraps upward");
    let layout = context_menu_layout(&menu, 320.0, 200);
    assert!(layout.rect.x >= 2.0 && layout.rect.y >= 2.0);
    assert_eq!(layout.rows.len(), menu.items.len());
    let row = layout.rows[1];
    assert_eq!(context_menu_hit(&layout, row.x + 2.0, row.y + 2.0), Some(1));
    assert_eq!(context_menu_hit(&layout, 1.0, 1.0), None);
    // Near the bottom edge the menu flips above the cursor.
    let flipped = ContextMenu::new(None, None, 300.0, 195.0);
    let flipped_layout = context_menu_layout(&flipped, 320.0, 200);
    assert!(flipped_layout.rect.y + flipped_layout.rect.height <= 200.0);
}

#[test]
fn history_menu_layout_hit_and_draw() {
    let entries = vec![
        crate::history::HistoryEntry {
            title: "A".to_string(),
            url: "https://example.com/a".to_string(),
            logical_time: 1,
        },
        crate::history::HistoryEntry {
            title: "B".to_string(),
            url: "https://example.com/b".to_string(),
            logical_time: 2,
        },
    ];
    let menu = crate::history::HistoryMenu::open(entries, 40.0, 60.0);
    let layout = history_menu_layout(&menu, 480.0, 300);
    assert_eq!(layout.rows.len(), 2);
    assert_eq!(
        history_menu_hit(&layout, layout.rows[1].x + 2.0, layout.rows[1].y + 2.0),
        Some(1)
    );
    assert_eq!(history_menu_hit(&layout, 1.0, 1.0), None);
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut dest = buffer(480, 300);
    let mut view = view(&address, None, &tabs);
    view.history_menu = Some(&menu);
    draw_chrome(&mut dest, 480, 300, 1.0, &view);
    let selected = layout.rows[0];
    assert_eq!(
        dest[((selected.y + 3.0) as u32 * 480 + (selected.x + selected.width - 4.0) as u32)
            as usize],
        rgb(COLOR_MENU_SELECTED),
        "selected history row"
    );
}

#[test]
fn navigation_buttons_layout_hit_and_disabled_state() {
    let buttons = navigation_buttons(640.0);
    assert_eq!(buttons.back.width, NAV_BUTTON_SIZE);
    assert!(buttons.forward.x > buttons.back.x);
    assert_eq!(
        navigation_button_at(
            &buttons,
            buttons.back.x + 2.0,
            buttons.back.y + 2.0,
            true,
            false
        ),
        Some(NavigationButton::Back)
    );
    assert_eq!(
        navigation_button_at(
            &buttons,
            buttons.back.x + 2.0,
            buttons.back.y + 2.0,
            false,
            false
        ),
        None,
        "disabled back button is not clickable"
    );
    assert_eq!(
        navigation_button_at(
            &buttons,
            buttons.forward.x + 2.0,
            buttons.forward.y + 2.0,
            false,
            true
        ),
        Some(NavigationButton::Forward)
    );
    assert_eq!(
        navigation_button_at(
            &buttons,
            buttons.reload.x + 2.0,
            buttons.reload.y + 2.0,
            false,
            false
        ),
        Some(NavigationButton::Reload),
        "reload is always enabled"
    );
    assert_eq!(navigation_button_at(&buttons, 1.0, 1.0, true, true), None);
}

#[test]
fn history_button_geometry_and_hit_testing() {
    let rect = history_button_rect(640.0);
    assert!(rect.x + rect.width <= 640.0);
    assert!(history_button_hit(640.0, rect.x + 2.0, rect.y + 2.0));
    assert!(!history_button_hit(640.0, 1.0, 1.0));
    let address = AddressBar::new();
    let tabs = vec!["Megi".to_string()];
    let mut view = view(&address, None, &tabs);
    view.history_button = true;
    assert!(view.history_button);
}

#[test]
fn chrome_geometry_helpers() {
    assert_eq!(CHROME_HEIGHT, 78.0);
    assert!(in_chrome(10.0, 10.0, 640.0));
    assert!(!in_chrome(10.0, CHROME_HEIGHT + 1.0, 640.0));
    assert_eq!(truncate_title("Hello World", 5), "Hell…");
    assert_eq!(truncate_title("Hi", 5), "Hi");
    assert_eq!(address_bar_rect(640.0).width, 632.0);
}
