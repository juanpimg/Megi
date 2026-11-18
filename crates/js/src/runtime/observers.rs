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

#[cfg(test)]
mod mutation_observer_delivery_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    pub(crate) fn mutation_observer_records_markup_insert_normalize_and_does_not_starve_siblings() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                var box = document.createElement('div');
                document.body.appendChild(box);
                var seenA = 0;
                var seenB = 0;
                var obsA = new MutationObserver(function(records) { seenA += records.length; });
                var obsB = new MutationObserver(function(records) { seenB += records.length; });
                obsA.observe(document.body, { childList: true, subtree: true });
                obsB.observe(document.body, { childList: true, subtree: true });

                box.innerHTML = '<span>hi</span>';
                box.firstChild.outerHTML = '<b>there</b>';
                box.insertAdjacentHTML('beforeend', '<i>!</i>');
                box.insertAdjacentText('afterbegin', 'pre');
                box.appendChild(document.createTextNode('a'));
                box.appendChild(document.createTextNode('b'));
                box.normalize();
                document.write('<p>written</p>');

                var takenA = obsA.takeRecords();
                var takenB = obsB.takeRecords();
                console.log('counts:' + takenA.length + ':' + takenB.length + ':' + seenA + ':' + seenB);
                "#,
                "observer.js",
            )
            .expect("eval observer script");
        let console = runtime.take_console();
        let line = console
            .iter()
            .find(|entry| entry.message.starts_with("counts:"))
            .expect("counts log");
        // All 7 mutations (innerHTML, outerHTML, insertAdjacentHTML, insertAdjacentText,
        // 2x appendChild, normalize, document.write = 8 mutations) must be seen by BOTH observers.
        assert_eq!(
            line.message, "counts:8:8:0:0",
            "both sibling observers must receive all 8 records without starvation: {console:?}"
        );
    }

    #[test]
    pub(crate) fn document_normalize_emits_child_list_record_when_nodes_are_merged() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                var count = 0;
                var obs = new MutationObserver(function(records) { count += records.length; });
                obs.observe(document.documentElement, { childList: true, subtree: true });
                document.body.appendChild(document.createTextNode('x'));
                document.body.appendChild(document.createTextNode('y'));
                obs.takeRecords();
                document.normalize();
                var after = obs.takeRecords().length;
                console.log('norm:' + after + ':' + count);
                "#,
                "norm.js",
            )
            .expect("eval normalize script");
        let console = runtime.take_console();
        let line = console
            .iter()
            .find(|entry| entry.message.starts_with("norm:"))
            .expect("norm log");
        assert_eq!(line.message, "norm:1:0");
    }

    #[test]
    pub(crate) fn mutation_observer_can_be_rearmed_after_disconnect() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                var target = document.createElement('div');
                document.body.appendChild(target);
                var obs = new MutationObserver(function() {});
                obs.observe(target, { attributes: true });
                target.setAttribute('a', '1');
                var before = obs.takeRecords().length;
                obs.disconnect();
                target.setAttribute('a', '2');
                var while_disconnected = obs.takeRecords().length;
                obs.observe(target, { attributes: true });
                target.setAttribute('a', '3');
                var after = obs.takeRecords().length;
                console.log('rearm:' + before + ':' + while_disconnected + ':' + after);
                "#,
                "observer_rearm.js",
            )
            .expect("eval observer rearm script");
        let console = runtime.take_console();
        let line = console
            .iter()
            .find(|entry| entry.message.starts_with("rearm:"))
            .expect("rearm log");
        assert_eq!(
            line.message, "rearm:1:0:1",
            "disconnect() must drop pending records and leave the observer re-armable (#1135): {console:?}"
        );
    }

    #[test]
    pub(crate) fn remove_attribute_node_records_an_attributes_mutation() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                var el = document.createElement('div');
                document.body.appendChild(el);
                el.setAttribute('data-x', '1');
                var attr = el.getAttributeNode('data-x');
                var obs = new MutationObserver(function() {});
                obs.observe(el, { attributes: true });
                el.removeAttributeNode(attr);
                var records = obs.takeRecords();
                console.log('attrnode:' + records.length + ':' + (records.length ? records[0].attributeName : '-'));
                "#,
                "observer_attrnode.js",
            )
            .expect("eval removeAttributeNode script");
        let console = runtime.take_console();
        let line = console
            .iter()
            .find(|entry| entry.message.starts_with("attrnode:"))
            .expect("attrnode log");
        assert_eq!(
            line.message, "attrnode:1:data-x",
            "removeAttributeNode must deliver an attributes record with its name (#1137): {console:?}"
        );
    }

    #[test]
    pub(crate) fn failing_worker_reply_does_not_starve_other_workers() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime.set_worker_loader(Arc::new(|url| {
            if url.ends_with("worker.js") {
                Ok("onmessage = function(e) { postMessage(e.data); };".to_string())
            } else {
                Err(crate::JsError::Runtime("not found".to_string()))
            }
        }));
        runtime
            .eval(
                "window.__a = null; window.__b = null;
                 var wa = new Worker('worker.js'); wa.onmessage = function(e) { __a = e.data; };
                 var wb = new Worker('worker.js'); wb.onmessage = function(e) { __b = e.data; };
                 wa.postMessage(1); wb.postMessage(2);",
                "workers.js",
            )
            .expect("eval workers script");
        // Make worker A's queued reply undeliverable (malformed JSON).
        {
            let mut host = host_cell(&mut runtime.context).expect("host installed");
            let worker_a = host.workers[0].as_mut().expect("worker A");
            let mut worker_host = host_cell(&mut worker_a.runtime.context).expect("worker host");
            worker_host
                .worker_outbox
                .as_mut()
                .expect("worker outbox")
                .push_front("not json".to_string());
        }
        // Before the fix this aborted on worker A, so worker B never received its reply.
        runtime
            .run_worker_tasks(16)
            .expect("worker pump must not abort");
        assert_eq!(runtime.eval("__b", "check.js").unwrap(), "2");
        assert!(
            runtime
                .take_console()
                .iter()
                .any(|entry| entry.level == "error"),
            "the failed delivery must be reported"
        );
    }

    #[test]
    pub(crate) fn mutation_observer_registrations_are_capped() {
        let mut runtime = JsRuntime::new().expect("runtime");
        let script = format!(
            r#"
            for (var i = 0; i < {MAX_OBSERVERS}; i++) {{
                new MutationObserver(function() {{}});
            }}
            console.log('cap-ok');
            try {{
                new MutationObserver(function() {{}});
                console.log('cap-fail');
            }} catch (e) {{
                console.log('cap-caught');
            }}
            "#
        );
        runtime
            .eval(&script, "observer_cap.js")
            .expect("eval cap script");
        let console = runtime.take_console();
        let messages: Vec<&str> = console.iter().map(|entry| entry.message.as_str()).collect();
        assert!(messages.contains(&"cap-ok"), "{console:?}");
        assert!(messages.contains(&"cap-caught"), "{console:?}");
        assert!(!messages.contains(&"cap-fail"), "{console:?}");
    }

    #[test]
    pub(crate) fn throwing_observer_callback_does_not_starve_other_observers() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                var el = document.createElement('div');
                document.body.appendChild(el);
                var seen = 0;
                var bad = new MutationObserver(function() { throw new Error('boom'); });
                var good = new MutationObserver(function(records) { seen += records.length; });
                bad.observe(el, { childList: true });
                good.observe(el, { childList: true });
                el.appendChild(document.createTextNode('x'));
                "#,
                "observer_throw.js",
            )
            .expect("eval observer throw script");
        // A callback throwing must be reported, not abort the delivery pass.
        runtime.run_microtasks().expect("microtask delivery");
        runtime
            .eval(
                "console.log('throwisolate:' + seen);",
                "observer_throw_check.js",
            )
            .expect("eval check script");
        let console = runtime.take_console();
        let line = console
            .iter()
            .find(|entry| entry.message.starts_with("throwisolate:"))
            .expect("throwisolate log");
        assert_eq!(
            line.message, "throwisolate:1",
            "the healthy observer must still receive its records (#1136): {console:?}"
        );
        assert!(
            console
                .iter()
                .any(|entry| entry.level == "error" && entry.message.contains("boom")),
            "the thrown exception must be reported as uncaught: {console:?}"
        );
    }

    #[test]
    pub(crate) fn mutation_observer_cap_prevents_unbounded_js_root_growth() {
        let mut runtime = JsRuntime::new().expect("runtime");
        let summary = runtime
            .eval(
                r#"
                var ok = 0;
                for (var i = 0; i < 1024; i++) {
                    new MutationObserver(function() {});
                    ok++;
                }
                var errName = '';
                try {
                    new MutationObserver(function() {});
                } catch (e) {
                    errName = e.name;
                }
                ok + ':' + errName + ':' + __megi_observer_callbacks.length + ':' + __megi_observer_objects.length
                "#,
                "observer_cap.js",
            )
            .expect("eval cap script");
        assert_eq!(summary, "1024:RangeError:1024:1024");
        assert_eq!(
            host_ref(&runtime.context)
                .expect("host installed")
                .observers
                .len(),
            MAX_OBSERVERS
        );
    }

    #[test]
    pub(crate) fn attribute_old_value_and_remove_attribute_node_respect_option_and_case() {
        let mut runtime = JsRuntime::new().expect("runtime");
        let result = runtime
            .eval(
                r#"
                var el = document.createElement('div');
                document.body.appendChild(el);
                var withOld = new MutationObserver(function() {});
                var withoutOld = new MutationObserver(function() {});
                withOld.observe(el, { attributeOldValue: true });
                withoutOld.observe(el, { attributes: true });

                el.setAttribute('DATA-X', 'v1');
                el.setAttribute('data-x', 'v2');
                el.removeAttributeNode({ name: 'DATA-X' });

                var a = withOld.takeRecords();
                var b = withoutOld.takeRecords();
                [
                    a.length,
                    a[0].attributeName + '=' + String(a[0].oldValue),
                    a[1].attributeName + '=' + a[1].oldValue,
                    a[2].attributeName + '=' + a[2].oldValue,
                    b.length,
                    String(b[1].oldValue),
                    String(b[2].oldValue)
                ].join('|')
                "#,
                "observer_old_value.js",
            )
            .expect("eval oldValue script");
        assert_eq!(result, "3|data-x=null|data-x=v1|data-x=v2|3|null|null");
    }

    #[test]
    pub(crate) fn throwing_observer_hits_onerror_and_budget_error_propagates() {
        let mut runtime = JsRuntime::new().expect("runtime");
        runtime
            .eval(
                r#"
                window.__onerror_msgs = [];
                window.onerror = function(msg) { window.__onerror_msgs.push(msg); };
                var el = document.createElement('div');
                document.body.appendChild(el);
                var obs = new MutationObserver(function() { throw new Error('cb_fail'); });
                obs.observe(el, { childList: true });
                el.appendChild(document.createTextNode('a'));
                "#,
                "onerror_obs.js",
            )
            .expect("eval");
        runtime.run_microtasks().expect("microtasks");
        assert_eq!(
            runtime
                .eval("window.__onerror_msgs.length", "check")
                .unwrap(),
            "1"
        );

        let budget = Budget {
            loop_iterations: 500,
            ..Budget::default()
        };
        let mut budgeted = JsRuntime::new().expect("runtime");
        budgeted.set_budget(budget);
        budgeted
            .eval(
                r#"
                var el = document.createElement('div');
                document.body.appendChild(el);
                var obs = new MutationObserver(function() {
                    while (true) {}
                });
                obs.observe(el, { childList: true });
                el.appendChild(document.createTextNode('b'));
                "#,
                "budget_obs.js",
            )
            .expect("setup mutation");
        let err = budgeted
            .run_microtasks()
            .expect_err("budget error in MutationObserver callback must propagate");
        assert!(
            matches!(err, JsError::Budget(_)),
            "expected JsError::Budget, got {err:?}"
        );
    }
}
