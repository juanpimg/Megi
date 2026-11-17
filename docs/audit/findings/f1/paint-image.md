# paint+image tests (31 files, 9456 lines) — agent 58aa2101
P1 paint/tests/clipping.rs:257-334 — background-clip:text tests count WHITE pixels over the whole 100x80 canvas (container is 60x50 at (10,10)) so ~5000 white px remain with the mask unimplemented; :291 assert white>100, :331 blue>20.
P1 image/tests/exif.rs:55-74 — orientation oracle uses the production transform under test: expected = apply_orientation(reference, Some(orientation)) compared against decoded, which itself came from decode()->apply_orientation; identity regression keeps all 8 cases green.
P2 image/tests/exif.rs:30-74 — all eight jpeg/exif_o*.jpg are 828 B, 12x8, differ only in tag 0x0112; md5 of SOS..EOF identical; no generator, README omits them.
P2 paint/tests/robustness.rs:72-93 — vacuous limit tests: `const { assert!(MAX_DISPLAY_LIST_ITEMS >= 1_000_000) }` (compile-time) and `display_list.len() <= MAX` with 20_000 items vs 106_136.
P2 paint/tests/robustness.rs:140-172 — hostile text size: `if let DisplayItem::Text{..}` with no assertion that a Text exists.
P2 paint/tests/png_format.rs:157-167 — filters diversity: inserts then `assert!(!filters.is_empty())` (tautology).
P2 paint/tests/effects.rs:263-290 — box-shadow cap only `shadows <= 32` with 40 authored (zero passes).
P2 paint/tests/gradients.rs:363-387, :806 — stop-cap tests assert only `pixmap.width == 60`.
P2 image/tests/jpeg_gif.rs:116-130 — GIF per-index transparency never asserted inside the frame (alpha 0 only checked outside the 8x6 frame).
P3 image/tests/webp_tests.rs:299-321 — VP8X ALPH checks a single byte (`decoded.data[3] == 128`).
P3 image/tests/webp_tests.rs:520-556 — animated WebP disposal unobservable (frame 1 is last; green bg never asserted).
P3 paint/tests/end_to_end_html.rs:331-341 vs 529 — duplicate determinism tests.
P3 image/tests/fixtures/README.md:23 — says progressive.jpg "must be rejected", but jpeg_gif.rs:83 decodes it with expect(); large_800x600.png perf claim untested; exif_o* undocumented.
P3 paint/tests/replaced_backdrop.rs:157-175 — cover-crop accepts red OR green (`top_left.r > 120 || top_left.g > 120`).
