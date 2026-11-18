use super::*;

// --- MutationObserver -----------------------------------------------------

pub(crate) fn observer_slot(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("observer receiver"))?;
    object
        .get(js_string!("__megi_observer"), context)
        .map_err(|error| js_error(error.to_string()))?
        .as_number()
        .filter(|number| *number >= 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("invalid MutationObserver"))
}

pub(crate) fn mutation_observer_constructor(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let Some(callback) = args.first().filter(|value| value.is_callable()) else {
        return Err(js_error("MutationObserver callback must be a function"));
    };
    // Cap registrations before rooting anything (issue #1134): a script must not be
    // able to create millions of observers that are all visited on every mutation.
    let observer_count = with_context_host(context, |host| host.observers.len()).unwrap_or(0);
    if observer_count >= MAX_OBSERVERS {
        return Err(range_error(format!(
            "MutationObserver limit of {MAX_OBSERVERS} exceeded"
        )));
    }
    let callback_slot = root_push(context, "__megi_observer_callbacks", callback.clone())?;
    let object_slot = {
        let array = root_array(context, "__megi_observer_objects", true)?;
        array
            .get(js_string!("length"), context)
            .map_err(|error| js_error(error.to_string()))?
            .as_number()
            .unwrap_or(0.0) as u32
    };
    let mut init = ObjectInitializer::new(context);
    init.function(native(observer_observe), js_string!("observe"), 2);
    init.function(native(observer_disconnect), js_string!("disconnect"), 0);
    init.function(native(observer_take_records), js_string!("takeRecords"), 0);
    let observer = init.build();
    observer
        .set(
            js_string!("__megi_observer"),
            JsValue::from(object_slot),
            false,
            context,
        )
        .map_err(|error| js_error(error.to_string()))?;
    root_set(
        context,
        "__megi_observer_objects",
        u64::from(object_slot),
        JsValue::from(observer.clone()),
    )?;
    with_context_host(context, |host| {
        host.observers.push(ObserverRegistration {
            callback_slot,
            object_slot,
            targets: Vec::new(),
            records: Vec::new(),
        });
    });
    Ok(JsValue::from(observer))
}

pub(crate) fn observer_observe(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let slot = observer_slot(this, context)?;
    let node = argument_node(args, 0, context)?;
    let options = args
        .get(1)
        .and_then(|value| value.as_object())
        .ok_or_else(|| js_error("MutationObserver.observe options are required"))?;
    let explicit_attributes = options
        .get(js_string!("attributes"), context)
        .ok()
        .filter(|value| !value.is_undefined())
        .map(|value| value.to_boolean());
    let mut flag = |name: &str| -> bool {
        options
            .get(js_string!(name), context)
            .ok()
            .is_some_and(|value| value.to_boolean())
    };
    // DOM §4.3.2: if `attributeOldValue` is true and `attributes` is omitted, `attributes`
    // defaults to true.
    let attribute_old_value = flag("attributeOldValue");
    let target = ObserverTarget {
        node,
        child_list: flag("childList"),
        attributes: explicit_attributes.unwrap_or(attribute_old_value),
        attribute_old_value,
        character_data: flag("characterData"),
        subtree: flag("subtree"),
    };
    let registered = with_context_host(context, |host| host.observe_target(slot, target));
    if let Some(Err(message)) = registered {
        return Err(js_error(message));
    }
    Ok(JsValue::undefined())
}

pub(crate) fn observer_disconnect(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let slot = observer_slot(this, context)?;
    // Per DOM §4.3.1, disconnect() only stops delivering future records: the observer
    // stays registered so a later observe() can re-arm it. Removing the registration
    // made observe() silently no-op forever (#1135).
    with_context_host(context, |host| host.disconnect_observer(slot));
    Ok(JsValue::undefined())
}

pub(crate) fn observer_take_records(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let slot = observer_slot(this, context)?;
    let records = collect_observer_records(context, slot, true)?;
    Ok(JsValue::from(records))
}

pub(crate) fn note_mutation(
    kind: MutationKind,
    target: NodeId,
    name: Option<String>,
    old_value: Option<String>,
    context: &mut Context,
) {
    with_context_host(context, |host| {
        if !host.observer_index.is_empty() {
            host.record_mutation(MutationRecord {
                kind,
                target,
                name,
                old_value,
            });
        }
    });
}
