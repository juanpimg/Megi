//! W3C WebDriver BiDi `script` domain backed by Boa on the context's live DOM.

use megi_webview::render::{evaluate_expression, evaluate_on_document};
use serde_json::{Value, json};

use super::BiDiSession;

fn to_remote_value(raw: &str) -> Value {
    match raw {
        "undefined" => json!({ "type": "undefined" }),
        "null" => json!({ "type": "null" }),
        "true" => json!({ "type": "boolean", "value": true }),
        "false" => json!({ "type": "boolean", "value": false }),
        "NaN" | "Infinity" | "-Infinity" => json!({ "type": "number", "value": raw }),
        _ => match serde_json::from_str::<Value>(raw) {
            Ok(Value::Number(num)) => json!({ "type": "number", "value": num }),
            Ok(Value::Bool(b)) => json!({ "type": "boolean", "value": b }),
            Ok(Value::Null) => json!({ "type": "null" }),
            Ok(Value::Array(arr)) => json!({ "type": "array", "value": arr }),
            Ok(Value::Object(obj)) => json!({ "type": "object", "value": obj }),
            Ok(Value::String(s)) => json!({ "type": "string", "value": s }),
            Err(_) => json!({ "type": "string", "value": raw }),
        },
    }
}

fn eval_in_context(
    session: &mut BiDiSession,
    context_id: &str,
    expression: &str,
) -> Result<Value, (&'static str, String)> {
    let ctx = session
        .find_context_mut(context_id)
        .ok_or(("no such frame", format!("unknown context {context_id}")))?;
    let outcome = match ctx.render.as_mut() {
        Some(output) => {
            evaluate_on_document(&mut output.document, &output.styles, expression, &ctx.url)
        }
        None => evaluate_expression(expression),
    };
    match outcome {
        Ok(text) => Ok(json!({
            "type": "success",
            "realm": context_id,
            "result": to_remote_value(&text),
        })),
        Err(err) => Ok(json!({
            "type": "exception",
            "realm": context_id,
            "exceptionDetails": {
                "text": err,
            }
        })),
    }
}

pub(crate) fn evaluate(
    session: &mut BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let expression = params
        .get("expression")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing expression".to_owned()))?;
    let context_id = params
        .get("target")
        .and_then(|target| target.get("context"))
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing target.context".to_owned()))?;
    eval_in_context(session, context_id, expression)
}

pub(crate) fn call_function(
    session: &mut BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let function = params
        .get("functionDeclaration")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing functionDeclaration".to_owned()))?;
    let context_id = params
        .get("target")
        .and_then(|target| target.get("context"))
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing target.context".to_owned()))?;
    let mut args = Vec::new();
    if let Some(items) = params.get("arguments").and_then(Value::as_array) {
        for item in items {
            let encoded = match item.get("type").and_then(Value::as_str) {
                Some("undefined") => "undefined".to_owned(),
                Some("null") => "null".to_owned(),
                Some("boolean" | "number" | "string") => item
                    .get("value")
                    .map(Value::to_string)
                    .unwrap_or_else(|| "undefined".to_owned()),
                _ => item
                    .get("value")
                    .map(Value::to_string)
                    .unwrap_or_else(|| "undefined".to_owned()),
            };
            args.push(encoded);
        }
    }
    let expr = format!("({function})({})", args.join(", "));
    eval_in_context(session, context_id, &expr)
}
