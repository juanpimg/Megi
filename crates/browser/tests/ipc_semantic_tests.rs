//! Semantic security tests for the F0 IPC cookie boundary.
//!
//! Spec: docs/superpowers/plans/2026-11-24-security-isolation-plan.md §Task 2.
//! RFC 6265 §5.1.3 (domain matching), §5.2.5 (HttpOnly), PSL eTLD+1 site semantics.

use megi_browser::ipc::{
    CookiePolicyError, IpcCookie, IpcRenderOptions, IpcRenderResponse, IpcSameSite,
    seeded_cookies_for, validate_cookie_from_renderer,
};
use megi_browser::site::site_key;
use megi_webview::net::{CookieJar, Url};

fn ipc_cookie(domain: &str, name: &str, http_only: bool, secure: bool) -> IpcCookie {
    IpcCookie {
        name: name.to_owned(),
        value: "v".to_owned(),
        domain: domain.to_owned(),
        host_only: false,
        path: "/".to_owned(),
        secure,
        same_site: IpcSameSite::Lax,
        http_only,
        expires_epoch_secs: None,
    }
}

#[test]
fn cookie_from_other_site_is_rejected() {
    let site = site_key("https://site-a.test/");
    let forged = ipc_cookie("bank.example", "sid", false, false);
    assert_eq!(
        validate_cookie_from_renderer(&forged, &site),
        Err(CookiePolicyError::DomainMismatch)
    );
}

#[test]
fn httponly_never_seeded_to_renderer() {
    let mut jar = CookieJar::new();
    let url = Url::parse("https://site-a.test/").expect("url");
    assert!(jar.store_set_cookie(&url, "sid=secret; Path=/; HttpOnly; Secure"));
    assert!(jar.store_set_cookie(&url, "theme=dark; Path=/"));
    let site = site_key("https://site-a.test/");

    let seeded = seeded_cookies_for(&site, &jar);

    assert_eq!(seeded.len(), 1, "only the non-HttpOnly cookie may cross");
    assert_eq!(seeded[0].name, "theme");
    assert!(seeded.iter().all(|cookie| !cookie.http_only));
}

#[test]
fn same_etld_plus_one_is_same_site() {
    let site = site_key("https://foo.example.co.uk/");
    let neighbour = ipc_cookie("bar.example.co.uk", "pref", false, false);
    assert!(validate_cookie_from_renderer(&neighbour, &site).is_ok());
}

#[test]
fn different_registrable_domain_is_different_site() {
    let site = site_key("https://example.com/");
    let foreign = ipc_cookie("example.org", "pref", false, false);
    assert_eq!(
        validate_cookie_from_renderer(&foreign, &site),
        Err(CookiePolicyError::DomainMismatch)
    );
}

#[test]
fn renderer_forged_httponly_cookie_is_rejected() {
    let site = site_key("https://site-a.test/");
    let forged = ipc_cookie("site-a.test", "sid", true, false);
    assert_eq!(
        validate_cookie_from_renderer(&forged, &site),
        Err(CookiePolicyError::HttpOnlyForbidden)
    );
}

#[test]
fn renderer_forged_secure_cookie_is_rejected() {
    let site = site_key("https://site-a.test/");
    let forged = ipc_cookie("site-a.test", "sid", false, true);
    assert_eq!(
        validate_cookie_from_renderer(&forged, &site),
        Err(CookiePolicyError::SecureForbidden)
    );
}

#[test]
fn renderer_cannot_return_cookies_over_ipc() {
    let response = IpcRenderResponse {
        generation: 1,
        png_bytes: Vec::new(),
        content_height: 0,
        css_width: 0,
        title: None,
        document_url: "https://site-a.test/".to_owned(),
        scripts_executed: 0,
        script_timers_pending: 0,
        warnings: 0,
        script_errors: 0,
        links: Vec::new(),
        images: Vec::new(),
        forms: Vec::new(),
        find_matches: Vec::new(),
        find_rects: Vec::new(),
        document_html: String::new(),
        error: None,
    };
    let wire = serde_json::to_value(&response).expect("serialize response");
    assert!(
        wire.get("cookies").is_none(),
        "the renderer response must not carry a cookie channel at all"
    );
}

#[test]
fn legitimate_seeded_cookie_reaches_child_jar() {
    let mut jar = CookieJar::new();
    let url = Url::parse("https://site.test/login").expect("url");
    assert!(jar.store_set_cookie(&url, "prefs=dark; Path=/"));
    let source = megi_webview::render::RenderOptions {
        cookie_jar: Some(std::sync::Arc::new(std::sync::Mutex::new(jar))),
        origin: Some("https://site.test".to_owned()),
        ..megi_webview::render::RenderOptions::default()
    };

    let wire = IpcRenderOptions::from_render_options(&source);
    let restored = wire.to_render_options();
    let child = restored.cookie_jar.expect("child jar");
    let guard = child.lock().expect("lock child jar");
    assert_eq!(guard.len(), 1);
    assert_eq!(guard.iter().next().expect("cookie").name, "prefs");
}
