[svg] first_number ignora unidades: x="10px" en <text> vale 0 (y stdDeviation="2px" descarta el filtro)

## Resumen
`first_number` (`crates/svg/src/document.rs:2813-2821`) hace `part.trim().parse::<f32>()` directo del token, por lo que `"10px"` falla → `0.0`. Es inconsistente con `rect`/`length_attr` (que sí aceptan `px`). Efecto grave: `stdDeviation="2px"` → `0.0` → el primitivo `feGaussianBlur` se descarta entero (`document.rs:2621-2628`).

## Impacto
P2 — bug de corrección: texto desplazado al origen; filtros con unidades en `stdDeviation` se pierden en silencio. El P2 se sostiene por el efecto `feGaussianBlur`, más allá del desplazamiento de texto.

## Pasos para reproducir
1. `docs/audit/findings/wave2/repro-w2-assets/text-px.svg` vs `text-num.svg` (mismo texto, `x="10px"` vs `x="10"`).
2. Comparar mín-x de tinta (sonda Mesa A `textpos`).

## Esperado vs observado
Esperado: misma posición (SVG 1.1 §4, `px` es la unidad nula).
Observado: tinta más a la izquierda en `text-px` (origen) que en `text-num`.

## Evidencia literal
```
text-px.svg: ink_min_x=Some(1) ink_count=74
text-num.svg: ink_min_x=Some(11) ink_count=74
```
Mismo nº de glifos (74), desplazado al origen. La mitad `stdDeviation` es lectura estática (`first_number` + `document.rs:2621-2628`); mecanismo idéntico.

Commit: 3c06db8

## Fix sugerido
Usar `parse_length`-con-referencia o desnudar sufijo `px` en `first_number`.

## Dedup
Dedup: new (sin colisión en ledger). Mesa A: VERIFIED (vía `<text x>`; mitad `stdDeviation` por lectura con el mismo mecanismo). Mesa B: SOLO NUEVO, P2 mantener.
