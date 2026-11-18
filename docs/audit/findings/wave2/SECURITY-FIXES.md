# Ola 2 — correcciones de seguridad mergeadas a `main`

Los 7 hallazgos tratados como aviso privado han sido arreglados con TDD (test RED visto antes del fix) y mergeados directamente a `main` el 2026-11-18.

| Advisory | Commit | Arreglo |
|---|---|---|
| `w2-net-001` cookie de PUSH antes del CORS | `b26fe91` | `store_pushed` valida CORS antes de escribir en el jar; el push rechazado no toca jar ni caché. Test: `cross_origin_push_with_set_cookie_leaves_no_cookie`. |
| `w2-net-002` status bogus en handshake | `54bf6c6` | `validate_handshake_response` parsea estrictamente el código `101` (rechaza `200 OK ( 101 ...)`). |
| `w2-net-d01` sin validación Upgrade/Connection | `54bf6c6` | Exige `Upgrade: websocket` y token `Upgrade` en `Connection` (case-insensitive, listas). |
| `w2-net-003` máscara WS fija | `4f80af4` | `client_mask()` deriva 4 bytes de `RandomState` + contador + tiempo + pid; `encode_frame` y el vector RFC 6455 intactos. |
| `w2-render-01` amplificación de iframes | `d302b2f` | `FrameBudget` global compartido por la recursión; 4×4 self-nested pasa de 341 document loads a 17 (1 + 16 frames). |
| `w2-st-1173` favicon `file:` desde http | `8f7e5d7` | `fetch(icon_url, document_url, options)`: `file:` sólo se permite si la página también es `file:`. |
| `w2-st-1176` `.tmp` predecible / symlink race | `bb08550` | Nombres `.<name>.<pid>.<nanos>.<counter>.tmp` con `OpenOptions::create_new` y reintento; usado por `write_atomic`, `save_screenshot` y `write_bytes`. |

## Verificación

En `HEAD` `3a9218f` (worktree limpio): `cargo fmt --all -- --check` → 0, `cargo clippy --workspace --all-targets -- -D warnings` → 0, `cargo check --workspace --no-default-features` → 0, `cargo test --workspace` → 0, `cargo run --release -p test_harness -- --summary` → 0, `python3 tools/audit_coverage.py --check --strict` → 0. Evidencia en `/tmp/megi-pushverify.log`.

`git rev-list --left-right --count origin/main...HEAD` → `0 0` tras `git push origin main`.

Los issues públicos `#1618–#1687`, `#1689–#1691` siguen abiertos; los avisos no se publicaron como issues, pero su corrección ya está en `main`.
