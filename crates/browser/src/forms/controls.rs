use super::*;

/// Assigns a stable `data-megi-form-id` to every form control in document order.
///
/// Controls that already carry an id keep it (so ids survive the serialize → parse cycle);
/// returns the next free id.
pub fn assign_form_ids(document: &mut Document) -> u32 {
    let ids = element_ids(document);
    let mut used: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut next = 0u32;
    for node_id in ids {
        let Some(element) = document.node(node_id).as_element() else {
            continue;
        };
        let tag = element.local_name.clone();
        let type_attr = element.get_attribute("type").map(str::to_owned);
        if FormControlKind::from_element(&tag, type_attr.as_deref()).is_none() {
            continue;
        }
        // Page content controls these attributes: an id of `u32::MAX` used to overflow
        // `id + 1`, and duplicate ids made controls ambiguous. Keep a usable existing id
        // (unique so far) and renumber the rest from a counter that skips used ids.
        let existing = element
            .get_attribute(FORM_ID_ATTR)
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|id| !used.contains(id));
        let _id = match existing {
            Some(id) => {
                used.insert(id);
                next = next.max(id.saturating_add(1));
                id
            }
            None => {
                let id = loop {
                    let candidate = next;
                    next = next.checked_add(1).unwrap_or(0);
                    if used.insert(candidate) {
                        break candidate;
                    }
                };
                let element = document
                    .node_mut(node_id)
                    .as_element_mut()
                    .expect("element node");
                element.set_attribute(Attribute::new(FORM_ID_ATTR, id.to_string()), true);
                id
            }
        };
    }
    // The counter may still sit on a used id (existing ids advance it, and it wraps at
    // `u32::MAX`); return a genuinely free one.
    while used.contains(&next) {
        next = next.checked_add(1).unwrap_or(0);
    }
    next
}

/// Returns the ids of every element node in document order (iterative traversal).
pub(crate) fn element_ids(document: &Document) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![document.root()];
    while let Some(node_id) = stack.pop() {
        let node = document.node(node_id);
        if node.as_element().is_some() {
            out.push(node_id);
        }
        if let Some(sibling) = node.next_sibling {
            stack.push(sibling);
        }
        if let Some(child) = node.first_child {
            stack.push(child);
        }
    }
    out
}

pub(crate) fn text_content(document: &Document, node_id: NodeId) -> String {
    let mut out = String::new();
    let mut stack = Vec::new();
    if let Some(child) = document.node(node_id).first_child {
        stack.push(child);
    }
    while let Some(id) = stack.pop() {
        let node = document.node(id);
        if let megi_webview::core::NodeKind::Text(text) = &node.kind {
            out.push_str(text);
        }
        if let Some(sibling) = node.next_sibling {
            stack.push(sibling);
        }
        if let Some(child) = node.first_child {
            stack.push(child);
        }
    }
    out.trim().to_owned()
}

/// Extracts the rendered form controls from `layout` + `document` (painting order).
pub fn collect_controls(document: &Document, layout: &LayoutBox) -> Vec<FormControl> {
    let mut controls = Vec::new();
    let mut stack: Vec<&LayoutBox> = vec![layout];
    while let Some(layout_box) = stack.pop() {
        if let Some(node_id) = layout_box.node_id()
            && let Some(element) = document.node(node_id).as_element()
            && let Some(kind) =
                FormControlKind::from_element(&element.local_name, element.get_attribute("type"))
        {
            let rect = layout_box.dimensions.border_box();
            if rect.width > 0.0 && rect.height > 0.0 {
                let form_id = element
                    .get_attribute(FORM_ID_ATTR)
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(u32::MAX);
                let (value, options) = match kind {
                    FormControlKind::Textarea => (text_content(document, node_id), Vec::new()),
                    FormControlKind::Select => {
                        let options = select_options(document, node_id);
                        let value = options
                            .iter()
                            .find(|option| option.selected)
                            .or_else(|| options.first())
                            .map(|option| option.label.clone())
                            .unwrap_or_default();
                        (value, options)
                    }
                    _ => (
                        element.get_attribute("value").unwrap_or("").to_owned(),
                        Vec::new(),
                    ),
                };
                controls.push(FormControl {
                    form_id,
                    name: element.get_attribute("name").unwrap_or("").to_owned(),
                    node_id,
                    kind,
                    rect: CssRect::new(rect.x, rect.y, rect.width, rect.height),
                    value,
                    checked: element.get_attribute("checked").is_some(),
                    disabled: element.get_attribute("disabled").is_some(),
                    options,
                    multiple: element.get_attribute("multiple").is_some(),
                });
            }
        }
        for child in layout_box.children.iter().rev() {
            stack.push(child);
        }
    }
    controls
}

