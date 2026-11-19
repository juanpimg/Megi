use super::*;

use std::collections::HashMap;

pub(crate) fn is_control_disabled(document: &megi_core::Document, node_id: NodeId) -> bool {
    let Some(element) = document.try_node(node_id).and_then(|n| n.as_element()) else {
        return false;
    };
    if element.has_attribute("disabled") {
        return true;
    }
    let mut prev = node_id;
    let mut cur = document.try_node(node_id).and_then(|n| n.parent);
    while let Some(ancestor_id) = cur {
        let Some(ancestor) = document.try_node(ancestor_id).and_then(|n| n.as_element()) else {
            break;
        };
        if ancestor.local_name == "form" {
            break;
        }
        if ancestor.local_name == "fieldset" && ancestor.has_attribute("disabled") {
            let first_legend = document.children(ancestor_id).find(|child| {
                document
                    .try_node(*child)
                    .and_then(|n| n.as_element())
                    .is_some_and(|e| e.local_name == "legend")
            });
            if first_legend != Some(prev) {
                return true;
            }
        }
        prev = ancestor_id;
        cur = document.try_node(ancestor_id).and_then(|n| n.parent);
    }
    false
}

pub(crate) fn enclosing_form(document: &megi_core::Document, node_id: NodeId) -> Option<NodeId> {
    let mut cur = document.try_node(node_id).and_then(|n| n.parent);
    while let Some(ancestor_id) = cur {
        if document
            .try_node(ancestor_id)
            .and_then(|n| n.as_element())
            .is_some_and(|el| el.local_name == "form")
        {
            return Some(ancestor_id);
        }
        cur = document.try_node(ancestor_id).and_then(|n| n.parent);
    }
    None
}

