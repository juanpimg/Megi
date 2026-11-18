//! #1534: minimal ws:// handshake and masked text frames against a loopback server.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use megi_net::websocket::accept_value;
use megi_net::{WebSocket, WebSocketError};

fn read_head(stream: &mut std::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    while bytes.len() < 16 * 1024 {
        match stream.read(&mut byte) {
            Ok(1) => {
                bytes.push(byte[0]);
                if bytes.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            _ => break,
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn read_frame(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).expect("frame head");
    assert_ne!(head[1] & 0x80, 0, "client frames must be masked");
    let length = (head[1] & 0x7F) as usize;
    let mut mask = [0u8; 4];
    stream.read_exact(&mut mask).expect("mask");
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).expect("payload");
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % 4];
    }
    payload
}

fn serve() -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let request = read_head(&mut stream);
        assert!(request.starts_with("GET /chat HTTP/1.1"), "{request}");
        assert!(request.contains("Upgrade: websocket"), "{request}");
        assert!(request.contains("Sec-WebSocket-Version: 13"), "{request}");
        let key = request
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("Sec-WebSocket-Key"))
                    .map(|(_, value)| value.trim().to_owned())
            })
            .expect("client key");
        let accept = accept_value(&key);
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        );
        stream
            .write_all(response.as_bytes())
            .expect("handshake response");
        let payload = read_frame(&mut stream);
        assert_eq!(payload, b"ping");
        // Unmasked server text frame "pong".
        stream
            .write_all(&[0x81, 0x04, b'p', b'o', b'n', b'g'])
            .expect("pong frame");
        // Followed by a close frame in the same TCP segment (tests buffered reads).
        stream.write_all(&[0x88, 0x00]).expect("close frame");
        let _ = stream.flush();
    });
    (port, handle)
}

#[test]
fn ws_handshake_and_text_roundtrip() {
    let (port, server) = serve();
    let mut socket = WebSocket::connect(&format!("ws://127.0.0.1:{port}/chat")).expect("connect");
    socket.send_text("ping").expect("send");
    assert_eq!(socket.recv_text().expect("pong"), "pong");
    assert!(matches!(socket.recv_text(), Err(WebSocketError::Closed)));
    socket.close().expect("close");
    server.join().expect("server thread");
}

#[test]
fn ws_scheme_is_case_insensitive() {
    // #1644: the gate lowercases but parse_ws_url must also handle uppercase prefixes.
    let upper = megi_net::websocket::parse_ws_url("WS://example.test/chat").expect("uppercase ws");
    let lower = megi_net::websocket::parse_ws_url("ws://example.test/chat").expect("lowercase ws");
    assert_eq!(upper.as_str(), lower.as_str());
    let upper_secure =
        megi_net::websocket::parse_ws_url("WSS://example.test/chat").expect("uppercase wss");
    let lower_secure =
        megi_net::websocket::parse_ws_url("wss://example.test/chat").expect("lowercase wss");
    assert_eq!(upper_secure.as_str(), lower_secure.as_str());
}

#[test]
fn ws_url_with_surrounding_whitespace_is_rejected() {
    // #1641: silent trimming hides caller mistakes; reject instead.
    assert!(megi_net::websocket::parse_ws_url("  ws://example.test/x  ").is_err());
    assert!(megi_net::websocket::parse_ws_url("\tws://example.test/x").is_err());
    assert!(megi_net::websocket::parse_ws_url("ws://example.test/x\n").is_err());
}

#[test]
fn non_ws_scheme_is_rejected() {
    assert!(matches!(
        WebSocket::connect("https://example.test/"),
        Err(WebSocketError::UnsupportedScheme)
    ));
}

#[cfg(feature = "tls")]
mod secure {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread::JoinHandle;

    use megi_net::FetchOptions;
    use megi_net::websocket::{self, WebSocketClient};
    use rcgen::generate_simple_self_signed;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use rustls::{ClientConfig, RootCertStore, ServerConfig};

    fn provider() -> Arc<rustls::crypto::CryptoProvider> {
        Arc::new(rustls::crypto::ring::default_provider())
    }

