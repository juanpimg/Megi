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
        let document = &host.document;
        let element = document
            .try_node(node_id)
            .and_then(|node| node.as_element())?;
        if element.local_name == "textarea" {
            return Some(document.text_content(node_id));
        }
        if element.local_name == "select" {
            let selected = document
                .descendants(node_id)
                .filter_map(|child| document.try_node(child).and_then(|node| node.as_element()))
                .find(|option| {
                    option.local_name == "option" && option.get_attribute("selected").is_some()
                });
            if let Some(option) = selected {
                if let Some(value) = option.get_attribute("value") {
                    return Some(value.to_string());
                }
                if let Some(option_id) = document.descendants(node_id).find(|child| {
                    document
                        .try_node(*child)
                        .and_then(|node| node.as_element())
                        .is_some_and(|candidate| {
                            candidate.local_name == "option"
                                && candidate.get_attribute("selected").is_some()
                        })
                }) {
                    return Some(document.text_content(option_id));
                }
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
    value_missing: bool,
    custom_error: bool,
    pattern_mismatch: bool,
    type_mismatch: bool,
    too_short: bool,
    too_long: bool,
    range_underflow: bool,
    range_overflow: bool,
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
) -> String {
    if element.local_name == "textarea" {
        document.text_content(node_id)
    } else {
        element.get_attribute("value").unwrap_or("").to_string()
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
        let value = control_text_value(document, node_id, element);
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
        flags.value_missing = required && !element.has_attribute("checked");
        return (flags, None);
    }

    if input_type == "radio" {
        if required {
            let name = element.get_attribute("name").unwrap_or("");
            let any_checked = if name.is_empty() {
                element.has_attribute("checked")
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
                                && el.has_attribute("checked")
                        })
                })
            };
            flags.value_missing = !any_checked;
        }
        return (flags, None);
    }

    let value = control_text_value(document, node_id, element);
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

pub(crate) fn validity_flags(node_id: NodeId, context: &mut Context) -> ValidityFlags {
    let computed = with_context_host(context, |host| {
        let document = &host.document;
        let element = document
            .try_node(node_id)
            .and_then(|node| node.as_element())?;
        Some(validity_flags_element(document, node_id, element))
    })
    .flatten();
    let Some((mut flags, pattern_check)) = computed else {
        return ValidityFlags::default();
    };
    if let Some((pattern, value)) = pattern_check
        && !test_pattern_in_context(&pattern, &value, context)
    {
        flags.pattern_mismatch = true;
    }
    flags
}

pub(crate) fn validate_control_and_fire_invalid(
    node_id: NodeId,
    wrapper: &JsValue,
    context: &mut Context,
) -> JsResult<bool> {
    let flags = validity_flags(node_id, context);
    if flags.is_valid() {
        Ok(true)
    } else {
        let _ = crate::events::dispatch_event_type(wrapper, "invalid", false, true, context)?;
        Ok(false)
    }
}

pub(crate) fn form_descendant_controls(
    document: &megi_core::Document,
    form_id: NodeId,
) -> Vec<NodeId> {
    document
        .descendants(form_id)
        .filter(|child| {
            document
                .try_node(*child)
                .and_then(|node| node.as_element())
                .is_some_and(|el| matches!(el.local_name.as_str(), "input" | "textarea" | "select"))
        })
        .collect()
}

pub(crate) fn validate_node_or_form(
    this: &JsValue,
    node_id: NodeId,
    context: &mut Context,
) -> JsResult<bool> {
    let is_form = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .is_some_and(|el| el.local_name == "form")
    })
    .unwrap_or(false);
    if !is_form {
        return validate_control_and_fire_invalid(node_id, this, context);
    }
    let controls = with_context_host(context, |host| {
        form_descendant_controls(&host.document, node_id)
    })
    .unwrap_or_default();
    let mut all_valid = true;
    for control_id in controls {
        let wrapper = wrap_node(context, control_id)?;
        if !validate_control_and_fire_invalid(control_id, &wrapper, context)? {
            all_valid = false;
        }
    }
    Ok(all_valid)
}

