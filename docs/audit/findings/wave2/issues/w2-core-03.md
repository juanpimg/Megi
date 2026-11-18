[css] calc() elude el tope `|x| > 1e6` de Length::parse

## Resumen
`Length::parse` topa en `|x| > 1e6` (`crates/css/src/properties.rs:386-424`),
pero `calc.rs::number()` (`crates/css/src/calc.rs:260-326`) sólo filtra no-finitos
y `to_length` (`:46-47`) no aplica tope. `width: 99999999px` es inválido pero
`width: calc(99999999px)` computa `Px(1e8)` (y `calc(99999999em)` → `Px(1.6e9)`).

## Impacto
P3 — validación inconsistente; valores gigantes llegan a layout/paint por una sola
vía. Resultantes finitos, sin no-finitos ni crash → no P2.

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas):
   `Length::parse("99999999px")` vs `Length::parse("calc(99999999px)")`,
   `Length::parse("calc(99999999em)")`, caso pct.

## Esperado vs observado
Esperado: el tope anti-DoS se aplica a todo camino que produzca `Length`
(o excepción documentada).
Observado: vía `calc()` sin tope.

## Evidencia literal
```
L1 px huge: None
L2 calc huge: Some(Px(100000000.0))
L5 calc em huge: Some(Px(1600000000.0))
L7 calc pct huge: Some(Percent(100000000.0))
```

Commit: 3c06db8

## Dedup
Dedup: new (rel. #1128 recursión de signos, #1469 viewport — distintas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener.
