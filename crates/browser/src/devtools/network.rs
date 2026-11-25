//! Firefox/CDP Network domain backed by the render pipeline's real loader events.
//!
//! Every frame emitted here derives from [`NetworkEvent`] values recorded by
//! `megi_net::ResourceLoader` while the attached page was rendered. No request is invented in
//! the DevTools layer. Event names follow the CDP/Firefox Network domain:
//! `Network.requestWillBeSent`, `Network.responseReceived`, `Network.loadingFinished`.

use megi_webview::net::NetworkEvent;
use serde_json::{Value, json};

/// Frames for `Network.enable`: one event trio per real request, then the command result.
///
/// The caller passes the page's already-owned event log; this module depends only on the
/// neutral `NetworkEvent` contract, never on the protocol page type (which would create a
/// `protocol ↔ network` module cycle).
pub(super) fn enable(events: &[NetworkEvent], id: Value) -> Vec<String> {
    let mut messages = Vec::with_capacity(events.len() * 3 + 1);
    for (index, event) in events.iter().enumerate() {
        let request_id = format!("req-{index}");
        messages.push(request_will_be_sent(&request_id, event));
        if let Some(error) = &event.error {
            messages.push(
                json!({
                    "method": "Network.loadingFailed",
                    "params": {
                        "requestId": request_id,
                        "errorText": error,
                        "canceled": false,
                    },
                })
                .to_string(),
            );
            continue;
        }
        messages.push(response_received(&request_id, event));
        messages.push(
            json!({
                "method": "Network.loadingFinished",
                "params": {
                    "requestId": request_id,
                    "encodedDataLength": event.encoded_bytes,
                },
            })
            .to_string(),
        );
    }
    messages.push(json!({ "id": id, "result": {} }).to_string());
    messages
}

fn request_will_be_sent(request_id: &str, event: &NetworkEvent) -> String {
    json!({
        "method": "Network.requestWillBeSent",
        "params": {
            "requestId": request_id,
            "request": {
                "url": event.url,
                "method": event.method,
                "headers": { "Content-Type": event.content_type },
            },
            "type": resource_type(event),
        },
    })
    .to_string()
}

fn response_received(request_id: &str, event: &NetworkEvent) -> String {
    json!({
        "method": "Network.responseReceived",
        "params": {
            "requestId": request_id,
            "type": resource_type(event),
            "response": {
                "url": event.url,
                "status": event.status,
                "headers": { "content-type": event.content_type },
                "encodedDataLength": event.encoded_bytes,
                "fromDiskCache": event.from_cache,
            },
        },
    })
    .to_string()
}

/// Coarse CDP resource type derived only from the resolved content type of the real response.
fn resource_type(event: &NetworkEvent) -> &'static str {
    let mime = event.content_type.to_ascii_lowercase();
    if mime.contains("html") {
        "Document"
    } else if mime.contains("css") {
        "Stylesheet"
    } else if mime.contains("script") || mime.contains("javascript") {
        "Script"
    } else if mime.contains("image/") || mime.contains("font/") {
        "Image"
    } else {
        "Other"
    }
}
