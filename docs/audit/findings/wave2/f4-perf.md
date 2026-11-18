# F4 — Performance (ola 2)

Auditor: wave2 F3/F4 (scope F4 = `w2-perf`, este fichero).
HEAD: `3c06db8c186e4f7a19c7816aed4955a0b68f91ef`.
Mediciones: `cargo run --release -p bench` ejecutado en árbol principal (sucio) y
re-ejecutado en `/tmp/opencode/megi-verify` (limpio, `worktree_dirty:false`).
Constantes del arnés verificadas idénticas en HEAD (`crates/bench` sin WIP).
Metodología: `docs/PERFORMANCE.md` §1/§13/§36 (7 pasadas, mediana por fila, warmup 1 /
iterations 5, heavy 3).

## Alcance

- `crates/bench/src/{benches.rs,runner.rs,report.rs,cli.rs,main.rs,generators.rs}` (leídos
  en lo relevante a #1197/#1198/#1199/#1201; el resto del crate es scope F2/F4-harness).
- `docs/PERFORMANCE.md` (405 líneas, leído entero), `docs/audit/findings/f4/00-perf-baseline.md`,
  `docs/status/bench-baseline.json` (schema `megi-bench-baseline-v1`, round 36, commit
  `a080249`, 81 filas — coherente con PERFORMANCE).
- Fixtures: `tests/fixtures/render/filter_effects.html` (29 líneas),
  `tests/fixtures/render/backdrop_filter.html` (31 líneas).
- Contraste pedido: #1197/#1198/#1199/#1201/#1270 (los 5 cerrados como Deferred con
  "complex fix… Documented for future work"; #1197–#1199 cerrados 2026-11-17T01:03Z).

## Cobertura

- `docs/PERFORMANCE.md` — leído, sin hallazgo nuevo (la cabecera r36 documenta load 8,1–13,0
  y no-comparabilidad 1:1; §13 documenta el comparador automático; presupuestos §11).
- `f4/00-perf-baseline.md` — leído, sin hallazgo (bloqueadores #1197/#1198/#1201 que cita
  siguen vigentes: ver abajo).
- `bench-baseline.json`, `bench_compare.py`, `tools/bench.sh` — sin hallazgo
  (comparador: regresión >20 % y >0,5 ms, exit 1; `bench.sh` reenvía args + `set -euo pipefail`).
- `benches.rs` strip/filter/MPix, `runner.rs` heavy caps, `layout` measurer, `svg/gradient.rs`
  — **hallazgos w2-perf-01…05**.

## Hallazgos

### w2-perf-01 — las filas `filter-fixture*-no-effect` miden CSS byte-idéntico (#1197 sigue vivo)

- `archivo:línea`: `crates/bench/src/benches.rs:882-889`, `962-973`
- `lente`: límites y DoS (coste que no se mide) / contrato vs documentación
- `severidad`: P2
- `categoría`: perf
- `title`: el strip de `filter:` no elimina ninguna línea del fixture; el delta no-effect es estructuralmente cero
- `evidence`:
  - `filter_effects.html`: 12 líneas contienen `filter:`, **0** empiezan (tras trim) por
    `filter:` — todas son reglas de una línea (`.gray { filter: grayscale(1); }`).
    Repro (2 ejecuciones): el strip de `benches.rs:882-889` elimina **0/29** líneas;
    el de backdrop (`backdrop_filter.html`) elimina 2/31 (ese sí funciona).
  - Bench HEAD limpio (`3c06db8`, `worktree_dirty:false`, iters 5/warmup 1, release):
    `paint/filter-fixture` **0,816 ms** vs `-no-effect` **0,818 ms** (Δ 0,002 ms, idénticos);
    `pipeline/filter-fixture-300x260` 2,363 vs no-effect 2,320 (el "sin efecto" sale
    MÁS rápido: ruido). Contraste: `paint/backdrop-fixture` 0,786 vs no-effect 0,062
    (×12,7, el strip que sí funciona). r36 publicada: 0,271 vs 0,269.
  - El `trim_start()` (mitigación parcial) no basta: el problema es de mitad-de-línea,
    no de indentación. `without_property()` (:962-973) comparte el defecto.
- `impact`: dos filas de bench (×2 variantes paint/pipeline) no miden coste de `filter:`;
  una regresión real del filtro pasaría el gate `bench_compare.py`.
- `expected`: las filas `-no-effect` deben medir el fixture sin declaraciones `filter:`
  (como promete `benches.rs:853-856` y `docs/PERFORMANCE.md:356`).
- `fix`: strip por declaración (parsear el `<style>` y filtrar declaraciones, no líneas)
  o reformatear el fixture a una declaración por línea; test que asevere
  `stripped != fixture` y `!stripped.contains("filter:")`.
- `repro`: `python3` one-liner de arriba + `cargo run --release -p bench -- --filter filter-fixture`
  en HEAD limpio (0,816 vs 0,818 ms).
- `dedup`: #1197 (Deferred; evidencia NUEVA de que el fix parcial no lo cierra).

### w2-perf-02 — `render/html-to-png` declara 0,096 MPix renderizando 0,48 MPix (#1198 sigue vivo)

- `archivo:línea`: `crates/bench/src/benches.rs:1425` (viewport 800×600 en :1414-1419)
- `lente`: contrato vs documentación
- `severidad`: P2
- `categoría`: perf
- `title`: el throughput publicado de la fila render está infravalorado 5×
- `evidence`:
  - 800×600 px = 0,48 MPix; declarado `0.096`. Medido en HEAD limpio (`3c06db8`, release):
    `render/html-to-png` mediana **28,701 ms** (iters=3) → **3,35 MPix/s** publicados
    (0,096/0,028701); con 0,48 → **16,7 MPix/s**. Testigo misma corrida:
    `image/decode-png-scaled-x2` 8,929 ms → 13,44 MPix/s (0,12/0,008929 ✓ fórmula
    `count*1000/median`, `runner.rs:118-124`).
  - Constante idéntica en HEAD (`crates/bench` sin WIP; verificado por diff vacío).
- `impact`: la fila E2E de referencia parece 5× más lenta de lo que es; comparaciones
  MPix/s entre filas (`render` vs `image`) inválidas.
- `expected`: `with_units(0.48, "MPix")` (o el área real del viewport).
- `fix`: una línea + regenerar `bench-baseline.json` para esa fila.
- `repro`: `cargo run --release -p bench -- --filter render/html-to-png` + división.
- `dedup`: #1198 (Deferred; la constante no cambió).

### w2-perf-03 — caps `heavy` parcialmente documentados: warmup cap silencioso, `--warmup 0` aceptado, meta con valores pedidos (#1199 parcial)

- `archivo:línea`: `crates/bench/src/runner.rs:108-116`, `:155-156`; `cli.rs` USAGE; `report.rs:24-30,134-135,159`
- `lente`: contrato vs documentación
- `severidad`: P3
- `categoría`: perf
- `title`: el arnés sigue capando warmup a 1 sin documentarlo y publica los valores pedidos en vez de los efectivos
- `evidence`:
  - Caps intactos: `iterations = opts.iterations.min(3)`, `warmup = opts.warmup.min(1)`.
  - USAGE documenta "heavy cases cap at 3" (iterations) pero `--warmup` no menciona el cap.
  - `--warmup 0` aceptado (`warmup > MAX` rechaza, `0` pasa) → mediciones totalmente en frío.
  - Meta/header JSON y cabecera humana publican `opts.iterations/opts.warmup` (pedidos);
    sólo la tabla y el row-JSON llevan el efectivo (`result.iterations`).
  - Mitigado desde el issue: `--iterations 0` rechazado + cap documentado en USAGE/tabla.
- `impact`: una baseline puede registrar `iterations:10/warmup:5` cuando se midió 3/1;
  `--warmup 0` cuela ruido cold en comparaciones.
- `expected`: documentar ambos caps en `--help`, rechazar `--warmup 0`, o estampar
  efectivos en la meta.
- `fix`: 3 líneas en `cli.rs`/`report.rs` + test de CLI.
- `repro`: `cargo run --release -p bench -- --help` + `--warmup 0 --filter shell` (iters=3/0).
- `dedup`: #1199 (Deferred; fix parcial verificable).

### w2-perf-04 — `layout/*` y `pipeline/*` siguen con el measurer fallback, no el de producción (#1201 sigue vivo)

- `archivo:línea`: `crates/bench/src/benches.rs:861,942,1025` (`layout::layout_document`)
- `lente`: contrato vs documentación
- `severidad`: P2
- `categoría`: perf
- `title`: todas las filas de layout/pipeline miden el "CSS ratio fallback", no `FontTextMeasurer`
- `evidence`:
  - `crates/layout/src/lib.rs:49-64`: `layout_document()` = "Backwards-compatible deterministic
    measurement (CSS ratio fallback). The production render pipeline must use
    `layout_document_with_measurer` with `FontTextMeasurer`…".
  - Producción lo usa (`crates/render/src/lib.rs:1263,1283,1503,1510`); el bench no:
    3 call sites con `layout_document(&doc, &styles, viewport)` (+ `expect("layout")` en :1025).
  - En HEAD limpio `layout/realistic-1mb` y `pipeline/html-to-png` se miden con
    fallback: el coste real de shaping/line-box no está en la baseline
    (r36: 35,763 ms y 110,124 ms).
- `impact`: una regresión de `FontTextMeasurer` es invisible salvo en la fila E2E;
  presupuestos (§11: layout ≤45, pipeline ≤140) calibrados contra el measurer barato.
- `expected`: filas layout/pipeline con el measurer de producción (determinista con fuentes
  empaquetadas) o documentar la divergencia en PERFORMANCE.md.
- `fix`: `layout_document_with_measurer(&doc, &styles, viewport, &FontTextMeasurer::bundled()?)`
  en los 3 sitios (nota: feature `fonts` del bench ya existe, `BENCH_FEATURES=fonts`).
- `repro`: grep de arriba + `cargo run --release -p bench -- --filter layout/realistic`.
- `dedup`: #1201 (Deferred; call sites intactos).

### w2-perf-05 — `GradientSpec::evaluate` sigue siendo scan lineal por píxel (#1270 sigue vivo)

- `archivo:línea`: `crates/svg/src/gradient.rs:206-…` (`for stop in &self.stops[1..]`),
  llamado por píxel cubierto desde `crates/svg/src/raster.rs:1583` (`composite_gradient`)
- `lente`: límites y DoS
- `severidad`: P2
- `categoría`: perf
- `title`: color de gradiente por scan lineal O(stops) por píxel; sin `partition_point` ni LUT
- `evidence`:
  - Código: bucle lineal con early-out por `t <= stop.offset`; `MAX_STOPS = 256`
    (`gradient.rs:20`); `composite_gradient` evalúa por cada píxel con cobertura > 0.
  - Stops ordenados (test `test_linear_defaults_and_stops_sorted`) ⇒ `partition_point`
    aplicable, como pedía #1270.
  - Medición (HEAD limpio, `render` release): probe `repro-w2-perf/gradient-{4,256}-stops.svg`
    (rect 1024², 1 Mpx; PNG verificado con `png_check.py`: 1048576 px, 1024 colores):
    4 stops: 2077/368/**313** ms; 256 stops: 466/436/**440** ms (mínimos 313 vs 440:
    +41 %). A 2048² (4 Mpx, 5 runs): 4 stops mín **300** ms (300–468); 256 stops mín
    **565** ms (565–1826, primeras corridas lastradas por caché fría) → **+88 %** para
    64× stops. El scan lineal se ve (dirección correcta) pero NO domina: el coste fijo
    (parse/layout/sintético/PNG ≈ 300 ms) lo tapa a 1–4 Mpx. El peor caso del issue
    (64 Mpx × 256 stops) sigue sin medirse.
- `impact`: gradiente 256-stops a viewport completo ≈ 16,7 Mpx × 256 comparaciones
  (~4 G ops) por shape en el peor caso; a tamaños comunes el sobrecoste es ~+100–265 ms,
  no catastrófico pero evitable con `partition_point`.
- `expected`: `partition_point` (O(log n)) o LUT horneada para lineales.
- `fix`: `let i = self.stops.partition_point(|s| s.offset < t)` + interpolación; test
  4-vs-256 stops a tiempo ~constante.
- `repro`: `docs/audit/findings/wave2/repro-w2-perf/gradient-*.svg` + timing render.
- `dedup`: #1270 (Deferred como "optimización"; evidencia: código intacto + probe).

## Comandos ejecutados

| Comando (worktree limpio salvo nota) | Salida |
|---|---|
| `cargo run --release -p bench -- --json` (limpio, `3c06db8`, dirty:false) | exit 0; filter 0,816 vs 0,818 · pipeline-filter 2,363 vs 2,320 · backdrop 0,786 vs 0,062 · render/html-to-png 28,701 ms = 3,35 MPix/s (0,096) |
| gradient probe ×3×2 + ×5×2 a 2048² (limpio) | 1024²: 313 vs 440 ms mín · 2048²: 300 vs 565 ms mín (+88 % para 64× stops) |
| strip 0/29 vs 2/31 | `python3` sobre fixtures (2 ejecuciones, idéntico) |
| `bench_compare.py` | leído; sin ejecutar contra baseline nueva (comparación pendiente de próxima ventana) |
| `cargo deny check` | no ejecutable en local (ver F3) |

## Concern

1. Los 5 issues de perf contrastados están cerrados como Deferred con la misma fórmula;
   4 siguen vivos en HEAD con evidencia nueva (#1197 parcial, #1198, #1201, #1270 intactos;
   #1199 parcial). El gate `bench` (20 % + 0,5 ms) no puede detectar #1197 (delta cero por
   construcción) ni #1198 (constante, no medición).
2. La baseline r36 se midió a load 8–13 (documentado) y la ventana limpia r32 quedó
   congelada; los deltas r32→r36 mezclan código y ruido. Cualquier veredicto <10 % exige A/B.
3. `render/html-to-png` es la única fila con el crate `render`; medida en HEAD limpio
   (28,701 ms), sin interferencia del WIP.
