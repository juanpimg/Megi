//! Socket transport: DNS/TCP connect, proxy route selection and the HTTP/1.1 byte stream.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

use crate::error::FetchError;
use crate::url::Scheme;

use super::chunked::ConnReader;
use super::response::parse_head;
use super::tls::connect_tls;
use super::*;

/// Resolves `host:port` and opens a TCP stream with the configured timeouts.
pub(super) fn connect_tcp(
    host: &str,
    port: u16,
    options: &FetchOptions,
) -> Result<TcpStream, FetchError> {
    let addresses: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|error| FetchError::Dns(format!("{host}: {error}")))?
        .collect();
    if addresses.is_empty() {
        return Err(FetchError::Dns(host.to_owned()));
    }
    let mut last_error: Option<io::Error> = None;
    let mut tcp_stream = None;
    for address in &addresses {
        match TcpStream::connect_timeout(address, options.connect_timeout) {
            Ok(connected) => {
                tcp_stream = Some(connected);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let tcp_stream = tcp_stream.ok_or_else(|| {
        FetchError::Connect(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::AddrNotAvailable, "no address to connect to")
        }))
    })?;
    tcp_stream
        .set_read_timeout(Some(options.read_timeout))
        .map_err(FetchError::Io)?;
    // #1635: honor dedicated write_timeout when set; fall back to read_timeout when None so
    // existing HTTP defaults are preserved.
    let write_timeout = options.write_timeout.unwrap_or(options.read_timeout);
    tcp_stream
        .set_write_timeout(Some(write_timeout))
        .map_err(FetchError::Io)?;
    Ok(tcp_stream)
}

/// Absolute-form request target used by `http:` requests through a proxy.
pub(super) fn absolute_request_target(url: &Url, host_header: &str) -> String {
    format!(
        "{}://{}{}",
        url.scheme().as_str(),
        host_header,
        url.request_target()
    )
}

pub(super) fn is_route_failover_error(error: &FetchError) -> bool {
    matches!(
        error,
        FetchError::Connect(_)
            | FetchError::Io(_)
            | FetchError::ProxyConnect(_)
            | FetchError::ProxyTunnel(_)
            | FetchError::Timeout
    )
}

pub(super) fn open_proxy_or_direct_transport(
    route: &crate::proxy::ProxyRoute,
    url: &Url,
    host: &str,
    port: u16,
    host_header: &str,
    allow_h2: bool,
    options: &FetchOptions,
) -> Result<(Transport, bool, Option<String>, Option<String>), FetchError> {
    match route {
        crate::proxy::ProxyRoute::Socks5(socks) => {
            let mut tunnel = connect_tcp(&socks.host, socks.port, options)?;
            crate::socks::connect(&mut tunnel, socks, url.host_for_dns(), port)?;
            match url.scheme() {
                Scheme::Http => Ok((Transport::Plain(tunnel), false, None, None)),
                Scheme::Https => {
                    let (transport, negotiated_h2) = connect_tls(host, tunnel, options, allow_h2)
                        .map_err(|error| match error {
                        FetchError::Io(reason) => FetchError::ProxyTunnel(reason.to_string()),
                        other => other,
                    })?;
                    Ok((transport, negotiated_h2, None, None))
                }
                Scheme::File | Scheme::Data => Err(FetchError::UnsupportedScheme(
                    url.scheme().as_str().to_owned(),
                )),
            }
        }
        crate::proxy::ProxyRoute::Socks4(socks4) => {
            let mut tunnel = connect_tcp(&socks4.host, socks4.port, options)?;
            crate::socks::connect_socks4(&mut tunnel, socks4, url.host_for_dns(), port)?;
            match url.scheme() {
                Scheme::Http => Ok((Transport::Plain(tunnel), false, None, None)),
                Scheme::Https => {
                    let (transport, negotiated_h2) = connect_tls(host, tunnel, options, allow_h2)
                        .map_err(|error| match error {
                        FetchError::Io(reason) => FetchError::ProxyTunnel(reason.to_string()),
                        other => other,
                    })?;
                    Ok((transport, negotiated_h2, None, None))
                }
                Scheme::File | Scheme::Data => Err(FetchError::UnsupportedScheme(
                    url.scheme().as_str().to_owned(),
                )),
            }
        }
        crate::proxy::ProxyRoute::Http(proxy_url) => {
            let proxy_host = proxy_url.host_for_dns();
            let proxy_port = proxy_url.port_or_default().ok_or_else(|| {
                FetchError::UnsupportedScheme(format!(
                    "proxy URL without a usable port: {proxy_url}"
                ))
            })?;
            let auth = crate::proxy::proxy_authorization(proxy_url);
            let tcp_stream = connect_tcp(proxy_host, proxy_port, options)?;
            if url.scheme() == Scheme::Https {
                if proxy_url.scheme() == Scheme::Https {
                    return Err(FetchError::ProxyConnect(
                        "an https proxy can only forward http targets (nested TLS is not supported)"
                            .to_owned(),
                    ));
                }
                let tunnel = establish_connect_tunnel(
                    tcp_stream,
                    host,
                    port,
                    auth.as_deref(),
                    proxy_url,
                    options,
                )?;
                let (transport, negotiated_h2) = connect_tls(host, tunnel, options, allow_h2)
                    .map_err(|error| match error {
                        FetchError::Io(reason) => FetchError::ProxyTunnel(reason.to_string()),
                        other => other,
                    })?;
                Ok((transport, negotiated_h2, None, None))
            } else {
                let absolute_form = Some(absolute_request_target(url, host_header));
                if proxy_url.scheme() == Scheme::Https {
                    let (transport, _) = connect_tls(proxy_host, tcp_stream, options, false)?;
                    Ok((transport, false, absolute_form, auth))
                } else {
                    Ok((Transport::Plain(tcp_stream), false, absolute_form, auth))
                }
            }
        }
        crate::proxy::ProxyRoute::Direct => {
            let tcp_stream = connect_tcp(host, port, options)?;
            match url.scheme() {
                Scheme::Http => Ok((Transport::Plain(tcp_stream), false, None, None)),
                Scheme::Https => {
                    let (transport, negotiated_h2) =
                        connect_tls(host, tcp_stream, options, allow_h2)?;
                    Ok((transport, negotiated_h2, None, None))
                }
                Scheme::File | Scheme::Data => Err(FetchError::UnsupportedScheme(
                    url.scheme().as_str().to_owned(),
                )),
            }
        }
    }
}

