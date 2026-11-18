//! TLS setup: rustls client configuration, ALPN negotiation and config caching.

use std::net::TcpStream;
#[cfg(feature = "tls")]
use std::sync::Arc;

use crate::error::FetchError;

use super::transport::Transport;
#[cfg(feature = "tls")]
use super::transport::map_io_error;
use super::*;

/// Establishes the TLS layer over an already-connected TCP stream.
///
/// Advertises ALPN `h2` (then `http/1.1`) when `advertise_h2` is set (plain `GET`
/// requests), drives the handshake to completion and reports whether the server
/// negotiated HTTP/2. The HTTP/1.1 path is unchanged when `h2` is not selected.
#[cfg(feature = "tls")]
pub(super) fn connect_tls(
    host: &str,
    mut tcp_stream: TcpStream,
    options: &FetchOptions,
    advertise_h2: bool,
) -> Result<(Transport, bool), FetchError> {
    let config = match &options.tls_config {
        Some(config) => (**config).clone(),
        None => (*default_tls_config()?).clone(),
    };
    let mut config = config;
    config.alpn_protocols = if advertise_h2 {
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
    } else {
        vec![b"http/1.1".to_vec()]
    };
    let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|error| FetchError::Tls(format!("invalid TLS server name `{host}`: {error}")))?;
    let mut connection = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(|error| FetchError::Tls(format!("cannot create TLS client: {error}")))?;
    // Complete the handshake eagerly so ALPN (`h2` vs `http/1.1`) is known before
    // choosing the request protocol; the socket read/write timeouts are already set.
    while connection.is_handshaking() {
        connection
            .complete_io(&mut tcp_stream)
            .map_err(map_io_error)?;
    }
    let negotiate_h2 = connection.alpn_protocol() == Some(b"h2".as_slice());
    Ok((
        Transport::Tls(Box::new(rustls::StreamOwned::new(connection, tcp_stream))),
        negotiate_h2,
    ))
}

/// Without the `tls` feature, `https:` URLs fail with a typed, actionable error.
#[cfg(not(feature = "tls"))]
pub(super) fn connect_tls(
    _host: &str,
    _tcp_stream: TcpStream,
    _options: &FetchOptions,
    _advertise_h2: bool,
) -> Result<(Transport, bool), FetchError> {
    Err(FetchError::TlsUnsupported)
}

/// Built-in `rustls` client configuration: webpki-roots (Mozilla) trust anchors and the
/// `ring` crypto provider with safe default protocol versions (TLS 1.2 + TLS 1.3).
#[cfg(feature = "tls")]
pub(super) fn default_tls_config() -> Result<Arc<rustls::ClientConfig>, FetchError> {
    // Cache only successful builds: a transient/environmental failure must not make
    // every later https fetch fail for the rest of the process (#1473).
    static CACHE: std::sync::OnceLock<Arc<rustls::ClientConfig>> = std::sync::OnceLock::new();
    if let Some(config) = CACHE.get() {
        return Ok(Arc::clone(config));
    }
    let config = build_default_tls_config().map_err(|error| {
        eprintln!("megi: cannot initialize default rustls configuration: {error}");
        FetchError::Tls(format!(
            "cannot initialize the default rustls client configuration: {error}"
        ))
    })?;
    let _ = CACHE.set(Arc::clone(&config));
    Ok(config)
}

#[cfg(feature = "tls")]
fn build_default_tls_config() -> Result<Arc<rustls::ClientConfig>, rustls::Error> {
    use rustls::RootCertStore;
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}
