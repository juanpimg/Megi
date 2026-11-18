//! Request-header validation (RFC 9110 §5.1/§5.3/§5.5) and token bytes.

use crate::error::FetchError;

pub(super) fn validate_request_headers(
    headers: &[(String, String)],
    max_bytes: usize,
) -> Result<(), FetchError> {
    let mut total = 0usize;
    for (name, value) in headers {
        let bad_name = name.is_empty() || !name.bytes().all(is_token_byte);
        let bad_value = value
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | 0) || (byte < 0x20 && byte != b'\t'));
        if bad_name || bad_value {
            return Err(FetchError::InvalidRequestHeader(
                "invalid request header name/value".to_owned(),
            ));
        }
        total = total.saturating_add(name.len() + value.len() + 4);
    }
    if total > max_bytes {
        return Err(FetchError::HeadersTooLarge);
    }
    Ok(())
}

/// The URL is the authoritative source for `Host`, so `net` never accepts an extra one:
/// a duplicate `Host` is always a header-injection/request-smuggling primitive. Other
/// browser-owned headers (`Content-Length`, `Transfer-Encoding`, cookies, CORS/referrer
/// headers, ...) are blocked in the JavaScript fetch layer; this low-level HTTP client
/// keeps accepting the framing headers its own API supports (`Expect`,
/// `Transfer-Encoding`, `Content-Length` for explicit bodies).
const FORBIDDEN_REQUEST_HEADERS: &[&str] = &["host"];

/// Validates one caller-supplied request header (RFC 9110 §5.1/§5.3/§5.5).
///
/// The name must be a non-empty token and the value must not contain CR, LF, NUL or any
/// other control byte except HTAB. Both checks are required before the header is framed:
/// a CR/LF in either field would start a new header line, and a duplicate `Host` or
/// `Content-Length` would change how the peer interprets the request.
pub(super) fn validate_request_header(name: &str, value: &str) -> Result<(), FetchError> {
    if name.is_empty() || !name.bytes().all(is_token_byte) {
        return Err(FetchError::InvalidRequestHeader(
            "invalid request header name".to_owned(),
        ));
    }
    if FORBIDDEN_REQUEST_HEADERS
        .iter()
        .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
    {
        return Err(FetchError::InvalidRequestHeader(format!(
            "forbidden request header `{name}`"
        )));
    }
    if value
        .bytes()
        .any(|byte| byte == 0x7f || (byte < 0x20 && byte != b'\t'))
    {
        return Err(FetchError::InvalidRequestHeader(format!(
            "invalid value for request header `{name}`"
        )));
    }
    Ok(())
}

pub(super) fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}
