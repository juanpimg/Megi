//! HTML table DOM API (WHATWG HTML §4.9): collections and row/cell mutators.

use super::*;
use megi_core::{Attribute, Namespace};

fn child_elements(document: &megi_core::Document, parent: NodeId, names: &[&str]) -> Vec<NodeId> {
    document
        .children(parent)
        .filter(|id| {
            document
                .try_node(*id)
                .and_then(|node| node.as_element())
                .is_some_and(|element| {
                    names
                        .iter()
                        .any(|name| element.local_name.eq_ignore_ascii_case(name))
                })
        })
        .collect()
}

fn table_rows_in(document: &megi_core::Document, root: NodeId) -> Vec<NodeId> {
    // Direct `tr` children of `table`, or of each `thead`/`tbody`/`tfoot` section.
    let direct = child_elements(document, root, &["tr"]);
    if !direct.is_empty() {
        return direct;
    }
    let mut rows = Vec::new();
    for section in child_elements(document, root, &["thead", "tbody", "tfoot"]) {
        rows.extend(child_elements(document, section, &["tr"]));
    }
    rows
}

fn first_named_child(document: &megi_core::Document, parent: NodeId, name: &str) -> Option<NodeId> {
    child_elements(document, parent, &[name]).into_iter().next()
}

/// `table.rows` / `section.rows` — HTMLCollection of `tr` elements (#1974).
pub(crate) fn node_table_rows(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let rows =
        with_context_host(context, |host| table_rows_in(&host.document, node)).unwrap_or_default();
    node_list(context, rows)
}

/// `row.cells` — HTMLCollection of `td`/`th` elements (#1974).
pub(crate) fn node_row_cells(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let cells = with_context_host(context, |host| {
        child_elements(&host.document, node, &["td", "th"])
    })
    .unwrap_or_default();
    node_list(context, cells)
}

/// `table.tBodies` — HTMLCollection of `tbody` elements (#1974).
pub(crate) fn node_table_t_bodies(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let bodies = with_context_host(context, |host| {
        child_elements(&host.document, node, &["tbody"])
    })
    .unwrap_or_default();
    node_list(context, bodies)
}

pub(crate) fn node_table_t_head(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let head = with_context_host(context, |host| {
        first_named_child(&host.document, node, "thead")
    })
    .flatten();
    wrap_optional(context, head)
}

pub(crate) fn node_table_t_foot(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let foot = with_context_host(context, |host| {
        first_named_child(&host.document, node, "tfoot")
    })
    .flatten();
    wrap_optional(context, foot)
}

pub(crate) fn node_table_caption(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let node = this_node(this, context)?;
    let caption = with_context_host(context, |host| {
        first_named_child(&host.document, node, "caption")
    })
    .flatten();
    wrap_optional(context, caption)
}

fn index_argument(args: &[JsValue], context: &mut Context) -> JsResult<i64> {
    let Some(value) = args.first() else {
        return Ok(-1);
    };
    if value.is_null_or_undefined() {
        return Ok(-1);
    }
    let index = value
        .to_number(context)
        .map_err(|error| js_error(error.to_string()))?;
    Ok(if index.is_finite() {
        index.trunc() as i64
    } else {
        -1
    })
}

/// `table.insertRow(index)` — appends when index is -1 or out of range (#1974).
pub(crate) fn node_table_insert_row(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let table = this_node(this, context)?;
    let index = index_argument(args, context)?;
    let row = with_context_host(context, |host| {
        let document = &mut host.document;
        let is_section = document
            .try_node(table)
            .and_then(|node| node.as_element())
            .is_some_and(|element| {
                matches!(element.local_name.as_str(), "thead" | "tbody" | "tfoot")
            });
        if is_section {
            // HTMLTableSectionElement.insertRow() inserts directly into the section.
            let rows = child_elements(document, table, &["tr"]);
            let before = if index < 0 || index as usize >= rows.len() {
                None
            } else {
                Some(rows[index as usize])
            };
            let new_row = document.create_element(Namespace::Html, "tr", Vec::<Attribute>::new());
            document.insert_node(table, new_row, before).ok()?;
            return Some(new_row);
        }
        let rows = table_rows_in(document, table);
        let insert_before = if index < 0 || index as usize >= rows.len() {
            None
        } else {
            Some(rows[index as usize])
        };
        // Rows always live in a section. Use the existing row's parent, or create/find a tbody.
        let section = insert_before
            .and_then(|row| document.try_node(row).and_then(|node| node.parent))
            .or_else(|| first_named_child(document, table, "tbody"))
            .unwrap_or_else(|| {
                document.create_element(Namespace::Html, "tbody", Vec::<Attribute>::new())
            });
        if document.try_node(section).is_none() {
            let _ = document.insert_node(table, section, None);
        }
        let new_row = document.create_element(Namespace::Html, "tr", Vec::<Attribute>::new());
        document.insert_node(section, new_row, insert_before).ok()?;
        Some(new_row)
    })
    .flatten()
    .ok_or_else(|| js_error("insertRow failed (node is not a table)"))?;
    note_mutation(MutationKind::ChildList, table, None, None, context);
    wrap_node(context, row)
}

/// `table.deleteRow(index)` — removes the row at the given index (#1974).
pub(crate) fn node_table_delete_row(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let table = this_node(this, context)?;
    let index = index_argument(args, context)?;
    if index < 0 {
        return Ok(JsValue::undefined());
    }
    let removed = with_context_host(context, |host| {
        let document = &mut host.document;
        let rows = table_rows_in(document, table);
        let Some(row) = rows.get(index as usize).copied() else {
            return false;
        };
        let Some(parent) = document.try_node(row).and_then(|node| node.parent) else {
            return false;
        };
        document.remove_child(parent, row).is_ok()
    })
    .unwrap_or(false);
    if removed {
        note_mutation(MutationKind::ChildList, table, None, None, context);
    }
    Ok(JsValue::undefined())
}

/// `table.createTBody()` — creates and appends an empty `<tbody>` (#1974).
pub(crate) fn node_table_create_t_body(
    this: &JsValue,
    _args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let table = this_node(this, context)?;
    let body = with_context_host(context, |host| {
        let document = &mut host.document;
        let body = document.create_element(Namespace::Html, "tbody", Vec::<Attribute>::new());
        document.insert_node(table, body, None).ok()?;
        Some(body)
    })
    .flatten()
    .ok_or_else(|| js_error("createTBody failed (node is not a table)"))?;
    note_mutation(MutationKind::ChildList, table, None, None, context);
    wrap_node(context, body)
}

/// `row.insertCell(index)` — appends a `<td>` when out of range (#1974).
pub(crate) fn node_row_insert_cell(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let row = this_node(this, context)?;
    let index = index_argument(args, context)?;
    let cell = with_context_host(context, |host| {
        let document = &mut host.document;
        let cells = child_elements(document, row, &["td", "th"]);
        let insert_before = if index < 0 || index as usize >= cells.len() {
            None
        } else {
            Some(cells[index as usize])
        };
        let new_cell = document.create_element(Namespace::Html, "td", Vec::<Attribute>::new());
        document.insert_node(row, new_cell, insert_before).ok()?;
        Some(new_cell)
    })
    .flatten()
    .ok_or_else(|| js_error("insertCell failed (node is not a row)"))?;
    note_mutation(MutationKind::ChildList, row, None, None, context);
    wrap_node(context, cell)
}
