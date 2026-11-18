use super::*;

pub(crate) fn collect_observer_records(
    context: &mut Context,
    slot: u32,
    take: bool,
) -> JsResult<JsArray> {
    let records = with_context_host(context, |host| {
        let taken = {
            let Some(registration) = host.observers.iter_mut().find(|r| r.object_slot == slot)
            else {
                return Vec::new();
            };
            if take {
                std::mem::take(&mut registration.records)
            } else {
                registration.records.clone()
            }
        };
        taken
            .into_iter()
            .filter(|record| host.document.try_node(record.target).is_some())
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    let array = JsArray::from_iter([], context);
    for (index, record) in records.iter().enumerate() {
        let kind = match record.kind {
            MutationKind::ChildList => "childList",
            MutationKind::Attributes => "attributes",
            MutationKind::CharacterData => "characterData",
        };
        let target = wrap_node(context, record.target)?;
        let mut init = ObjectInitializer::new(context);
        init.property(
            js_string!("type"),
            JsValue::from(js_string!(kind)),
            Attribute::READONLY,
        );
        init.property(js_string!("target"), target, Attribute::READONLY);
        if let Some(name) = &record.name {
            init.property(
                js_string!("attributeName"),
                JsValue::from(js_string!(name.clone())),
                Attribute::READONLY,
            );
        }
        let old_value = match &record.old_value {
            Some(old) => JsValue::from(js_string!(old.clone())),
            None => JsValue::null(),
        };
        init.property(js_string!("oldValue"), old_value, Attribute::READONLY);
        let record = init.build();
        array
            .set(
                js_string!(index.to_string()),
                JsValue::from(record),
                false,
                context,
            )
            .map_err(|error| js_error(error.to_string()))?;
    }
    Ok(array)
}

pub(crate) fn deliver_mutation_records(context: &mut Context) -> Result<(), JsError> {
    let registrations: Vec<(u32, u32)> = with_context_host(context, |host| {
        host.observers
            .iter()
            .filter(|r| !r.records.is_empty())
            .map(|r| (r.object_slot, r.callback_slot))
            .collect()
    })
    .unwrap_or_default();
    for (object_slot, callback_slot) in registrations {
        let records = collect_observer_records(context, object_slot, true)
            .map_err(|error| JsError::Runtime(error.to_string()))?;
        if records.length(context).unwrap_or(0) == 0 {
            continue;
        }
        let callback = root_get(
            context,
            "__megi_observer_callbacks",
            u64::from(callback_slot),
        )
        .map_err(|error| JsError::Runtime(error.to_string()))?;
        let object = root_get(context, "__megi_observer_objects", u64::from(object_slot))
            .map_err(|error| JsError::Runtime(error.to_string()))?;
        let Some(callback) = callback.as_object() else {
            continue;
        };
        let args = [JsValue::from(records), object];
        if let Err(error) = callback.call(&JsValue::undefined(), &args, context) {
            let message = error.to_string();
            let typed = classify_error(&message);
            if matches!(typed, JsError::Budget(_)) {
                return Err(typed);
            }
            // A throwing callback must be reported, but must not abort delivery to
            // the remaining observers (#1136).
            crate::events::report_uncaught(context, &message, "<mutation-observer>")?;
        }
    }
    Ok(())
}
