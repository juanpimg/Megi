use crate::*;

// ---------------------------------------------------------------------------
// Reduced PSL / sites
// ---------------------------------------------------------------------------

#[test]
fn reduced_psl_recognises_public_suffixes() {
    for suffix in ["com", "org", "co.uk", "com.ar", "github.io", "pages.dev"] {
        assert!(is_public_suffix(suffix), "{suffix} must be a public suffix");
    }
    for host in ["example.com", "a.co.uk", "www.example.github.io"] {
        assert!(!is_public_suffix(host), "{host} is not a public suffix");
    }
    assert!(is_public_suffix("localhost"));
    assert!(!is_public_suffix("127.0.0.1"));
}

#[test]
fn registrable_domain_uses_longest_listed_suffix() {
    assert_eq!(
        registrable_domain("www.example.com").as_deref(),
        Some("example.com")
    );
    assert_eq!(
        registrable_domain("a.b.example.co.uk").as_deref(),
        Some("example.co.uk")
    );
    assert_eq!(
        registrable_domain("shop.example.com.ar").as_deref(),
        Some("example.com.ar")
    );
    assert_eq!(
        registrable_domain("tenant.github.io").as_deref(),
        Some("tenant.github.io")
    );
    assert_eq!(registrable_domain("co.uk"), None);
    assert_eq!(registrable_domain("github.io"), None);
    assert_eq!(registrable_domain("127.0.0.1"), None);
    assert_eq!(registrable_domain("localhost"), None);
    // Last-label fallback for unknown TLDs.
    assert_eq!(
        registrable_domain("www.example.unknown-tld").as_deref(),
        Some("example.unknown-tld")
    );
}

#[test]
fn site_and_origin_helpers() {
    let a = url("https://www.example.co.uk/a");
    let b = url("https://shop.example.co.uk/b");
    let c = url("https://example.com/");
    assert_eq!(site_for_url(&a).as_deref(), Some("https://example.co.uk"));
    assert!(same_site(&a, &b));
    assert!(!same_site(&a, &c));
    assert!(same_origin(&a, &url("https://www.example.co.uk/other")));
    assert!(!same_origin(&a, &b));
    assert!(!same_origin(&a, &url("http://www.example.co.uk/a")));
}
