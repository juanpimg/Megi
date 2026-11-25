//! W3C WebDriver BiDi `browsingContext` domain backed by `megi_webview::render::render_url`.

use megi_webview::net::base64_encode;
use megi_webview::render::{RenderOptions, render_url};
use serde_json::{Value, json};

use super::{BiDiContext, BiDiSession};

/// Allows safe browser navigation schemes only (`http`, `https`, `file`, `data`, `about`).
pub(crate) fn is_allowed_url(url: &str) -> bool {
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("file://")
        || url.starts_with("data:")
        || url.starts_with("about:")
}

pub(crate) fn navigate_context(
    ctx: &mut BiDiContext,
    url: &str,
    options: &RenderOptions,
    nav_id: &str,
) -> Result<Value, (&'static str, String)> {
    if !is_allowed_url(url) {
        return Err(("invalid argument", format!("unsupported URL scheme: {url}")));
    }
    let output = render_url(url, options)
        .map_err(|error| ("unknown error", format!("render failed: {error}")))?;
    let final_url = output.document_url.as_str().to_owned();
    ctx.url = final_url.clone();
    ctx.forms = crate::forms::collect_controls(&output.document, &output.layout);
    ctx.links = crate::links::collect_link_regions(&output);
    ctx.focused_form = None;
    ctx.editor = None;
    ctx.render = Some(output);
    Ok(json!({
        "navigation": nav_id,
        "url": final_url,
    }))
}

pub(crate) fn create(
    session: &mut BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let kind = params.get("type").and_then(Value::as_str).unwrap_or("tab");
    if kind != "tab" && kind != "window" {
        return Err(("invalid argument", format!("invalid context type: {kind}")));
    }
    let ctx = session.allocate_context();
    Ok(json!({ "context": ctx.id }))
}

pub(crate) fn get_tree(
    session: &BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let root = params.get("root").and_then(Value::as_str);
    let contexts: Vec<Value> = match root {
        Some(id) => {
            let ctx = session
                .find_context(id)
                .ok_or(("no such frame", format!("unknown context {id}")))?;
            vec![json!({
                "context": ctx.id,
                "url": ctx.url,
                "children": [],
                "parent": Value::Null,
            })]
        }
        None => session.context_tree_json(),
    };
    Ok(json!({ "contexts": contexts }))
}

pub(crate) fn navigate(
    session: &mut BiDiSession,
    params: &Value,
    events: &mut Vec<String>,
) -> Result<Value, (&'static str, String)> {
    let context_id = params
        .get("context")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing context".to_owned()))?
        .to_owned();
    let url = params
        .get("url")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing url".to_owned()))?
        .to_owned();
    let nav_id = session.allocate_nav_id();
    let options = session.options.clone();
    let subscribed_load = session.is_subscribed("browsingContext.load");
    let ctx = session
        .find_context_mut(&context_id)
        .ok_or(("no such frame", format!("unknown context {context_id}")))?;
    let result = navigate_context(ctx, &url, &options, &nav_id)?;
    if subscribed_load {
        events.push(
            json!({
                "type": "event",
                "method": "browsingContext.load",
                "params": {
                    "context": context_id,
                    "navigation": nav_id,
                    "url": result["url"],
                }
            })
            .to_string(),
        );
    }
    Ok(result)
}

pub(crate) fn reload(
    session: &mut BiDiSession,
    params: &Value,
    events: &mut Vec<String>,
) -> Result<Value, (&'static str, String)> {
    let context_id = params
        .get("context")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing context".to_owned()))?
        .to_owned();
    let url = session
        .find_context(&context_id)
        .ok_or(("no such frame", format!("unknown context {context_id}")))?
        .url
        .clone();
    let mut reload_params = params.clone();
    reload_params["url"] = Value::String(url);
    navigate(session, &reload_params, events)
}

pub(crate) fn capture_screenshot(
    session: &mut BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let context_id = params
        .get("context")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing context".to_owned()))?;
    let options = session.options.clone();
    let ctx = session
        .find_context_mut(context_id)
        .ok_or(("no such frame", format!("unknown context {context_id}")))?;
    if ctx.render.is_none() {
        let output = render_url(&ctx.url, &options)
            .map_err(|error| ("unknown error", format!("render failed: {error}")))?;
        ctx.render = Some(output);
    }
    let png = ctx.render.as_mut().expect("render populated").png_bytes();
    Ok(json!({ "data": base64_encode(png) }))
}

pub(crate) fn close(
    session: &mut BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let context_id = params
        .get("context")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing context".to_owned()))?;
    if !session.close_context(context_id) {
        return Err(("no such frame", format!("unknown context {context_id}")));
    }
    Ok(json!({}))
}
