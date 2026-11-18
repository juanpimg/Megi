[css] opacity: NaN se almacena como NaN (el clamp no filtra no-finitos)

## Resumen
`crates/css/src/properties.rs:3200-3203` hace `.parse::<f32>().ok()` sin
`is_finite` y luego `.clamp(0,1)`. `NaN.clamp(0,1)` devuelve NaN, que se almacena
y llega a paint/mezclas alfa. (`opacity: 1e999` colapsa a 1.0 por accidente.)

## Impacto
P2 — NaN en alfa propaga a blending/paint (píxeles indefinidos aguas abajo).

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas):
   `compute("opacity: NaN;").opacity`, `compute("opacity: 1e999;").opacity`.

## Esperado vs observado
Esperado: valores no-finitos son inválidos en CSS → declaración ignorada (1.0 inicial).
Observado: `opacity: NaN` computa `NaN`.

## Evidencia literal
```
O1 opacity 1e999: 1
O2 opacity NaN: NaN
```

Commit: 3c06db8

## Dedup
Dedup: new (rel. #1153 capa layout cerrada, #1445 slice en `Color::parse` — distintas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P2 mantener.