    fn cert() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
        let generated = generate_simple_self_signed(vec!["localhost".to_owned()]).expect("cert");
        (
            generated.cert.der().clone(),
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                generated.signing_key.serialize_der(),
            )),
        )
    }

    fn client_config(cert: CertificateDer<'static>) -> Arc<ClientConfig> {
        let mut roots = RootCertStore::empty();
        roots.add(cert).expect("root");
        Arc::new(
            ClientConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("versions")
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    }

    /// TLS loopback server that completes a WS handshake and echoes one text frame.
    fn spawn_wss(
        cert: CertificateDer<'static>,
        key: PrivateKeyDer<'static>,
    ) -> (u16, JoinHandle<()>) {
        let config = Arc::new(
            ServerConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("versions")
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .expect("cert/key"),
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            let Ok((tcp, _)) = listener.accept() else {
                return;
            };
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::clone(&config)).expect("server conn"),
                tcp,
            );
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if tls.read(&mut byte).unwrap_or(0) == 0 {
                    return;
                }
                head.push(byte[0]);
            }
            let key = String::from_utf8_lossy(&head)
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(n, v)| {
                        if n.eq_ignore_ascii_case("sec-websocket-key") {
                            Some(v.trim().to_owned())
                        } else {
                            None
                        }
                    })
                })
                .expect("key");
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                websocket::accept_value(&key)
            );
            let _ = tls.write_all(response.as_bytes());
            let _ = tls.flush();
            let mut frame = [0u8; 2];
            if tls.read_exact(&mut frame).is_err() {
                return;
            }
            let len = (frame[1] & 0x7F) as usize;
            let mut rest = vec![0u8; 4 + len];
            if tls.read_exact(&mut rest).is_err() {
                return;
            }
            let mut payload = rest[4..].to_vec();
            websocket::mask(&mut payload, [rest[0], rest[1], rest[2], rest[3]]);
            let mut echo = vec![0x81, len as u8];
            echo.extend_from_slice(&payload);
            let _ = tls.write_all(&echo);
            let _ = tls.flush();
        });
        (port, handle)
    }

    #[test]
    fn wss_handshake_and_text_roundtrip() {
        let (certificate, key) = cert();
        let tls_config = client_config(certificate.clone());
        let (port, server) = spawn_wss(certificate, key);

        let url =
            websocket::parse_ws_url(&format!("wss://localhost:{port}/secure")).expect("wss url");
        assert_eq!(
            url.scheme().as_str(),
            "https",
            "wss maps to https internally"
        );
        let options = FetchOptions {
            tls_config: Some(tls_config),
            connect_timeout: std::time::Duration::from_secs(2),
            read_timeout: std::time::Duration::from_secs(2),
            ..FetchOptions::default()
        };
        let mut client = WebSocketClient::connect(&url, &options).expect("wss connect");
        client.send_text("secure hello").expect("send");
        let frame = client.recv().expect("recv");
        assert_eq!(frame.opcode, websocket::Opcode::Text);
        assert_eq!(frame.payload, b"secure hello");
        server.join().expect("server");
    }

    #[test]
    fn wss_rejects_untrusted_certificate() {
        let (certificate, key) = cert();
        let (port, server) = spawn_wss(certificate, key);
        let url = websocket::parse_ws_url(&format!("wss://localhost:{port}/")).expect("wss url");
        // No tls_config => webpki-roots cannot validate the self-signed test cert.
        let result = WebSocketClient::connect(&url, &FetchOptions::default());
        assert!(result.is_err(), "untrusted wss certificate must fail");
        let _ = server.join();
    }
}

