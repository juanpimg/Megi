# Wave F2 — seven risk lenses over `crates/*/src`

Scope: all 160 source files (127,173 lines) read a second time, each lens with its own
question, instead of once more with the same question. Every lens ran as an isolated
read-only auditor over the whole `src` tree and reported only findings that wave 1 had not
already filed (the already-filed ranges #1105–#1390 were listed in the prompt).

| lens | question | findings | issues |
| --- | --- | --- | --- |
| L1 arithmetic | overflow, saturating casts, truncated `as`, unguarded `+`/`*` | 3 | #1440–#1442 |
| L2 panics | indexing, slicing, `unwrap`/`expect`, `panic!` reachable from page input | 4 | #1445 (dup of the L4 hit), #1465–#1467 |
| L3 limits | unbudgeted work, superlinear loops, per-element budget resets | 7 (+2 withdrawn) | #1412–#1418 |
| L4 UTF-8 | byte indexing, lossy decoding, bidi, CSS escapes | 10 | #1444–#1453 |
| L5 swallowed errors | `let _ =`, `ok()`, error→default, dead invariants | 10 | #1454–#1463 |
| L6 global state | thread-locals, `OnceLock` caches, process-wide registries, panics | 9 | #1468–#1476 |
| L7 contract | docs vs behaviour, dead variants, wrong error type, spec deviation | 8 | #1404–#1411 |

## Highest-severity findings

* **#1465** `worker_terminate` indexed `host.workers[id as usize]` with an
  attacker-writable `this.__megi_worker`: `w.__megi_worker = 1000; w.terminate()` aborted the
  host with `index out of bounds: the len is 1 but the index is 10000`. Fixed in PR #1490.
* **#1445** `Color::parse` sliced `s[..4]`/`s[..5]` before the char-boundary check, so
  `color:abcé` panicked inside the cascade. Fixed in PR #1477.
* **#1444** the fragment parser byte-sliced the context element name (`&trimmed[..256]`);
  **#1454** then replaced the caller's `Document` with an empty one when that panic
  unwound. Both need a fix that also stops wiping the DOM.
* **#1412** CSS rule nesting recurses with no depth cap (~52 KB of `a{` overflows the
  stack); **#1413** nothing caps the rule count and `match_scope()` resets its 200 000-step
  budget per element.
* **#1415** box/text-shadow layers never debit `PaintBudget` (32 layers × 16.7 Mpx × 6 blur
  passes); **#1417** float placement is O(F²) with no float cap.
* **#1404** URL normalization decoded any `%?E` to `.` (`%7Ealice` → `.alice`). Fixed in
  PR #1443.
* **#1468** the single render worker has no `catch_unwind`: one panicking page bricks
  rendering for the whole session; **#1471** tall pages render twice with scripting on, so
  every script, timer and fetch runs twice.
* **#1469** `calc(50vw)` resolves against a hard-coded 1280×720 viewport that production
  never installs; **#1470** `getBoundingClientRect()` is frozen to a pre-script `OnceLock`
  snapshot.

## Recurring classes

1. **Budget resets and uncharged work.** Limits exist but are charged per element, per paint
   or per definition, so a document can multiply the work without tripping any of them
   (#1413, #1415, #1417, and wave-1 #1267 for SVG effects).
2. **Global caches that make one failure permanent.** `OnceLock<Option<..>>` and friends turn
   a transient error into a process-lifetime outage (#1473 TLS, #1461/#1474 fonts and
   color-mix, #1475 PAC).
3. **Internals reachable from page script.** Writable handles and unchecked indexes (#1465)
   let a page abort the host, bypassing every limit.
4. **Panic paths that also destroy state.** #1444/#1454 (panic then wipe), #1468 (no
   `catch_unwind`), #1472 (dispatch state not restored on error).
5. **Silent degradation.** Warnings and limit violations are computed and thrown away
   (#1457, #1458, #1460), so a truncated page looks like a success.

## Withdrawn

* #1419 (GIF frame budget) and #1420 (JPEG progressive scans) were filed from inference, not
  from evidence: `crates/image/src/gif.rs:130-165` decodes only the first frame and charges it
  against `limits.max_dimension`/`max_pixels`. Both were closed immediately. Lesson: a finding
  ships only with a probe, a measurement or a spec quote.
