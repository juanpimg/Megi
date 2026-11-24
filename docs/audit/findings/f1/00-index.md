# Phase F1 — test suites, fuzz targets and fixtures

Every file in the scope below was read in full by a dedicated auditor; the raw reports were
consolidated into this index on 2026-11-24 and each tracked finding was filed as an issue.

| Scope | Files | Lines | Report | Issues |
| --- | --- | --- | --- | --- |
| `crates/layout/tests` | 21 | 9,335 | `layout.md` | #1380–#1385 |
| `crates/net/tests` | 28 | 9,251 | `net.md` | #1343–#1352 |
| `crates/render/tests`, `crates/fonts/tests`, `crates/test_harness/tests`, `crates/core/tests` | 42 | 8,665 | `render-fonts-th-dom.md` | #1386–#1390 |
| `crates/paint/tests`, `crates/image/tests` | 31 | 9,456 | `paint-image.md` | #1363–#1371 |
| `crates/js/tests`, `crates/browser/tests` | 22 | 7,614 | `js-shell.md` | #1372–#1379 |
| `crates/css/tests`, `crates/svg/tests`, `crates/html/tests` | 27 | 7,163 | `css-svg-html.md` | #1353–#1362 |

Totals: 171 test files, 51,484 lines. (The ledger counts 179 test files; the remainder are
helper modules and fixture generators read within the same scopes.)

Recurring defects, in the order they matter:

1. **Assertions that cannot fail** — `|| true` (`crates/html/tests/proptest_html.rs:45,105`),
   a call compared with itself (`crates/css/tests/proptest_css.rs:79`), a `const {}` block
   (`crates/paint/tests/robustness.rs:72`), a counter computed and discarded
   (`crates/fonts/tests/font_engine.rs:192`), an eval of an unrelated literal
   (`crates/js/tests/gap_apis.rs:158`).
2. **Detached server threads** — the h2 test servers are spawned with `std::thread::spawn` and
   the handle dropped (`crates/net/tests/upload_streaming.rs:354`,
   `crates/net/tests/h2_advanced.rs:73`), so the assertions inside them never fail a test.
3. **Wall-clock budgets** — 2/3/5/10 s thresholds in `crates/html/tests/robustness.rs`,
   `crates/layout/tests/floats.rs:474`, `crates/net/tests/push_priority_tests.rs:101`; some
   ignore the `MEGI_PERF_BUDGET_SCALE` knob that exists for exactly this.
4. **Negative windows** — 150 ms/400 ms windows that cannot distinguish a compliant from a
   violating peer (`crates/net/tests/h2_flow_multiplex_tests.rs:478`).
5. **Missing coverage of the core** — no tests for `@supports`, `@import`, `animation`,
   `transition` or numeric specificity in `crates/css/tests`, and an empty
   `crates/html/tests/serializer_tests.rs`.
6. **Oracles built from the code under test** — the EXIF orientation check calls the same
   `apply_orientation` it is supposed to validate (`crates/image/tests/exif.rs:58`).


## css-svg-html (consolidado 2026-11-24)

# css+svg+html tests (27 rs, 7163 lines) — agent 5ae87258
P1 html/tests/serializer_tests.rs:1 — file is 1 byte (a newline), tracked in git → zero WHATWG §13.3 serializer tests.
P1 html/tests/proptest_html.rs:45,105 — `|| true` makes two proptests infallible (`!serialize_dom_for_test(&doc).is_empty() || input.is_empty() || true`; `html_out.len() >= frag_out.len() || true`).
P1 html/tests/browser_api.rs:269-280 — set_outer_html rollback test never provokes rejection: `let _ = set_outer_html(...)` then `assert!(doc.allocated() >= before)`.
P2 css/tests/parser_depth.rs:26-31 — `let _ = sheet.rules.len();` no assertion (only "no panic").
P2 css/tests/proptest_css.rs:76-92 — proptests compare a call with itself (`sheet.rules.len() == parse_stylesheet(&input).rules.len()`), evaluate_media discarded.
P2 css/tests/effects.rs:115-123 — cap assertion duplicates the const literal (`<= 16` implied by `== MAX_FILTER_FUNCTIONS`); same in replaced_effects.rs:148-149, gradients.rs:233-234.
P2 css/tests/state_vars_calc.rs:663-673 — calc checked via Debug substring `format!("{position:?}").contains("15")` → passes for 15px/115px/15%/-15px.
P2 css/tests — NO tests for @supports, @import, animation, tr

## js-shell (consolidado 2026-11-24)

