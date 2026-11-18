[css] font-weight: 0 aceptado (fuera del rango CSS 1–1000)

## Resumen
`parse_font_weight` (`crates/css/src/properties.rs:3397-3405`) hace
`other.parse().unwrap_or(400)` sin validación de rango. `font-weight: 0 → 0`
(y `1001 → 1001`); desbordes (`99999999999999999999`) caen a 400 por accidente.
Escisión Mesa B de w2-core-10 (mitad `font-weight`).

## Impacto
P3 — peso 0 indefinido por spec; comportamiento casual según la entrada.

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas, casos W1/W0b):
   `parse_font_weight("0")`, `parse_font_weight("1001")`.

## Esperado vs observado
Esperado: CSS Fonts 4 §2.2 — `<number>` en 1–1000; fuera de rango → inválido (400).
Observado: 0 y 1001 aceptados.

## Evidencia literal
```
W1 font-weight 0: 0
W0b font-weight 1001: 1001
```

Commit: 3c06db8

## Dedup
Dedup: new (escisión de w2-core-10; corrección Mesa B).
Mesa A: PARTIAL (mitad font-weight confirmada). Mesa B/B2: SOLO NUEVO, P3 mantener.
