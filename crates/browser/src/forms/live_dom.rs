//! O(1) single-node DOM mutators for live form editing without whole-tree scans (#2199).

use crate::forms::{
    ACTIVE_ATTR, Attribute, Document, FOCUS_ATTR, FormControl, FormControlKind, NodeId,
    option_nodes, set_text_content,
};

/// Sets or clears a boolean marker attribute on a single element node without scanning the DOM.
fn set_bool_attr_in_place(
    document: &mut Document,
    node_id: NodeId,
    attr: &str,
    wanted: bool,
) -> bool {
    let Some(node) = document.try_node_mut(node_id) else {
        return false;
    };
    let Some(element) = node.as_element_mut() else {
        return false;
    };
    let has = element.get_attribute(attr).is_some();
    if wanted && !has {
        element.set_attribute(Attribute::new(attr, ""), true);
        true
    } else if !wanted && has {
        element.attributes.retain(|a| a.local_name != attr);
        true
    } else {
        false
    }
}

/// Clears `data-megi-focused` and/or `data-megi-active` on `node_id` in O(1).
pub fn clear_node_markers(
    document: &mut Document,
    node_id: NodeId,
    clear_focus: bool,
    clear_active: bool,
) -> bool {
    let mut changed = false;
    if clear_focus {
        changed |= set_bool_attr_in_place(document, node_id, FOCUS_ATTR, false);
    }
    if clear_active {
        changed |= set_bool_attr_in_place(document, node_id, ACTIVE_ATTR, false);
    }
    changed
}

/// Mutates a single control element `node_id` in-place in O(1) without traversing the DOM.
pub fn patch_control_node(
    document: &mut Document,
    node_id: NodeId,
    value: Option<&str>,
    checked: Option<bool>,
    focus: bool,
    active: bool,
) -> bool {
    let (changed, is_textarea) = {
        let Some(node) = document.try_node_mut(node_id) else {
            return false;
        };
        let Some(element) = node.as_element_mut() else {
            return false;
        };
        let mut changed = false;
        if let Some(val) = value
            && element.get_attribute("value") != Some(val)
        {
            element.set_attribute(Attribute::new("value", val), true);
            changed = true;
        }
        if let Some(is_checked) = checked {
            let has = element.get_attribute("checked").is_some();
            if has != is_checked {
                if is_checked {
                    element.set_attribute(Attribute::new("checked", ""), true);
                } else {
                    element.attributes.retain(|a| a.local_name != "checked");
                }
                changed = true;
            }
        }
        for (attr, wanted) in [(FOCUS_ATTR, focus), (ACTIVE_ATTR, active)] {
            let has = element.get_attribute(attr).is_some();
            if wanted && !has {
                element.set_attribute(Attribute::new(attr, ""), true);
                changed = true;
            } else if !wanted && has {
                element.attributes.retain(|a| a.local_name != attr);
                changed = true;
            }
        }
        (changed, element.local_name == "textarea")
    };
    let mut total_changed = changed;
    if is_textarea && let Some(val) = value {
        total_changed |= set_text_content(document, node_id, val);
    }
    total_changed
}

fn apply_option_selection_nodes<F>(
    document: &mut Document,
    select_id: NodeId,
    is_selected: F,
) -> bool
where
    F: Fn(usize) -> bool,
{
    let options = option_nodes(document, select_id);
    if options.is_empty() {
        return false;
    }
    let mut changed = false;
    for (index, node_id) in options.into_iter().enumerate() {
        changed |= set_bool_attr_in_place(document, node_id, "selected", is_selected(index));
    }
    changed
}

/// Updates `<option selected>` attributes directly under `select_id` (O(options), no DOM scan).
pub fn patch_select_node(document: &mut Document, select_id: NodeId, option_index: usize) -> bool {
    let options = option_nodes(document, select_id);
    if option_index >= options.len() {
        return false;
    }
    let mut changed = false;
    for (index, node_id) in options.into_iter().enumerate() {
        changed |= set_bool_attr_in_place(document, node_id, "selected", index == option_index);
    }
    changed
}

/// Updates `<option selected>` attributes directly under multi-select `select_id` (O(options)).
pub fn patch_multi_select_node(
    document: &mut Document,
    select_id: NodeId,
    selected: &[usize],
) -> bool {
    apply_option_selection_nodes(document, select_id, |index| selected.contains(&index))
}

/// Display label shown locally inside a native form control overlay.
pub fn control_display_text(control: &FormControl) -> &str {
    if control.kind == FormControlKind::Select {
        if let Some(selected) = control.options.iter().find(|opt| opt.selected) {
            return if selected.label.is_empty() {
                selected.value.as_str()
            } else {
                selected.label.as_str()
            };
        }
        if let Some(first) = control.options.first() {
            return if first.label.is_empty() {
                first.value.as_str()
            } else {
                first.label.as_str()
            };
        }
    }
    control.value.as_str()
}
