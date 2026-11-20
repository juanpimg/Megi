use super::*;

// ---------------------------------------------------------------------------
// Node wrappers
// ---------------------------------------------------------------------------

/// Creates the JS wrapper for a side-table node handle (shared with events/utilities).
pub(crate) fn wrap_handle_public(context: &mut Context, handle: u32) -> JsResult<JsValue> {
    node_wrapper(context, handle)
}

/// Creates a JS wrapper for `node_id`, reusing the realm's cached object (identity).
pub(crate) fn node_wrapper(context: &mut Context, handle: u32) -> JsResult<JsValue> {
    let cache = context
        .global_object()
        .get(js_string!("__megi_wrappers"), context)
        .ok()
        .and_then(|value| value.as_object());
    if let Some(cache) = &cache
        && let Ok(existing) = cache.get(js_string!(handle.to_string()), context)
        && existing.is_object()
    {
        return Ok(existing);
    }
    // Accessors: tagName, id, className, textContent, innerHTML, outerHTML, children,
    // parentNode, firstChild, nextSibling, style, classList.
    let accessors: [(&str, AccessorKind); 57] = [
        ("nodeType", AccessorKind::NodeType),
        ("tagName", AccessorKind::TagName),
        ("id", AccessorKind::Id),
        ("className", AccessorKind::ClassName),
        ("textContent", AccessorKind::TextContent),
        // #1943: nodeValue/data are the CharacterData payload (Text/Comment).
        ("nodeValue", AccessorKind::NodeValue),
        ("data", AccessorKind::NodeValue),
        ("innerHTML", AccessorKind::InnerHtml),
        ("outerHTML", AccessorKind::OuterHtml),
        ("children", AccessorKind::Children),
        ("parentNode", AccessorKind::ParentNode),
        ("firstChild", AccessorKind::FirstChild),
        ("lastChild", AccessorKind::LastChild),
        ("nextSibling", AccessorKind::NextSibling),
        ("previousSibling", AccessorKind::PreviousSibling),
        ("style", AccessorKind::Style),
        ("classList", AccessorKind::ClassList),
        ("childNodes", AccessorKind::ChildNodes),
        ("value", AccessorKind::Value),
        ("checked", AccessorKind::Checked),
        ("disabled", AccessorKind::Disabled),
        ("selected", AccessorKind::Selected),
        ("options", AccessorKind::Options),
        ("innerText", AccessorKind::InnerText),
        ("dataset", AccessorKind::Dataset),
        ("attributes", AccessorKind::Attributes),
        ("offsetWidth", AccessorKind::OffsetWidth),
        ("offsetHeight", AccessorKind::OffsetHeight),
        ("clientWidth", AccessorKind::ClientWidth),
        ("clientHeight", AccessorKind::ClientHeight),
        ("title", AccessorKind::Title),
        ("lang", AccessorKind::Lang),
        ("href", AccessorKind::Href),
        ("src", AccessorKind::Src),
        ("selectedIndex", AccessorKind::SelectedIndex),
        ("validity", AccessorKind::Validity),
        ("action", AccessorKind::Action),
        ("method", AccessorKind::Method),
        ("enctype", AccessorKind::Enctype),
        ("noValidate", AccessorKind::NoValidate),
        ("elements", AccessorKind::Elements),
        ("shadowRoot", AccessorKind::ShadowRoot),
        ("open", AccessorKind::DialogOpen),
        ("returnValue", AccessorKind::DialogReturnValue),
        ("contentWindow", AccessorKind::ContentWindow),
        ("contentDocument", AccessorKind::ContentDocument),
        ("srcdoc", AccessorKind::Srcdoc),
        ("sandbox", AccessorKind::Sandbox),
        ("rows", AccessorKind::TableRows),
        ("cells", AccessorKind::TableCells),
        ("tBodies", AccessorKind::TableBodies),
        ("tHead", AccessorKind::TableHead),
        ("tFoot", AccessorKind::TableFoot),
        ("caption", AccessorKind::TableCaption),
        ("namespaceURI", AccessorKind::NamespaceUri),
        ("localName", AccessorKind::LocalName),
        ("prefix", AccessorKind::Prefix),
    ];
    type Prepared<'a> = (
        &'a str,
        boa_engine::object::builtins::JsFunction,
        Option<boa_engine::object::builtins::JsFunction>,
    );
    let mut prepared: Vec<Prepared<'_>> = Vec::with_capacity(accessors.len());
    for (name, kind) in accessors {
        let (get, set): AccessorPair = accessor_functions(kind);
        prepared.push((
            name,
            accessor_fn(context, get),
            set.map(|set| accessor_fn(context, set)),
        ));
    }
    let inline_accessors: Vec<(
        &'static str,
        boa_engine::object::builtins::JsFunction,
        boa_engine::object::builtins::JsFunction,
    )> = crate::events::inline_node_accessors()
        .into_iter()
        .map(|(name, get, set)| (name, accessor_fn(context, get), accessor_fn(context, set)))
        .collect();
    let mut init = ObjectInitializer::new(context);
    // `__megi_node` is the Rust side-table handle; it must never be writable, enumerable or
    // configurable from script, otherwise a forged value repoints the wrapper (#644).
    init.property(
        js_string!("__megi_node"),
        f64::from(handle),
        Attribute::default(),
    );
    for (name, get, set) in prepared {
        init.accessor(js_string!(name), Some(get), set, Attribute::all());
    }
    for (name, get, set) in inline_accessors {
        init.accessor(js_string!(name), Some(get), Some(set), Attribute::all());
    }
    // Methods.
    for (name, length, function) in [
        (
            "setAttribute",
            2usize,
            node_set_attribute as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        ("getAttribute", 1, node_get_attribute),
        ("getAttributeNode", 1, node_get_attribute_node),
        ("removeAttributeNode", 1, node_remove_attribute_node),
        ("removeAttribute", 1, node_remove_attribute),
        ("hasAttribute", 1, node_has_attribute),
        ("appendChild", 1, node_append_child),
        ("insertBefore", 2, node_insert_before),
        ("removeChild", 1, node_remove_child),
        ("replaceChild", 2, node_replace_child),
        ("matches", 1, node_matches),
        ("closest", 1, node_closest),
        ("getElementsByTagName", 1, node_get_elements_by_tag_name),
        ("querySelector", 1, node_query_selector),
        ("querySelectorAll", 1, node_query_selector_all),
        ("insertAdjacentHTML", 2, node_insert_adjacent_html),
        ("insertAdjacentText", 2, node_insert_adjacent_text),
        ("insertAdjacentElement", 2, node_insert_adjacent_element),
        ("toggleAttribute", 1, node_toggle_attribute),
        ("getAttributeNames", 0, node_get_attribute_names),
        ("hasAttributes", 0, node_has_attributes),
        ("cloneNode", 1, node_clone_node),
        ("normalize", 0, node_normalize),
        ("compareDocumentPosition", 1, node_compare_document_position),
        ("focus", 0, node_focus),
        ("blur", 0, node_blur),
        ("scrollIntoView", 0, node_scroll_into_view),
        ("getBoundingClientRect", 0, node_get_bounding_client_rect),
        ("checkValidity", 0, node_check_validity),
        ("reportValidity", 0, node_report_validity),
        ("setCustomValidity", 1, node_set_custom_validity),
        ("requestSubmit", 0, node_request_submit),
        ("submit", 0, node_submit),
        ("reset", 0, node_reset),
        ("attachShadow", 1, node_attach_shadow),
        ("getRootNode", 0, node_get_root_node),
        ("assignedNodes", 0, slot_assigned_nodes),
        ("assignedElements", 0, slot_assigned_elements),
        ("getContext", 1, canvas_get_context),
        ("show", 0, node_dialog_show),
        ("showModal", 0, node_dialog_show_modal),
        ("close", 1, node_dialog_close),
        ("insertRow", 1, node_table_insert_row),
        ("deleteRow", 1, node_table_delete_row),
        ("createTBody", 0, node_table_create_t_body),
        ("insertCell", 1, node_row_insert_cell),
    ] {
        init.function(native(function), js_string!(name), length);
    }
    for (name, length, function) in crate::events::node_methods() {
        init.function(native(function), js_string!(name), length);
    }
    let wrapper = init.build();
    // #1915: element wrappers inherit from `HTMLElement.prototype`, which itself
    // inherits from `Element.prototype`, so both instanceof checks work.
    if let Some(prototype) = context
        .global_object()
        .get(js_string!("HTMLElement"), context)
        .ok()
        .and_then(|value| value.as_object())
        .and_then(|ctor| ctor.get(js_string!("prototype"), context).ok())
        .and_then(|value| value.as_object())
    {
        let _ = wrapper.set_prototype(Some(prototype));
    }
    if let Some(cache) = &cache {
        let _ = cache.set(
            js_string!(handle.to_string()),
            JsValue::from(wrapper.clone()),
            false,
            context,
        );
    }
    Ok(JsValue::from(wrapper))
}

