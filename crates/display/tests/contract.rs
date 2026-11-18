//! The display-list contract is pure data: owned, 'static and lifetime-free.

use megi_display::{DisplayItem, DisplayList, HitRegion};

fn assert_owned<T: 'static>() {}

#[test]
fn display_contract_types_are_owned_and_lifetime_free() {
    assert_owned::<DisplayItem>();
    assert_owned::<DisplayList>();
    assert_owned::<HitRegion>();

    let list = DisplayList::new();
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
}
