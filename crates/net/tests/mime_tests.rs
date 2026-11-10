//! MIME sniffing tests (WHATWG MIME Sniffing §5 subset) and content-type resolution.

use net::mime::{self, MimeHint};

#[test]
fn sniffs_html_documents() {
    assert_eq!(mime::sniff(b"<!DOCTYPE html><p>x"), MimeHint::Html);
    assert_eq!(mime::sniff(b"<html><body>x"), MimeHint::Html);
    assert_eq!(mime::sniff(b"  \n\t<head><title>x"), MimeHint::Html);
    // UTF-8 BOM is skipped before the HTML pattern.
    assert_eq!(mime::sniff(b"\xEF\xBB\xBF<!doctype html>"), MimeHint::Html);
    // The HTML pattern requires a tag-terminating byte, so this stays text.
    assert_eq!(mime::sniff(b"<htmlish>"), MimeHint::Text);
}

#[test]
fn sniffs_css() {
    assert_eq!(
        mime::sniff(b"@charset \"utf-8\";\nbody{color:red}"),
        MimeHint::Css
    );
    assert_eq!(mime::sniff(b"  @CHARSET \"UTF-8\";"), MimeHint::Css);
}

#[test]
fn sniffs_json_heuristic() {
    assert_eq!(mime::sniff(b"{\"a\":1}"), MimeHint::Json);
    assert_eq!(mime::sniff(b"  [1, 2, 3]"), MimeHint::Json);
    assert_eq!(mime::sniff(b"{}"), MimeHint::Json);
}

#[test]
fn sniffs_svg_and_xml() {
    assert_eq!(
        mime::sniff(b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        MimeHint::Svg
    );
    assert_eq!(
        mime::sniff(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"),
        MimeHint::Svg
    );
    assert_eq!(
        mime::sniff(b"<?xml version=\"1.0\"?><root><child/></root>"),
        MimeHint::Xml
    );
}

#[test]
fn sniffs_image_and_document_signatures() {
    assert_eq!(mime::sniff(b"\x89PNG\r\n\x1a\n\x00\x00"), MimeHint::Png);
    assert_eq!(mime::sniff(b"\xFF\xD8\xFF\xE0\x00\x10"), MimeHint::Jpeg);
    assert_eq!(mime::sniff(b"GIF87a...."), MimeHint::Gif);
    assert_eq!(mime::sniff(b"GIF89a...."), MimeHint::Gif);
    assert_eq!(mime::sniff(b"RIFF\x00\x00\x00\x00WEBPVP8 "), MimeHint::Webp);
    assert_eq!(mime::sniff(b"%PDF-1.7\n"), MimeHint::Pdf);
}

#[test]
fn falls_back_to_text_or_binary() {
    assert_eq!(mime::sniff(b"hello world\n"), MimeHint::Text);
    assert_eq!(mime::sniff("café — text".as_bytes()), MimeHint::Text);
    assert_eq!(mime::sniff(b"\x00\x01\x02"), MimeHint::Binary);
    assert_eq!(mime::sniff(&[0xFF, 0xFE, 0x00]), MimeHint::Binary);
    // Invalid UTF-8 without a known signature is binary.
    assert_eq!(mime::sniff(&[0x80, 0x81, 0x82]), MimeHint::Binary);
}

#[test]
fn mime_types_are_canonical() {
    assert_eq!(MimeHint::Html.mime_type(), "text/html");
    assert_eq!(MimeHint::Svg.mime_type(), "image/svg+xml");
    assert_eq!(MimeHint::Png.mime_type(), "image/png");
    assert_eq!(MimeHint::Binary.mime_type(), "application/octet-stream");
    assert_eq!(net::mime::mime_type(MimeHint::Json), "application/json");
}

#[test]
fn content_type_header_wins_over_sniffing() {
    let headers = vec![(
        "Content-Type".to_owned(),
        "text/html; charset=utf-8".to_owned(),
    )];
    assert_eq!(
        net::resolve_content_type(&headers, b"\x89PNG\r\n\x1a\n"),
        "text/html; charset=utf-8"
    );
}

#[test]
fn empty_or_missing_content_type_falls_back_to_sniffing() {
    let empty = vec![("content-type".to_owned(), "  ".to_owned())];
    assert_eq!(
        net::resolve_content_type(&empty, b"\x89PNG\r\n\x1a\n"),
        "image/png"
    );
    assert_eq!(net::resolve_content_type(&[], b"<html>"), "text/html");
    assert_eq!(net::resolve_content_type(&[], b"plain"), "text/plain");
}
