# Phase F1 — test suites, fuzz targets and fixtures

Every file in the scope below was read in full by a dedicated auditor; the raw reports live
next to this index and each finding that was worth tracking was filed as an issue.

| Scope | Files | Lines | Report | Issues |
| --- | --- | --- | --- | --- |
| `crates/layout/tests` | 21 | 9,335 | `layout.md` | #1380–#1385 |
| `crates/net/tests` | 28 | 9,251 | `net.md` | #1343–#1352 |
| `crates/render/tests`, `crates/fonts/tests`, `crates/test_harness/tests`, `crates/dom/tests` | 42 | 8,665 | `render-fonts-th-dom.md` | #1386–#1390 |
| `crates/paint/tests`, `crates/image/tests` | 31 | 9,456 | `paint-image.md` | #1363–#1371 |
| `crates/js/tests`, `crates/shell/tests` | 22 | 7,614 | `js-shell.md` | #1372–#1379 |
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
