[css] Brazo muerto `strip_suffix("fr ")` en parse_minmax (máximo ya recortado)

## Resumen
`crates/css/src/grid.rs:623-627`: `.strip_suffix("fr").or_else(strip_suffix("FR"))`
`.or_else(|| max.strip_suffix("fr "))`. `max` viene de `max.trim()` (`:615`),
luego ningún valor termina en espacio: el tercer `or_else` es inalcanzable.
`minmax(10px, 20fr )` casa por la primera rama, nunca por la tercera.

## Impacto
P3 (suelo) cleanup — nulo en runtime; confusión de mantenimiento.

## Pasos para reproducir
1. Lectura estática de `grid.rs:615-631` (2 revisiones del fichero).
2. `parse_template("minmax(100px, 150px)")` (test existente `round19_tests` lo cubre
   por la vía viva).

## Esperado vs observado
Esperado: eliminar el brazo o recortar dentro de la rama si se pretendía tolerar
espacios internos.
Observado: brazo muerto inalcanzable tras `trim()`.

## Evidencia literal
```
let max = max.trim(); … .strip_suffix("fr").or_else(|| max.strip_suffix("FR")).or_else(|| max.strip_suffix("fr "))
```
(`grid.rs:623`; `max` ya `trim()`eado en `:615`).

Commit: 3c06db8

## Dedup
Dedup: new (sin relacionadas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener (suelo cleanup).
