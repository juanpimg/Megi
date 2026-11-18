[css] VerticalAlign::parse sin tope de magnitud 1e6 (is_finite SÍ se filtra)

## Resumen
`VerticalAlign::parse` (`crates/css/src/properties.rs:989-1018`) filtra
`is_finite` en ambas ramas (`:1004-1006` y `:1009-1011`) — el claim anterior
«sin `is_finite`» era falso y se retira. Lo que falta es sólo el tope de magnitud
1e6 de `Length::parse`: `vertical-align: 999999999px → Length(1e9)`.
Negativos SÍ son válidos (CSS 2.1 §10.8.1 permite bajar la caja). Escisión Mesa B
de w2-core-10 (mitad `VerticalAlign`, cuerpo corregido).

## Impacto
P3 — inconsistencia de validación entre dos parseadores de longitudes.

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas, casos V1/V0b):
   `VerticalAlign::parse("999999999px")`, `VerticalAlign::parse("inf")`.

## Esperado vs observado
Esperado: mismo tope 1e6 que `Length::parse` (o excepción documentada).
Observado: `Some(Length(1000000000.0))` para 1e9; `inf → None` (finito sí filtrado).

## Evidencia literal
```
V1 valign huge: Some(Length(1000000000.0))
V0b valign inf: None
```

Commit: 3c06db8

## Dedup
Dedup: new (escisión de w2-core-10; corrección Mesa B: retirado el claim falso sobre `is_finite`).
Mesa A: PARTIAL (tope ausente confirmado, `is_finite` presente). Mesa B/B2: SOLO NUEVO CON CORRECCIÓN, P3 mantener.