/// Returns the topmost control containing page point `(x, y)`.
pub fn control_at(controls: &[FormControl], x: f32, y: f32) -> Option<usize> {
    controls
        .iter()
        .rposition(|control| control.rect.contains(x, y))
}

/// Returns the enclosing `<optgroup label>` of an `<option>` (group header label).
pub(crate) fn option_group_label(document: &Document, option_id: NodeId) -> Option<String> {
    let mut parent = document.node(option_id).parent;
    while let Some(node_id) = parent {
        let node = document.node(node_id);
        if let Some(element) = node.as_element() {
            if element.local_name == "optgroup" {
                return element
                    .get_attribute("label")
                    .map(str::to_owned)
                    .filter(|label| !label.trim().is_empty());
            }
            if element.local_name == "select" {
                return None;
            }
        }
        parent = node.parent;
    }
    None
}

/// Collects the `<option>` descendants of a `<select>` in document order.
pub(crate) fn select_options(document: &Document, select_id: NodeId) -> Vec<SelectOption> {
    let mut options = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    let mut child = document.node(select_id).last_child;
    while let Some(id) = child {
        stack.push(id);
        child = document.node(id).prev_sibling;
    }
    while let Some(node_id) = stack.pop() {
        let node = document.node(node_id);
        if let Some(element) = node.as_element()
            && element.local_name == "option"
        {
            let label = text_content(document, node_id);
            let value = element.get_attribute("value").unwrap_or(&label).to_owned();
            let group = option_group_label(document, node_id);
            options.push(
                SelectOption::new(label, value, element.get_attribute("selected").is_some())
                    .with_group(group),
            );
        }
        let mut child = node.last_child;
        while let Some(id) = child {
            stack.push(id);
            child = document.node(id).prev_sibling;
        }
    }
    // A `<select>` with no explicit selected option displays (and selects) the first one.
    let any_selected = options.iter().any(|option| option.selected);
    if !any_selected && let Some(first) = options.first_mut() {
        first.selected = true;
    }
    options
}

/// Selected option indices of a multi-select control.
pub fn selected_indices(control: &FormControl) -> Vec<usize> {
    control
        .options
        .iter()
        .enumerate()
        .filter_map(|(index, option)| option.selected.then_some(index))
        .collect()
}

/// Applies a plain/Ctrl/Shift click to a multi-select list, returning the new selected
/// indices (Ctrl toggles, Shift extends from `anchor`, plain replaces the selection).
pub fn multi_click(
    options: &[SelectOption],
    current: &[usize],
    anchor: Option<usize>,
    index: usize,
    ctrl: bool,
    shift: bool,
) -> Vec<usize> {
    if index >= options.len() {
        return current.to_vec();
    }
    if shift {
        let from = anchor.unwrap_or(index).min(index);
        let to = anchor.unwrap_or(index).max(index);
        return (from..=to).collect();
    }
    if ctrl {
        let mut selected: Vec<usize> = current.to_vec();
        if let Some(position) = selected.iter().position(|value| *value == index) {
            selected.remove(position);
        } else {
            selected.push(index);
            selected.sort_unstable();
        }
        return selected;
    }
    vec![index]
}

