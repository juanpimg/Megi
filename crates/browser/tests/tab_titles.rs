//! Tab display titles: readable fallback for `file:` tabs (WP-USER-ui-titles).
//!
//! Plan: docs/superpowers/plans/2026-11-26-ui-quality-unblock-plan.md §Task 4.
//! WHATWG URL §4.4: the URL path is a sequence of segments separated by `/`;
//! the last segment is the file name and may contain percent-encoded bytes.

use megi_browser::tabs::Tab;

fn tab(url: &str, title: Option<&str>) -> Tab {
    let mut tab = Tab::new(url).expect("test URL must parse");
    tab.title = title.map(str::to_owned);
    tab
}

#[test]
fn file_tab_without_title_shows_file_name_not_url() {
    let tab = tab("file:///home/jp/doc/wiki.html", None);
    assert_eq!(tab.display_title(), "wiki.html");
}

#[test]
fn http_tab_without_title_keeps_url() {
    let tab = tab("https://example.com/a", None);
    assert_eq!(tab.display_title(), "https://example.com/a");
}

#[test]
fn file_tab_with_title_keeps_document_title() {
    let tab = tab("file:///home/jp/doc/wiki.html", Some("Wiki"));
    assert_eq!(tab.display_title(), "Wiki");
}

#[test]
fn file_tab_percent_decodes_file_name() {
    let tab = tab("file:///home/jp/doc/wiki%20notes.html", None);
    assert_eq!(tab.display_title(), "wiki notes.html");
}

#[test]
fn file_tab_root_without_title_keeps_url() {
    let tab = tab("file:///", None);
    assert_eq!(tab.display_title(), "file:///");
}

#[test]
fn file_tab_directory_without_title_shows_last_segment() {
    let tab = tab("file:///home/jp/doc/", None);
    assert_eq!(tab.display_title(), "doc");
}

#[test]
fn data_tab_without_title_keeps_url() {
    let tab = tab("data:text/html,x", None);
    assert_eq!(tab.display_title(), "data:text/html,x");
}
