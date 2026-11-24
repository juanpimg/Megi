use super::*;

/// Builds the `navigator` host object.
pub(crate) fn build_navigator(context: &mut Context) -> JsObject {
    let languages = JsArray::from_iter([JsValue::from(js_string!("en-US"))], context);
    let plugins = JsArray::from_iter([], context);
    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("userAgent"),
        js_string!("Megi/1.0"),
        Attribute::all(),
    );
    init.property(
        js_string!("language"),
        js_string!("en-US"),
        Attribute::all(),
    );
    init.property(
        js_string!("languages"),
        JsValue::from(languages),
        Attribute::all(),
    );
    init.property(js_string!("onLine"), JsValue::from(true), Attribute::all());
    init.property(
        js_string!("hardwareConcurrency"),
        JsValue::from(1),
        Attribute::all(),
    );
    init.property(
        js_string!("platform"),
        js_string!("Linux x86_64"),
        Attribute::all(),
    );
    init.property(
        js_string!("plugins"),
        JsValue::from(plugins),
        Attribute::all(),
    );
    init.function(
        native(|_this, _args, _context| Ok(JsValue::from(true))),
        js_string!("sendBeacon"),
        0,
    );
    init.build()
}

/// Builds the `performance` host object.
pub(crate) fn build_performance(context: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(context);
    init.function(
        native(|_this, _args, _context| {
            let now = with_context_host(_context, |host| host.now_ms()).unwrap_or(0);
            Ok(JsValue::from(now as f64))
        }),
        js_string!("now"),
        0,
    );
    init.property(
        js_string!("timeOrigin"),
        JsValue::from(0.0_f64),
        Attribute::all(),
    );
    for name in [
        "mark",
        "measure",
        "getEntries",
        "getEntriesByName",
        "getEntriesByType",
        "clearMarks",
        "clearMeasures",
    ] {
        init.function(
            native(|_this, _args, _context| Ok(JsValue::undefined())),
            js_string!(name),
            0,
        );
    }
    init.build()
}

/// Builds the `crypto` host object.
pub(crate) fn build_crypto(context: &mut Context) -> JsObject {
    // Build `subtle` first: ObjectInitializer holds a mutable borrow of
    // `context`, so only one initializer may be alive at a time.
    let subtle = {
        let mut subtle_init = ObjectInitializer::new(context);
        subtle_init.function(
            native(|_this, args, context| {
                let algorithm = args
                    .first()
                    .map(|v| {
                        v.to_string(context)
                            .map(|s| s.to_std_string_escaped())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if algorithm != "sha-256" {
                    return Err(js_error("subtle.digest: unsupported algorithm"));
                }
                let data = args.get(1).cloned().unwrap_or(JsValue::undefined());
                let bytes: Vec<u8> = if let Some(s) = data.as_string() {
                    // Our JS caller passes a BufferSource as a Latin-1 string.
                    s.to_std_string_escaped()
                        .chars()
                        .filter_map(|c| u8::try_from(u32::from(c)).ok())
                        .collect()
                } else if let Some(obj) = data.as_object() {
                    if let Ok(arr) = JsUint8Array::from_object(obj.clone()) {
                        arr.to_vec(context).unwrap_or_default()
                    } else {
                        return Err(js_error("subtle.digest requires a BufferSource"));
                    }
                } else {
                    return Err(js_error("subtle.digest requires a BufferSource"));
                };
                let digest = crate::crypto::sha256(&bytes);
                let block = AlignedVec::from_iter(0, digest.iter().copied());
                let buffer = JsArrayBuffer::from_byte_block(block, context)
                    .map_err(|e| js_error(e.to_string()))?;
                let promise = boa_engine::object::builtins::JsPromise::new(
                    move |resolvers, ctx| {
                        resolvers.resolve.call(
                            &JsValue::undefined(),
                            &[JsValue::from(buffer.clone())],
                            ctx,
                        )?;
                        Ok(JsValue::undefined())
                    },
                    context,
                )?;
                Ok(JsValue::from(promise))
            }),
            js_string!("digest"),
            2,
        );
        subtle_init.build()
    };
    let mut init = ObjectInitializer::new(context);
    init.function(
        native(|_this, args, context| {
            let array = args.first().cloned().unwrap_or(JsValue::undefined());
            let obj = array
                .as_object()
                .ok_or_else(|| js_error("getRandomValues requires a typed array"))?;
            // Web IDL: BufferSource only — a plain object has no byteLength.
            // Web Crypto §4.1.1 caps a single call at 2^16 bytes.
            let byte_length = obj
                .get(js_string!("byteLength"), context)
                .ok()
                .and_then(|v| v.to_number(context).ok())
                .unwrap_or(f64::NAN);
            if !byte_length.is_finite() || byte_length < 0.0 || byte_length > 65_536.0 {
                return Err(js_error("getRandomValues requires a typed array"));
            }
            let len = byte_length as usize;
            let mut bytes = vec![0u8; len];
            crate::crypto::fill_random(&mut bytes);
            for (i, byte) in bytes.into_iter().enumerate() {
                let _ = obj.set(
                    js_string!(i.to_string()),
                    JsValue::from(byte as i32),
                    false,
                    context,
                );
            }
            Ok(array)
        }),
        js_string!("getRandomValues"),
        1,
    );
    init.function(
        native(|_this, _args, _context| {
            // Generate a v4 UUID from the OS CSPRNG (RFC 4122 §4.4).
            let mut bytes = [0u8; 16];
            crate::crypto::fill_random(&mut bytes);
            bytes[6] = (bytes[6] & 0x0f) | 0x40;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
            let uuid = format!(
                "{}-{}-{}-{}-{}",
                &hex[0..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..32]
            );
            Ok(JsValue::from(js_string!(uuid)))
        }),
        js_string!("randomUUID"),
        0,
    );
    init.property(
        js_string!("subtle"),
        JsValue::from(subtle),
        Attribute::all(),
    );
    init.build()
}

/// Builds the `TextEncoder` constructor.
pub(crate) fn build_text_encoder(
    context: &mut Context,
) -> boa_engine::object::builtins::JsFunction {
    let constructor = FunctionObjectBuilder::new(context.realm(), native(text_encoder_constructor))
        .name(js_string!("TextEncoder"))
        .length(0)
        .constructor(true)
        .build();
    let proto = constructor
        .get(js_string!("prototype"), context)
        .unwrap_or(JsValue::undefined())
        .as_object()
        .unwrap_or_else(|| ObjectInitializer::new(context).build());
    let encode_fn = FunctionObjectBuilder::new(context.realm(), native(text_encoder_encode))
        .name(js_string!("encode"))
        .length(1)
        .build();
    let _ = proto.set(
        js_string!("encode"),
        JsValue::from(encode_fn),
        false,
        context,
    );
    let encoding_get = accessor_fn(context, text_encoder_encoding_get);
    let _ = proto.define_property_or_throw(
        js_string!("encoding"),
        PropertyDescriptor::builder()
            .get(JsValue::from(encoding_get))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );
    constructor
}

pub(crate) fn text_encoder_constructor(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let mut init = ObjectInitializer::new(context);
    Ok(JsValue::from(init.build()))
}

pub(crate) fn text_encoder_encoding_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!("utf-8")))
}

