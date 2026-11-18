[css] line-height acepta infinito, NaN y negativos

## Resumen
`parse_line_height_factor` filtra `>= 0` pero no finitud
(`crates/css/src/properties.rs:3408-3414`); `parse_line_height` acepta cualquier
`f32` (`:3416-3430`). `line-height: 1e999 → Some(inf)`, `NaN → Some(NaN)`,
`-5 → Some(-80.0)`.

## Impacto
P2 — inf/NaN/negativo en altura de línea → matemática de layout con no-finitos.

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas, casos H1/H2/H3):
   `parse_line_height("1e999")`, `parse_line_height("NaN")`, `parse_line_height("-5")`.

## Esperado vs observado
Esperado: CSS 2.2 §10.8.1 — `<number>` no-negativo y `<length>` no-negativa;
no-finitos inválidos.
Observado: inf, NaN y negativos aceptados.

## Evidencia literal
```
H1 line-height 1e999: Some(inf)
H2 line-height NaN: Some(NaN)
H3 line-height -5: Some(-80.0)
```

Commit: 3c06db8

## Dedup
Dedup: new (sin relacionadas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P2 mantener.
