use super::*;
use crate::i18n::{Locale, strings};

#[test]
fn error_page_uses_locale_strings_and_lang() {
    let es = error_html_with_locale("http://x/", "HTTP", "boom", Locale::Es);
    assert!(es.contains("lang=\"es\""));
    assert!(es.contains(strings(Locale::Es).error_loading));
    assert!(es.contains(">Reintentar</a>"));
    let en = error_html_with_locale("http://x/", "HTTP", "boom", Locale::En);
    assert!(en.contains("lang=\"en\""));
    assert!(en.contains("Error loading page"));
    assert!(en.contains(">Retry</a>"));
    assert!(!en.contains("Reintentar"));
}

#[test]
fn reader_page_uses_locale_lang_and_source_label() {
    let en = reader_page_with_locale("T", &["p".to_owned()], "http://x/", Locale::En);
    assert!(en.contains("lang=\"en\""));
    assert!(en.contains("Read from"));
    let es = reader_page_with_locale("T", &["p".to_owned()], "http://x/", Locale::Es);
    assert!(es.contains("lang=\"es\""));
    assert!(es.contains("Leído desde"));
}