pub(crate) fn text_encoder_encode(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let input = args
        .first()
        .map(|v| {
            v.to_string(context)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let block = AlignedVec::from_iter(0, input.as_bytes().iter().copied());
    let buffer =
        JsArrayBuffer::from_byte_block(block, context).map_err(|e| js_error(e.to_string()))?;
    let arr =
        JsUint8Array::from_array_buffer(buffer, context).map_err(|e| js_error(e.to_string()))?;
    Ok(JsValue::from(arr))
}

/// Builds the `TextDecoder` constructor.
pub(crate) fn build_text_decoder(
    context: &mut Context,
) -> boa_engine::object::builtins::JsFunction {
    let constructor = FunctionObjectBuilder::new(context.realm(), native(text_decoder_constructor))
        .name(js_string!("TextDecoder"))
        .length(0)
        .constructor(true)
        .build();
    let proto = constructor
        .get(js_string!("prototype"), context)
        .unwrap_or(JsValue::undefined())
        .as_object()
        .unwrap_or_else(|| ObjectInitializer::new(context).build());
    let decode_fn = FunctionObjectBuilder::new(context.realm(), native(text_decoder_decode))
        .name(js_string!("decode"))
        .length(1)
        .build();
    let _ = proto.set(
        js_string!("decode"),
        JsValue::from(decode_fn),
        false,
        context,
    );
    let encoding_get = accessor_fn(context, text_decoder_encoding_get);
    let _ = proto.define_property_or_throw(
        js_string!("encoding"),
        PropertyDescriptor::builder()
            .get(JsValue::from(encoding_get))
            .enumerable(true)
            .configurable(true)
            .build(),
        context,
    );
    constructor
}

pub(crate) fn text_decoder_constructor(
    _this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let mut init = ObjectInitializer::new(context);
    Ok(JsValue::from(init.build()))
}

pub(crate) fn text_decoder_encoding_get(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> JsResult<JsValue> {
    Ok(JsValue::from(js_string!("utf-8")))
}

pub(crate) fn text_decoder_decode(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let input = args.first().cloned().unwrap_or(JsValue::undefined());
    if input.is_undefined() || input.is_null() {
        return Ok(JsValue::from(js_string!("")));
    }
    // Try to read as Uint8Array
    let obj = input
        .as_object()
        .ok_or_else(|| js_error("decode requires a BufferSource"))?;
    let len = obj
        .get(js_string!("length"), context)
        .ok()
        .and_then(|v| v.to_number(context).ok())
        .unwrap_or(0.0) as usize;
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len {
        let b = obj
            .get(js_string!(i.to_string()), context)
            .ok()
            .and_then(|v| v.to_number(context).ok())
            .unwrap_or(0.0) as u8;
        bytes.push(b);
    }
    let text = String::from_utf8_lossy(&bytes);
    Ok(JsValue::from(js_string!(text.to_string())))
}
