[perf] GradientSpec::evaluate sigue siendo scan lineal por píxel (O(stops) sin partition_point ni LUT)

## Resumen
`crates/svg/src/gradient.rs:206-…` (`for stop in &self.stops[1..]`, early-out por
`t <= stop.offset`), llamado por píxel cubierto desde
`crates/svg/src/raster.rs:1583` (`composite_gradient`). `MAX_STOPS = 256`
(`gradient.rs:20`). Stops ordenados (test `test_linear_defaults_and_stops_sorted`)
⇒ `partition_point` aplicable, como pedía #1270. Sin `partition_point` ni LUT.

## Impacto
P2 — gradiente 256-stops a viewport completo ≈ 16,7 Mpx × 256 comparaciones
(~4 G ops) por shape en el peor caso; a tamaños comunes +100–265 ms evitables
con `partition_point`. No catastrófico pero evitable.

## Pasos para reproducir
1. Leer `gradient.rs:206-…` (bucle lineal intacto) y `raster.rs:1583`.
2. Probe HEAD limpio `repro-w2-perf/gradient-{4,256}-stops.svg` (rect 1024²;
   PNG verificado `png_check.py`: 1048576 px, 1024 colores):
   auditor 1024² mín 313 vs 440 ms (+41 %); Mesa A 4 stops 129/119/120 ms,
   256 stops 179/169/170 ms (≈+42 %). A 2048²: 300 vs 565 ms mín (+88 %).
   El scan se ve (dirección correcta) pero NO domina: coste fijo lo tapa a 1–4 Mpx;
   el peor caso del issue (64 Mpx × 256 stops) sigue sin medirse.

## Esperado vs observado
Esperado: `partition_point` (O(log n)) o LUT horneada para lineales; tiempo
~constante 4-vs-256 stops.
Observado: scan lineal O(stops) por píxel; +41–88 % medido.

## Evidencia literal
```
1024²: mín 313 vs 440 ms (+41 %, auditor); 119-129 vs 169-179 ms (+42 %, A)
2048²: mín 300 vs 565 ms (+88 % para 64× stops)
```

Commit: 3c06db8

## Workaround
Limitar stops en SVGs de viewport completo; el sobrecoste a tamaños comunes es moderado.

## Fix sugerido
`let i = self.stops.partition_point(|s| s.offset < t)` + interpolación;
test 4-vs-256 stops a tiempo ~constante.

## Dedup
Rel. #1270 (Deferred como "optimización"; código intacto + probe).
Se abre issue nuevo referenciando #1270 con evidencia de código + timing en HEAD.
Mesa A: VERIFIED.
