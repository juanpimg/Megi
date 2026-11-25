//! Host state shared between the Rust runtime and the Boa native callbacks.
//!
//! The bridge is deliberately small and auditable:
//!
//! - **One realm per runtime/document.** [`crate::JsRuntime`] owns a [`megi_core::Document`] and a
//!   Boa `Context`; a script mutates the document through native functions.
//! - **Side table `NodeId` ↔ JS handle.** JS wrappers carry a numeric handle (index into
//!   the host side table); the actual generation-checked [`NodeId`] stays in Rust. Every access
//!   revalidates it with [`megi_core::Document::try_node`], so stale/removed nodes raise a typed
//!   JS error instead of panicking (handles are never dereferenced blindly).
//! - **Context-installed host.** Boa native functions are plain `fn` pointers (no
//!   captures); the shared host cell is installed in the `Context`'s `HostDefined` data
//!   at construction. Every native function borrows it from the `Context` it receives,
//!   so a nested realm (iframe/worker) always reaches its own host without thread-local
//!   state. Tests run in parallel threads with independent runtimes.
//!
//! Limits documented for consumers: the side table keeps one entry per distinct node
//! handle ever exposed to JS (bounded by the DOM arena plus removed nodes until wrappers
//! are collected; a generational arena keeps the raw ids distinct). No network/FS APIs are
//! exposed to scripts.

use std::cell::{Ref, RefCell, RefMut};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use megi_core::{Document, NodeId};

// Provider aliases now live in [`crate::providers`] as immutable `Arc<dyn Fn…>` closures.
// They are re-exported here to type the legacy host-side mirror fields (and so downstream
// `use crate::host::…` paths keep working); the authoritative slot is the `HostProviders`
// installed in the Boa context.
pub use crate::providers::{
    ComputedStyleProvider, CookieGetFn, CookieSetFn, FetchProvider, FrameLoader, HistoryProvider,
    LayoutProvider, LocationProvider, MediaProvider, ModuleLoader, ModuleResolver, StorageLoadFn,
    StorageSaveFn, ViewportProvider, WebSocketProvider, WorkerLoader,
};

// Host state and Boa-bridge types (split verbatim from the former monolithic host.rs).
mod context;
mod impl_host;
mod state;
mod types;

pub(crate) use context::*;
pub(crate) use state::*;
pub use types::*;

#[cfg(test)]
mod context_host_tests {
    use super::*;
    use boa_engine::Context;

    /// A standalone host cell installed into a fresh context.
    fn demo_host() -> ContextHost {
        Rc::new(RefCell::new(Host::new(
            Document::new(),
            "about:blank".into(),
        )))
    }

    #[test]
    fn host_is_reachable_from_the_context() {
        let mut context = Context::default();
        install_host(&mut context, demo_host());
        assert!(host_ref(&context).is_ok());
        assert_eq!(host_ref(&context).unwrap().url, "about:blank");
    }

    #[test]
    fn host_cell_and_host_ref_observe_the_same_host() {
        let mut context = Context::default();
        install_host(&mut context, demo_host());
        host_cell(&mut context).unwrap().url = "https://example.com/".to_string();
        assert_eq!(host_ref(&context).unwrap().url, "https://example.com/");
    }

    #[test]
    fn missing_host_returns_a_typed_error() {
        let context = Context::default();
        assert!(host_ref(&context).is_err());
    }
}

#[cfg(test)]
mod storage_and_console_tests {
    use super::*;

    #[test]
    fn storage_map_keeps_insertion_order_with_hash_lookup() {
        let mut map = StorageMap::default();
        assert_eq!(map.insert("b".into(), "1".into()), None);
        assert_eq!(map.insert("a".into(), "2".into()), None);
        assert_eq!(map.insert("b".into(), "3".into()), Some("1".into()));
        assert_eq!(
            map.keys().map(String::as_str).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(map.get("a").map(String::as_str), Some("2"));
        assert_eq!(map.remove("b").as_deref(), Some("3"));
        assert_eq!(map.keys().map(String::as_str).collect::<Vec<_>>(), ["a"]);
        assert_eq!(map.iter().count(), 1);
    }

    #[test]
    fn console_drops_oldest_at_cap_in_constant_time() {
        let mut host = Host::new(Document::new(), "about:blank".into());
        for i in 0..MAX_CONSOLE_ENTRIES + 3 {
            host.push_console(ConsoleEntry {
                level: "log".into(),
                message: i.to_string(),
            });
        }
        assert_eq!(host.console.len(), MAX_CONSOLE_ENTRIES);
        assert_eq!(host.console.front().expect("oldest").message, "3");
        assert_eq!(host.console.back().expect("newest").message, "4098");
    }

    #[test]
    fn console_message_caps_bytes_without_splitting_utf8() {
        let mut host = Host::new(Document::new(), "about:blank".into());
        host.push_console(ConsoleEntry {
            level: "log".into(),
            message: "€".repeat(10_000),
        });
        let entry = host.take_console().pop().expect("entry");
        // 8 KiB is not a multiple of the 3 bytes of `€`, so the last code point
        // must be dropped rather than cut in half (#2310).
        assert_eq!(entry.message.len(), 8190);
        assert!(entry.message.chars().all(|character| character == '€'));
    }

    #[test]
    fn console_aggregate_byte_cap_keeps_every_entry() {
        let mut host = Host::new(Document::new(), "about:blank".into());
        for i in 0..MAX_CONSOLE_ENTRIES + 8 {
            host.push_console(ConsoleEntry {
                level: "warn".into(),
                message: format!("{i:04}").repeat(MAX_CONSOLE_MESSAGE_BYTES / 4),
            });
        }
        let total: usize = host
            .console
            .iter()
            .map(|entry| entry.level.len() + entry.message.len())
            .sum();
        assert_eq!(host.console.len(), MAX_CONSOLE_ENTRIES);
        assert!(total <= MAX_CONSOLE_BYTES, "{total} bytes");
        assert_eq!(host.console_bytes, total);
        assert!(host.take_console().len() == MAX_CONSOLE_ENTRIES);
        assert_eq!(host.console_bytes, 0);
    }
}

#[cfg(test)]
mod mutation_observer_index_tests {
    use super::*;
    use megi_core::Namespace;

