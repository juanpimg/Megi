//! Unit tests kept alongside the HTTP client (moved out of `http.rs` for file-size limits).

#[cfg(test)]
#[cfg(feature = "tls")]
mod tls_cache_tests {
    use crate::http::tls::default_tls_config;
    use std::sync::Arc;

    #[test]
    fn successful_tls_config_is_cached() {
        let a = default_tls_config().expect("first build");
        let b = default_tls_config().expect("cached build");
        assert!(
            Arc::ptr_eq(&a, &b),
            "successful configurations must be shared across fetches"
        );
    }
}

#[cfg(test)]
mod header_encoding_tests {
    use crate::http::response::parse_head_verbose;

    #[test]
    fn latin1_header_bytes_are_preserved() {
        let head = b"HTTP/1.1 200 OK\r\nX-City: Z\xFCrich\r\n\r\n";
        let (_major, status, reason, headers) = parse_head_verbose(head).expect("parse");
        assert_eq!(status, 200);
        assert_eq!(reason, "OK");
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("x-city"))
                .map(|(_, value)| value.as_str()),
            Some("Z\u{00FC}rich")
        );
    }
}

#[cfg(test)]
mod parse_head_validity_tests {
    use crate::http::response::parse_head_is_valid;

    #[test]
    fn head_validity_matches_parser() {
        assert!(parse_head_is_valid(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"
        ));
        assert!(!parse_head_is_valid(b"garbage\r\n\r\n"));
        assert!(!parse_head_is_valid(b""));
    }
}
