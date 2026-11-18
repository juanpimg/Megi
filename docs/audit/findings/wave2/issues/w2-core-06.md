[css] background-repeat: `repeat, bogus` no invalida la declaración (cicla parcial)

## Resumen
`cycle_property_list` (`crates/css/src/properties.rs:4315-4335`) usa `filter_map`:
descarta componentes inválidos y cicla el resto. Con 2 capas,
`no-repeat, bogus` computa `[NoRepeat, NoRepeat]` en vez de la inicial.
El propio docstring promete invalidez total si falta la declaración pero no si un
componente falla — inconsistencia interna.

## Impacto
P3 — declaración inválida parcialmente aplicada; desalineación del ciclo multicapa.

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas, discriminador B1b):
   capa count=2, `repeat, bogus` → `[Repeat, Repeat]`;
   `no-repeat, bogus` → `[NoRepeat, NoRepeat]` (debió ser la inicial);
   `bogus` solo → inicial. Nota: el caso B2 (`bogus` solo) es inobservable porque
   la inicial de `repeat` ES `Repeat`; el discriminador limpio es B1b.

## Esperado vs observado
Esperado: CSS Backgrounds 3 §3.1 — un componente inválido invalida la declaración
completa → valor inicial.
Observado: ciclado parcial del resto válido.

## Evidencia literal
```
B1 repeat layers: [Repeat, Repeat]
B1b norepeat+bogus 2 layers: [NoRepeat, NoRepeat]
B2 bogus solo: [Repeat]
```

Commit: 3c06db8

## Dedup
Dedup: new (sin relacionadas).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener.