    #[test]
    fn observer_index_updates_cleans_up_and_preserves_order() {
        let mut doc = Document::new();
        let parent = doc.create_element(Namespace::Html, "div", vec![]);
        let child = doc.create_element(Namespace::Html, "span", vec![]);
        let other = doc.create_element(Namespace::Html, "p", vec![]);
        assert!(doc.append_child(doc.root(), parent));
        assert!(doc.append_child(parent, child));
        assert!(doc.append_child(doc.root(), other));

        let mut host = Host::new(doc, "https://example.com".to_string());
        for slot in 0..3u32 {
            host.observers.push(ObserverRegistration {
                callback_slot: slot,
                object_slot: slot,
                targets: Vec::new(),
                records: Vec::new(),
            });
        }
        host.observe_target(
            0,
            ObserverTarget {
                node: parent,
                child_list: true,
                attributes: true,
                attribute_old_value: false,
                character_data: false,
                subtree: true,
            },
        )
        .expect("observe 0");
        host.observe_target(
            1,
            ObserverTarget {
                node: child,
                child_list: false,
                attributes: true,
                attribute_old_value: true,
                character_data: false,
                subtree: false,
            },
        )
        .expect("observe 1");
        host.observe_target(
            2,
            ObserverTarget {
                node: other,
                child_list: true,
                attributes: true,
                attribute_old_value: true,
                character_data: false,
                subtree: true,
            },
        )
        .expect("observe 2");
        assert_eq!(host.observer_index.len(), 3);

        host.observe_target(
            0,
            ObserverTarget {
                node: child,
                child_list: false,
                attributes: true,
                attribute_old_value: true,
                character_data: false,
                subtree: false,
            },
        )
        .expect("observe 0 child");

        host.record_mutation(MutationRecord {
            kind: MutationKind::Attributes,
            target: child,
            name: Some("data-x".to_string()),
            old_value: Some("before".to_string()),
        });
        assert_eq!(host.observers[0].records.len(), 1);
        assert_eq!(
            host.observers[0].records[0].old_value.as_deref(),
            Some("before")
        );
        assert_eq!(host.observers[1].records.len(), 1);
        assert_eq!(
            host.observers[1].records[0].old_value.as_deref(),
            Some("before")
        );
        assert!(host.observers[2].records.is_empty());

        host.observe_target(
            0,
            ObserverTarget {
                node: child,
                child_list: false,
                attributes: true,
                attribute_old_value: false,
                character_data: false,
                subtree: false,
            },
        )
        .expect("re-observe 0 child");
        host.record_mutation(MutationRecord {
            kind: MutationKind::Attributes,
            target: child,
            name: Some("data-x".to_string()),
            old_value: Some("after".to_string()),
        });
        assert_eq!(host.observers[0].records.len(), 2);
        assert_eq!(host.observers[0].records[1].old_value, None);
        assert_eq!(
            host.observers[1].records[1].old_value.as_deref(),
            Some("after")
        );

        host.disconnect_observer(0);
        assert!(!host.observer_index.contains_key(&parent));
        assert_eq!(host.observer_index.get(&child).map(Vec::len), Some(1));
        host.disconnect_observer(1);
        assert!(!host.observer_index.contains_key(&child));
        host.disconnect_observer(2);
        assert!(host.observer_index.is_empty());
    }

    #[test]
    fn record_mutation_terminates_on_cyclic_parent_chain() {
        let mut doc = Document::new();
        let a = doc.create_element(Namespace::Html, "div", vec![]);
        let b = doc.create_element(Namespace::Html, "span", vec![]);
        let unrelated = doc.create_element(Namespace::Html, "p", vec![]);
        doc.try_node_mut(a).expect("a").parent = Some(b);
        doc.try_node_mut(b).expect("b").parent = Some(a);

        let mut host = Host::new(doc, "https://example.com".to_string());
        host.observers.push(ObserverRegistration {
            callback_slot: 0,
            object_slot: 0,
            targets: Vec::new(),
            records: Vec::new(),
        });
        host.observe_target(
            0,
            ObserverTarget {
                node: unrelated,
                child_list: true,
                attributes: true,
                attribute_old_value: false,
                character_data: false,
                subtree: true,
            },
        )
        .expect("observe");

        host.record_mutation(MutationRecord {
            kind: MutationKind::ChildList,
            target: b,
            name: None,
            old_value: None,
        });
        assert!(host.observers[0].records.is_empty());
    }
}

mod timer_queue;
pub(crate) use timer_queue::*;
