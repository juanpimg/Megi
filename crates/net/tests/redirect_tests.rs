//! Offline redirect policy integration tests (#1536):
//! - `RedirectMode::Manual` returns 3xx responses without following `Location`.
//! - `RedirectMode::Error` fails with `FetchError::RedirectDisallowed`.
//! - Per-hop CORS validation rejects unchecked intermediate cross-origin redirects.
//! - HTTPS -> HTTP downgrade redirects are blocked unless explicitly opted in or HSTS-upgraded.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use megi_net::{
    FetchError, FetchOptions, RedirectMode, Url, fetch, fetch_streaming, fetch_with_method,
};

fn read_head(stream: &mut impl Read) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    request
}

fn spawn_sequence(responses: Vec<Vec<u8>>) -> (u16, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().expect("local addr").port();
    let handle = std::thread::spawn(move || {
        let mut received = Vec::with_capacity(responses.len());
        for body in responses {
            let (mut tcp, _) = listener.accept().expect("accept");
            let req = read_head(&mut tcp);
            received.push(String::from_utf8_lossy(&req).into_owned());
            let _ = tcp.write_all(&body);
            let _ = tcp.flush();
            let _ = tcp.shutdown(Shutdown::Write);
        }
        received
    });
    (port, handle)
}

fn options() -> FetchOptions {
    FetchOptions {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        ..FetchOptions::default()
    }
}

#[test]
fn head_303_keeps_method() {
    // WHATWG Fetch §4.5: a 303 switches to GET only for non-GET/HEAD methods.
    let responses = vec![
        b"HTTP/1.1 303 See Other\r\nLocation: /next\r\nContent-Length: 0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
    ];
    let (port, server) = spawn_sequence(responses);
    let url = Url::parse(&format!("http://127.0.0.1:{port}/start")).expect("url");

    let response = fetch_with_method(&url, &options(), "HEAD", &[], None).expect("HEAD 303");
    assert_eq!(response.status, 200);
    let received = server.join().expect("server");
    assert_eq!(received.len(), 2);
    assert!(
        received[1].starts_with("HEAD /next HTTP/1.1"),
        "second hop must keep HEAD, got {:?}",
        received[1]
    );
}

#[test]
fn redirect_mode_manual_returns_3xx_without_following() {
    let body = b"HTTP/1.1 302 Found\r\nLocation: /next\r\nContent-Length: 4\r\n\r\nhop1".to_vec();
    let (port_a, server_a) = spawn_sequence(vec![body.clone()]);
    let url_a = Url::parse(&format!("http://127.0.0.1:{port_a}/start")).expect("url");
    let opts = FetchOptions {
        redirect_mode: RedirectMode::Manual,
        ..options()
    };

    let response = fetch(&url_a, &opts).expect("manual mode returns 3xx");
    assert_eq!(response.status, 302);
    assert_eq!(response.final_url.as_str(), url_a.as_str());
    assert_eq!(response.body, b"hop1");
    assert_eq!(response.header("location"), Some("/next"));
    assert_eq!(server_a.join().expect("server a").len(), 1);

    let (port_b, server_b) = spawn_sequence(vec![body]);
    let url_b = Url::parse(&format!("http://127.0.0.1:{port_b}/start")).expect("url");
    let mut streamed = Vec::new();
    let response = fetch_streaming(&url_b, &opts, &mut |chunk| {
        streamed.extend_from_slice(chunk);
        Ok(())
    })
    .expect("streaming manual mode returns 3xx");
    assert_eq!(response.status, 302);
    assert_eq!(response.header("location"), Some("/next"));
    assert!(
        streamed.is_empty(),
        "3xx body is suppressed while streaming"
    );
    assert_eq!(server_b.join().expect("server b").len(), 1);
}

#[test]
fn redirect_mode_error_rejects_redirect_across_fetch_variants() {
    let body =
        b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /next\r\nContent-Length: 0\r\n\r\n".to_vec();
    let (port_a, server_a) = spawn_sequence(vec![body.clone()]);
    let url_a = Url::parse(&format!("http://127.0.0.1:{port_a}/start")).expect("url");
    let opts = FetchOptions {
        redirect_mode: RedirectMode::Error,
        ..options()
    };

    let err = fetch(&url_a, &opts).expect_err("error mode rejects 3xx");
    assert!(
        matches!(err, FetchError::RedirectDisallowed(307)),
        "expected RedirectDisallowed(307), got {err}"
    );
    assert_eq!(server_a.join().expect("server a").len(), 1);

    let (port_b, server_b) = spawn_sequence(vec![body.clone()]);
    let url_b = Url::parse(&format!("http://127.0.0.1:{port_b}/start")).expect("url");
    let err = fetch_streaming(&url_b, &opts, &mut |_| Ok(())).expect_err("streaming rejects 3xx");
    assert!(
        matches!(err, FetchError::RedirectDisallowed(307)),
        "expected RedirectDisallowed(307), got {err}"
    );
    assert_eq!(server_b.join().expect("server b").len(), 1);

    let (port_c, server_c) = spawn_sequence(vec![body]);
    let url_c = Url::parse(&format!("http://127.0.0.1:{port_c}/start")).expect("url");
    let mut loader = megi_net::ResourceLoader::with_options(opts);
    let err = loader.load(&url_c).expect_err("loader rejects 3xx");
    assert!(
        matches!(
            err,
            megi_net::LoadError::Fetch(FetchError::RedirectDisallowed(307))
        ),
        "expected RedirectDisallowed(307), got {err}"
    );
    assert_eq!(server_c.join().expect("server c").len(), 1);
}

