[perf] render/html-to-png declara 0,096 MPix renderizando 0,48 MPix (throughput 5× infravalorado)

## Resumen
`crates/bench/src/benches.rs:1425` declara `.with_units(0.096, "MPix")` para un
viewport 800×600 (`:1414-1419`) = 0,48 MPix. Constante idéntica en HEAD
(`crates/bench` sin WIP).

## Impacto
P2 — la fila E2E de referencia parece 5× más lenta de lo que es; comparaciones
MPix/s entre filas (`render` vs `image`) inválidas.

## Pasos para reproducir
1. Leer `crates/bench/src/benches.rs:1414-1428`: viewport 800×600, `0.096 MPix`.
2. Aritmética: 800×600 = 480000 px = 0,48 MPix = 5× lo declarado.
3. Medido HEAD limpio release: mediana 28,701 ms → 3,35 MPix/s publicados
   (0,096/0,028701); con 0,48 → 16,7 MPix/s. Testigo misma corrida:
   `image/decode-png-scaled-x2` 8,929 ms → 13,44 MPix/s (fórmula
   `count*1000/median`, `runner.rs:118-124` ✓).

## Esperado vs observado
Esperado: `with_units(0.48, "MPix")` (área real del viewport).
Observado: `0.096 MPix`, 5× por debajo.

## Evidencia literal
```
Viewport::new(800, 600); .with_units(0.096, "MPix"); 800*600 = 0.48 MPix
```

Commit: 3c06db8

## Workaround
Multiplicar por 5 el MPix/s publicado de esa fila.

## Fix sugerido
Una línea + regenerar `bench-baseline.json` para esa fila.

## Dedup
Rel. #1198 (Deferred; la constante no cambió). Se abre issue nuevo referenciando
#1198 con aritmética verificada en HEAD. Mesa A: VERIFIED.
