# css+svg+html tests (27 rs, 7163 lines) — agent 5ae87258
P1 html/tests/serializer_tests.rs:1 — file is 1 byte (a newline), tracked in git → zero WHATWG §13.3 serializer tests.
P1 html/tests/proptest_html.rs:45,105 — `|| true` makes two proptests infallible (`!serialize_dom_for_test(&doc).is_empty() || input.is_empty() || true`; `html_out.len() >= frag_out.len() || true`).
P1 html/tests/browser_api.rs:269-280 — set_outer_html rollback test never provokes rejection: `let _ = set_outer_html(...)` then `assert!(doc.allocated() >= before)`.
P2 css/tests/parser_depth.rs:26-31 — `let _ = sheet.rules.len();` no assertion (only "no panic").
P2 css/tests/proptest_css.rs:76-92 — proptests compare a call with itself (`sheet.rules.len() == parse_stylesheet(&input).rules.len()`), evaluate_media discarded.
P2 css/tests/effects.rs:115-123 — cap assertion duplicates the const literal (`<= 16` implied by `== MAX_FILTER_FUNCTIONS`); same in replaced_effects.rs:148-149, gradients.rs:233-234.
P2 css/tests/state_vars_calc.rs:663-673 — calc checked via Debug substring `format!("{position:?}").contains("15")` → passes for 15px/115px/15%/-15px.
P2 css/tests — NO tests for @supports, @import, animation, transition, numeric specificity, :has() (grep 0 hits) despite src refs (selector.rs:339 specificity; 176 animation, 81 transition refs).
P2 html/tests/robustness.rs:183,511,608,655,711 — wall-clock thresholds 2/3/5/10 s without perf.rs's MEGI_PERF_BUDGET_SCALE knob.
P2 html/tests/robustness.rs:582-588 — PI cap asserted only `if let Some(len) = target_len` (test passes if no PI is emitted).
P3 svg/tests/pattern_text.rs:118,266,283 — only buffer size and laxo inequalities (`count(&stroked) >= count(&filled)`).
P3 svg/tests/render.rs:266-311 — miter join asserted by `data.iter().any(|byte| *byte != 0)`.
P3 svg/tests/style_budget.rs:48-59 — renders `<style>.a{fill:red}</style>` but only checks image dimensions.
P3 css/tests/typography_props.rs:83-95 — test ends in dead statement `let _ = Display::Block;`.
Cut for size: calc_lengths.rs:44 (`calc(100% - 20px)` leaves style.width = Px(-20.0)); color4.rs:108-144 (lab/lch vs oklab compared to each other, no reference values); filters_smil.rs:88-90 duplicates blend_lighten_with_flood:70; textpath_use.rs:121-124 (dark-pixel count depends on font); shape_budget.rs:95-106 (5 s recv_timeout); svg/src/lib.rs:54-57 (markers/stroke-dasharray/vector-effect/feTile documented as ignored, no test asserting they are ignored without panic).
Note: crates/html/tests has NO html5lib conformance tests; vendored suites run from crates/test_harness (serializer_harness.rs:828 `let passed = case.expected.contains(&actual)` compares for real).
