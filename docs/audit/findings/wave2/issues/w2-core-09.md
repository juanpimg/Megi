[css] Brazo duplicado `first-line | first-letter` (código muerto, cleanup)

## Resumen
Dos brazos idénticos patrón+guarda en `crates/css/src/selector.rs:1725-1727`;
el segundo nunca ejecuta y rustc no avisa con guardas. Es cleanup de
mantenimiento, no bug de producto: impacto nulo en runtime.

## Impacto
P3 (suelo) cleanup — nulo en runtime; ruido de mantenimiento.

## Pasos para reproducir
1. Lectura de `selector.rs:1725-1727`.
2. `cargo build -p css` tras `cargo clean -p css` → 0 warnings.
3. Programa mínimo `/tmp/dup.rs` que imprime `1` (rama 1): el brazo 2 es
   inalcanzable verificado.

## Esperado vs observado
Esperado: un solo brazo.
Observado: duplicación exacta; compilación limpia sin warnings.

## Evidencia literal
```
1
Finished dev profile …
```
(sin warnings en `cargo build -p css`).

Commit: 3c06db8

## Dedup
Dedup: new (sin relacionadas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener (suelo; recategorizar: cleanup, no "test").
