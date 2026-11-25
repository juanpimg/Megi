# ADR — Evolución del motor JS: contribuir/actualizar Boa mientras sirva

- **Estado**: ACEPTADO — decisión derivada de la spec v2 aprobada en **PR #2290**, que autoriza expresamente la rama (a) «contribuir/actualizar Boa a medida que MSRV lo permita» y exige que la alternativa se decida «por datos del corpus (no por dogma)». No se registra aprobación individual adicional del dueño.
- **Fecha**: 2026-11-24 (plan), registrado 2026-11-25 con mediciones reales.
- **Hito**: M8 — Motor ofensivo, criterio «JIT o migración de motor decidida por datos».
- **Referencias**: spec v2 §4.2 (`docs/superpowers/specs/2026-11-24-megi-ambition-design.md:88`); master plan M8 (`docs/superpowers/plans/2026-11-24-megi-ambition-master-plan.md:97`); plan de evolución JS, Tasks 2–3 (`docs/superpowers/plans/2026-11-24-js-evolution-plan.md:61-112`); ADR-001 (`docs/JAVASCRIPT-STRATEGY.md:75-118`); medición (`docs/PERFORMANCE.md:420`, `docs/status/bench-baseline.json` → `addenda.js-r37`).

## Contexto

La barra de M8 no exige «tener JIT», sino **decidir con datos** entre tres ramas: (a) contribuir/actualizar Boa, (b) JIT en perfil `performance`, (c) migrar a rquickjs. La spec mantiene Boa «mientras sirva» y prohíbe rebajar la barra de producción de §2. Antes de este ADR no había ninguna medición JS publicada: `docs/status/bench-baseline.json` tenía 81 filas, ninguna del grupo `js`; `docs/JAVASCRIPT-STRATEGY.md` sólo recogía *claims* de upstream.

Hecho verificable en este árbol: `crates/js` usa `boa_engine 0.22.0` (MSRV 1.91) detrás de `#![forbid(unsafe_code)]`; `rust-toolchain.toml` fija 1.99.1, mayor que ese MSRV, y no hay dependencias circulares nuevas. No existe un arnés Test262 local (`tests/test262/**` y `tools/js262_gate.py` no están en el repo), por lo que **no hay medición propia de cobertura ECMAScript**. El «>90 % de spec» de Boa y el «~100 % Test262» de QuickJS son afirmaciones de los respectivos upstream citadas en `docs/JAVASCRIPT-STRATEGY.md:80-86`, no resultados de este proyecto.

## Datos medidos

Banco añadido en `crates/bench/src/benches/js_hot.rs` (commits `c6394c3f`, `76d3be52`) con el patrón real de `tests/fixtures/js_corpus/jquery_like.html` (`querySelectorAll('.item')` + 100 pasadas × 200 nodos con `getAttribute('data-index')`). Las filas `js/hot-engine` y `js/hot-bindings` ejecutan exactamente el mismo número de llamadas (20 000) y difieren sólo en el objeto: *shims* JS planos frente a *wrappers* DOM que cruzan a Rust. El *setup* de los *shims* se evalúa fuera de la ventana temporizada; una primera ronda que lo hacía dentro (`target/bench-r37-js-run1..7.json`) se descartó por confundir asignación con binding.

Comando: `cargo +1.99.1 run --release -p megi-bench -- --filter js --iterations 5 --warmup 1 --json`, siete pasadas con `nice -n 19`, HEAD `76d3be52`, árbol limpio, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, i7-12700H/20 hilos, load 1 min 2,33–5,64 (máquina compartida). Resultados publicados en `docs/status/bench-baseline.json` → `addenda.js-r37` y detallados en `docs/PERFORMANCE.md` §15:

| Medida | Mediana de 7 pasadas | Rango | p95 muestral |
|---|---:|---:|---:|
| `engine_time_ms` (sin bridge) | 5,229 ms | 4,799–5,754 | 6,145 |
| `binding_time_ms` + engine (DOM real) | 9,458 ms | 8,336–10,007 | 10,663 |
| Script real `wikipedia_null` (sin modificar) | 0,136 ms | 0,113–0,184 | 0,188 |

- Diferencia del par: **+4,229 ms** (1,809×), es decir **0,211 µs por llamada** al puente Rust. Motor: 3,825 M llamadas/s; bindings: 2,115 M llamadas/s.
- El script real del corpus cuesta **0,136 ms**, el 2,6 % del tiempo del bucle de motor. En este corpus el cuello medido es el *bridge*, no el intérprete.
- Dispersión: 10 % en motor, 20 % en bindings, con load compartido. Los datos son **direccionales**: no sirven para atribuir mejoras <10 % ni para prometer un SLA.
- Esto **no** es una comparación Boa frente a QuickJS: sólo separa motor de bindings dentro de Megi. Tampoco mide Test262, memoria ni arranque.

