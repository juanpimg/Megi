[perf] layout/* y pipeline/* miden con el measurer fallback, no el de producción

## Resumen
Las 3 call sites citadas (`crates/bench/src/benches.rs:861,942,1025`) usan
`layout_document(&doc, &styles, viewport)` (+ `expect("layout")` en :1025).
`crates/layout/src/lib.rs:49-64`: `layout_document()` = "Backwards-compatible
deterministic measurement (CSS ratio fallback). The production render pipeline
must use `layout_document_with_measurer` with `FontTextMeasurer`…".
Producción sí lo usa (`crates/render/src/lib.rs:1269,1496`
`layout_document_with_measurer_and_pseudos`); el bench no.

## Impacto
P2 — el coste real de shaping/line-box no está en la baseline (r36: 35,763 ms y
110,124 ms); una regresión de `FontTextMeasurer` es invisible salvo en la fila E2E;
presupuestos (§11: layout ≤45, pipeline ≤140) calibrados contra el measurer barato.

## Pasos para reproducir
1. `grep -n "layout_document(" crates/bench/src/benches.rs` → call sites
   `:353,:387,:452,:568,:600,:623,:861,:942,:1025`, todos a `layout_document(...)`.
2. `sed -n '49,64p' crates/layout/src/lib.rs` (cita fallback).
3. `grep layout_document_with_measurer crates/render/src/lib.rs` (producción lo usa).
4. `cargo run --release -p bench -- --filter layout/realistic` (mide con fallback).

## Esperado vs observado
Esperado: filas layout/pipeline con el measurer de producción (determinista con
fuentes empaquetadas) o divergencia documentada en PERFORMANCE.md.
Observado: todas las filas layout/pipeline con el fallback barato.

## Evidencia literal
```
bench: layout_document(...) en :861,:942,:1025
lib: "Backwards-compatible deterministic measurement (CSS ratio fallback)…"
producción: layout_document_with_measurer_and_pseudos (render/src/lib.rs:1269,1496)
```

Commit: 3c06db8

## Workaround
Ninguno para la baseline; calibrar presupuestos sabiendo que excluyen shaping real.

## Fix sugerido
`layout_document_with_measurer(&doc, &styles, viewport, &FontTextMeasurer::bundled()?)`
en los 3 sitios (feature `fonts` del bench ya existe, `BENCH_FEATURES=fonts`);
o documentar la divergencia.

## Dedup
Rel. #1201 (Deferred; call sites intactos). Se abre issue nuevo referenciando
#1201 con call sites verificados en HEAD. Mesa A: VERIFIED.
