[html] line_starts amplifica memoria ~12 B por byte de entrada (16 MiB → ~200 MB transitorios)

## Resumen
`line_starts: Vec<usize>` una entrada por línea
(`crates/html/src/tokenizer.rs:772-778`) + `out: Vec<u32>` 4 B/cp
(`input_stream.rs:112-201`). 2 MiB de `\n` → +~25 MB RSS medidos
(4 B/cp + 8 B/línea ≈ 12.4×); el cap de entrada 16 MiB permite ~200 MB.
Pasa todos los límites (cero violaciones): amplificación con input permitido,
sin crash pero presión transitoria alta.

## Impacto
P3 perf (no DoS: sin crash/hang) — coste documentable; fix: `Vec<u32>` para
line_starts (16 MiB → 64 MB máx.) o cómputo bajo demanda; documentar en
`ParseOptions`. `line_starts` se conserva para lookups línea/columna (uso legítimo).

## Pasos para reproducir
1. Sonda `mem` en HEAD limpio `3c06db8` (2 ejecuciones):
   `cargo run --manifest-path /tmp/probe/Cargo.toml -- mem`
   (input `"\n".repeat(2MiB)`).

## Esperado vs observado
Esperado: tope documentado o tabla compacta; como mínimo documentar el coste.
Observado: `nodes=4`, cero violaciones, ΔRSS ≈ 25 MB para 2 MiB.

## Evidencia literal
```
MEM nodes=4 rss_before_kb=5752 rss_after_kb=30788 delta_kb=25036
violations: LimitViolations { … all zero }
```
(≈12.2 B por byte de entrada).

Commit: 3c06db8

## Dedup
Dedup: new (rel. #1124 cerrada — era el cap de posiciones, que se revirtió; citar, no duplica).
Mesa A: VERIFIED. Mesa B/B2: SOLO NUEVO (rel. #1124 cerrada), P3 mantener.
