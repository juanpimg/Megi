//! Pool crash/failure propagation tests, split from `process_pool.rs` to respect §4.

use super::*;

fn test_req(generation: u64, site: &str) -> IpcRenderRequest {
    IpcRenderRequest {
        generation,
        input: site.to_owned(),
        css_width: 200,
        css_height: 100,
        base_override: None,
        scripting: false,
        find_query: None,
        options: IpcRenderOptions::default(),
    }
}

#[test]
fn fatal_ipc_error_notifies_every_in_flight_generation() {
    let mut pool = ProcessRenderPool::new(2);
    let (tx, rx) = std::sync::mpsc::channel();
    pool.push_mock_slot_for_test(crate::site::site_key("https://crash.example/"), rx, true);

    pool.send(test_req(11, "https://crash.example/a"))
        .expect("send 11");
    pool.send(test_req(12, "https://crash.example/b"))
        .expect("send 12");

    tx.send(Err(IpcError::Truncated)).expect("inject fatal");
    let drained = pool.drain_tagged();
    assert_eq!(
        drained.len(),
        2,
        "both in-flight requests must receive errors"
    );
    assert_eq!(drained[0].0, Some(11));
    assert_eq!(drained[1].0, Some(12));
    assert!(drained.iter().all(|(_, res)| res.is_err()));
    assert_eq!(pool.len(), 0);
    assert_eq!(pool.recoveries(), 1);
}

#[test]
fn dead_slot_replaced_by_send_reports_in_flight_failures() {
    let mut pool = ProcessRenderPool::new(2);
    let site = crate::site::site_key("https://dead.example/");
    let (_tx, rx) = std::sync::mpsc::channel();
    pool.push_mock_slot_for_test(site.clone(), rx, true);
    pool.send(test_req(31, "https://dead.example/a"))
        .expect("send 31");
    pool.send(test_req(32, "https://dead.example/b"))
        .expect("send 32");
    assert!(pool.kill_child(&site));

    let retired = pool.slots.remove(0);
    pool.retire_slot(retired, "renderer died");
    let drained = pool.drain_tagged();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].0, Some(31));
    assert_eq!(drained[1].0, Some(32));
    assert!(drained.iter().all(|(_, res)| res.is_err()));
}

#[test]
fn disconnected_reader_notifies_every_in_flight_generation() {
    let mut pool = ProcessRenderPool::new(2);
    let (tx, rx) = std::sync::mpsc::channel();
    pool.push_mock_slot_for_test(crate::site::site_key("https://eof.example/"), rx, true);

    pool.send(test_req(21, "https://eof.example/a"))
        .expect("send 21");
    pool.send(test_req(22, "https://eof.example/b"))
        .expect("send 22");

    drop(tx);
    let drained = pool.drain_tagged();
    assert_eq!(
        drained.len(),
        2,
        "channel close must fail both pending calls"
    );
    assert_eq!(drained[0].0, Some(21));
    assert_eq!(drained[1].0, Some(22));
    assert!(drained.iter().all(|(_, res)| res.is_err()));
    assert_eq!(pool.len(), 0);
}

#[test]
fn validation_failure_is_fatal_and_retires_the_slot() {
    // #2308: a renderer that returns an unauthentic frame must not keep serving the site.
    let mut pool = ProcessRenderPool::new(2);
    let (tx, rx) = std::sync::mpsc::channel();
    pool.push_mock_slot_for_test(crate::site::site_key("https://forged.example/"), rx, true);
    pool.send(test_req(41, "https://forged.example/a"))
        .expect("send 41");
    pool.send(test_req(42, "https://forged.example/b"))
        .expect("send 42");

    tx.send(Err(IpcError::Validation("bad site token".to_owned())))
        .expect("inject validation failure");
    let drained = pool.drain_tagged();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].0, Some(41));
    assert_eq!(drained[1].0, Some(42));
    assert!(drained.iter().all(|(_, res)| res.is_err()));
    assert_eq!(pool.len(), 0, "a forged renderer must be retired");
}

#[test]
fn slots_for_distinct_sites_get_distinct_tokens() {
    // Deterministic derivation still separates sites; this is the tag authenticated by
    // the reader thread on every response envelope (#2308).
    let mut pool = ProcessRenderPool::new(2);
    let (tx_a, rx_a) = std::sync::mpsc::channel();
    let (tx_b, rx_b) = std::sync::mpsc::channel();
    pool.push_mock_slot_for_test(
        crate::site::site_key("https://token-a.example/"),
        rx_a,
        true,
    );
    pool.push_mock_slot_for_test(
        crate::site::site_key("https://token-b.example/"),
        rx_b,
        true,
    );
    // Channels stay alive for the duration of the assertions.
    let _ = (tx_a, tx_b);
    assert_ne!(pool.slots[0].site_token, pool.slots[1].site_token);
    assert_eq!(pool.slots[0].site_token.len(), 32);
}
