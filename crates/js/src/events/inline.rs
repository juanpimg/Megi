use super::*;

pub(crate) fn inline_get(
    this: &JsValue,
    event_type: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let slot = {
        let host = host_ref(context)?;
        host.inline_handlers
            .iter()
            .find(|handler| handler.target == target && handler.event_type == event_type)
            .map(|handler| handler.slot)
    };
    match slot {
        Some(slot) => array_get(context, "__megi_inline_handlers", u64::from(slot)),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn inline_set(
    this: &JsValue,
    event_type: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = target_key(this, context)?;
    let value = args.first().cloned().unwrap_or_else(JsValue::undefined);
    if value.as_object().is_none() {
        host_cell(context)?
            .inline_handlers
            .retain(|handler| !(handler.target == target && handler.event_type == event_type));
        return Ok(JsValue::undefined());
    }
    let slot = array_push(context, "__megi_inline_handlers", value)?;
    {
        let mut host = host_cell(context)?;
        host.inline_handlers
            .retain(|handler| !(handler.target == target && handler.event_type == event_type));
        host.inline_handlers.push(InlineHandler {
            target,
            event_type: event_type.to_string(),
            slot,
        });
    }
    Ok(JsValue::undefined())
}

macro_rules! inline_handler_accessors {
    ($(($get:ident, $set:ident, $property:literal, $event:literal)),+ $(,)?) => {
        $(
            fn $get(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                inline_get(this, $event, context)
            }

            fn $set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
                inline_set(this, $event, args, context)
            }
        )+

        /// Name/getter/setter triples for the supported inline handlers (#1504).
        pub(crate) fn inline_handler_accessors() -> Vec<(&'static str, NativeFn, NativeFn)> {
            vec![$(($property, $get as NativeFn, $set as NativeFn)),+]
        }
    };
}

inline_handler_accessors!(
    (inline_onclick_get, inline_onclick_set, "onclick", "click"),
    (inline_onload_get, inline_onload_set, "onload", "load"),
    (inline_onerror_get, inline_onerror_set, "onerror", "error"),
    (inline_oninput_get, inline_oninput_set, "oninput", "input"),
    (
        inline_onchange_get,
        inline_onchange_set,
        "onchange",
        "change"
    ),
    (
        inline_onsubmit_get,
        inline_onsubmit_set,
        "onsubmit",
        "submit"
    ),
    (
        inline_onkeydown_get,
        inline_onkeydown_set,
        "onkeydown",
        "keydown"
    ),
    (inline_onfocus_get, inline_onfocus_set, "onfocus", "focus"),
    (inline_onblur_get, inline_onblur_set, "onblur", "blur"),
    (
        inline_oninvalid_get,
        inline_oninvalid_set,
        "oninvalid",
        "invalid"
    ),
    (inline_onreset_get, inline_onreset_set, "onreset", "reset"),
    (
        inline_onmessage_get,
        inline_onmessage_set,
        "onmessage",
        "message"
    ),
    (
        inline_onanimationstart_get,
        inline_onanimationstart_set,
        "onanimationstart",
        "animationstart"
    ),
    (
        inline_onanimationiteration_get,
        inline_onanimationiteration_set,
        "onanimationiteration",
        "animationiteration"
    ),
    (
        inline_onanimationend_get,
        inline_onanimationend_set,
        "onanimationend",
        "animationend"
    ),
    (
        inline_ontransitionrun_get,
        inline_ontransitionrun_set,
        "ontransitionrun",
        "transitionrun"
    ),
    (
        inline_ontransitionstart_get,
        inline_ontransitionstart_set,
        "ontransitionstart",
        "transitionstart"
    ),
    (
        inline_ontransitionend_get,
        inline_ontransitionend_set,
        "ontransitionend",
        "transitionend"
    ),
);

/// Inline handler accessor pairs shared with node/document wrappers.
pub(crate) fn inline_node_accessors() -> Vec<(&'static str, NativeFn, NativeFn)> {
    inline_handler_accessors()
}