pub(crate) fn node_check_validity(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let valid = validate_node_or_form(this, node_id, context)?;
    Ok(JsValue::from(valid))
}

pub(crate) fn node_report_validity(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let valid = validate_node_or_form(this, node_id, context)?;
    Ok(JsValue::from(valid))
}

pub(crate) fn node_set_custom_validity(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let message = arg_string(args, 0, context)?;
    with_context_host(context, |host| {
        let document = &mut host.document;
        let Some(node) = document.try_node_mut(node_id) else {
            return;
        };
        let Some(element) = node.as_element_mut() else {
            return;
        };
        if message.is_empty() {
            element
                .attributes
                .retain(|attr| attr.local_name != "data-megi-validity-error");
        } else {
            element.set_attribute(
                megi_core::Attribute::new("data-megi-validity-error", &message),
                true,
            );
        }
    });
    Ok(JsValue::undefined())
}

pub(crate) fn node_validity_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let flags = validity_flags(node_id, context);
    let valid = flags.is_valid();
    let object = ObjectInitializer::new(context).build();
    for (name, value) in [
        ("valid", valid),
        ("valueMissing", flags.value_missing),
        ("customError", flags.custom_error),
        ("patternMismatch", flags.pattern_mismatch),
        ("typeMismatch", flags.type_mismatch),
        ("tooShort", flags.too_short),
        ("tooLong", flags.too_long),
        ("rangeUnderflow", flags.range_underflow),
        ("rangeOverflow", flags.range_overflow),
        ("stepMismatch", false),
        ("badInput", false),
    ] {
        let _ = object.set(js_string!(name), JsValue::from(value), false, context);
    }
    Ok(JsValue::from(object))
}

pub(crate) fn node_request_submit(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if !with_context_host(context, |host| host.allow_forms).unwrap_or(true) {
        return Ok(JsValue::from(false));
    }
    // `requestSubmit(submitter?)` runs constraint validation unless suppressed by `novalidate`
    // or submitter `formnovalidate`, and fires `submit` if valid (#1505, #1538).
    let node_id = this_node(this, context)?;
    let submitter_arg = args.first().cloned().unwrap_or_else(JsValue::undefined);
    let submitter_id = if !submitter_arg.is_undefined() && !submitter_arg.is_null() {
        Some(this_node(&submitter_arg, context)?)
    } else {
        None
    };
    let skip_validation = with_context_host(context, |host| {
        let document = &host.document;
        let form_novalidate = document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .is_some_and(|el| el.has_attribute("novalidate"));
        let submitter_novalidate = submitter_id.is_some_and(|id| {
            document
                .try_node(id)
                .and_then(|n| n.as_element())
                .is_some_and(|el| el.has_attribute("formnovalidate"))
        });
        form_novalidate || submitter_novalidate
    })
    .unwrap_or(false);

    if !skip_validation && !validate_node_or_form(this, node_id, context)? {
        return Ok(JsValue::from(false));
    }
    let submitter = if submitter_id.is_some() {
        submitter_arg
    } else {
        JsValue::null()
    };
    let dispatched = crate::events::dispatch_submit_event(this, submitter, context)?;
    Ok(JsValue::from(dispatched))
}

pub(crate) fn node_submit(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    if !with_context_host(context, |host| host.allow_forms).unwrap_or(true) {
        return Ok(JsValue::from(false));
    }
    // `form.submit()` bypasses constraint validation (#1505).
    let dispatched = crate::events::dispatch_submit_event(this, JsValue::null(), context)?;
    Ok(JsValue::from(dispatched))
}

