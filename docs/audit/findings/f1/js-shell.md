# js+shell tests (22 files, 7614 lines) — agent 7929d7c8
P2 js/tests/es_modules.rs:236-264 — eval_module rejects legal ES cycles and the suite pins it (`expect_err(cycle)` + `ModuleCycle`); tla_live_bindings.rs:196-221 expects total==3 on the same graph; eval_module_async supports cycles → two entry points with opposite semantics, both green (runtime.rs:4124).
P2 js/tests/gap_apis.rs:79-86 — normalize() asserted to return 2; spec says undefined; wpt_conformance_r233.rs:294 same; runtime.rs:4592 returns the number of removed nodes.
P2 js/tests/gap_apis.rs:158-167 — scroll_into_view_is_noop tautology: `eval(el.scrollIntoView())` then `assert_eq!(eval("1"),1)`.
P2 js/tests/js_tests.rs:293-315 — stale_node accepts any JsError::Runtime (third clause `matches!(error, JsError::Runtime(_))` is always true).
P2 js/tests/event_advanced_tests.rs:272-297 — budget test discards the Result (`let _ = result`) and only greps the console.
P2 js/tests/dom_edges_tests.rs:186-209 — HierarchyRequestError captured into __err but never asserted (only arena `after <= before + 4`).
P2 js/tests/es_modules.rs:180-204, :235 — import_meta_url_is_available never inspects import.meta.url (exports ignored; asserts ModuleOutcome.url of another module).
P2 shell/tests/shell_tests.rs:1440-1448 — about:history accepts two exclusive states (`contains("No history yet") || contains("Title")`) and drops dimensions().
P3 js/tests/wpt_conformance_r233_extra.rs:181-195 — dataset_reflects_and_deletes asserts dataset does NOT reflect (el.dataset.fooBar stays null) and comments "read-only view"; real spec deviation presented as expected.
P3 js/tests/wpt_conformance_r233_extra.rs:366-383 — getComputedStyle contract contradiction: test asserts style.display === undefined ("documented gap") while js/src/lib.rs:78 documents returning "" (events.rs:1178-1245 builds a plain object from the provider).
P3 js/tests/es_modules_stub.rs:15-19 — placeholder asserts JsRuntime::new().is_ok(); gap_apis.rs:190 `let _ = Budget::default();` inside `#![cfg(feature="js")]` → --no-default-features stub only covered by es_modules_stub.rs:3-13.
P3 js/tests/wpt_conformance_r233.rs:253-268 — timing: `log.contains(...)` with no order, real clock; wpt_like_r167.rs:259-274 `let _ = run_tasks(64); let _ = advance(20ms)` then expects timeout/raf; browser_features_r24.rs:457-482 wall-clock with MEGI_PERF_BUDGET_SCALE.
P3 shell/tests/shell_tests.rs:1382-1412 — image fixture reached via `../image/tests/fixtures/png/rgba8.png` and copied as favicon.ico.
P3 shell/tests/shell_tests.rs:1209-1246 — 13 `std::process::Command::new` spawns (~22 renders) duplicating library-level tests.
