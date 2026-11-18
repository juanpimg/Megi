[css] Comentario UA obsoleto: dice que `:checked`/`:disabled`/`:focus` son `Never`, pero el motor los implementa

## Resumen
`crates/css/src/user_agent.rs:8-9` documenta que el motor mapea
`:checked`/`:disabled`/`:focus` a `PseudoClass::Never`, pero `selector.rs`
los implementa de verdad (`:359-370` variantes `Focus`/`Checked`/`Disabled` con
`state.rs`; `:415-426` evaluación en `matches()`).

## Impacto
P3 docs — confusión de mantenedor/consumidor sobre qué pseudoclases casan.
Sin efecto en runtime (el código es correcto, el comentario no).

## Pasos para reproducir
1. Lectura de `user_agent.rs:8-9` frente a `selector.rs:359-370` y `:415-426`.
2. Sonda Q3/Q3b contra HEAD limpio `3c06db8` (2 ejecuciones idénticas):
   `:checked` casa en input checked, no casa en div plano.

## Esperado vs observado
Esperado: docs y código de acuerdo.
Observado: contradicción doc-vs-código confirmada.

## Evidencia literal
```
Q3 :checked on checked input: true
Q3b :checked on plain div: false
```
Código: `user_agent.rs:8-9` vs `selector.rs:1702-1711` + `:415-426`.

Commit: 3c06db8

## Dedup
Dedup: new (sin relacionadas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener.