pub(crate) fn is_single_valid_email(addr: &str) -> bool {
    let Some((local, domain)) = addr.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return false;
    }
    let local_ok = local
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+/=?^_`{|}~.-".contains(&b));
    if !local_ok {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

pub(crate) fn is_valid_email_value(value: &str, multiple: bool) -> bool {
    if multiple {
        value.split(',').all(|part| {
            let trimmed = part.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r'));
            is_single_valid_email(trimmed)
        })
    } else {
        is_single_valid_email(value)
    }
}

pub(crate) fn is_valid_url_value(value: &str) -> bool {
    if value.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((scheme, rest)) = value.split_once(':') else {
        return false;
    };
    let mut scheme_bytes = scheme.bytes();
    let Some(first) = scheme_bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic()
        || !scheme_bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    {
        return false;
    }
    let lower_scheme = scheme.to_ascii_lowercase();
    if matches!(
        lower_scheme.as_str(),
        "http" | "https" | "ws" | "wss" | "ftp"
    ) {
        let Some(after_slashes) = rest.strip_prefix("//") else {
            return false;
        };
        let host = after_slashes.split(['/', '?', '#']).next().unwrap_or("");
        !host.is_empty()
    } else {
        !rest.is_empty()
    }
}

pub(crate) fn control_text_value(
    document: &megi_core::Document,
    node_id: NodeId,
    element: &megi_core::ElementData,
    idl_value: Option<&str>,
) -> String {
    if element.local_name == "textarea" {
        document.text_content(node_id)
    } else {
        // #1953: a dirty IDL value shadows the `value` content attribute.
        idl_value
            .map(str::to_owned)
            .unwrap_or_else(|| element.get_attribute("value").unwrap_or("").to_string())
    }
}

pub(crate) fn select_selected_value(
    document: &megi_core::Document,
    select_id: NodeId,
    multiple: bool,
) -> Option<String> {
    let options: Vec<(NodeId, &megi_core::ElementData)> = document
        .descendants(select_id)
        .filter_map(|child| {
            document
                .try_node(child)
                .and_then(|node| node.as_element())
                .filter(|el| el.local_name == "option")
                .map(|el| (child, el))
        })
        .collect();
    if options.is_empty() {
        return None;
    }
    let option_value = |opt_id: NodeId, el: &megi_core::ElementData| -> String {
        el.get_attribute("value")
            .map(str::to_owned)
            .unwrap_or_else(|| document.text_content(opt_id))
    };
    if multiple {
        options
            .into_iter()
            .find(|(_, el)| el.has_attribute("selected") && !el.has_attribute("disabled"))
            .map(|(id, el)| option_value(id, el))
    } else if let Some((id, el)) = options
        .iter()
        .copied()
        .find(|(_, el)| el.has_attribute("selected") && !el.has_attribute("disabled"))
    {
        Some(option_value(id, el))
    } else {
        options
            .into_iter()
            .find(|(_, el)| !el.has_attribute("disabled"))
            .map(|(id, el)| option_value(id, el))
    }
}

pub(crate) fn validity_flags_element(
    document: &megi_core::Document,
    node_id: NodeId,
    element: &megi_core::ElementData,
    idl_value: Option<&str>,
    idl_checked: &HashMap<NodeId, bool>,
) -> (ValidityFlags, Option<(String, String)>) {
    if is_control_disabled(document, node_id) {
        return (ValidityFlags::default(), None);
    }
    let tag = element.local_name.as_str();
    if tag == "input"
        && element
            .get_attribute("type")
            .is_some_and(|t| t.eq_ignore_ascii_case("hidden"))
    {
        return (ValidityFlags::default(), None);
    }
    if !matches!(tag, "input" | "textarea" | "select") {
        return (ValidityFlags::default(), None);
    }

    // A custom validity message is stored on the element as a data attribute so no extra
    // host state is needed and it survives snapshots/re-renders.
    let custom_error = element
        .get_attribute("data-megi-validity-error")
        .is_some_and(|message| !message.is_empty());
    let required = element.has_attribute("required");

    let mut flags = ValidityFlags {
        custom_error,
        ..ValidityFlags::default()
    };

    if tag == "select" {
        if required {
            let selected =
                select_selected_value(document, node_id, element.has_attribute("multiple"));
            flags.value_missing = selected.is_none_or(|val| val.is_empty());
        }
        return (flags, None);
    }

    if tag == "textarea" {
        let value = control_text_value(document, node_id, element, idl_value);
        flags.value_missing = required && value.trim().is_empty();
        if !value.is_empty() && !element.has_attribute("readonly") {
            let len = value.encode_utf16().count();
            if let Some(min_len) = element
                .get_attribute("minlength")
                .and_then(|s| s.parse::<usize>().ok())
                && len < min_len
            {
                flags.too_short = true;
            }
            if let Some(max_len) = element
                .get_attribute("maxlength")
                .and_then(|s| s.parse::<usize>().ok())
                && len > max_len
            {
                flags.too_long = true;
            }
        }
        return (flags, None);
    }

    // `<input>`
    let input_type = element
        .get_attribute("type")
        .unwrap_or("text")
        .to_ascii_lowercase();
    if matches!(input_type.as_str(), "button" | "submit" | "reset" | "image") {
        return (
            ValidityFlags {
                custom_error,
                ..ValidityFlags::default()
            },
            None,
        );
    }

    if input_type == "checkbox" {
        flags.value_missing = required
            && !idl_checked
                .get(&node_id)
                .copied()
                .unwrap_or_else(|| element.has_attribute("checked"));
        return (flags, None);
    }

    if input_type == "radio" {
        if required {
            let name = element.get_attribute("name").unwrap_or("");
            let any_checked = if name.is_empty() {
                idl_checked
                    .get(&node_id)
                    .copied()
                    .unwrap_or_else(|| element.has_attribute("checked"))
            } else {
                let scope = enclosing_form(document, node_id).unwrap_or_else(|| document.root());
                document.descendants(scope).any(|cand| {
                    document
                        .try_node(cand)
                        .and_then(|n| n.as_element())
                        .is_some_and(|el| {
                            el.local_name == "input"
                                && el
                                    .get_attribute("type")
                                    .is_some_and(|t| t.eq_ignore_ascii_case("radio"))
                                && el.get_attribute("name") == Some(name)
                                && idl_checked
                                    .get(&cand)
                                    .copied()
                                    .unwrap_or_else(|| el.has_attribute("checked"))
                        })
                })
            };
            flags.value_missing = !any_checked;
        }
        return (flags, None);
    }

    let value = control_text_value(document, node_id, element, idl_value);
    flags.value_missing = required && value.is_empty();

    if value.is_empty() || element.has_attribute("readonly") {
        return (flags, None);
    }

    match input_type.as_str() {
        "email" => {
            if !is_valid_email_value(&value, element.has_attribute("multiple")) {
                flags.type_mismatch = true;
            }
        }
        "url" => {
            if !is_valid_url_value(&value) {
                flags.type_mismatch = true;
            }
        }
        "number" | "range" => {
            if let Ok(num) = value.trim().parse::<f64>()
                && num.is_finite()
            {
                if let Some(min_val) = element
                    .get_attribute("min")
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .filter(|n| n.is_finite())
                    && num < min_val
                {
                    flags.range_underflow = true;
                }
                if let Some(max_val) = element
                    .get_attribute("max")
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .filter(|n| n.is_finite())
                    && num > max_val
                {
                    flags.range_overflow = true;
                }
            }
        }
        _ => {}
    }

    if matches!(
        input_type.as_str(),
        "text" | "search" | "url" | "tel" | "email" | "password"
    ) {
        let len = value.encode_utf16().count();
        if let Some(min_len) = element
            .get_attribute("minlength")
            .and_then(|s| s.parse::<usize>().ok())
            && len < min_len
        {
            flags.too_short = true;
        }
        if let Some(max_len) = element
            .get_attribute("maxlength")
            .and_then(|s| s.parse::<usize>().ok())
            && len > max_len
        {
            flags.too_long = true;
        }
    }

    let pattern_check = if matches!(
        input_type.as_str(),
        "text" | "search" | "url" | "tel" | "email" | "password"
    ) {
        element
            .get_attribute("pattern")
            .map(|pat| (pat.to_string(), value))
    } else {
        None
    };

    (flags, pattern_check)
}

pub(crate) fn test_pattern_in_context(pattern: &str, value: &str, context: &mut Context) -> bool {
    let Ok(func_val) = context
        .global_object()
        .get(js_string!("__megi_test_pattern"), context)
    else {
        return true;
    };
    let Some(func) = func_val.as_callable() else {
        return true;
    };
    func.call(
        &JsValue::undefined(),
        &[
            JsValue::from(js_string!(pattern)),
            JsValue::from(js_string!(value)),
        ],
        context,
    )
    .map(|v| v.to_boolean())
    .unwrap_or(true)
}
