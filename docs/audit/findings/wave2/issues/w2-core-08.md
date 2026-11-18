[css] Docstring obsoleto: dice que `:is()/:where()/:not()/:has()` invalidan, pero están soportados

## Resumen
Docstring `crates/css/src/selector.rs:1219-1220` («currently invalidate»)
contradicho por el parser (`:1690-1693` parsea `is`/`where`/`not`/`has` reales)
y sus tests (`test_is_where_not_matching_and_specificity`). La parte F13 del
docstring sigue cierta (pseudos desconocidas sí invalidan).

## Impacto
P3 docs — lectores creen que `:is()` invalida listas; el comportamiento real es
correcto. Sin efecto en runtime.

## Pasos para reproducir
1. Sonda Q1/Q2 contra HEAD limpio `3c06db8` (2 ejecuciones idénticas):
   `parse_selectors("div:is(.a)").matches` → true;
   `div, p:bogus` → 0 selectores (F13 sigue cierta).
2. Test existente `css/src/lib.rs:831`.

## Esperado vs observado
Esperado: docstring al día (conservar la parte F13 de pseudos desconocidas).
Observado: docstring pendiente obsoleto.

## Evidencia literal
```
Q1 :is matches: true
Q2 unknown invalidates: 0
```

Commit: 3c06db8

## Dedup
Dedup: new (patrón rel. #1407 — distinta).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener.
