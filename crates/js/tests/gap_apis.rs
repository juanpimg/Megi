//! Round 27: DOM/JS gap APIs (URL/SearchParams, dataset, innerText, attributes,
//! normalize/compareDocumentPosition, queueMicrotask/idle, MouseEvent, base64, clone).

#![cfg(feature = "js")]

#[path = "gap_apis/dom.rs"]
mod dom;
#[path = "gap_apis/helpers.rs"]
mod helpers;
#[path = "gap_apis/misc.rs"]
mod misc;