# js+shell tests (22 files, 7614 lines) — agent 7929d7c8
P2 js/tests/es_modules.rs:236-264 — eval_module rejects legal ES cycles and the suite pins it (`expect_err(cycle)` + `ModuleCycle`); tla_live_bindings.rs:196-221 expects total==3 on the same graph; eval_module_async supports cycles → two entry points with opposite semantics, both green (runtime.rs:4124).
P2 js/tests/gap_apis.rs:79-86 — normalize() asserted to return 2; spec says undefined; wpt_conformance_r233.rs:294 same; runtime.rs:4592 returns the number of removed nodes.
P2 js/tests/gap_apis.rs:158-167 — scroll_into_view_is_noop tautology: `eval(el.scrollIntoView())` then `assert_eq!(eval("1"),1)`.
P2 js/tests/js_tests.rs:293-315 — stale_node accepts any JsError::Runtime (third clause `matches!(error, JsError::Runtime(_))` is always true).
P2 js/tests/event_advanced_tests.rs:272-297 — budget test discards the Result (`let _ = result`) and only greps the console.
P2 js/tests/dom_edges_tests.rs:186-209 — HierarchyRequestError captured into __err but never asserted (only arena `after <= before + 4`).
P2 js/tests/es_modules.rs:180-204, :235 — import_meta_url_is_available never inspects import.meta.url (exports ignored; assert

## layout (consolidado 2026-11-24)

# layout tests (21 files, 9335 lines) — agent abe967a9
P2 proptest_layout.rs:110 (and :140,:155,:167,:188,:208,:238,:264,:284) — `if let Some(root) = layout_document(...)`; None makes all 9 properties vacuous.
P2 flex.rs:321-338 — test_baseline_alignment_uses_line_boxes has comment "only checks that the option is accepted"; asserts y>=0 and x==50 only.
P2 adversarial_harness.rs:250-319, :786-867 — vertical-align:middle only bounded inside the line box.
P2 floats.rs:474-500 (also ifc_adversarial_tests.rs:202,229,652-656,757-761,791-795) — wall-clock thresholds (3 s, 500 ms, 2 s), as_secs() truncation; name says "linear" but single N.
P2 deep_tree.rs:61-92 — "default sized stack" test sets .stack_size(8 MiB) explicitly at :67.
P2 tables.rs:826-829 — column width only asserted > 0.0.
P3 ifc_adversarial_tests.rs:259-285 — NaN/negative/subnormal width test only asserts is_some + println.
P3 tables.rs:806-809 — one-sided bound width <= 12.5 for empty cells.
P3 calc.rs:196-262 — "em/multiplication" tests use neither em nor product (CalcValue{percent,px} only).
P3 aspect_ratio.rs:129-132 — first layout cloned then `drop(replaced)`; asserts nothing.
P3 grid.rs:891-901 — 0.1 px tolerance vs 

## net (consolidado 2026-11-24)

# net tests (28 files, 9251 lines) — agent 697c60fe
P1 browser_features_tests.rs:670-691 — test blesses s-maxage in a private cache (RFC 9111 5.2.2.1): asserts some(Duration::from_secs(90)) for `s-maxage=120, Age: 30`; crates/net/src/cache.rs:79-83 returns Some(s_maxage). Fix: ignore s_maxage in explicit_lifetime.
P1 browser_features_tests.rs:694-712 — must-revalidate revalidates FRESH entries; cache.rs:98-101 `pub const fn requires_revalidation(&self) -> bool { self.no_cache || self.must_revalidate }` has no staleness input; test asserts the wrong side at :711.
P1 upload_streaming.rs:354-362 — `std::thread::spawn(move || { ... handler(&mut stream); });` handle dropped; tests :432/:472/:496/:529 never join so asserts at :436/:446/:481 are dead.
P1 h2_advanced.rs:73-81 — spawn_server drops handle; :213-216 assert_eq!(order, vec!["/one","/high","/low"]) and :407 enable_push==Some(1) run in that thread; :239-253 accepts any Err(Http2).
P1 request_bodies.rs:71-103 — collect_request_body hardcodes END_STREAM=true and empty trailers → :259 assert!(end_seen) tautology, request trailers never sent.
P2 trailers_tests.rs:206-249 — helper named ..._with_end_stream never reads end_stream, so :

## paint-image (consolidado 2026-11-24)

# paint+image tests (31 files, 9456 lines) — agent 58aa2101
P1 paint/tests/clipping.rs:257-334 — background-clip:text tests count WHITE pixels over the whole 100x80 canvas (container is 60x50 at (10,10)) so ~5000 white px remain with the mask unimplemented; :291 assert white>100, :331 blue>20.
P1 image/tests/exif.rs:55-74 — orientation oracle uses the production transform under test: expected = apply_orientation(reference, Some(orientation)) compared against decoded, which itself came from decode()->apply_orientation; identity regression keeps all 8 cases green.
P2 image/tests/exif.rs:30-74 — all eight jpeg/exif_o*.jpg are 828 B, 12x8, differ only in tag 0x0112; md5 of SOS..EOF identical; no generator, README omits them.
P2 paint/tests/robustness.rs:72-93 — vacuous limit tests: `const { assert!(MAX_DISPLAY_LIST_ITEMS >= 1_000_000) }` (compile-time) and `display_list.len() <= MAX` with 20_000 items vs 106_136.
P2 paint/tests/robustness.rs:140-172 — hostile text size: `if let DisplayItem::Text{..}` with no assertion that a Text exists.
P2 paint/tests/png_format.rs:157-167 — filters diversity: inserts then `assert!(!filters.is_empty())` (tautology).
P2 paint/tests/effects.rs:263-290 —

## render-fonts-th-dom (consolidado 2026-11-24)

# render/fonts/test_harness/dom tests (42 files, 8665 lines) — agent 99804d8d
P2 render/tests/import_map_tests.rs:66-91 — dynamic import() test renders a data: URL (`render_html(html, 240, 160)`), admits ./dyn.js cannot resolve, only requires a warning containing "module"; the fixture written at :76-78 is never used.
P2 render/tests/network_advanced_tests.rs:114-125 — spawn_router blocks in accept() without a deadline inside `for _ in 0..connections`; server.join() at :228,:303,:333,:364 hangs if the renderer requests fewer subresources.
P2 fonts/tests/font_engine.rs:192-197 — `.notdef` count computed then discarded (`let _ = run.glyphs.iter().filter(|g| g.glyph_id == 0).count();`); only "run non-empty" and width>0 asserted.
P2 test_harness/tests/closed_loop.rs:284-289 — parse-error order gate compares cardinality only: `assert_eq!(suite_order_gaps, allowed_order_gaps)` with allowlist [("test3.test",34),("test4.test",3)]; one gap fixed + one new in the same suite keeps the count.
P2 render/tests/streaming_option_tests.rs:20-24 — test of `stream_responses` cannot fail: the flag is only read at definition (src/lib.rs:436), default (:498) and this test; it certifies a no-op option.
P3