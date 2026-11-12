//! Minimal SOCKS5 client (RFC 1928) with username/password authentication (RFC 1929).
//!
//! Supports the `CONNECT` command only, no UDP/BIND and no proxy-side authentication
//! negotiation beyond "no auth" and username/password. The tunnel is then used exactly
//! like a direct TCP stream (TLS is layered on top for `https:` targets).

use std::io::{Read, Write};
use std::net::TcpStream;

use crate::FetchError;

/// A parsed `socks5://[user:pass@]host:port` proxy endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocksProxy {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl SocksProxy {
    /// Parses a SOCKS5 proxy URL (`socks5://`/`socks5h://`; `socks://` is accepted as an
    /// alias). Returns `None` for other schemes or malformed URLs.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let rest = value
            .strip_prefix("socks5://")
            .or_else(|| value.strip_prefix("socks5h://"))
            .or_else(|| value.strip_prefix("socks://"))?;
        let (authority, _path) = match rest.split_once('/') {
            Some((authority, _)) => (authority, ()),
            None => (rest, ()),
        };
        let (userinfo, host_port) = match authority.rsplit_once('@') {
            Some((userinfo, host_port)) => (Some(userinfo), host_port),
            None => (None, authority),
        };
        let (username, password) = match userinfo {
            Some(info) => {
                let (user, pass) = info.split_once(':').unwrap_or((info, ""));
                (Some(percent_decode(user)), Some(percent_decode(pass)))
            }
            None => (None, None),
        };
        let (host, port) = if let Some(rest) = host_port.strip_prefix('[') {
            let (host, tail) = rest.split_once(']')?;
            let port = tail.strip_prefix(':')?.parse().ok()?;
            (host.to_owned(), port)
        } else {
            let (host, port) = host_port.rsplit_once(':')?;
            if host.is_empty() {
                return None;
            }
            (host.to_owned(), port.parse().ok()?)
        };
        if host.is_empty() || port == 0 {
            return None;
        }
        Some(Self {
            host,
            port,
            username,
            password,
        })
    }

    /// Returns `true` when username/password authentication is configured.
    pub fn requires_auth(&self) -> bool {
        self.username.is_some() || self.password.is_some()
    }
}

/// Performs the SOCKS5 greeting, optional auth and CONNECT to `target_host:target_port`.
///
/// On success the stream is positioned after the CONNECT reply and can be used as a raw
/// byte tunnel. Errors are typed `FetchError::ProxyConnect` (malformed/unsupported proxy
/// reply) or `FetchError::Io` for socket failures.
pub fn connect(
    stream: &mut TcpStream,
    proxy: &SocksProxy,
    target_host: &str,
    target_port: u16,
) -> Result<(), FetchError> {
    let methods: &[u8] = if proxy.requires_auth() {
        &[0x00, 0x02]
    } else {
        &[0x00]
    };
    let mut greeting = Vec::with_capacity(2 + methods.len());
    greeting.push(0x05);
    greeting.push(methods.len() as u8);
    greeting.extend_from_slice(methods);
    stream
        .write_all(&greeting)
        .map_err(|error| FetchError::ProxyConnect(format!("SOCKS5 greeting write: {error}")))?;
    let mut choice = [0u8; 2];
    read_exact(stream, &mut choice, "SOCKS5 method reply")?;
    if choice[0] != 0x05 {
        return Err(FetchError::ProxyConnect(
            "SOCKS5 proxy replied with a non-5 version".to_owned(),
        ));
    }
    match choice[1] {
        0x00 => {}
        0x02 => {
            let (Some(username), Some(password)) =
                (proxy.username.as_deref(), proxy.password.as_deref())
            else {
                return Err(FetchError::ProxyConnect(
                    "SOCKS5 proxy requested username/password but none is configured".to_owned(),
                ));
            };
            let username = username.as_bytes();
            let password = password.as_bytes();
            if username.len() > 255 || password.len() > 255 {
                return Err(FetchError::ProxyConnect(
                    "SOCKS5 username/password longer than 255 bytes".to_owned(),
                ));
            }
            let mut auth = Vec::with_capacity(3 + username.len() + password.len());
            auth.push(0x01);
            auth.push(username.len() as u8);
            auth.extend_from_slice(username);
            auth.push(password.len() as u8);
            auth.extend_from_slice(password);
            stream
                .write_all(&auth)
                .map_err(|error| FetchError::ProxyConnect(format!("SOCKS5 auth write: {error}")))?;
            let mut reply = [0u8; 2];
            read_exact(stream, &mut reply, "SOCKS5 auth reply")?;
            if reply[0] != 0x01 || reply[1] != 0x00 {
                return Err(FetchError::ProxyConnect(
                    "SOCKS5 username/password authentication failed".to_owned(),
                ));
            }
        }
        method => {
            return Err(FetchError::ProxyConnect(format!(
                "SOCKS5 proxy selected unsupported auth method {method}"
            )));
        }
    }

    let mut request = Vec::with_capacity(7 + target_host.len());
    request.extend_from_slice(&[0x05, 0x01, 0x00]);
    // Strip brackets from IPv6 literals (e.g. "[::1]" -> "::1").
    let host = target_host.trim_start_matches('[').trim_end_matches(']');
    // Try to parse as an IP literal first.
    if let Ok(ipv4) = host.parse::<std::net::Ipv4Addr>() {
        request.push(0x01);
        request.extend_from_slice(&ipv4.octets());
    } else if let Ok(ipv6) = host.parse::<std::net::Ipv6Addr>() {
        request.push(0x04);
        request.extend_from_slice(&ipv6.octets());
    } else {
        // Domain name: ATYP=3, proxy resolves (socks5h semantics).
        if host.len() > 255 {
            return Err(FetchError::ProxyConnect(
                "SOCKS5 target host longer than 255 bytes".to_owned(),
            ));
        }
        request.push(0x03);
        request.push(host.len() as u8);
        request.extend_from_slice(host.as_bytes());
    }
    request.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&request)
        .map_err(|error| FetchError::ProxyConnect(format!("SOCKS5 CONNECT write: {error}")))?;
    let mut reply = [0u8; 4];
    read_exact(stream, &mut reply, "SOCKS5 CONNECT reply")?;
    if reply[0] != 0x05 {
        return Err(FetchError::ProxyConnect(
            "SOCKS5 proxy replied with a non-5 version".to_owned(),
        ));
    }
    if reply[1] != 0x00 {
        return Err(FetchError::ProxyConnect(format!(
            "SOCKS5 CONNECT failed with reply code {}",
            reply[1]
        )));
    }
    let address_len = match reply[3] {
        0x01 => 4,
        0x03 => {
            let mut len = [0u8; 1];
            read_exact(stream, &mut len, "SOCKS5 address length")?;
            usize::from(len[0])
        }
        0x04 => 16,
        atyp => {
            return Err(FetchError::ProxyConnect(format!(
                "SOCKS5 CONNECT reply has unsupported address type {atyp}"
            )));
        }
    };
    let mut discard = vec![0u8; address_len + 2];
    read_exact(stream, &mut discard, "SOCKS5 bound address")?;
    Ok(())
}

fn read_exact(stream: &mut TcpStream, buffer: &mut [u8], context: &str) -> Result<(), FetchError> {
    stream
        .read_exact(buffer)
        .map_err(|error| FetchError::ProxyConnect(format!("{context}: {error}")))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
