[render] build --no-default-features no compila (ws_bridge sin puerta cfg)

## Resumen
`crates/render/src/ws_bridge.rs:8-9` hace `use js::...` incondicional y `crates/render/src/lib.rs:215` declara `pub mod ws_bridge;` incondicional, frente a `#[cfg(feature = "js")] mod browser_host`. La configuración documentada sin `js` (`--no-default-features`) es incompilable desde cf388f6.

## Impacto
P2 — contrato roto: configuración documentada incompilable. Sin impacto en confidencialidad/integridad; bloquea builds mínimos/embebidos.

## Pasos para reproducir
1. En worktree limpio HEAD 3c06db8, con `TMPDIR=/home/jp/.cache/megi-tmp CARGO_TARGET_DIR=/home/jp/.cache/f2-target`:
2. `cargo check -p render --no-default-features`

## Esperado vs observado
Esperado: check ok sin feature `js`.
Observado: `error[E0433]: cannot find module or crate 'js' in this scope --> crates/render/src/ws_bridge.rs:8:5` (ídem `:9:5`) · `error: could not compile 'render' (lib) due to 2 previous errors`.

## Evidencia literal
```
error[E0433]: cannot find module or crate 'js' in this scope --> crates/render/src/ws_bridge.rs:8:5
```
Código: `ws_bridge.rs:8-9` (`use js::...` incondicional) + `lib.rs:215` (`pub mod ws_bridge;` incondicional).

Commit: 3c06db8

## Workaround
Compilar con features por defecto (`js` activado).

## Dedup
Dedup: new (sin colisión en ledger, GAPS ni abiertas). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P2 mantener. Fix sugerido: `#[cfg(feature = "js")] pub mod ws_bridge;`.
