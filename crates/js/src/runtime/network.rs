use super::*;

// ---------------------------------------------------------------------------
// fetch() subset (round 31): host provider, Headers/Request/Response prelude
// ---------------------------------------------------------------------------

pub(crate) const MAX_FETCH_HEADERS: usize = 64;

/// Per-entry cap for `history.pushState`/`replaceState` state, matching the Web Storage quota.
/// The JSON snapshot is kept in Rust (`host.history_state`), rooted as a JS value and copied to
/// the host provider, so an unbounded state is unbounded retention (#663).
pub(crate) const MAX_HISTORY_STATE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_FETCH_BODY: usize = 8 * 1024 * 1024;

/// Request headers the browser owns; page scripts must not set them through `fetch()`.
pub(crate) const FORBIDDEN_FETCH_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "cookie",
    "origin",
    "referer",
    "user-agent",
    "accept-encoding",
    "expect",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "upgrade",
];

/// RFC 9110 §5.1 header-name token grammar.
pub(crate) fn is_valid_fetch_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
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
        })
}

/// RFC 9110 §5.5 field-value: printable bytes and HTAB only; no CR/LF/NUL/CTL/DEL.
pub(crate) fn is_valid_fetch_header_value(value: &str) -> bool {
    !value
        .bytes()
        .any(|byte| byte == 0x7f || (byte < 0x20 && byte != b'\t'))
}

pub(crate) fn fetch_native(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let url = arg_string(args, 0, context)?;
    let method = arg_string(args, 1, context)?.trim().to_ascii_uppercase();
    let headers_raw = arg_string(args, 2, context)?;
    let body_text = arg_string(args, 3, context)?;
    let mut headers = Vec::new();
    for line in headers_raw.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if !is_valid_fetch_header_name(name) {
            return Err(js_error("fetch: invalid request header name"));
        }
        if FORBIDDEN_FETCH_HEADERS
            .iter()
            .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
        {
            return Err(js_error(format!(
                "fetch: forbidden request header `{name}`"
            )));
        }
        if !is_valid_fetch_header_value(value) {
            return Err(js_error(format!(
                "fetch: invalid value for request header `{name}`"
            )));
        }
        if headers.len() >= MAX_FETCH_HEADERS {
            return Err(js_error("fetch headers limit exceeded"));
        }
        headers.push((name.to_string(), value.to_string()));
    }
    if body_text.len() > MAX_FETCH_BODY {
        return Err(js_error("fetch body limit exceeded"));
    }
    if !matches!(
        method.as_str(),
        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS"
    ) {
        return Err(js_error(
            "fetch supports GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS",
        ));
    }
    let redirect = if args.len() > 4 && !args[4].is_undefined() {
        Some(arg_string(args, 4, context)?)
    } else {
        None
    };
    let request = crate::host::FetchRequest {
        url,
        method: method.clone(),
        headers,
        body: (!body_text.is_empty() && method != "GET" && method != "HEAD")
            .then(|| body_text.into_bytes()),
        redirect,
    };
    let provider = ctx_or_mirror(
        context,
        |slot| slot.fetch.clone(),
        |host| host.fetch_provider.clone(),
    )
    .ok_or_else(|| js_error("fetch is not available in this host"))?;
    let result = provider(request).map_err(|error| js_error(error.to_string()))?;
    if result.body.len() > MAX_FETCH_BODY {
        return Err(js_error("fetch response body limit exceeded"));
    }
    let header_array = JsArray::from_iter([], context);
    for (index, (name, value)) in result.headers.iter().enumerate() {
        let pair = JsArray::from_iter([], context);
        pair.set(
            js_string!("0"),
            JsValue::from(js_string!(name.clone())),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
        pair.set(
            js_string!("1"),
            JsValue::from(js_string!(value.clone())),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
        header_array
            .set(
                js_string!(index.to_string()),
                JsValue::from(pair),
                false,
                context,
            )
            .map_err(|error| js_error(error.to_string()))?;
    }
    let body_text = String::from_utf8_lossy(&result.body).to_string();
    // Expose the body as a packed Uint8Array backed by a single byte block. The old
    // per-byte JsArray allocated ~8M JsValues and did 8M property sets at the
    // MAX_FETCH_BODY cap, from untrusted network input (#1143).
    let block = AlignedVec::from_iter(0, result.body.iter().copied());
    let buffer = JsArrayBuffer::from_byte_block(block, context)
        .map_err(|error| js_error(error.to_string()))?;
    let body_bytes = JsUint8Array::from_array_buffer(buffer, context)
        .map_err(|error| js_error(error.to_string()))?;
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("status"),
        JsValue::from(f64::from(result.status)),
        Attribute::all(),
    )
    .property(
        js_string!("statusText"),
        JsValue::from(js_string!(result.status_text)),
        Attribute::all(),
    )
    .property(
        js_string!("url"),
        JsValue::from(js_string!(result.url)),
        Attribute::all(),
    )
    .property(
        js_string!("headers"),
        JsValue::from(header_array),
        Attribute::all(),
    )
    .property(
        js_string!("bodyText"),
        JsValue::from(js_string!(body_text)),
        Attribute::all(),
    )
    .property(
        js_string!("bodyLength"),
        JsValue::from(result.body.len() as f64),
        Attribute::all(),
    )
    .property(
        js_string!("bodyBytes"),
        JsValue::from(body_bytes),
        Attribute::all(),
    );

    Ok(JsValue::from(init.build()))
}