#[derive(Clone, Copy)]
pub(crate) enum AccessorKind {
    NodeType,
    TagName,
    Id,
    ClassName,
    TextContent,
    NodeValue,
    InnerHtml,
    OuterHtml,
    Children,
    ParentNode,
    FirstChild,
    LastChild,
    NextSibling,
    PreviousSibling,
    Style,
    ClassList,
    ChildNodes,
    Value,
    Checked,
    Disabled,
    Selected,
    Options,
    InnerText,
    Dataset,
    Attributes,
    OffsetWidth,
    OffsetHeight,
    ClientWidth,
    ClientHeight,
    Title,
    Lang,
    Href,
    Src,
    SelectedIndex,
    Validity,
    Action,
    Method,
    Enctype,
    NoValidate,
    Elements,
    ShadowRoot,
    DialogOpen,
    DialogReturnValue,
    ContentWindow,
    ContentDocument,
    Srcdoc,
    Sandbox,
    TableRows,
    TableCells,
    TableBodies,
    TableHead,
    TableFoot,
    TableCaption,
    NamespaceUri,
    LocalName,
    Prefix,
}

pub(crate) fn accessor_functions(kind: AccessorKind) -> AccessorPair {
    match kind {
        AccessorKind::NodeType => (node_node_type, None),
        AccessorKind::TagName => (node_tag_name, None),
        AccessorKind::Id => (node_id_get, Some(node_id_set)),
        AccessorKind::ClassName => (node_class_get, Some(node_class_set)),
        AccessorKind::TextContent => (node_text_content_get, Some(node_text_content_set)),
        AccessorKind::NodeValue => (node_value_data_get, Some(node_value_data_set)),
        AccessorKind::InnerHtml => (node_inner_html_get, Some(node_inner_html_set)),
        AccessorKind::OuterHtml => (node_outer_html_get, Some(node_outer_html_set)),
        AccessorKind::Children => (node_children, None),
        AccessorKind::ParentNode => (node_parent_node, None),
        AccessorKind::FirstChild => (node_first_child, None),
        AccessorKind::LastChild => (node_last_child, None),
        AccessorKind::NextSibling => (node_next_sibling, None),
        AccessorKind::PreviousSibling => (node_previous_sibling, None),
        AccessorKind::Style => (node_style_get, Some(node_style_set)),
        AccessorKind::ClassList => (node_class_list_get, None),
        AccessorKind::ChildNodes => (node_child_nodes, None),
        AccessorKind::Value => (node_value_get, Some(node_value_set)),
        AccessorKind::Checked => (node_checked_get, Some(node_checked_set)),
        AccessorKind::Disabled => (node_disabled_get, Some(node_disabled_set)),
        AccessorKind::Selected => (node_selected_get, Some(node_selected_set)),
        AccessorKind::Options => (node_options, None),
        AccessorKind::InnerText => (node_inner_text_get, Some(node_text_content_set)),
        AccessorKind::Dataset => (node_dataset_get, None),
        AccessorKind::Attributes => (node_attributes_get, None),
        AccessorKind::OffsetWidth => (node_offset_width, None),
        AccessorKind::OffsetHeight => (node_offset_height, None),
        AccessorKind::ClientWidth => (node_client_width, None),
        AccessorKind::ClientHeight => (node_client_height, None),
        AccessorKind::Title => (node_title_get, Some(node_title_set)),
        AccessorKind::Lang => (node_lang_get, Some(node_lang_set)),
        AccessorKind::Href => (node_href_get, Some(node_href_set)),
        AccessorKind::Src => (node_src_get, Some(node_src_set)),
        AccessorKind::SelectedIndex => (node_selected_index_get, Some(node_selected_index_set)),
        AccessorKind::Validity => (node_validity_get, None),
        AccessorKind::Action => (node_action_get, Some(node_action_set)),
        AccessorKind::Method => (node_method_get, Some(node_method_set)),
        AccessorKind::Enctype => (node_enctype_get, Some(node_enctype_set)),
        AccessorKind::NoValidate => (node_no_validate_get, Some(node_no_validate_set)),
        AccessorKind::Elements => (node_elements_get, None),
        AccessorKind::ShadowRoot => (node_shadow_root_get, None),
        AccessorKind::DialogOpen => (node_open_get, Some(node_open_set)),
        AccessorKind::DialogReturnValue => (node_dialog_return_value_get, None),
        AccessorKind::ContentWindow => (node_content_window_get, None),
        AccessorKind::ContentDocument => (node_content_document_get, None),
        AccessorKind::Srcdoc => (node_srcdoc_get, Some(node_srcdoc_set)),
        AccessorKind::Sandbox => (node_sandbox_get, Some(node_sandbox_set)),
        AccessorKind::TableRows => (node_table_rows, None),
        AccessorKind::TableCells => (node_row_cells, None),
        AccessorKind::TableBodies => (node_table_t_bodies, None),
        AccessorKind::TableHead => (node_table_t_head, None),
        AccessorKind::TableFoot => (node_table_t_foot, None),
        AccessorKind::TableCaption => (node_table_caption, None),
        AccessorKind::NamespaceUri => (node_namespace_uri, None),
        AccessorKind::LocalName => (node_local_name, None),
        AccessorKind::Prefix => (node_prefix, None),
    }
}

