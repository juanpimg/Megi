use super::*;

/// A form's submission parameters (#1538).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormSubmission {
    /// Resolved destination URL (the form's `action`, or the document URL when absent).
    pub action: String,
    /// Submitted method, lowercased (`get` or `post`).
    pub method: String,
    /// `application/x-www-form-urlencoded` body (empty when there are no successful controls).
    pub body: String,
}

impl FormSubmission {
    /// Returns the `(Content-Type, body)` pair passed to `ResourceLoader::fetch_with_method`
    /// for a POST submission (#1538). GET carries no body/header — its values are in the URL.
    pub fn post_request(&self) -> Option<(&'static str, &[u8])> {
        if self.method != "post" {
            return None;
        }
        Some((
            "application/x-www-form-urlencoded; charset=UTF-8",
            self.body.as_bytes(),
        ))
    }
}

/// Resolves a form action against the document URL (absolute URLs pass through; `Url::join`
/// covers the relative forms the shell already supports for links).
pub(crate) fn resolve_action(base: &megi_webview::net::Url, action: &str) -> Option<String> {
    let action = action.trim();
    if action.is_empty() {
        return Some(base.as_str());
    }
    base.join(action).map(|url| url.as_str()).ok().or_else(|| {
        megi_webview::net::Url::parse(action)
            .ok()
            .map(|url| url.as_str())
    })
}

/// Builds the submission parameters for a form from its controls (#1538).
///
/// Finds the nearest `<form>` ancestor of `form_node`, reads `action`/`method`, and collects
/// only the controls belonging to that form. When no `action` is present the document URL is
/// used, matching browser behaviour.
pub fn form_submission(
    document: &Document,
    base: &megi_webview::net::Url,
    form_node: NodeId,
    controls: &[FormControl],
) -> Option<FormSubmission> {
    // The form node itself, or its nearest ancestor, must be a <form>.
    let form_elements: Vec<NodeId> = std::iter::once(form_node)
        .chain(document.ancestors(form_node))
        .filter(|&id| {
            document
                .try_node(id)
                .and_then(|node| node.as_element())
                .is_some_and(|element| element.local_name.eq_ignore_ascii_case("form"))
        })
        .collect();
    let form_id = *form_elements.first()?;
    let element = document.try_node(form_id)?.as_element()?;
    let action = resolve_action(base, element.get_attribute("action").unwrap_or(""))?;
    let method = element
        .get_attribute("method")
        .unwrap_or("get")
        .to_ascii_lowercase();
    let method = if method == "post" { "post" } else { "get" }.to_owned();
    let mut owned = controls.to_vec();
    owned.retain(|control| {
        if !form_elements.contains(&control.node_id) {
            // A control belongs to the form when the form is its ancestor.
            return document
                .ancestors(control.node_id)
                .any(|ancestor| form_elements.contains(&ancestor));
        }
        true
    });
    let body = form_urlencoded(&owned);
    Some(FormSubmission {
        action,
        method,
        body,
    })
}

/// Applies a GET submission body to the action URL, producing the navigation target.
pub fn form_get_target(submission: &FormSubmission) -> Option<String> {
    if submission.method != "get" || submission.body.is_empty() {
        return Some(submission.action.clone());
    }
    let separator = megi_webview::net::Url::parse(&submission.action)
        .ok()
        .and_then(|url| url.query().map(|_| '&'))
        .unwrap_or('?');
    megi_webview::net::Url::parse(&format!(
        "{}{}{}",
        submission.action, separator, submission.body
    ))
    .ok()
    .map(|url| url.as_str())
}

/// Percent-encodes a string using the `application/x-www-form-urlencoded` rules:
/// ASCII alphanumerics and `* - . _` are unreserved, space becomes `+`, and every other
/// UTF-8 byte becomes uppercase `%HH`. This is the WHATWG "urlencoded serializer".
pub fn form_urlencode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0F) as usize] as char);
            }
        }
    }
    out
}

/// Builds an `application/x-www-form-urlencoded` body from the successful controls (#1538).
///
/// A control is successful when it has a non-empty `name`, is not disabled, is not a
/// submit/reset/button, and (for checkboxes/radios) is checked. `select` contributes the value
/// of its selected option. Pairs are emitted in document order, joined by `&`.
pub fn form_urlencoded(controls: &[FormControl]) -> String {
    let mut pairs: Vec<String> = Vec::new();
    for control in controls {
        if control.name.is_empty()
            || control.disabled
            || matches!(
                control.kind,
                FormControlKind::Submit | FormControlKind::Reset | FormControlKind::Button
            )
        {
            continue;
        }
        let values: Vec<&str> = match control.kind {
            FormControlKind::Checkbox | FormControlKind::Radio => {
                if control.checked {
                    vec![if control.value.is_empty() {
                        "on"
                    } else {
                        &control.value
                    }]
                } else {
                    Vec::new()
                }
            }
            FormControlKind::Select => control
                .options
                .iter()
                .filter(|option| option.selected)
                .map(|option| option.value.as_str())
                .collect(),
            _ => vec![control.value.as_str()],
        };
        for value in values {
            pairs.push(format!(
                "{}={}",
                form_urlencode(&control.name),
                form_urlencode(value)
            ));
        }
    }
    pairs.join("&")
}