## Decisión

Se adopta la **rama (a)**: **contribuir/actualizar Boa mientras sirva**, y se prioriza optimizar el *bridge* DOM antes de cualquier JIT o migración. Concretamente:

1. Se mantiene `boa_engine 0.22` en `crates/js`; cualquier actualización sigue supeditada al MSRV del workspace (hoy 1.99.1 > 1,91). La frontera sigue siendo safe Rust con `#![forbid(unsafe_code)]`.
2. **No se implementa un JIT propio** en este WP (rama (b) queda diferida, no abandonada): el único coste aislado con datos es el puente Rust (4,229 ms / 20 000 llamadas), mientras que el script de corpus real tarda 0,136 ms. Un JIT atacaría `engine_time_ms`, que hoy no domina la medida.
3. **No se migra a rquickjs** en este WP (rama (c) queda como contingencia documentada): no hay medición local de Test262 que justifique el cambio y el coste incluye toolchain C, `build.rs` nativo y frontera FFI `unsafe`, además de reimplementar la capa DOM de `crates/js`.
4. Revisar este ADR y abrir un WP nuevo **sólo** si aparece evidencia medible: (i) con el bridge optimizado, `engine_time_ms` pasa a dominar el presupuesto de scripts de la barra §2 (p. ej. >50 % del tiempo de script en el corpus de 100 sitios) con A/B intercalado reproducible; o (ii) un arnés Test262 local publicado muestra un hueco de conformidad de Boa que bloquee la barra y que no se pueda cubrir contribuyendo upstream.
5. Cerrar M8#1 significa «decisión con datos publicada», **no** «JIT implementado», «migración hecha», «Test262 en 0» ni «barra §2 alcanzada». Los otros dos criterios `pending` (HTTP/3/QUIC y paralelismo real de estilo/layout/paint) siguen abiertos, y M9 mantiene su barra intacta.

## Coste y consecuencias de la decisión

- **Coste inmediato**: tres filas de bench, un publicador de addenda reproducible (`tools/update_bench_baseline.py`), este ADR y la documentación de la medición; ninguna dependencia nueva externa, ningún `unsafe`. El único cambio de DAG es `bench → js` (herramienta → motor, sin ciclo), registrado en `docs/architecture-deps.toml` y `docs/ARCHITECTURE.md`.
- **Coste continuo**: seguir el upstream de Boa y contribuir cuando el MSRV lo permita; el siguiente WP de rendimiento JS debe atacar el bridge y repetir el par con A/B, en vez de repetir la medición confundida.
- **Beneficio**: se evita una migración con toolchain C y `unsafe` sin evidencia local, y se deja una primera línea base JS auditable que futuras decisiones pueden reutilizar.

## Límites explícitos

- Una sola carga sintética (20 000 `getAttribute`) y un solo script de corpus real (`wikipedia_null.html`); no se mide arranque, GC, módulos, Web Workers ni sitios de 100/100.
- No hay arnés Test262 ni medición de cobertura local; las cifras de conformidad citadas en `docs/JAVASCRIPT-STRATEGY.md` son de upstream.
- La ventana no estuvo ociosa (load 1 min 2,33–5,64): los números valen como dirección y para detectar regresiones grandes, no como umbral de cumplimiento.
- `cargo bench -p megi-bench js_hot` imprime `running 0 tests`: el verificador correcto es `cargo run --release -p megi-bench -- --filter js --json`.
- `docs/status/bench-baseline.json` mantiene las 81 filas de r36 en `rows`; el addendum `js-r37` es parcial y `tools/bench_compare.py` no lo usa para detectar regresiones.

## Reproducción

```bash
nice -n 19 env CARGO_BUILD_JOBS=1 TMPDIR=/var/tmp \
  CARGO_TARGET_DIR=/home/jp/.cache/megi-target-jsbench \
  PATH=/home/jp/.cargo/bin:$PATH \
  cargo +1.99.1 run --release -p megi-bench -- --filter js --iterations 5 --warmup 1 --json

python3 tools/update_bench_baseline.py --check --id js-r37 \
  --passes target/bench-r37-js-fixed-run{1..7}.json \
  --commit 76d3be52 --date 2026-11-25 \
  --method "7 consecutive passes of cargo run --release -p megi-bench -- --filter js --iterations 5 --warmup 1 --json; per-row median of the 7 pass medians" \
  --load-note "load average 2.33-5.64 (1 min) and 22.72-22.95 (15 min) during the window: shared machine, nice -n 19 used" \
  --artifacts "target/bench-r37-js-fixed-run1..7.json (not committed; see docs/PERFORMANCE.md §15)"

python3 tools/check_milestone.py M8 --match 'JIT o migración'
```

Los artefactos `target/bench-r37-js-fixed-run1..7.json` no se commitean; el addendum publicado contiene las medianas, rangos y p95 calculados por la herramienta.