/// Wraps a node id from the current host, if it still resolves.
pub(crate) fn wrap_node(context: &mut Context, node_id: NodeId) -> JsResult<JsValue> {
    let handle = with_context_host(context, |host| host.handle_for(node_id))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)?;
    node_wrapper(context, handle)
}

/// Extracts the side-table handle stored on `this`.
pub(crate) fn this_handle(this: &JsValue, context: &mut Context) -> JsResult<u32> {
    let object = this
        .as_object()
        .ok_or_else(|| js_error("DOM method called with a non-object receiver"))?;
    let value = object
        .get(js_string!("__megi_node"), context)
        .map_err(|error| js_error(error.to_string()))?;
    value
        .as_number()
        .filter(|number| *number >= 0.0 && *number <= u32::MAX as f64 && number.fract() == 0.0)
        .map(|number| number as u32)
        .ok_or_else(|| js_error("invalid DOM node handle"))
}

/// Resolves the receiver to a live node id (typed error on stale handles).
pub(crate) fn this_node(this: &JsValue, context: &mut Context) -> JsResult<NodeId> {
    let handle = this_handle(this, context)?;
    with_context_host(context, |host| host.node_for_handle(handle))
        .ok_or_else(|| js_error("no active JavaScript host"))?
        .map_err(js_error)
}