pub(crate) fn node_reset(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let is_form = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|n| n.as_element())
            .is_some_and(|el| el.local_name == "form")
    })
    .unwrap_or(false);
    if !is_form {
        return Ok(JsValue::undefined());
    }
    let dispatched = crate::events::dispatch_event_type(this, "reset", true, true, context)?;
    if !dispatched {
        return Ok(JsValue::undefined());
    }
    with_context_host(context, |host| {
        let descendants: Vec<NodeId> = host.document.descendants(node_id).collect();
        for child_id in descendants {
            let tag = host
                .document
                .try_node(child_id)
                .and_then(|n| n.as_element())
                .map(|el| el.local_name.clone());
            let Some(tag) = tag else { continue };
            match tag.as_str() {
                "input" => {
                    if let Some(orig) = host.initial_control_values.remove(&child_id)
                        && let Some(el) = host
                            .document
                            .try_node_mut(child_id)
                            .and_then(|n| n.as_element_mut())
                    {
                        match orig {
                            Some(v) => {
                                el.set_attribute(DomAttribute::new("value", v), true);
                            }
                            None => {
                                el.remove_attribute("value");
                            }
                        }
                    }
                    if let Some(orig_checked) = host.initial_control_checked.remove(&child_id)
                        && let Some(el) = host
                            .document
                            .try_node_mut(child_id)
                            .and_then(|n| n.as_element_mut())
                    {
                        if orig_checked {
                            el.set_attribute(DomAttribute::new("checked", ""), true);
                        } else {
                            el.remove_attribute("checked");
                        }
                    }
                    if let Some(el) = host
                        .document
                        .try_node_mut(child_id)
                        .and_then(|n| n.as_element_mut())
                    {
                        el.attributes
                            .retain(|attr| attr.local_name != "data-megi-validity-error");
                    }
                }
                "textarea" => {
                    if let Some(orig) = host.initial_control_values.remove(&child_id) {
                        let _ = host
                            .document
                            .set_text_content(child_id, &orig.unwrap_or_default());
                    }
                    if let Some(el) = host
                        .document
                        .try_node_mut(child_id)
                        .and_then(|n| n.as_element_mut())
                    {
                        el.attributes
                            .retain(|attr| attr.local_name != "data-megi-validity-error");
                    }
                }
                "option" => {
                    if let Some(orig_selected) = host.initial_control_checked.remove(&child_id)
                        && let Some(el) = host
                            .document
                            .try_node_mut(child_id)
                            .and_then(|n| n.as_element_mut())
                    {
                        if orig_selected {
                            el.set_attribute(DomAttribute::new("selected", ""), true);
                        } else {
                            el.remove_attribute("selected");
                        }
                    }
                }
                "select" => {
                    if let Some(el) = host
                        .document
                        .try_node_mut(child_id)
                        .and_then(|n| n.as_element_mut())
                    {
                        el.attributes
                            .retain(|attr| attr.local_name != "data-megi-validity-error");
                    }
                }
                _ => {}
            }
        }
    });
    Ok(JsValue::undefined())
}

/// Sets/clears the `open` attribute on a dialog element (#1546).
pub(crate) fn dialog_open(this: &JsValue, open: bool, context: &mut Context) -> JsResult<()> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut());
        if let Some(element) = element {
            if open {
                element.set_attribute(DomAttribute::new("open", ""), true);
            } else {
                element.attributes.retain(|attr| attr.local_name != "open");
            }
        }
    });
    Ok(())
}

/// `HTMLDialogElement.show()` / `showModal()` (#1546). The engine has no separate top-layer yet,
/// so both reveal the dialog by setting `open`; `showModal()` additionally records the modal
/// state on the element so scripts and the shell can observe which dialog is modal.
pub(crate) fn node_dialog_show(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    dialog_open(this, true, context)?;
    let _ = crate::events::dispatch_event_type(this, "toggle", false, false, context);
    Ok(JsValue::undefined())
}