/// Writes a multi-select selection to the DOM (`selected` attributes on the chosen options;
/// every other option is cleared). Returns whether the document changed.
pub fn patch_multi_select(document: &mut Document, form_id: u32, selected: &[usize]) -> bool {
    let ids = element_ids(document);
    let target = ids.iter().copied().find(|node_id| {
        document
            .node(*node_id)
            .as_element()
            .and_then(|element| element.get_attribute(FORM_ID_ATTR))
            .and_then(|value| value.parse::<u32>().ok())
            == Some(form_id)
    });
    let Some(select_id) = target else {
        return false;
    };
    let options = option_nodes(document, select_id);
    let mut changed = false;
    for (index, node_id) in options.into_iter().enumerate() {
        let element = document
            .node(node_id)
            .as_element()
            .expect("option node")
            .clone();
        let has = element.get_attribute("selected").is_some();
        let wanted = selected.contains(&index);
        if has != wanted {
            let mut updated = element;
            if wanted {
                updated.set_attribute(Attribute::new("selected", ""), true);
            } else {
                updated
                    .attributes
                    .retain(|attr| attr.local_name != "selected");
            }
            *document
                .node_mut(node_id)
                .as_element_mut()
                .expect("element") = updated;
            changed = true;
        }
    }
    changed
}

/// Row index of a multi-select list at control-relative `y` (rows divide the box evenly).
pub fn multi_option_at(control: &FormControl, y_relative: f32) -> Option<usize> {
    if !control.multiple || control.options.is_empty() {
        return None;
    }
    let height = control.rect.height.max(1.0);
    let row = (y_relative / (height / control.options.len() as f32)).floor();
    if row < 0.0 || row as usize >= control.options.len() {
        None
    } else {
        Some(row as usize)
    }
}

/// Marks option `option_index` as selected inside the `<select>` with `form_id`.
///
/// Clears every previous `selected` attribute on its options and sets it on the chosen one
/// (the renderer re-lays out the snapshot and the dropdown reads the new state). Returns
/// whether the document changed.
pub fn patch_select(document: &mut Document, form_id: u32, option_index: usize) -> bool {
    let ids = element_ids(document);
    let target = ids.iter().copied().find(|node_id| {
        document
            .node(*node_id)
            .as_element()
            .and_then(|element| element.get_attribute(FORM_ID_ATTR))
            .and_then(|value| value.parse::<u32>().ok())
            == Some(form_id)
    });
    let Some(select_id) = target else {
        return false;
    };
    let options = option_nodes(document, select_id);
    if option_index >= options.len() {
        return false;
    }
    let mut changed = false;
    for (index, node_id) in options.into_iter().enumerate() {
        let element = document
            .node(node_id)
            .as_element()
            .expect("option node")
            .clone();
        let has = element.get_attribute("selected").is_some();
        let wanted = index == option_index;
        if has != wanted {
            let mut updated = element;
            if wanted {
                updated.set_attribute(Attribute::new("selected", ""), true);
            } else {
                updated
                    .attributes
                    .retain(|attr| attr.local_name != "selected");
            }
            *document
                .node_mut(node_id)
                .as_element_mut()
                .expect("element") = updated;
            changed = true;
        }
    }
    changed
}

/// `<option>` element ids under `select_id`, in document order.
pub(crate) fn option_nodes(document: &Document, select_id: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    let mut child = document.node(select_id).last_child;
    while let Some(id) = child {
        stack.push(id);
        child = document.node(id).prev_sibling;
    }
    while let Some(node_id) = stack.pop() {
        let node = document.node(node_id);
        if node
            .as_element()
            .is_some_and(|element| element.local_name == "option")
        {
            out.push(node_id);
        }
        let mut child = node.last_child;
        while let Some(id) = child {
            stack.push(id);
            child = document.node(id).prev_sibling;
        }
    }
    out
}
