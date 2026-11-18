[perf] caps heavy parcialmente documentados: warmup cap silencioso, --warmup 0 aceptado, meta con pedidos

## Resumen
Caps intactos en `crates/bench/src/runner.rs:108-116`: `iterations.min(3)`,
`warmup.min(1)`. USAGE (`cli.rs`) documenta "heavy cases cap at 3" (iterations)
pero `--warmup` no menciona el cap. `--warmup 0` aceptado (`warmup > MAX` rechaza,
`0` pasa) → mediciones en frío. Meta/header JSON y cabecera humana publican
`opts.iterations/opts.warmup` (pedidos); sólo tabla y row-JSON llevan el efectivo
(`result.iterations`, `report.rs:24-30,134-135,159`).

## Impacto
P3 — una baseline puede registrar `iterations:10/warmup:5` cuando se midió 3/1;
`--warmup 0` cuela ruido cold en comparaciones.

## Pasos para reproducir
1. `cargo run --release -p bench -- --help` (cap sólo en `--iterations`).
2. `--warmup 0 --iterations 1 --filter tag-40k-attrs` (+ variante `--json`):
   header `1 iterations (+0 warm-up)`, fila OK; JSON meta `"iterations": 5,
   "warmup": 0` (pedidos) vs fila `"iterations": 3` con 3 samples (efectivo).
   Mitigado desde el issue: `--iterations 0` rechazado + cap documentado en USAGE/tabla.

## Esperado vs observado
Esperado: ambos caps en `--help`, rechazar `--warmup 0` o estampar efectivos en meta.
Observado: warmup cap silencioso, `0` aceptado en runtime, meta con pedidos.

## Evidencia literal
```
código: opts.iterations.min(3), opts.warmup.min(1)
JSON: meta "iterations": 5, "warmup": 0 vs fila "iterations": 3 (3 samples)
```

Commit: 3c06db8

## Workaround
Pasar siempre `--warmup >= 1` y leer iteraciones efectivas de la fila, no de la meta.

## Fix sugerido
3 líneas en `cli.rs`/`report.rs` + test de CLI.

## Dedup
Rel. #1199 (Deferred; fix parcial verificable). Se abre issue nuevo referenciando
#1199 con el residuo documentado en HEAD. Mesa A: VERIFIED.
