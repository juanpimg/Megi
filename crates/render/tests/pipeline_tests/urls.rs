//! Non-HTTP URLs, charset handling and network gating.

use super::common::*;

#[test]
fn file_url_fixture_uses_base_href_and_ignores_print_media() {
    let url = format!("file://{}", fixture("basic.html").display());
    let output = render_url(&url, &options(300, 200)).expect("file render");
    assert_eq!(output.stats.external_stylesheets, 1);
    assert_eq!(output.stats.inline_stylesheets, 1);
    assert_eq!(output.document_url.scheme(), megi_net::Scheme::File);
    // theme.css (resolved through <base href="sub/">) paints green.
    assert!(
        pixel_is(&output.pixmap, 10, 10, 0, 128, 0),
        "expected green canvas from sub/theme.css, got {:?}",
        output.pixmap.pixel(10, 10)
    );
    // The print-only sheet would paint red; it must be ignored.
    assert!(
        !pixel_is(&output.pixmap, 10, 10, 255, 0, 0),
        "media=\"print\" stylesheet must be ignored on screen"
    );
}

#[test]
fn data_url_html_renders_inline_style() {
    let output = render_url(
        "data:text/html,<html><body style=\"background-color:blue\"><p>data</p></body></html>",
        &options(120, 80),
    )
    .expect("data render");
    assert_eq!(output.stats.status, None);
    assert!(pixel_is(&output.pixmap, 3, 3, 0, 0, 255));
}

#[test]
fn no_network_rejects_http_documents() {
    let options = RenderOptions {
        no_network: true,
        ..options(100, 100)
    };
    let error = render_url("http://127.0.0.1:1/page.html", &options).expect_err("no network");
    assert!(
        matches!(error, RenderError::NetworkDisabled { .. }),
        "{error:?}"
    );
}

#[test]
fn windows_1252_meta_charset_is_decoded() {
    // `%E9` is the windows-1252/latin-1 byte for 'é'; the meta tag declares the encoding.
    let url = "data:text/html,%3Cmeta%20charset=windows-1252%3E%3Cbody%3E%3Cp%3Ecaf%E9%3C/p%3E%3C/body%3E";
    let output = render_url(url, &options(200, 80)).expect("charset render");
    assert_eq!(output.stats.charset, "windows-1252");
    // The rendered glyphs paint non-white pixels (real text, no panic on the byte).
    let painted = output
        .pixmap
        .pixels
        .iter()
        .filter(|pixel| pixel.r != 255 || pixel.g != 255 || pixel.b != 255)
        .count();
    assert!(painted > 0, "text must be painted");
}

#[test]
fn unsupported_charset_is_a_typed_error() {
    // GBK is supported now (#1910); use a genuinely unsupported label.
    let url = "data:text/html,%3Cmeta%20charset=shift_jis%3E%3Cp%3Ex%3C/p%3E";
    let error = render_url(url, &options(100, 60)).expect_err("unsupported charset");
    let message = format!("{error:?}");
    assert!(
        matches!(error, RenderError::UnsupportedCharset(ref label) if label == "shift_jis"),
        "{message}"
    );
}
