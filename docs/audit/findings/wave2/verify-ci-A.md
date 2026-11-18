# Verificación Mesa A — CI/tooling + perf (ola 2)

Fecha: 2026-11-18. Verificador (Mesa A, scope tools/CI/perf).
Base: worktree limpio `/tmp/opencode/megi-verify` @ `3c06db8`
(`git status` limpio salvo 2 fixtures untracked ajenos al scope:
`tests/fixtures/render/f2-exp10.html`, `f2-exp12.html` — no se tocan).
Env: `TMPDIR=/home/jp/.cache/megi-tmp`,
`CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`.
Chequeo HEAD actual (`d82118c`): `git diff 3c06db8 HEAD --` sobre
`tools/real_site_check.py`, `.github/workflows/ci.yml`,
`.github/workflows/coverage.yml`, `docs/audit/FINDINGS.md`,
`docs/FUZZING.md`, `docs/TOOLING.md`, `SECURITY.md`, `CONTRIBUTING.md`,
`fuzz/Cargo.toml`, `crates/bench`, `crates/svg`, `crates/layout`,
`tools/bench.sh`, `tools/bench_compare.py`, `docs/PERFORMANCE.md` →
**vacío**. Todo lo verificado vale también en HEAD actual.

## Tabla

| id | veredicto | comando | salida literal | nota |
|---|---|---|---|---|
| w2-ci-01 | VERIFIED | `python3 docs/audit/findings/wave2/repro-w2-ci/blank_gate.py` (desde verify; `sys.path` → `tools` del worktree) | `classify blank -> 'PÁGINA EN BLANCO'` / `failure_reasons -> []` / `exit=1` | `classify()` (`tools/real_site_check.py:54`) devuelve `"PÁGINA EN BLANCO"` pero `failure_reasons()` (`:192`) compara contra `"PÁGINA EN BLANK"`. El gate nunca falla por página en blanco, contra su docstring (hard failure, issue #1485). |
| w2-ci-02 | VERIFIED | `git blame -L 135,140 -L 181,186 .github/workflows/ci.yml` + demos `bash -c '(exit 3) … \| tee …'` con/sin `pipefail` + `coverage_report.py --min 60` sobre lcov sintético al 50 % | blame: `7c81cb4 … set -o pipefail` sobre ambos `\| tee` (`:138`, `:184`); sin pipefail `step_status=0`, con pipefail `step_status=3`; tool: `coverage 50.00 % is below the required 60 %`, `tool-exit=1`; bajo `set -e` el step muere (`step-exit=1`, `STEP_SURVIVED` no se imprime); `FINDINGS.md:108-109` sigue diciendo `Deferred` para #1308 y #1311 | Ambos fixes existen en HEAD; el ledger los registra como diferidos por el boilerplate de cierre. Disposición errónea confirmada. |
| w2-ci-03 | VERIFIED | `grep -c '\[\[bin\]\]' fuzz/Cargo.toml`; conteo matriz ASan/UBSan en `fuzz-nightly.yml`; `find fuzz/corpus -type f \| wc -l`; `grep` de conteos en docs | `bins=28`; matriz1=25 ASan, matriz2=9 UBSan; corpus=170; docs: `FUZZING.md:11` "25 harnesses … 26 `[[bin]]`", `:24` "26 libFuzzer harnesses", `:240` "26 harnesses … 25 run"; `TOOLING.md:169-170` "(26; 25 en matriz … 158 ficheros)"; `SECURITY.md:72` "**26 targets**", `:81` "7-target matrix"; `CONTRIBUTING.md:44` "21 targets" | Deriva documental real (26→28 bins, etc.). Nota: mi conteo de corpus es 170, no 258 (auditor) ni 158 (docs) — deriva confirmada bajo cualquier número; varianza anotada como concern, no refuta. |
| w2-perf-01 | VERIFIED | strip simulado en python sobre fixtures + `cargo run --release -p megi-bench -- --filter 'filter-fixture'` (verify, release) | strip: `filter_effects.html total=29 removed=0`; `backdrop_filter.html total=31 removed=2`; bench: `paint/filter-fixture 0.307 ms` vs `-no-effect 0.307 ms`; `pipeline/filter-fixture-300x260 0.905` vs `-no-effect 0.868` (ruido; el "sin efecto" sale más rápido) | El strip por línea no elimina nada del fixture (reglas de una línea); el delta no-effect es estructuralmente cero. Valores absolutos difieren de los del auditor (0.816/0.818, carga de máquina), pero la identidad de medianas reproduce. |
| w2-perf-02 | VERIFIED | lectura `crates/bench/src/benches.rs:1414-1428` + aritmética | viewport `megi_render::Viewport::new(800, 600)` (`:1415`); `.with_units(0.096, "MPix")` (`:1425`); 800×600 = 0.48 MPix = 5× lo declarado | Constante infravalorada 5×; throughput publicado inválido. (Sin re-medir: la división es exacta.) |
| w2-perf-03 | VERIFIED | lectura `runner.rs:108-116,155-156`, `cli.rs:12-13,90-115`, `report.rs:24-30,134-135,159` + runtime `./bench --help` y `--warmup 0 --iterations 1 --filter tag-40k-attrs` (+ variante `--json`) | código: `opts.iterations.min(3)`, `opts.warmup.min(1)`; USAGE documenta cap sólo en `--iterations`; `--warmup` sólo rechaza `> MAX` (0 pasa; `--iterations` sí rechaza 0); meta estampa `opts.*` (pedidos); runtime: header `1 iterations (+0 warm-up)`, fila OK; JSON: meta `"iterations": 5, "warmup": 0` pedidos vs fila `"iterations": 3` con 3 samples (efectivo) | `--warmup 0` aceptado en runtime y la meta publica valores pedidos. |
| w2-perf-04 | VERIFIED | `grep -n "layout_document(" crates/bench/src/benches.rs` + `sed -n '49,64p' crates/layout/src/lib.rs` + `grep layout_document_with_measurer crates/render/src/lib.rs` | bench: call sites `:353,:387,:452,:568,:600,:623,:861,:942,:1025` todos a `layout_document(...)`; lib: `"Backwards-compatible deterministic measurement (CSS ratio fallback). The production render pipeline must use layout_document_with_measurer with FontTextMeasurer…"`; producción (`render/src/lib.rs:1269,1496`) usa `layout_document_with_measurer_and_pseudos` | Los 3 call sites citados (:861,:942,:1025) usan el fallback; producción usa el measurer real. (Sin re-medir baseline: el defecto es de qué se mide, visible en código.) |
| w2-perf-05 | VERIFIED | lectura `gradient.rs:206-…` (bucle `for stop in &self.stops[1..]`), `:20 MAX_STOPS=256`, `raster.rs:1583` (`spec.evaluate(...)` por píxel) + timing `release/render` sobre `repro-w2-perf/gradient-{4,256}-stops.svg` a 1024² ×3 + `png_check.py` | código: scan lineal con early-out intacto, sin `partition_point`/LUT; timing: 4 stops `129/119/120 ms`, 256 stops `179/169/170 ms` (≈+42 %); PNG 1024×1024 = 1048576 px, `png_check OK` | Dirección del escalado reproduce la del auditor (ellos: mín 313 vs 440 ms, +41 % a 1024²; yo: +42 %). Absolutos menores (render directo del .svg vs su arnés). Scan lineal confirmado en código. |

## Comandos globales

- `git diff 3c06db8 HEAD --stat -- <paths del scope>` (árbol principal) → vacío:
  hallazgos vigentes en HEAD actual, ningún `WIP` implicado.
- `cargo run --release -p megi-bench` compila en el target en disco sin errores
  (el tmpfs de `/tmp` sigue siendo el motivo del `CARGO_TARGET_DIR` redirigido).
- No se editó código de producto ni ledger. Artefactos generados: PNGs en
  `/tmp/opencode/g*.png`, `/tmp/opencode/cov50.info`, logs en `/tmp/opencode/`.
  Nada escrito fuera de este acta y esos temporales.

## Concerns

1. **Conteo de corpus (w2-ci-03):** mido 170 ficheros (`git ls-files` y `find`
   coinciden, en verify y en principal); el auditor cita 258 y los docs 158.
   La deriva existe bajo los tres números, pero el 258 no reproduce — posible
   conteo con artefactos generados presentes en su momento. Mesa B/C: pedir
   precisión o aceptar con el número verificado (170).
2. **Tiempos absolutos (w2-perf-01/05):** difieren de los del informe por carga
   de máquina y método (render directo vs arnés del auditor), pero deltas y
   dirección reproducen. Ningún veredicto depende del valor absoluto.
3. **w2-perf-02/03/04** se verifican por código + aritmética/runtime CLI, sin
   re-medir baselines completas; suficiente porque el defecto es estructural
   (constante errónea, parseo que acepta 0, call site equivocado), no de magnitud.
4. El worktree verify reporta `worktree_dirty:true` en el JSON del bench por 2
   fixtures untracked ajenos al scope (`f2-exp10/12.html`); no afectan a ningún
   hallazgo (verificado por diff vacío en todos los paths del scope).
