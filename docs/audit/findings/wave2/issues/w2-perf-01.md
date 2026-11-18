[perf] filas filter-fixture-no-effect miden CSS byte-idéntico (strip por línea no elimina nada)

## Resumen
El strip de `crates/bench/src/benches.rs:882-889` elimina 0/29 líneas de
`tests/fixtures/render/filter_effects.html` (todas las reglas `filter:` van a mitad
de línea, p. ej. `.gray { filter: grayscale(1); }`; ninguna empieza por `filter:`).
`without_property()` (`:962-973`) comparte el defecto. El `trim_start()` no basta:
el problema es mitad-de-línea, no indentación.

## Impacto
P2 — perf/contrato: dos filas de bench (×2 variantes paint/pipeline) no miden coste
de `filter:`; una regresión real del filtro pasa el gate `bench_compare.py`.

## Pasos para reproducir
1. Simular strip en python sobre fixtures → `filter_effects.html total=29 removed=0`;
   `backdrop_filter.html total=31 removed=2` (ese sí funciona).
2. `cargo run --release -p bench -- --filter filter-fixture` en HEAD limpio:
   `paint/filter-fixture` 0,816 ms vs `-no-effect` 0,818 ms (Δ 0,002, idénticos);
   Mesa A reproduce identidad: 0,307 vs 0,307 ms. Contraste:
   `paint/backdrop-fixture` 0,786 vs no-effect 0,062 (×12,7).

## Esperado vs observado
Esperado: las filas `-no-effect` miden el fixture sin declaraciones `filter:`
(`benches.rs:853-856`, `docs/PERFORMANCE.md:356`).
Observado: `-no-effect` byte-idéntico al fixture; delta estructuralmente cero
(incluso "sin efecto" sale más rápido por ruido: pipeline 2,363 vs 2,320).

## Evidencia literal
```
strip: filter_effects.html total=29 removed=0; backdrop_filter.html total=31 removed=2
paint/filter-fixture 0.816 ms vs -no-effect 0.818 ms (auditor); 0.307 vs 0.307 (A)
```

Commit: 3c06db8

## Workaround
Comparar contra la fila `backdrop-fixture` (strip funcional) para validar el gate.

## Fix sugerido
Strip por declaración (parsear `<style>` y filtrar declaraciones, no líneas) o
reformatear el fixture a una declaración por línea; test que asevere
`stripped != fixture` y `!stripped.contains("filter:")`.

## Dedup
Rel. #1197 (Deferred; evidencia NUEVA de que el fix parcial no lo cierra).
Se abre issue nuevo referenciando #1197 (cerrado) con medición nueva en HEAD.
Mesa A: VERIFIED.