pub(crate) fn node_dialog_show_modal(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    dialog_open(this, true, context)?;
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(DomAttribute::new("data-megi-modal", ""), true);
        }
    });
    let _ = crate::events::dispatch_event_type(this, "toggle", false, false, context);
    Ok(JsValue::undefined())
}

/// `HTMLDialogElement.close(returnValue)` (#1546): hides the dialog, stores its return value and
/// fires a `close` event.
pub(crate) fn node_dialog_close(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let return_value = args
        .first()
        .map(|value| {
            value
                .to_string(context)
                .map(|text| text.to_std_string_escaped())
        })
        .transpose()
        .map_err(|error| js_error(error.to_string()))?
        .unwrap_or_default();
    dialog_open(this, false, context)?;
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element.set_attribute(
                DomAttribute::new("data-megi-return-value", &return_value),
                true,
            );
            element
                .attributes
                .retain(|attr| attr.local_name != "data-megi-modal");
        }
    });
    let _ = crate::events::dispatch_event_type(this, "close", false, false, context);
    Ok(JsValue::undefined())
}

pub(crate) fn node_dialog_return_value_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| {
                element
                    .get_attribute("data-megi-return-value")
                    .map(str::to_owned)
            })
    })
    .flatten()
    .unwrap_or_default();
    Ok(JsValue::from(js_string!(value)))
}

pub(crate) fn node_value_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let value = arg_string(args, 0, context)?;
    let result = with_context_host(context, |host| {
        let tag = host
            .document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .map(|element| element.local_name.clone())
            .ok_or_else(|| "node is not an element".to_string())?;
        if tag == "textarea" {
            if !host.initial_control_values.contains_key(&node_id) {
                let initial = host.document.text_content(node_id);
                host.initial_control_values.insert(node_id, Some(initial));
            }
            host.document
                .set_text_content(node_id, &value)
                .map_err(|error| error.to_string())?;
            return Ok::<(), String>(());
        }
        if tag == "select" {
            let options = option_ids(&host.document, node_id);
            for option_id in options {
                let opt_val = host
                    .document
                    .try_node(option_id)
                    .and_then(|n| n.as_element())
                    .map(|el| {
                        el.get_attribute("value")
                            .map(str::to_owned)
                            .unwrap_or_else(|| host.document.text_content(option_id))
                    })
                    .unwrap_or_default();
                let matches = opt_val == value;
                if !host.initial_control_checked.contains_key(&option_id) {
                    let was_selected = host
                        .document
                        .try_node(option_id)
                        .and_then(|n| n.as_element())
                        .is_some_and(|el| el.get_attribute("selected").is_some());
                    host.initial_control_checked.insert(option_id, was_selected);
                }
                if let Some(el) = host
                    .document
                    .try_node_mut(option_id)
                    .and_then(|n| n.as_element_mut())
                {
                    if matches {
                        el.set_attribute(DomAttribute::new("selected", ""), true);
                    } else {
                        el.remove_attribute("selected");
                    }
                }
            }
            return Ok(());
        }
        if !host.initial_control_values.contains_key(&node_id) {
            let initial = host
                .document
                .try_node(node_id)
                .and_then(|node| node.as_element())
                .and_then(|el| el.get_attribute("value").map(str::to_owned));
            host.initial_control_values.insert(node_id, initial);
        }
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        element.set_attribute(DomAttribute::new("value", value.clone()), true);
        Ok(())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn boolean_attribute_get(
    this: &JsValue,
    name: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let present = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .is_some_and(|element| element.get_attribute(name).is_some())
    })
    .unwrap_or(false);
    Ok(JsValue::from(present))
}

pub(crate) fn boolean_attribute_set(
    this: &JsValue,
    name: &str,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let enabled = args.first().is_some_and(JsValue::to_boolean);
    let result = with_context_host(context, |host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        if enabled {
            element.set_attribute(DomAttribute::new(name, ""), true);
        } else {
            element.remove_attribute(name);
        }
        Ok::<(), String>(())
    });
    match result {
        Some(Ok(())) => Ok(JsValue::undefined()),
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn node_open_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "open", context)
}

