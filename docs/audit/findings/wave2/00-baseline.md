# Wave 2 baseline (2026-11-18)

HEAD: `35151bd8e8b3fc9bc33acec9d04a7019c4ac205e`
User directive: autonomous execution; publish verified issues; security defects go to private advisory.

## Ledger before `--generate`

| category | files | covered | pending | cov% |
|---|---:|---:|---:|---:|
| src | 172 | 160 | 12 | 97.1% |
| test | 217 | 179 | 38 | 90.8% |
| fuzz | 29 | 26 | 3 | 95.7% |
| fixture | 145 | 143 | 2 | 99.9% |
| tool | 31 | 30 | 1 | 99.4% |
| docs | 45 | 44 | 1 | 99.1% |
| TOTAL | 1179 | 623 | 57 | 96.0% |

`--check --strict` before: 3 `not in ledger` (`crates/js/tests/slot_tests.rs`,
`crates/render/src/ws_bridge.rs`, `crates/render/tests/websocket_e2e_tests.rs`)
and 8 `re-read needed`.

## Re-read queue (recomputed at execution time, not inherited from the plan)

- `crates/js/src/host.rs` 1020 → 1035
- `crates/js/src/runtime.rs` 9510 → 9840
- `crates/net/src/websocket.rs` 385 → 678
- `crates/net/tests/websocket_tests.rs` 83 → 100
- `crates/render/src/frames.rs` 129 → 142
- `crates/render/src/lib.rs` 2402 → 2421
- `crates/render/src/scripts.rs` 1973 → 2009
- `crates/render/tests/iframe_tests.rs` 118 → 190

## Environment

`gh auth status`: authenticated as `juanpimg`, scopes gist/read:org/repo/workflow;
API rate limit remaining ~4998. Publication is therefore possible.
