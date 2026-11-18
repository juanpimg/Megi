[css] Transform::parse distingue mayúsculas en nombres de función

## Resumen
`match name` (`crates/css/src/transform.rs:51`) usa literales minúsculos y
`split_functions` no pliega (`:170`). `TRANSLATE(10px)` / `Rotate(90deg)` se
rechazan (valor inválido). Sólo `"none"` usa `eq_ignore_ascii_case` (`:44`).

## Impacto
P3 — declaraciones válidas caen a `none`. Sin crash ni corrupción.

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas):
   `Transform::parse("TRANSLATE(10px)")` → `None`;
   `Transform::parse("translate(10px)")` → `Some(...)`.

## Esperado vs observado
Esperado: CSS Syntax §4.3.4 / Transforms — function names ASCII case-insensitive.
Observado: `TRANSLATE` en mayúsculas se rechaza.

## Evidencia literal
```
T1 TRANSLATE upper: None
T1b translate lower: Some(Transform { e: 10.0, .. })
```

Commit: 3c06db8

## Dedup
Dedup: new (rel. #1127 índice por tag, #1452 nombres de grid — distintas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener.
