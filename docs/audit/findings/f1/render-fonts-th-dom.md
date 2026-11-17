# render/fonts/test_harness/dom tests (42 files, 8665 lines) — agent 99804d8d
P2 render/tests/import_map_tests.rs:66-91 — dynamic import() test renders a data: URL (`render_html(html, 240, 160)`), admits ./dyn.js cannot resolve, only requires a warning containing "module"; the fixture written at :76-78 is never used.
P2 render/tests/network_advanced_tests.rs:114-125 — spawn_router blocks in accept() without a deadline inside `for _ in 0..connections`; server.join() at :228,:303,:333,:364 hangs if the renderer requests fewer subresources.
P2 fonts/tests/font_engine.rs:192-197 — `.notdef` count computed then discarded (`let _ = run.glyphs.iter().filter(|g| g.glyph_id == 0).count();`); only "run non-empty" and width>0 asserted.
P2 test_harness/tests/closed_loop.rs:284-289 — parse-error order gate compares cardinality only: `assert_eq!(suite_order_gaps, allowed_order_gaps)` with allowlist [("test3.test",34),("test4.test",3)]; one gap fixed + one new in the same suite keeps the count.
P2 render/tests/streaming_option_tests.rs:20-24 — test of `stream_responses` cannot fail: the flag is only read at definition (src/lib.rs:436), default (:498) and this test; it certifies a no-op option.
P3 fonts/tests/font_engine.rs:173-181 — fi ligature assertion near-unfalsifiable: `(ligature.width - unshaped).abs() <= 1.0 || ligature.glyphs.len() == 1`.
P3 render/tests/pipeline_tests.rs:416-429 — windows-1252 only checked as "painted > 0", which U+FFFD also satisfies.
P3 render/tests/scripting_tests.rs:437-458 — "once per element" not verified: data-count written at :447, never read; only scripts_executed == 3.
P3 render/tests/webp_tests.rs:70-95 — "paints_exact_pixels" compares 4 of 3072 pixels.
P3 fonts/tests/web_fonts.rs:184-194 — rejects_woff2_collections does not isolate the cause (crafted_woff2 already invalid with 1 byte for 65536).
P3 render/tests/images_tests.rs:136-240 — test server replies `HTTP/1.1 {status} OK` even for 404 (same in backgrounds_tests.rs:146); :231-240 accepts `(contains("image") && contains("404")) || contains("skipping image")`.
P3 render/tests/images_tests.rs:94-141 — TestServer/write_response duplicated in 7 files (~90 lines each).
P3 render/tests/scripting_tests.rs:438-458 — temp dirs cleaned only at the end; a failed assert leaves /tmp litter and pid collisions.