#[test]
fn per_hop_cors_rejects_unchecked_intermediate_redirect() {
    // Final hop permits CORS, but intermediate cross-origin 302 lacks ACAO: must fail!
    let (final_port, final_server) = spawn_sequence(Vec::new());
    let hop1 = format!(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{final_port}/final\r\nContent-Length: 0\r\n\r\n"
    )
    .into_bytes();
    let (hop_port, hop_server) = spawn_sequence(vec![hop1.clone(), hop1]);
    let origin = Some("https://app.example".to_owned());
    let opts = FetchOptions {
        origin,
        ..options()
    };

    let url = Url::parse(&format!("http://127.0.0.1:{hop_port}/hop")).expect("url");
    let err = fetch(&url, &opts).expect_err("hop1 lacks ACAO");
    assert!(
        matches!(err, FetchError::Cors(_)),
        "expected FetchError::Cors, got {err}"
    );

    let err = fetch_streaming(&url, &opts, &mut |_| Ok(())).expect_err("streaming hop1 lacks ACAO");
    assert!(
        matches!(err, FetchError::Cors(_)),
        "expected FetchError::Cors, got {err}"
    );

    assert_eq!(hop_server.join().expect("hop server").len(), 2);
    assert!(final_server.join().expect("final server").is_empty());
}

#[cfg(feature = "tls")]
mod tls_redirect_tests {
    use super::*;
    use rcgen::generate_simple_self_signed;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use rustls::{ClientConfig, RootCertStore, ServerConfig};

    fn provider() -> Arc<rustls::crypto::CryptoProvider> {
        Arc::new(rustls::crypto::ring::default_provider())
    }

    fn cert_and_client() -> (
        CertificateDer<'static>,
        PrivateKeyDer<'static>,
        Arc<ClientConfig>,
    ) {
        let generated = generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("generate test certificate");
        let cert = generated.cert.der().clone();
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            generated.signing_key.serialize_der(),
        ));
        let mut roots = RootCertStore::empty();
        roots.add(cert.clone()).expect("add cert");
        let client = Arc::new(
            ClientConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("TLS versions")
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        (cert, key, client)
    }

    fn spawn_tls_sequence(
        cert: CertificateDer<'static>,
        key: PrivateKeyDer<'static>,
        responses: Vec<Vec<u8>>,
    ) -> (u16, JoinHandle<()>) {
        let server_config = Arc::new(
            ServerConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("TLS versions")
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .expect("server config"),
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            for response in responses {
                let Ok((tcp, _)) = listener.accept() else {
                    return;
                };
                let Ok(conn) = rustls::ServerConnection::new(Arc::clone(&server_config)) else {
                    return;
                };
                let mut stream = rustls::StreamOwned::new(conn, tcp);
                let _ = read_head(&mut stream);
                let _ = stream.write_all(&response);
                let _ = stream.flush();
                let _ = stream.sock.shutdown(Shutdown::Write);
            }
        });
        (port, handle)
    }

    #[test]
    fn https_to_http_downgrade_blocked_by_default_and_allowed_when_opted_in() {
        megi_net::hsts::clear();
        let (cert, key, client) = cert_and_client();
        let (http_port, http_server) = spawn_sequence(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\ndowngraded".to_vec(),
        ]);
        let redir = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{http_port}/plain\r\nContent-Length: 0\r\n\r\n"
        )
        .into_bytes();
        let (tls_port, tls_server) = spawn_tls_sequence(cert, key, vec![redir.clone(), redir]);
        let url = Url::parse(&format!("https://localhost:{tls_port}/start")).expect("https url");

        let blocked_opts = FetchOptions {
            tls_config: Some(Arc::clone(&client)),
            ..options()
        };
        let err = fetch(&url, &blocked_opts).expect_err("downgrade must be blocked by default");
        assert!(
            matches!(err, FetchError::HttpsDowngradeBlocked(_)),
            "expected HttpsDowngradeBlocked, got {err}"
        );

        let allowed_opts = FetchOptions {
            tls_config: Some(client),
            allow_https_downgrade: true,
            ..options()
        };
        let response = fetch(&url, &allowed_opts).expect("explicit opt-in follows downgrade");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"downgraded");

        tls_server.join().expect("tls server");
        assert_eq!(http_server.join().expect("http server").len(), 1);
    }

    #[test]
    fn https_redirect_to_http_upgraded_by_hsts_succeeds() {
        megi_net::hsts::clear();
        let (cert, key, client) = cert_and_client();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind TLS");
        let port = listener.local_addr().expect("addr").port();
        let server_config = Arc::new(
            ServerConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .expect("TLS versions")
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .expect("server config"),
        );
        let handle = std::thread::spawn(move || {
            let responses = [
                format!(
                    "HTTP/1.1 302 Found\r\nStrict-Transport-Security: max-age=3600\r\nLocation: http://localhost:{port}/final\r\nContent-Length: 0\r\n\r\n"
                )
                .into_bytes(),
                b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nsecure".to_vec(),
            ];
            for response in responses {
                let (tcp, _) = listener.accept().expect("accept");
                let conn = rustls::ServerConnection::new(Arc::clone(&server_config)).expect("conn");
                let mut stream = rustls::StreamOwned::new(conn, tcp);
                let _ = read_head(&mut stream);
                let _ = stream.write_all(&response);
                let _ = stream.flush();
                let _ = stream.sock.shutdown(Shutdown::Write);
            }
        });

        let url = Url::parse(&format!("https://localhost:{port}/start")).expect("https url");
        let opts = FetchOptions {
            tls_config: Some(client),
            ..options()
        };
        let response = fetch(&url, &opts).expect("HSTS upgrades redirect hop to HTTPS");
        assert_eq!(response.status, 200);
        assert_eq!(response.final_url.scheme(), megi_net::Scheme::Https);
        assert_eq!(response.body, b"secure");
        handle.join().expect("tls server");
        megi_net::hsts::clear();
    }
}
