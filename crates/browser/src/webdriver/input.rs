//! W3C WebDriver BiDi `input` domain: pointer hit-testing and key entry over the real DOM.

use serde_json::{Value, json};

use crate::forms::{FormControlKind, TextEdit, patch_control_node};
use crate::links::link_url_at;

use super::browsing_context::navigate_context;
use super::{BiDiContext, BiDiSession};

fn click_at(
    ctx: &mut BiDiContext,
    x: f32,
    y: f32,
    options: &megi_webview::render::RenderOptions,
    nav_id: &str,
) {
    if let Some(index) = crate::forms::control_at(&ctx.forms, x, y) {
        let (node_id, kind, value, checked) = {
            let control = &ctx.forms[index];
            (
                control.node_id,
                control.kind,
                control.value.clone(),
                control.checked,
            )
        };
        if kind.is_text_editable() {
            ctx.focused_form = Some(index);
            ctx.editor = Some(TextEdit::new(&value));
            if let Some(render) = ctx.render.as_mut() {
                let _ = patch_control_node(&mut render.document, node_id, None, None, true, false);
            }
            return;
        }
        if matches!(kind, FormControlKind::Checkbox | FormControlKind::Radio) {
            let next = !checked;
            ctx.forms[index].checked = next;
            if let Some(render) = ctx.render.as_mut() {
                let _ = patch_control_node(
                    &mut render.document,
                    node_id,
                    None,
                    Some(next),
                    true,
                    false,
                );
            }
            return;
        }
    }

    let base = megi_webview::net::Url::parse(&ctx.url).ok();
    if let Ok(Some(target)) = link_url_at(base.as_ref(), &ctx.links, x, y) {
        let target_url = target.as_str();
        let _ = navigate_context(ctx, &target_url, options, nav_id);
    }
}

fn apply_key_down(ctx: &mut BiDiContext, key_value: &str) {
    let Some(index) = ctx.focused_form else {
        return;
    };
    let Some(editor) = ctx.editor.as_mut() else {
        return;
    };
    match key_value {
        "Backspace" | "\u{E003}" => {
            editor.backspace();
        }
        "ArrowLeft" | "\u{E012}" => {
            editor.move_caret(false);
        }
        "ArrowRight" | "\u{E014}" => {
            editor.move_caret(true);
        }
        _ => {
            let text: String = key_value.chars().filter(|ch| !ch.is_control()).collect();
            editor.insert(&text);
        }
    }
    let updated = editor.value().to_owned();
    if let Some(control) = ctx.forms.get_mut(index) {
        control.value = updated.clone();
        let node_id = control.node_id;
        if let Some(render) = ctx.render.as_mut() {
            let _ = patch_control_node(
                &mut render.document,
                node_id,
                Some(&updated),
                None,
                true,
                false,
            );
        }
    }
}

pub(crate) fn perform_actions(
    session: &mut BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let context_id = params
        .get("context")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing context".to_owned()))?
        .to_owned();
    let actions = params
        .get("actions")
        .and_then(Value::as_array)
        .ok_or(("invalid argument", "missing actions".to_owned()))?;
    let options = session.options.clone();
    let nav_id = session.allocate_nav_id();
    let ctx = session
        .find_context_mut(&context_id)
        .ok_or(("no such frame", format!("unknown context {context_id}")))?;

    for source in actions {
        let kind = source.get("type").and_then(Value::as_str).unwrap_or("");
        let Some(steps) = source.get("actions").and_then(Value::as_array) else {
            continue;
        };
        match kind {
            "pointer" => {
                let mut x = 0.0_f32;
                let mut y = 0.0_f32;
                let mut pressed = false;
                for step in steps {
                    match step.get("type").and_then(Value::as_str) {
                        Some("pointerMove") => {
                            x = step.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                            y = step.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                        }
                        Some("pointerDown") => {
                            if step.get("button").and_then(Value::as_u64).unwrap_or(0) == 0 {
                                pressed = true;
                            }
                        }
                        Some("pointerUp")
                            if pressed
                                && step.get("button").and_then(Value::as_u64).unwrap_or(0) == 0 =>
                        {
                            pressed = false;
                            click_at(ctx, x, y, &options, &nav_id);
                        }
                        _ => {}
                    }
                }
            }
            "key" => {
                for step in steps {
                    if step.get("type").and_then(Value::as_str) == Some("keyDown")
                        && let Some(value) = step.get("value").and_then(Value::as_str)
                    {
                        apply_key_down(ctx, value);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(json!({}))
}

pub(crate) fn release_actions(
    session: &mut BiDiSession,
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let context_id = params
        .get("context")
        .and_then(Value::as_str)
        .ok_or(("invalid argument", "missing context".to_owned()))?;
    let ctx = session
        .find_context_mut(context_id)
        .ok_or(("no such frame", format!("unknown context {context_id}")))?;
    ctx.focused_form = None;
    ctx.editor = None;
    Ok(json!({}))
}
