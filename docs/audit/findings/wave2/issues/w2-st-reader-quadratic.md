[shell] reader extract() re-escanea subárboles por cada contenedor (O(n²)); la caché no evita los walks

## Resumen
`crates/shell/src/reader.rs:82-104,44-73`: por cada nodo contenedor, `visible_text` recorre todo su subárbol vía `or_insert_with(|| visible_text(…).chars().count())` — el walk completo se ejecuta antes de cachear. Nodos anidados de profundidad D → ~D²/2 visitas. En ventana corre en el event loop (freeze UI); con `--reader` headless se mide el delta.

## Impacto
P2 — perf/DoS local: página hostil + Ctrl+Shift+R / `--reader` → freeze de segundos. Fix concreto (texto bottom-up en una sola pasada o cap de contenedores evaluados). P3 admisible porque exige página hostil + acción del usuario y el coste en release es menor.

## Pasos para reproducir
1. Generar fixture de divs anidados (auditor: 198 KB / 15k divs; sonda Mesa A: 71 KB / 3000 divs).
2. `time megi fixture.html --screenshot base.png` (control).
3. `time megi fixture.html --reader --screenshot r.png` (con extract).
4. Comparar tiempos (debug): el delta ≈ coste de `extract`.

## Esperado vs observado
Esperado: coste lineal del extract (una pasada bottom-up reutilizando hijos).
Observado (auditor, debug, fixture 198 KB/15k): `--screenshot` 1.54 s (falla por límites, exit 2) vs `--reader` 4.58 s (éxito) → Δ ≈ 3 s de `extract`. Sonda Mesa A (71 KB/3000): base `0.545 s` (wrote 25684 B) vs reader `3.495 s` (`reader mode found no article`) → Δ ≈ 6× aun sin artículo.

## Evidencia literal
```
auditor: --screenshot 1.54 s (exit 2, 10908 limit violations) vs --reader --screenshot 4.58 s (wrote 37515 bytes)
Mesa A: --screenshot 0.545 s (wrote 25684 B) vs --reader --screenshot 3.495 s (reader mode found no article on this page)
```
Código: `reader.rs:82-104` `or_insert_with(|| visible_text(…))` camina antes de cachear; por cada contenedor anidado → ~D²/2 visitas.

Commit: 3c06db8

## Dedup
Dedup: new. Mesa A: VERIFIED. Mesa B: NUEVO, P2 mantener (P3 admisible).