pub(crate) fn js_error(message: impl Into<String>) -> BoaError {
    JsNativeError::typ().with_message(message.into()).into()
}

/// `SyntaxError` for the DOM APIs that validate an enumerated string argument (#653).
pub(crate) fn syntax_error(message: impl Into<String>) -> BoaError {
    JsNativeError::syntax().with_message(message.into()).into()
}

/// `RangeError` for bounded host resource limits (such as `MAX_OBSERVERS`).
pub(crate) fn range_error(message: impl Into<String>) -> BoaError {
    JsNativeError::range().with_message(message.into()).into()
}

/// The four `insertAdjacent*` positions, after ASCII-lowercasing (#653).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdjacentPosition {
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
}

impl AdjacentPosition {
    pub(crate) fn parse(position: &str) -> Option<Self> {
        match position {
            "beforebegin" => Some(Self::BeforeBegin),
            "afterbegin" => Some(Self::AfterBegin),
            "beforeend" => Some(Self::BeforeEnd),
            "afterend" => Some(Self::AfterEnd),
            _ => None,
        }
    }
}

pub(crate) fn arg_string(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<String> {
    let value = args.get(index).cloned().unwrap_or_else(JsValue::undefined);
    value
        .to_string(context)
        .map(|text| text.to_std_string_escaped())
        .map_err(|error| js_error(error.to_string()))
}

pub(crate) fn wrap_optional(context: &mut Context, node: Option<NodeId>) -> JsResult<JsValue> {
    match node {
        Some(node) => wrap_node(context, node),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn wrap_list(context: &mut Context, nodes: Vec<NodeId>) -> JsResult<JsValue> {
    let array = boa_engine::object::builtins::JsArray::from_iter([], context);
    for node in nodes {
        array
            .push(wrap_node(context, node)?, context)
            .map_err(|error| js_error(error.to_string()))?;
    }
    Ok(JsValue::from(array))
}
