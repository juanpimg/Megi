[css] color-mix() rechaza espacios lab/lch/xyz-* válidos en CSS Color 5

## Resumen
`mix_colors` (`crates/css/src/color.rs:586-644`) sólo acepta
`srgb`/`srgb-linear`/`oklab`/`oklch` (`_ => None`). `color-mix(in lab, ...)` e
`in lch` (y `xyz-*`) devuelven `None` aunque el módulo promete «CSS Color 4/5»
(`color.rs:1-8`). Declaraciones válidas caen a valor inicial.

## Impacto
P3 — declaraciones válidas con interpolación lab/lch caen a inicial. Sin crash.

## Pasos para reproducir
1. Sonda contra HEAD limpio `3c06db8` (2 ejecuciones idénticas, casos M0/M1/M1b/M1c):
   `color-mix(in srgb, red 50%, blue)` → `Some(...)`;
   `in lab` / `in lch` / `in hsl` → `None`.

## Esperado vs observado
Esperado: CSS Color 5 §3.2 — espacios de interpolación incluyen `lab`, `lch`,
`oklab`, `oklch`, `srgb`, `srgb-linear`, `xyz*`.
Observado: `lab`/`lch` rechazados.

## Evidencia literal
```
M0 colormix srgb: Some(Color { r: 128, g: 0, b: 128, .. })
M1 colormix lab: None
M1b colormix lch: None
M1c colormix hsl: None
```

Commit: 3c06db8

## Dedup
Dedup: new (sin duplicados; citar #1124 sólo si se discute memoria — no aplica).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO, P3 mantener.