pub(crate) fn node_open_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_set(this, "open", args, context)
}

pub(crate) fn node_checked_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "checked", context)
}

pub(crate) fn node_checked_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if !host.initial_control_checked.contains_key(&node_id) {
            let initial = host
                .document
                .try_node(node_id)
                .and_then(|n| n.as_element())
                .is_some_and(|el| el.get_attribute("checked").is_some());
            host.initial_control_checked.insert(node_id, initial);
        }
    });
    boolean_attribute_set(this, "checked", args, context)
}

pub(crate) fn node_disabled_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "disabled", context)
}

pub(crate) fn node_disabled_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_set(this, "disabled", args, context)
}

pub(crate) fn node_selected_get(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    boolean_attribute_get(this, "selected", context)
}

pub(crate) fn node_selected_set(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    with_context_host(context, |host| {
        if !host.initial_control_checked.contains_key(&node_id) {
            let initial = host
                .document
                .try_node(node_id)
                .and_then(|n| n.as_element())
                .is_some_and(|el| el.get_attribute("selected").is_some());
            host.initial_control_checked.insert(node_id, initial);
        }
    });
    boolean_attribute_set(this, "selected", args, context)
}

pub(crate) fn node_options(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let options: Vec<NodeId> = with_context_host(context, |host| {
        host.document
            .descendants(node_id)
            .filter(|child| {
                host.document
                    .try_node(*child)
                    .and_then(|node| node.as_element())
                    .is_some_and(|element| element.local_name == "option")
            })
            .collect()
    })
    .unwrap_or_default();
    node_list(context, options)
}

pub(crate) fn node_set_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let value = arg_string(args, 1, context)?;
    let old_value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    let result = with_context_host(context, |host| {
        let element = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
            .ok_or_else(|| "node is not an element".to_string())?;
        element.set_attribute(DomAttribute::new(name.clone(), value), true);
        if matches!(name.as_str(), "src" | "srcdoc" | "sandbox") {
            invalidate_iframe_state(host, node_id);
        }
        Ok::<(), String>(())
    });
    match result {
        Some(Ok(())) => {
            note_mutation(
                MutationKind::Attributes,
                node_id,
                Some(name),
                old_value,
                context,
            );
            Ok(JsValue::undefined())
        }
        Some(Err(message)) => Err(js_error(message)),
        None => Err(js_error("no active JavaScript host")),
    }
}

pub(crate) fn node_get_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute(&name))
            })
            .map(str::to_owned)
    })
    .flatten();
    match value {
        Some(value) => Ok(JsValue::from(js_string!(value))),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn node_remove_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let old_value = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| node.as_element())
            .and_then(|element| element.get_attribute(&name).map(str::to_owned))
    })
    .flatten();
    with_context_host(context, |host| {
        if let Some(element) = host
            .document
            .try_node_mut(node_id)
            .and_then(|node| node.as_element_mut())
        {
            element
                .attributes
                .retain(|attribute| attribute.local_name != name);
        }
        if matches!(name.as_str(), "src" | "srcdoc" | "sandbox") {
            invalidate_iframe_state(host, node_id);
        }
    });
    note_mutation(
        MutationKind::Attributes,
        node_id,
        Some(name),
        old_value,
        context,
    );
    Ok(JsValue::undefined())
}

pub(crate) fn node_has_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let name = attribute_name(node_id, &arg_string(args, 0, context)?, context);
    let has = with_context_host(context, |host| {
        host.document
            .try_node(node_id)
            .and_then(|node| {
                node.as_element()
                    .and_then(|element| element.get_attribute(&name))
            })
            .is_some()
    })
    .unwrap_or(false);
    Ok(JsValue::from(has))
}

