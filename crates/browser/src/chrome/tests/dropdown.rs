use super::*;
use crate::forms::SelectOption;
use crate::links::CssRect;

#[test]
fn dropdown_layout_caps_rows_and_scrolls() {
    let control = CssRect::new(10.0, 100.0, 120.0, 18.0);
    let list = dropdown_layout(&control, 20, 400, 0, 0);
    assert_eq!(list.visible, DROPDOWN_MAX_VISIBLE);
    assert_eq!(list.total, 20);
    assert_eq!(list.first, 0);
    assert!(list.rect.height > 0.0);
    let scrolled = dropdown_layout(&control, 20, 400, 0, 5);
    assert_eq!(scrolled.first, 5);
    assert_eq!(
        dropdown_layout(&control, 20, 400, 0, 999).first,
        20 - DROPDOWN_MAX_VISIBLE
    );
}

#[test]
fn dropdown_hit_and_state_navigation() {
    let control = CssRect::new(10.0, 100.0, 120.0, 18.0);
    let options = vec![
        SelectOption::new("One", "1", true),
        SelectOption::new("Two", "2", false),
        SelectOption::new("Three", "3", false),
    ];
    let mut dropdown = DropdownState::open(7, control, options.clone());
    assert!(dropdown.is_open());
    assert_eq!(dropdown.active(), Some(0));
    assert!(dropdown.move_active(true));
    assert_eq!(dropdown.active(), Some(1));
    assert!(!dropdown.move_active(true) || dropdown.active() == Some(2));
    let list = dropdown_layout(&control, options.len(), 400, 0, 0);
    let row = list.rows[1];
    assert_eq!(list.hit_option(row.x + 2.0, row.y + 2.0), Some(1));
    assert!(list.contains_point(list.rect.x + 1.0, list.rect.y + 1.0));
    assert!(!dropdown.scroll_by(5), "nothing to scroll");
    dropdown.close();
    assert!(!dropdown.is_open());
}

#[test]
fn dropdown_opens_above_when_no_room_below() {
    let control = CssRect::new(10.0, 360.0, 120.0, 18.0);
    let list = dropdown_layout(&control, 6, 400, 0, 0);
    assert!(list.above);
    assert!(list.rect.y < 360.0);
}

#[test]
fn dropdown_optgroup_headers_are_not_selectable() {
    let options = vec![
        SelectOption::new("Apple", "a", false).with_group(Some("Fruits".to_string())),
        SelectOption::new("Pear", "p", false).with_group(Some("Fruits".to_string())),
        SelectOption::new("Leek", "l", false).with_group(Some("Veg".to_string())),
    ];
    let items = dropdown_items(&options);
    assert_eq!(items.len(), 5, "2 group headers + 3 options");
    assert_eq!(items[0].label, "Fruits");
    assert!(!items[0].is_option());
    assert_eq!(items[1].option_index, Some(0));
    let control = CssRect::new(10.0, 40.0, 120.0, 18.0);
    let list = dropdown_layout_items(&control, &items, 400, 0, 0);
    assert_eq!(
        list.hit_option(list.rows[0].x + 2.0, list.rows[0].y + 2.0),
        None
    );
    assert_eq!(
        list.hit_option(list.rows[1].x + 2.0, list.rows[1].y + 2.0),
        Some(0)
    );
}

#[test]
fn dropdown_keyboard_home_end_page_and_first_char_skip_headers() {
    let options = vec![
        SelectOption::new("Apple", "a", false).with_group(Some("Fruits".to_string())),
        SelectOption::new("Pear", "p", false).with_group(Some("Fruits".to_string())),
        SelectOption::new("Leek", "l", false).with_group(Some("Veg".to_string())),
    ];
    let control = CssRect::new(10.0, 40.0, 120.0, 18.0);
    let mut dropdown = DropdownState::open(1, control, options);
    assert!(dropdown.move_home());
    assert_eq!(
        dropdown.active(),
        Some(1),
        "first selectable row (after header)"
    );
    assert!(dropdown.move_page(true));
    assert_eq!(
        dropdown.active(),
        Some(4),
        "PageDown reaches the last option"
    );
    assert!(dropdown.move_page(false));
    assert_eq!(
        dropdown.active(),
        Some(1),
        "PageUp returns to the first option"
    );
    assert!(dropdown.first_char('p'));
    assert_eq!(dropdown.active(), Some(2));
    assert!(dropdown.move_end());
    assert_eq!(dropdown.active(), Some(4));
}
