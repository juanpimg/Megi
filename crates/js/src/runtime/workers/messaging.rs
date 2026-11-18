use super::*;

impl JsRuntime {
    pub(crate) fn install_worker_globals(&mut self) -> Result<(), JsError> {
        let global = self.context.global_object();
        let worker_post =
            FunctionObjectBuilder::new(self.context.realm(), native(worker_self_post_message))
                .name(js_string!("postMessage"))
                .length(1)
                .build();
        global
            .set(
                js_string!("postMessage"),
                JsValue::from(worker_post),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        global
            .set(
                js_string!("self"),
                JsValue::from(global.clone()),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        global
            .set(
                js_string!("document"),
                JsValue::undefined(),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        global
            .set(
                js_string!("window"),
                JsValue::undefined(),
                false,
                &mut self.context,
            )
            .map_err(|error| JsError::Internal(error.to_string()))?;
        let prelude = r#"
var __megi_worker_listeners = [];
function addEventListener(type, callback) { if (type === "message" && typeof callback === "function" && __megi_worker_listeners.length < 64) { __megi_worker_listeners.push(callback); } }
function removeEventListener() {}
"#;
        self.context
            .eval(Source::from_bytes(prelude))
            .map(|_| ())
            .map_err(|error| JsError::Internal(error.to_string()))
    }
}

pub(crate) fn worker_self_post_message(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let value = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let json = json_state(&value, context);
    let sent = with_context_host(context, |host| {
        let Some(outbox) = &mut host.worker_outbox else {
            return false;
        };
        if outbox.len() >= MAX_WORKER_MESSAGES {
            return false;
        }
        outbox.push_back(json);
        true
    })
    .unwrap_or(false);
    if !sent {
        return Err(js_error("worker message queue is full"));
    }
    Ok(JsValue::undefined())
}
