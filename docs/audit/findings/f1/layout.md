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
P3 grid.rs:891-901 — 0.1 px tolerance vs 0.01 elsewhere.
P3 inline_block.rs:76-143 — baseline tests assert height >= N only.
P3 fonts_integration.rs:76 — x_height 8.0 is the 0.5 em fallback (DejaVu OS/2 v1 has no sxHeight); test claims "real OpenType metrics".
P3 ifc_adversarial_tests.rs:152-171, 799-817, 820-837 — pre-wrap only checks children>=2; termination tests assert always-true box_type and len==1.
P3 tables.rs:768-786 — table inside inline only checks presence/size>0.
P3 tables.rs:224-228 — 1.0 px tolerance for caption+28 height.
info flex.rs:59-83 + grid.rs:69-95 + calc.rs:43-66 + arc_style.rs:156-167 + tables.rs:916-941 — find_box helper duplicated 5x; tables.rs version panics and ignores Table/TableRow.
info proptest_layout.rs:115-131 — determinism compared only at the root.
