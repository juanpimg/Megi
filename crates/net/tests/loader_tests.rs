//! [`net::ResourceLoader`] tests: `file://` loads, MIME resolution and the in-memory cache.
//!
//! Offline by construction: `file://` cases use temporary files, the HTTP case uses a
//! one-shot `127.0.0.1:0` server. Temporary files are removed on drop.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use net::{FetchError, FetchOptions, LoadError, ResourceLoader, Url};

struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(stem: &str, contents: &[u8]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "megi-net-test-{}-{unique}-{stem}",
            std::process::id()
        ));
        std::fs::write(&path, contents).expect("write temporary test file");
        Self { path }
    }

    fn url(&self) -> Url {
        Url::parse(&format!("file://{}", self.path.display()))
            .unwrap_or_else(|error| panic!("file URL for {}: {error}", self.path.display()))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn loader_reads_file_url_and_sniffs_content_type() {
    let file = TempFile::new("page.html", b"<!DOCTYPE html><p>hello</p>");
    let mut loader = ResourceLoader::new();
    let resource = loader.load(&file.url()).expect("file loads");

    assert_eq!(resource.content_type, "text/html");
    assert_eq!(resource.bytes, b"<!DOCTYPE html><p>hello</p>");
    assert_eq!(resource.text(), Some("<!DOCTYPE html><p>hello</p>"));
    assert_eq!(loader.cache_misses(), 1);
    assert_eq!(loader.cache_hits(), 0);
}

#[test]
fn loader_percent_decodes_file_paths() {
    let file = TempFile::new("with space & accent café.html", b"<html>");
    let url = file.url();
    assert!(
        url.as_str().contains('%'),
        "URL should percent-encode the path: {url}"
    );
    let mut loader = ResourceLoader::new();
    let resource = loader.load(&url).expect("percent-encoded file path loads");
    assert_eq!(resource.bytes, b"<html>");
}

#[test]
fn loader_cache_serves_content_after_file_removal() {
    let file = TempFile::new("cached.css", b"body { color: red }");
    let mut loader = ResourceLoader::new();
    let url = file.url();

    let first = loader.load(&url).expect("first load succeeds");
    assert!(loader.is_cached(&url));
    std::fs::remove_file(&file.path).expect("remove file to prove the cache is used");

    let second = loader
        .load(&url)
        .expect("second load is served from the cache");
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(loader.cache_hits(), 1);
    assert_eq!(loader.cache_misses(), 1);
    assert_eq!(loader.cache_len(), 1);
}

#[test]
fn loader_without_cache_reports_missing_file() {
    let file = TempFile::new("uncached.txt", b"plain");
    let mut loader = ResourceLoader::without_cache();
    let url = file.url();
    loader.load(&url).expect("first load succeeds");
    std::fs::remove_file(&file.path).expect("remove file");

    let error = loader.load(&url).expect_err("second load must fail");
    assert!(
        matches!(
            error,
            LoadError::Fetch(FetchError::FileNotFound(ref path))
                if path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with("uncached.txt"))
        ),
        "unexpected error: {error}"
    );
    assert!(!loader.is_cache_enabled());
    assert_eq!(loader.cache_len(), 0);
}

#[test]
fn loader_rejects_remote_file_hosts() {
    let url = Url::parse("file://server/share/page.html").expect("file URL parses");
    let mut loader = ResourceLoader::new();
    let error = loader.load(&url).expect_err("remote file host must fail");
    assert!(
        matches!(
            error,
            LoadError::Fetch(FetchError::UnsupportedFileHost(ref host)) if host == "server"
        ),
        "unexpected error: {error}"
    );
}

#[cfg(not(feature = "tls"))]
#[test]
fn loader_reports_tls_unsupported_for_https() {
    let url = Url::parse("https://example.com/").expect("https URL parses");
    let mut loader = ResourceLoader::new();
    let error = loader.load(&url).expect_err("https is not supported yet");
    assert!(
        matches!(error, LoadError::Fetch(FetchError::TlsUnsupported)),
        "unexpected error: {error}"
    );
}

#[test]
fn loader_reports_url_errors_for_bad_strings() {
    let mut loader = ResourceLoader::new();
    let error = loader
        .load_str("not a url")
        .expect_err("invalid URL must fail");
    assert!(
        matches!(error, LoadError::Url(_)),
        "unexpected error: {error}"
    );
}

/// Spawns a one-shot HTTP server; returns the base URL and the thread's received requests.
fn spawn_server(response: Vec<u8>) -> (String, std::thread::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");
    let handle = std::thread::spawn(move || {
        let mut requests = Vec::new();
        let Ok((mut stream, _)) = listener.accept() else {
            return requests;
        };
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        requests.push(request);
        let _ = stream.write_all(&response);
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Write);
        requests
    });
    (format!("http://{address}"), handle)
}

fn short_options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

#[test]
fn loader_http_uses_content_type_header() {
    let response =
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 6\r\n\r\n<html>"
            .to_vec();
    let (base, server) = spawn_server(response);
    let url = Url::parse(&base)
        .expect("base URL")
        .join("/data")
        .expect("join");
    let mut loader = ResourceLoader::with_options(short_options());
    let resource = loader.load(&url).expect("HTTP load succeeds");
    assert_eq!(resource.content_type, "application/json");
    assert_eq!(resource.bytes, b"<html>");
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn loader_http_sniffs_when_content_type_is_missing() {
    let png = b"\x89PNG\r\n\x1a\n0123";
    let mut response = Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\n"[..]);
    response.extend_from_slice(png);
    let (base, server) = spawn_server(response);
    let url = Url::parse(&base).expect("base URL");
    let mut loader = ResourceLoader::with_options(short_options());
    let resource = loader.load(&url).expect("HTTP load succeeds");
    assert_eq!(resource.content_type, "image/png");
    assert_eq!(resource.bytes, png);
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn loader_http_cache_avoids_second_request() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi".to_vec();
    let (base, server) = spawn_server(response);
    let url = Url::parse(&base).expect("base URL");
    let mut loader = ResourceLoader::with_options(short_options());

    let first = loader.load(&url).expect("first load");
    let second = loader.load(&url).expect("second load from cache");
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(loader.cache_hits(), 1);
    assert_eq!(loader.cache_misses(), 1);
    // Only one connection was served: the second load never touched the network.
    assert_eq!(server.join().expect("server thread").len(), 1);
}

#[test]
fn loader_cache_can_be_cleared() {
    let file = TempFile::new("cleared.txt", b"text");
    let mut loader = ResourceLoader::new();
    let url = file.url();
    loader.load(&url).expect("load");
    assert_eq!(loader.cache_len(), 1);
    loader.clear_cache();
    assert_eq!(loader.cache_len(), 0);
    assert!(!loader.is_cached(&url));
    loader.load(&url).expect("reload after clear");
    assert_eq!(loader.cache_misses(), 2);
}

#[test]
fn loader_blocks_file_urls_when_local_access_is_disabled() {
    let file = TempFile::new("blocked.txt", b"secret");
    let url = file.url();
    let mut loader = ResourceLoader::new().with_allow_file(false);

    for error in [
        loader
            .load(&url)
            .expect_err("buffered file load must be blocked"),
        loader
            .load_streaming(&url, &mut |_| Ok(()))
            .expect_err("streaming file load must be blocked"),
    ] {
        assert!(
            matches!(error, LoadError::Fetch(FetchError::FileAccessBlocked)),
            "{error}"
        );
    }

    // Re-enabling access restores the normal behaviour.
    loader.set_allow_file(true);
    assert_eq!(loader.load(&url).expect("allowed").bytes, b"secret");
}