/// Opens a `CONNECT host:port` tunnel through `proxy`, returning the raw stream.
///
/// Sends `Proxy-Authorization` when the proxy URL carries Basic credentials and maps
/// `407` to [`FetchError::ProxyAuthRequired`], other non-200 statuses to
/// [`FetchError::ProxyConnect`] and early/extra tunnel bytes to
/// [`FetchError::ProxyTunnel`].
pub(super) fn establish_connect_tunnel(
    mut tcp_stream: TcpStream,
    host: &str,
    port: u16,
    authorization: Option<&str>,
    proxy: &Url,
    options: &FetchOptions,
) -> Result<TcpStream, FetchError> {
    let authority = format!("{host}:{port}");
    let mut request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: keep-alive\r\n"
    );
    if let Some(value) = authorization {
        request.push_str("Proxy-Authorization: ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    tcp_stream
        .write_all(request.as_bytes())
        .map_err(FetchError::Io)?;
    tcp_stream.flush().map_err(FetchError::Io)?;
    let mut reader = ConnReader::new(Transport::Plain(tcp_stream));
    let head = reader.read_head(options.max_header_bytes)?;
    let (status, _reason, _headers) = parse_head(&head)?;
    if status == 407 {
        return Err(crate::proxy::auth_required_error(proxy));
    }
    if status != 200 {
        return Err(FetchError::ProxyConnect(format!(
            "proxy CONNECT returned status {status}"
        )));
    }
    if reader.position < reader.buffer.len() {
        return Err(FetchError::ProxyTunnel(
            "unexpected bytes after the CONNECT response".to_owned(),
        ));
    }
    match reader.stream {
        Transport::Plain(stream) => Ok(stream),
        #[cfg(feature = "tls")]
        Transport::Tls(_) => Err(FetchError::ProxyTunnel(
            "TLS transport used for a plain CONNECT tunnel".to_owned(),
        )),
    }
}

/// Byte-stream transport for one HTTP request: plain TCP or a rustls session.
#[derive(Debug)]
pub(super) enum Transport {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Transport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            #[cfg(feature = "tls")]
            Self::Tls(stream) => stream.flush(),
        }
    }
}

pub(super) fn map_io_error(error: io::Error) -> FetchError {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => FetchError::Timeout,
        _ => {
            // `rustls::StreamOwned` surfaces handshake/record-layer failures as
            // `io::Error` wrapping `rustls::Error`: classify those as TLS errors so
            // callers can distinguish certificate problems from plain socket I/O.
            #[cfg(feature = "tls")]
            if let Some(source) = error.get_ref()
                && source.downcast_ref::<rustls::Error>().is_some()
            {
                return FetchError::Tls(error.to_string());
            }
            FetchError::Io(error)
        }
    }
}
