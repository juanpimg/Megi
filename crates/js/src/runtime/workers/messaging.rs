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
        // F6-WORKERS-DOM: no direct DOM in the worker realm, but a message-based proxy
        // forwards queries to the owning document over the deterministic postMessage channel.
        let dom_prelude = r#"
var __megi_dom_seq = 0;
var __megi_dom_pending = {};
function __megi_dom_call(method, args) {
  var seq = ++__megi_dom_seq;
  return new Promise(function (resolve) {
    __megi_dom_pending[seq] = resolve;
    postMessage({ __megi_dom_call: { seq: seq, method: method, args: args } });
  });
}
function __megi_make_element(nodeId) {
  return {
    __megi_node_id: nodeId,
    textContent: __megi_dom_call('textContent', [nodeId]),
    setAttribute: function (name, value) {
      return __megi_dom_call('setAttribute', [nodeId, name, value]);
    },
    setTextContent: function (text) {
      return __megi_dom_call('setTextContent', [nodeId, text]);
    },
    getComputedStyle: function (property) {
      return __megi_dom_call('getComputedStyle', [nodeId, property]);
    },
    classList: {
      contains: function (name) { return __megi_dom_call('classList', [nodeId, 'contains', name]); },
      add: function (name) { return __megi_dom_call('classList', [nodeId, 'add', name]); },
      remove: function (name) { return __megi_dom_call('classList', [nodeId, 'remove', name]); },
      toggle: function (name) { return __megi_dom_call('classList', [nodeId, 'toggle', name]); }
    }
  };
}
var document = {
  getElementById: function (id) {
    return __megi_dom_call('getElementById', [id]).then(function (nodeId) {
      return nodeId === null ? null : __megi_make_element(nodeId);
    });
  },
  getElementsByTagName: function (tag) {
    return __megi_dom_call('getElementsByTagName', [tag]).then(function (ids) {
      return ids.map(__megi_make_element);
    });
  }
};
"#;
        self.context
            .eval(Source::from_bytes(dom_prelude))
            .map(|_| ())
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
function removeEventListener(type, callback) { if (type === "message") { var i = __megi_worker_listeners.indexOf(callback); if (i !== -1) { __megi_worker_listeners.splice(i, 1); } } }
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
