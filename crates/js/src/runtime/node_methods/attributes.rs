use super::*;

// ---------------------------------------------------------------------------
// Node methods
// ---------------------------------------------------------------------------

/// Returns the attribute name to use for `node_id`: HTML elements lowercase attribute names
/// (DOM `getAttribute`/`setAttribute` case-insensitivity for HTML), foreign elements keep
/// the authored case.
pub(crate) fn attribute_name(node_id: NodeId, name: &str, context: &mut Context) -> String {
    let is_html = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .is_some_and(|element| element.namespace == Namespace::Html)
    })
    .unwrap_or(false);
    if is_html {
        name.to_ascii_lowercase()
    } else {
        name.to_string()
    }
}

pub(crate) fn node_child_nodes(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let nodes: Vec<NodeId> =
        with_context_host(context, |host| host.document.children(parent).collect())
            .unwrap_or_default();
    node_list(context, nodes)
}

pub(crate) fn node_value_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = with_context_host(context, |host| {
        // WHATWG HTML §4.10.5: a dirty IDL value shadows the `value` content attribute.
        if let Some(idl) = host.idl_values.get(&node_id) {
            return Some(idl.clone());
        }
        let document = &host.document;
        let element = document
            .try_node(node_id)
            .and_then(|node| node.as_element())?;
        if element.local_name == "textarea" {
            return Some(document.text_content(node_id));
        }
        if element.local_name == "select" {
            let selected_id = document.descendants(node_id).find(|child| {
                document
                    .try_node(*child)
                    .and_then(|node| node.as_element())
                    .is_some_and(|candidate| {
                        candidate.local_name == "option"
                            && candidate.get_attribute("selected").is_some()
                    })
            });
            if let Some(option_id) = selected_id {
                // #1953: an option's dirty IDL value shadows its content attribute/text.
                if let Some(idl) = host.idl_values.get(&option_id) {
                    return Some(idl.clone());
                }
                let option = document
                    .try_node(option_id)
                    .and_then(|node| node.as_element());
                if let Some(value) = option.and_then(|el| el.get_attribute("value")) {
                    return Some(value.to_string());
                }
                return Some(document.text_content(option_id));
            }
            return Some(String::new());
        }
        if element.local_name == "option" {
            return Some(
                element
                    .get_attribute("value")
                    .map(str::to_owned)
                    .unwrap_or_else(|| document.text_content(node_id)),
            );
        }
        Some(element.get_attribute("value").unwrap_or("").to_string())
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(value)))
}

/// Validity flags for a form control (#1505, #1538).
#[derive(Default)]
pub(crate) struct ValidityFlags {
    pub(crate) value_missing: bool,
    pub(crate) custom_error: bool,
    pub(crate) pattern_mismatch: bool,
    pub(crate) type_mismatch: bool,
    pub(crate) too_short: bool,
    pub(crate) too_long: bool,
    pub(crate) range_underflow: bool,
    pub(crate) range_overflow: bool,
}

impl ValidityFlags {
    pub(crate) fn is_valid(&self) -> bool {
        !(self.value_missing
            || self.custom_error
            || self.pattern_mismatch
            || self.type_mismatch
            || self.too_short
            || self.too_long
            || self.range_underflow
            || self.range_overflow)
    }
}