/// Extracts a child argument (wrapper → node id).
pub(crate) fn argument_node(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<NodeId> {
    let value = args.get(index).cloned().unwrap_or_else(JsValue::undefined);
    let object = value
        .as_object()
        .ok_or_else(|| js_error("expected a DOM node argument"))?;
    let handle = object
        .get(js_string!("__megi_node"), context)
        .map_err(|error| js_error(error.to_string()))?;
    let handle = handle
        .as_number()
        .filter(|number| *number >= 0.0 && number.fract() == 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("expected a DOM node argument"))?;
    with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

pub(crate) fn node_clone_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node_id = this_node(this, context)?;
    let deep = args.first().and_then(JsValue::as_boolean).unwrap_or(false);
    // `cloneNode(true)` doubles the arena in one call, so the subtree size is charged before
    // the clone is allocated (#657).
    let allowed = with_context_host(context, |host| {
        let extra = if deep {
            host.document.descendants(node_id).count().max(1)
        } else {
            1
        };
        let total = host.document.len().saturating_add(extra);
        if total > host.max_dom_nodes {
            if !host.dom_budget_warned {
                host.dom_budget_warned = true;
                host.push_console(ConsoleEntry {
                    level: "warn".to_string(),
                    message: format!(
                        "cloneNode rejected: the DOM node budget ({} nodes) is exhausted",
                        host.max_dom_nodes
                    ),
                });
            }
            return false;
        }
        true
    })
    .unwrap_or(false);
    if !allowed {
        return Err(js_error(format!(
            "QuotaExceededError: DOM node budget exceeded ({} nodes)",
            with_context_host(context, |host| host.max_dom_nodes).unwrap_or(0)
        )));
    }
    let clone = with_context_host(context, |host| host.document.clone_node(node_id, deep))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .ok_or_else(|| js_error("NotFoundError: node not found"))?;
    wrap_node(context, clone)
}

pub(crate) fn node_append_child(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let child = argument_node(args, 0, context)?;
    // DOM insertion expands document fragments (their children are moved, the fragment
    // itself is never attached).
    let appended = with_context_host(context, |host| {
        host.document.insert_node(parent, child, None).is_ok()
    })
    .unwrap_or(false);
    if !appended {
        return Err(js_error("appendChild failed (hierarchy or unknown node)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, child)
}

pub(crate) fn node_insert_before(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let new_node = argument_node(args, 0, context)?;
    // `insertBefore(node, null)` (or a missing reference) appends, per DOM.
    let reference = match args.get(1) {
        Some(value) if !value.is_null_or_undefined() => Some(argument_node(args, 1, context)?),
        _ => None,
    };
    if reference == Some(new_node) {
        // DOM spec: inserting a node before itself is a no-op.
        return wrap_node(context, new_node);
    }
    let inserted = with_context_host(context, |host| {
        host.document
            .insert_node(parent, new_node, reference)
            .is_ok()
    })
    .unwrap_or(false);
    if !inserted {
        return Err(js_error("insertBefore failed (hierarchy or unknown node)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, new_node)
}

pub(crate) fn node_remove_child(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let child = argument_node(args, 0, context)?;
    let removed = with_context_host(context, |host| {
        host.document.remove_child(parent, child).is_ok()
    })
    .unwrap_or(false);
    if !removed {
        return Err(js_error("removeChild failed (node is not a child)"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, child)
}

pub(crate) fn node_replace_child(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let parent = this_node(this, context)?;
    let new_node = argument_node(args, 0, context)?;
    let old_node = argument_node(args, 1, context)?;
    let replaced = with_context_host(context, |host| {
        host.document
            .replace_node(parent, new_node, old_node)
            .is_ok()
    })
    .unwrap_or(false);
    if !replaced {
        return Err(js_error("replaceChild failed"));
    }
    note_mutation(MutationKind::ChildList, parent, None, None, context);
    wrap_node(context, old_node)
}
