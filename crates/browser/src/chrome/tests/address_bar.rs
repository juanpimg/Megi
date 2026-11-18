use super::*;
use crate::chrome::address_bar::split_url_for_display;

#[test]
fn address_bar_selects_all_on_open_and_edits() {
    let mut bar = AddressBar::new();
    assert!(!bar.is_editing());
    bar.open("https://example.com/a");
    assert!(bar.is_editing());
    assert!(bar.has_selection(), "Ctrl+L selects the URL");
    assert_eq!(bar.selection_range(), 0.."https://example.com/a".len());
    // Typing replaces the selection.
    assert!(bar.insert("x"));
    assert_eq!(bar.text(), "x");
    assert!(!bar.has_selection());
    bar.insert("yz");
    assert_eq!(bar.text(), "xyz");
    assert!(bar.backspace());
    assert_eq!(bar.text(), "xy");
    assert!(!bar.delete_forward(), "caret at end");
    assert_eq!(bar.text(), "xy");
    bar.move_home(false);
    assert_eq!(bar.caret(), 0);
    assert!(bar.delete_forward());
    assert_eq!(bar.text(), "y");
    bar.move_end(false);
    assert_eq!(bar.caret(), bar.text().len());
    assert!(bar.finish());
    assert!(!bar.is_editing());
}

#[test]
fn address_bar_selection_copy_cut_paste_and_word_delete() {
    let mut bar = AddressBar::new();
    bar.open("one two three");
    bar.select_all();
    assert_eq!(bar.selected_text(), "one two three");
    assert!(bar.copy());
    assert_eq!(bar.clipboard(), "one two three");
    bar.move_end(false);
    bar.delete_word_back();
    assert_eq!(bar.text(), "one two ");
    bar.delete_word_back();
    assert_eq!(bar.text(), "one ");
    bar.delete_word_back();
    assert_eq!(bar.text(), "");
    assert!(bar.paste());
    assert_eq!(bar.text(), "one two three");
    // Cut the selection, then paste it back.
    bar.select_all();
    assert!(bar.cut());
    assert_eq!(bar.text(), "");
    assert!(bar.paste());
    assert_eq!(bar.text(), "one two three");
    // Shift+arrow extends selection, Delete removes it.
    bar.move_home(false);
    bar.move_caret(true, true);
    bar.move_caret(true, true);
    assert!(bar.has_selection());
    bar.delete_forward();
    assert_eq!(bar.text(), "e two three");
}

#[test]
fn address_bar_autocomplete_needs_two_chars_and_accepts_suffix() {
    let mut bar = AddressBar::new();
    bar.open("");
    bar.insert("h");
    let history = vec![
        "https://example.com/hello".to_string(),
        "https://example.com/help".to_string(),
    ];
    assert_eq!(bar.autocomplete(&history), None, "min 2 chars");
    // Completion extends the typed text, so type the beginning of the URL.
    bar.insert("t");
    bar.insert("t");
    assert_eq!(
        bar.autocomplete(&history).as_deref(),
        Some("https://example.com/hello")
    );
    assert!(bar.accept_completion(&history));
    assert_eq!(bar.text(), "https://example.com/hello");
    assert!(bar.has_selection(), "completion suffix selected");
}

#[test]
fn resolve_address_parses_absolute_and_joins_relative() {
    let base = megi_webview::net::Url::parse("https://example.com/dir/page.html").expect("base");
    assert_eq!(
        resolve_address("https://other.example/x", Some(&base))
            .expect("absolute")
            .as_str(),
        "https://other.example/x"
    );
    assert_eq!(
        resolve_address("other.html", Some(&base))
            .expect("relative")
            .as_str(),
        "https://example.com/dir/other.html"
    );
    assert!(resolve_address("", Some(&base)).is_err());
    assert!(resolve_address("https://[bad", Some(&base)).is_err());
    assert_eq!(
        resolve_address("wikipedia.org", Some(&base))
            .expect("bare domain")
            .as_str(),
        "https://wikipedia.org/"
    );
    assert_eq!(
        resolve_address("es.wikipedia.org/wiki/Rust", None)
            .expect("bare domain with path")
            .as_str(),
        "https://es.wikipedia.org/wiki/Rust"
    );
    assert_eq!(
        resolve_address("localhost:8080/test", None)
            .expect("localhost")
            .as_str(),
        "http://localhost:8080/test"
    );
}

#[test]
fn address_bar_home_end_and_shift_selection() {
    let mut bar = AddressBar::new();
    bar.open("abc");
    bar.select_all();
    bar.move_home(false);
    assert_eq!(bar.caret(), 0);
    assert!(!bar.has_selection());
    bar.move_end(true);
    assert!(bar.has_selection());
    assert_eq!(bar.selection_range(), 0..3);
    bar.move_home(true);
    assert_eq!(bar.selection_range(), 0..0);
    assert!(bar.insert("x"));
    assert_eq!(bar.text(), "xabc");
}

#[test]
fn autocomplete_requires_a_prefix_and_selects_only_the_suffix() {
    let history = vec!["https://example.com/".to_string()];
    let mut bar = AddressBar::new();
    bar.open("");
    bar.insert("exa");
    assert_eq!(
        bar.autocomplete(&history),
        None,
        "a middle substring is not a completion"
    );
    let mut bar = AddressBar::new();
    bar.open("");
    bar.insert("htt");
    assert_eq!(
        bar.autocomplete(&history).as_deref(),
        Some("https://example.com/")
    );
    assert!(bar.accept_completion(&history));
    assert_eq!(bar.text(), "https://example.com/");
    assert_eq!(bar.selected_text(), "ps://example.com/");
}
#[test]
fn accepting_a_completion_with_surrounding_spaces_keeps_a_valid_selection() {
    let history = vec!["https://example.com/".to_string()];
    let mut bar = AddressBar::new();
    bar.open("");
    bar.insert("  ht  ");
    assert!(bar.accept_completion(&history));
    assert_eq!(bar.text(), "https://example.com/");
    assert_eq!(bar.selected_text(), "tps://example.com/");
    let range = bar.selection_range();
    assert!(range.end <= bar.text().len(), "{range:?}");
    bar.delete_selection();
    assert_eq!(bar.text(), "ht");
}

#[test]
fn url_display_splits_scheme_authority_and_path() {
    assert_eq!(
        split_url_for_display("https://example.com/a/b"),
        ("https://", "example.com", "/a/b")
    );
    assert_eq!(
        split_url_for_display("file:///tmp/page.html"),
        ("file://", "", "/tmp/page.html")
    );
    assert_eq!(
        split_url_for_display("data:text/html,<h1>x</h1>"),
        ("", "data:text/html,<h1>x</h1>", "")
    );
}
