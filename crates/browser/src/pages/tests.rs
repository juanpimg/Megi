use super::*;

fn entry(name: &str, is_dir: bool, size: Option<u64>) -> ListingEntry {
    ListingEntry {
        name: name.to_owned(),
        is_dir,
        size,
    }
}

#[test]
fn escapes_html_and_percent_encodes_links() {
    assert_eq!(escape_html("<a&\"'>"), "&lt;a&amp;&quot;&#39;&gt;");
    assert_eq!(percent_encode_segment("a b/c.html"), "a%20b%2Fc.html");
    assert_eq!(
        percent_encode_segment("plain-name_1.txt"),
        "plain-name_1.txt"
    );
}

#[test]
fn human_sizes_are_deterministic() {
    assert_eq!(human_size(0), "0 B");
    assert_eq!(human_size(1023), "1023 B");
    assert_eq!(human_size(1024), "1.0 KiB");
    assert_eq!(human_size(1536), "1.5 KiB");
    assert_eq!(human_size(3 * 1024 * 1024), "3.0 MiB");
}

#[test]
fn listing_sorts_directories_first_and_escapes_names() {
    let entries = vec![
        entry("zeta.txt", false, Some(10)),
        entry("alpha", true, None),
        entry("<evil>&.html", false, Some(2048)),
    ];
    let html = listing_html("file:///tmp/dir/", Path::new("/tmp/dir"), &entries);
    let alpha = html.find("alpha").expect("alpha row");
    let evil = html.find("&lt;evil&gt;&amp;.html").expect("escaped name");
    let zeta = html.find("zeta.txt").expect("zeta row");
    assert!(alpha < evil && evil < zeta, "dirs first, then names");
    assert!(html.contains("href=\"file:///tmp/dir/%3Cevil%3E%26.html\""));
    assert!(html.contains("2.0 KiB"));
    assert!(
        html.contains("Index of /tmp/dir") || html.contains("Índice de /tmp/dir"),
        "localized index title: {html}"
    );
}

#[test]
fn listings_are_capped() {
    let entries: Vec<ListingEntry> = (0..MAX_LISTING_ENTRIES + 50)
        .map(|index| ListingEntry {
            name: format!("file-{index}.txt"),
            is_dir: false,
            size: Some(1),
        })
        .collect();
    let html = listing_html("file:///tmp/dir/", Path::new("/tmp/dir"), &entries);
    assert!(
        html.contains("listing truncated") || html.contains("listado truncado"),
        "truncation marker present"
    );
    assert!(
        html.len() <= MAX_LISTING_BYTES + 4096,
        "listing stays near the byte cap: {}",
        html.len()
    );
    let rows = html.matches("<tr><td><a href=").count();
    assert!(
        rows <= MAX_LISTING_ENTRIES + 1,
        "at most one row per kept entry plus the parent row, got {rows}"
    );
    assert_eq!(
        truncate_name("a".repeat(1000)).chars().count(),
        MAX_ENTRY_NAME_CHARS
    );
}

#[test]
fn breadcrumb_links_are_absolute_file_urls() {
    let html = listing_html("file:///tmp/a/b/", Path::new("/tmp/a/b"), &[]);
    assert!(html.contains("href=\"file:///\">/</a>"));
    assert!(html.contains("href=\"file:///tmp/a/\">a</a>"));
    assert!(html.contains("href=\"file:///tmp/a/b/\">b</a>"));
}

#[test]
fn error_page_contains_url_code_message_and_retry() {
    let html = error_html(
        "https://example.test/<x>?a=1&b=2",
        "DNS",
        "no se pudo resolver <host>",
    );
    assert!(html.contains("https://example.test/&lt;x&gt;?a=1&amp;b=2"));
    assert!(html.contains("<code>DNS</code>"));
    assert!(html.contains("no se pudo resolver &lt;host&gt;"));
    assert!(html.contains(">Reintentar</a>"));
    assert!(html.contains("Pulsa <code>R</code>"));
}

#[test]
fn data_url_is_percent_encoded() {
    let url = data_url("<h1>a b</h1>");
    assert!(url.starts_with("data:text/html;charset=utf-8,"));
    assert!(url.contains("%3Ch1%3E"));
    assert!(!url.contains(' '));
}

#[test]
fn prepare_input_keeps_regular_files_and_http_urls() {
    assert_eq!(
        prepare_input("https://example.test/"),
        Ok("https://example.test/".to_owned())
    );
    let temp = std::env::temp_dir().join("megi-pages-file.html");
    std::fs::write(&temp, "<p>x</p>").expect("write");
    let url = file_url(&temp);
    assert_eq!(prepare_input(&url), Ok(url));
    let _ = std::fs::remove_file(temp);
}