#[test]
fn fragmented_text_message_is_reassembled() {
    // #1632: Text(FIN=0) + Continuation(FIN=1) must be delivered as one Text("ab").
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    fn spawn() -> (u16, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    return;
                }
                head.push(byte[0]);
            }
            let request = String::from_utf8_lossy(&head);
            let key = request
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(n, v)| {
                        if n.eq_ignore_ascii_case("sec-websocket-key") {
                            Some(v.trim().to_owned())
                        } else {
                            None
                        }
                    })
                })
                .expect("key");
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                megi_net::websocket::accept_value(&key)
            );
            let _ = stream.write_all(response.as_bytes());
            // Fragmented message: "a" (FIN=0), "b" (FIN=1).
            let _ = stream.write_all(&[0x01, 0x01, b'a', 0x80, 0x01, b'b']);
            let _ = stream.flush();
        });
        (port, handle)
    }

    let (port, server) = spawn();
    let url = megi_net::websocket::parse_ws_url(&format!("ws://127.0.0.1:{port}/")).expect("url");
    let mut client =
        megi_net::websocket::WebSocketClient::connect(&url, &megi_net::FetchOptions::default())
            .expect("connect");
    let frame = client.recv().expect("reassembled frame");
    assert_eq!(frame.opcode, megi_net::websocket::Opcode::Text);
    assert!(frame.fin);
    assert_eq!(
        frame.payload, b"ab",
        "fragmented message must be reassembled"
    );
    server.join().expect("server");
}

#[test]
fn raw_recv_replies_to_ping_and_recv_bytes_returns_binary() {
    // #1638/#1639: the raw API must auto-pong, and the wrapper must expose binary messages.
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    fn spawn() -> (u16, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    return;
                }
                head.push(byte[0]);
            }
            let key = String::from_utf8_lossy(&head)
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(n, v)| {
                        if n.eq_ignore_ascii_case("sec-websocket-key") {
                            Some(v.trim().to_owned())
                        } else {
                            None
                        }
                    })
                })
                .expect("key");
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                megi_net::websocket::accept_value(&key)
            );
            let _ = stream.write_all(response.as_bytes());
            // Ping, then a binary message with bytes 0xFF 0x00.
            let _ = stream.write_all(&[0x89, 0x00]);
            let _ = stream.write_all(&[0x82, 0x02, 0xFF, 0x00]);
            let _ = stream.flush();
            // Expect an auto-pong (unmasked, opcode 0xA) from the client. Keep the socket open
            // briefly afterwards so the client can still read the buffered binary frame (an
            // immediate drop can send RST and discard it).
            let mut reply = [0u8; 2];
            let pong_ok = stream.read_exact(&mut reply).is_ok() && reply[0] == 0x8A;
            std::thread::sleep(std::time::Duration::from_millis(50));
            assert!(pong_ok, "client must auto-pong a ping");
        });
        (port, handle)
    }

    let (port, server) = spawn();
    let url = megi_net::websocket::parse_ws_url(&format!("ws://127.0.0.1:{port}/")).expect("url");
    let options = megi_net::FetchOptions {
        read_timeout: std::time::Duration::from_secs(2),
        ..megi_net::FetchOptions::default()
    };
    let mut client =
        megi_net::websocket::WebSocketClient::connect(&url, &options).expect("connect");
    let ping = client.recv().expect("ping");
    assert_eq!(ping.opcode, megi_net::websocket::Opcode::Ping);
    let binary = client.recv().expect("binary");
    assert_eq!(binary.opcode, megi_net::websocket::Opcode::Binary);
    assert_eq!(binary.payload, vec![0xFF, 0x00]);
    server.join().expect("server");
}

#[test]
fn websocket_wrapper_recv_bytes_returns_binary() {
    // #1639: the string-oriented wrapper must expose a symmetric binary receive API.
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                return;
            }
            head.push(byte[0]);
        }
        let key = String::from_utf8_lossy(&head)
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(n, v)| {
                    if n.eq_ignore_ascii_case("sec-websocket-key") {
                        Some(v.trim().to_owned())
                    } else {
                        None
                    }
                })
            })
            .expect("key");
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            megi_net::websocket::accept_value(&key)
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(&[0x82, 0x02, 0xAB, 0xCD]);
        let _ = stream.flush();
        std::thread::sleep(std::time::Duration::from_millis(30));
    });
    let mut socket =
        megi_net::WebSocket::connect(&format!("ws://127.0.0.1:{port}/")).expect("connect");
    assert_eq!(socket.recv_bytes().expect("recv_bytes"), vec![0xAB, 0xCD]);
    server.join().expect("server");
}
