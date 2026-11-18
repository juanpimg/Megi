# F3 — Tooling, CI, manifiestos y docs (ola 2)

Auditor: wave2 F3/F4 (scope F3 = `w2-ci`, scope F4 = `w2-perf`, este fichero).
HEAD: `3c06db8c186e4f7a19c7816aed4955a0b68f91ef`.
Árbol principal con WIP del usuario en `crates/js/src/host.rs`, `crates/js/src/runtime.rs`,
`crates/render/src/lib.rs`, `crates/render/src/scripts.rs`, `crates/render/tests/iframe_tests.rs`
(leídos, nunca editados). Todos los comandos `cargo`/gates se ejecutaron en el worktree
limpio `/tmp/opencode/megi-verify` (mismo HEAD, `git status` limpio); los hallazgos que
dependen sólo de cambios sin commitear se marcan `excluir: WIP del usuario`.

## Alcance

- `tools/`: 16 `.py` + 6 `.sh` (ver Cobertura), leídos línea por línea.
- `.github/workflows/`: `bench.yml`, `ci.yml`, `coverage.yml`, `dependabot-auto-merge.yml`,
  `docs.yml`, `fuzz-nightly.yml`, `fuzz-soak.yml`, `issue-title-labeler.yml`, `labeler.yml`,
  `release.yml` + `.github/actions/install-system-deps/action.yml`, `.github/CODEOWNERS`,
  `.github/dependabot.yml`, `.github/labeler.yml`, `.github/PULL_REQUEST_TEMPLATE.md`,
  `.github/ISSUE_TEMPLATE/{bug_report,feature_request,security_report}.yml`, `.gitleaks.toml`.
- Manifiestos: `Cargo.toml`, `Cargo.lock` (verificación de pins por grep: 90 KB),
  `deny.toml`, `cliff.toml`, `rust-toolchain.toml`, `.cargo/config.toml`.
- Docs: 37 `.md` bajo `docs/` + 5 raíz (`README.md`, `CONTRIBUTING.md`, `SECURITY.md`,
  `CHANGELOG.md` —cabecera + grep de tooling—, `PROGRESS.md` —generado, verificado vía gate—).
  (El encargo habla de 45 archivos bajo `docs/`; en disco hay 37 `.md`.)
- Pendientes explícitos: `tools/check_fixture_hashes.py`, `docs/audit/findings/f7/00-closeout.md`.

## Cobertura

`leído — sin hallazgo` salvo donde se indica hallazgo.

- tools: `audit_coverage.py`, `audit_recover_issues.py`, `bench_compare.py`, `bench.sh`,
  `check_docs_links.py`, `check_fixture_hashes.py`, `coverage_report.py`, `dev.sh`, `megi.sh`,
  `png_check.py`, `render_fixture.sh`, `soak_fuzz.sh`, `test_audit_coverage.py`,
  `test_inspector.py`, `test_png_check.py`, `test_update_coverage.py`, `update_coverage.py`,
  `update_progress.py`, `update_psl.sh`, `whatwg_spec.py` — sin hallazgo.
- tools: `real_site_check.py` — **hallazgo w2-ci-01**.
- workflows: `bench.yml`, `dependabot-auto-merge.yml`, `docs.yml`, `fuzz-soak.yml`,
  `issue-title-labeler.yml`, `labeler.yml`, `release.yml`, `install-system-deps/action.yml`,
  `CODEOWNERS`, `dependabot.yml`, `labeler.yml`, `PULL_REQUEST_TEMPLATE.md`,
  `ISSUE_TEMPLATE/*`, `.gitleaks.toml` — sin hallazgo.
- workflows: `ci.yml`, `coverage.yml`, `fuzz-nightly.yml` — sin hallazgo nuevo;
  verificado que los defectos #1308/#1311 están corregidos en HEAD (comandos abajo).
- manifiestos: `Cargo.toml`, `deny.toml`, `cliff.toml`, `rust-toolchain.toml`,
  `.cargo/config.toml` — sin hallazgo. `Cargo.lock`: pins verificados por grep
  (`boa_engine 0.22.0`, `woff2-patched 0.4.0`, `rustls 0.23.46`, `winit 0.30.13`,
  `softbuffer 0.4.8`, `rustybuzz 0.20.1`, `ttf-parser 0.25.1`, `proptest 1.12.0`, …) — sin hallazgo.
- docs sin hallazgo nuevo (verificadas contra HEAD; el F5 `docs-audit` + issues
  #1506–#1508, #1511–#1515 ya cubren la deriva anterior): `ARCHITECTURE.md`,
  `CONTRIBUTOR-ONBOARDING.md`, `FUZZING.md` (ver w2-ci-03 por deriva nueva),
  `GAPS.md`, `GETTING-STARTED.md`, `HANDOFF.md`, `JAVASCRIPT-STRATEGY.md`, `MILESTONES.md`,
  `PERFORMANCE.md` (números r36 coherentes con `bench-baseline.json`: round 36, 81 filas),
  `PHASE4-SUPPORT.md`, `RELEASE.md`, `ROADMAP.md`, `SHOWCASE.md`, `TOOLING.md` (ver w2-ci-03),
  `status/COVERAGE.md`, `audit/README.md`, `audit/FINDINGS.md` (ver w2-ci-02),
  `audit/findings/f1/00-index.md`, `audit/findings/f2/00-index.md`,
  `audit/findings/f4/00-perf-baseline.md`, `audit/findings/f5/*.md`,
  `audit/findings/f7/00-closeout.md`, `audit/findings/wave2/00-baseline.md`,
  `superpowers/plans/2026-11-18-auditoria-linea-por-linea-megi.md`,
  `superpowers/specs/2026-11-17-honest-progress-design.md`,
  raíz `README.md`, `CONTRIBUTING.md` (ver w2-ci-03), `SECURITY.md` (ver w2-ci-03),
  `CHANGELOG.md`, `PROGRESS.md` (generado; `--check` verde en HEAD limpio).
- `docs/status/` datos (`coverage.toml`, `progress.toml`, `real_sites.toml`,
  `bench-baseline.json`): verificados por los gates, sin hallazgo.

## Hallazgos

### w2-ci-01 — `failure_reasons` compara contra `"PÁGINA EN BLANK"` y ninguna página en blanco falla nunca

- `archivo:línea`: `tools/real_site_check.py:192`
- `lente`: contrato vs documentación/especificación
- `severidad`: P2
- `categoría`: bug
- `title`: la cadena centinela de página en blanco no coincide con la que produce `classify()`; el gate documentado como "hard failure" nunca dispara
- `evidence`:
  - `tools/real_site_check.py:45-52`: `classify()` devuelve `"PÁGINA EN BLANCO"` cuando `display_items <= 0`.
  - `tools/real_site_check.py:192`: `if item.get("result") in ("ERROR", "PÁGINA EN BLANK"):` — `"PÁGINA EN BLANK"` (sin `CO`) no la produce ningún camino.
  - Repro (2 ejecuciones, árbol sucio + worktree limpio, mismo resultado):
    `python3 docs/audit/findings/wave2/repro-w2-ci/blank_gate.py` →
    `classify blank -> 'PÁGINA EN BLANCO'` y `failure_reasons -> []` (exit 1 = bug presente).
  - El docstring de `failure_reasons` (`issue #1485`): "A blank page, a render error, or fewer
    than the manifest's `min_items` are hard failures" — falso para páginas en blanco.
  - `tools/test_real_site_check.py` cubre `classify()` y `render_html()` pero no
    `failure_reasons()` (4 tests, ninguno lo toca): el defecto escapa por hueco de test.
- `impact`: una regresión que deja un snapshot en blanco (`display_items = 0`, el caso de
  `wikipedia`/`hn` con `issue`/`min_items`) sale con exit 0 salvo que `min_items` lo atrape;
  el informe HTML la pinta en rojo pero el gate no falla.
- `expected`: `failure_reasons` debe tratar `"PÁGINA EN BLANCO"` como fallo duro, como
  documenta su docstring y como cuenta `render_html()` (`"PÁGINA EN BLANCO"` en el resumen).
- `fix`: comparar contra la constante que produce `classify()` (extraer
  `BLANK = "PÁGINA EN BLANCO"` compartida) y añadir un test de `failure_reasons()` con
  resultado en blanco.
- `repro`: `docs/audit/findings/wave2/repro-w2-ci/blank_gate.py`.
- `dedup`: new (sin ocurrencias de `EN BLANK`/`failure_reasons` en `docs/audit/FINDINGS.md`
  ni en issues abiertas/cerradas: búsqueda `real_site_check failure_reasons` vacía).

### w2-ci-02 — #1308/#1311 cerrados como "Deferred" pero corregidos en HEAD; FINDINGS hereda la disposición errónea

- `archivo:línea`: `docs/audit/FINDINGS.md:108-109` (filas generadas para #1308/#1311)
- `lente`: contrato vs documentación
- `severidad`: P3
- `categoría`: docs
- `title`: dos issues CI cerrados con "Deferred: complex CI/docs fix…" están fijos en HEAD; el ledger de auditoría los registra como diferidos
- `evidence`:
  - #1308 (`| tee` sin `pipefail`): `git blame .github/workflows/ci.yml:137,183` →
    `set -o pipefail` añadido por `7c81cb4` (2026-11-17 01:49 +0100), 5 min ANTES del cierre
    como Deferred (2026-11-17T00:54:57Z). Demo: `bash -c '(exit 3) 2>&1 | tee /tmp/t.log'`
    → `step_status=0` (patrón antiguo traga el fallo); con `set -o pipefail` → `step_status=3`.
    En HEAD ambos `tee` (líneas 138, 184) están bajo `set -o pipefail`; no hay otros `| tee`.
  - #1311 (coverage floor): cerrado Deferred 00:54:56Z; `0243396 fix(ci): make coverage floor
    check fail the job (#1311)` (02:41 +0100) añade `set -e` a `coverage.yml`. Demo con lcov
    sintético al 50 %: `coverage_report.py --min 60` → exit 1; simulación del step actual
    `bash -c 'set -e; { ...; python3 ... --min 60; ...; } >> sum'` → `bash_exit=1`
    (el step muere, `STEP_SURVIVED` no se imprime). Patrón antiguo sin `set -e` → exit 0.
  - `audit_recover_issues.py` clasifica por comentario de cierre, así que el boilerplate
    "Deferred" contamina `FINDINGS.md` aunque el fix exista.
- `impact`: el informe F7 cuenta 118 diferidos wave-1 con 2 (al menos) que no lo son;
  priorización del backlog ligeramente distorsionada.
- `expected`: la disposición debe reflejar el estado del código (fixed), no sólo el comentario.
- `fix`: re-clasificar #1308/#1311 a `fixed` en la próxima regeneración de FINDINGS
  (o anotar la excepción); considerar regla en `audit_recover_issues.py` que cruce el
  comentario con commits `fix(...): ... (#n)`.
- `repro`: comandos `git blame`/`git log -S` + demos de arriba.
- `dedup`: new (disposición, no defecto de producto).

### w2-ci-03 — deriva nueva de conteos de fuzz en docs tras #1507 (26→28 bins, corpus 123→258)

- `archivo:línea`: `docs/FUZZING.md:11,24,240`, `docs/TOOLING.md:§2.8`, `SECURITY.md:72-85`,
  `CONTRIBUTING.md:44`
- `lente`: contrato vs documentación
- `severidad`: P3
- `categoría`: docs
- `title`: los documentos vuelven a declarar 26 targets de fuzz (y corpus 158) tras añadir `http_head`/`compression`
- `evidence`:
  - `grep -c "\[\[bin\]\]" fuzz/Cargo.toml` → **28** (bins nuevos: `compression` #1424
    2026-11-18, `http_head` #1426 2026-11-18; `charset` #1427 2026-11-17). Matriz CI ASan:
    25 (`grep -c "target: " fuzz-nightly.yml` = 34 = 25 ASan + 9 UBSan); build-only:
    `charset`, `compression`, `http_head` (3, no 1).
  - `find fuzz/corpus -type f | wc -l` → **258** (top: charset 96, hpack 21, …).
  - Docs: `FUZZING.md:11` "25 harnesses … (26 `[[bin]]`…; `charset` is compiled but build-only)";
    `:24` árbol "26 libFuzzer harnesses"; `:240` "26 harnesses … 25 run"; `TOOLING.md §2.8`
    "26; 25 in the CI ASan matrix … 158 ficheros"; `SECURITY.md:72` "**26 targets** (lista de 7)"
    + `:80-83` "5-minute smoke … over the 7-target matrix" (matriz real: 25 con presupuestos
    600 s ×10 / 300 s ×15 + UBSan ×9); `CONTRIBUTING.md:44` "compila los 21 targets".
  - Todos los targets tienen seed (`FuzzCorpusTest` verde; `charset` 96, `compression` 9,
    `http_head` 3) — el defecto es sólo documental.
- `impact`: un lector (o un auditor futuro) dimensiona mal la superficie fuzz; SECURITY
  describe una matriz CI que no existe (7 targets/5 min).
- `expected`: 28 bins, 25 en matriz ASan (+9 UBSan), 3 sólo-build, corpus con conteo vivo
  o sin número absoluto.
- `fix`: actualizar los 4 documentos; a futuro, frasear sin números absolutos o generar el
  conteo (`FuzzCorpusTest` ya valida seeds; un `--check` análogo para la doc es opcional).
- `repro`: los comandos `grep`/`find` de arriba.
- `dedup`: #1507 (fix parcial 998d6bf "shell/image counts and MSRV note", cerrado 2026-11-17T20:46Z;
  los bins #1424/#1426 entraron DESPUÉS → re-deriva nueva, no duplicado del contenido).

## Comandos ejecutados

Worktree limpio `/tmp/opencode/megi-verify` @ `3c06db8` (`git status` limpio):

| Comando | Salida |
|---|---|
| `cargo fmt --all -- --check` | exit 0 (3 ejecuciones) |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 (4m12s; confirma que el error del árbol sucio era sólo WIP) |
| `python3 tools/audit_coverage.py --check` | exit 0: `1182 files, 623 covered, 60 pending, …, 5 file(s) to re-read, 1 unknown` (unknown = `wave2/00-baseline.md`; re-reads = host/runtime/lib/scripts/iframe_tests + …) |
| `python3 tools/update_progress.py --check --no-gate --no-count` | exit 0: `progress status is up to date` |
| `python3 tools/update_coverage.py --check` | exit 0: `coverage status is up to date` |
| `python3 tools/check_docs_links.py` | exit 0: `docs links: OK (39 files checked)` |
| `cargo deny check` | NO EJECUTABLE EN LOCAL: `error: no such command: 'deny'` (cargo-deny no instalado; CI lo instala vía taiki-e/install-action). Concern, no hallazgo. |
| `cargo package --allow-dirty -p dom` (con `--verify`, sin `--no-verify`) | exit 0 (verify compila `dom-0.1.0` OK, 4m15s) |
| `python3 -m unittest tools.test_audit_coverage tools.test_update_coverage tools.test_png_check tools.test_real_site_check` | 52 tests OK |
| Demos #1308/#1311 | arriba en w2-ci-02 |

Árbol principal (con WIP del usuario) — informativo, `excluir: WIP del usuario`:

- `cargo clippy … -D warnings` → error `empty line after doc comment` en
  `crates/js/src/runtime.rs:6479` (línea dentro del hunk añadido `@@ -6474,6 +6477,216`,
  `/// returns its host element (#1541).` + línea en blanco + `fn frame_handle`).
  El gate funciona (detecta el defecto del WIP); en HEAD limpio: exit 0.
- `update_progress --check` → stale `rust_lines 205371 vs 205680` (+309 ≈ +310/−1 del `git diff`);
  en HEAD limpio: verde. Causa = WIP, no deriva del manifiesto.
- `cargo fmt --check`: un primer run mostró un diff en `frame_handle` que desapareció en 3 runs
  posteriores (exit 0, fichero ya con formato multicadena, mtime 12:32 del WIP externo);
  atribuido a edición concurrente del WIP durante la medición, no reproducible.

## Concern

1. `cargo deny`/`cargo audit` no son ejecutables en local (herramientas no instaladas) y
   `cargo package --workspace --locked` de CI usa `--no-verify` (no compila el paquete);
   el único `package` con verify ejecutado aquí es `-p dom`. La superficie
   empaquetado/SBOM depende entera de CI.
2. `/tmp` (tmpfs 7,7 GB) se llenó durante la auditoría (`target/` del worktree + salidas de
   varios agentes); los builds del worktree limpio se redirigieron a
   `/home/jp/.cache/megi-verify-target`. `cargo` serializa con locks pero la contención
   entre agentes ralentiza todos los gates.
3. Disposiciones "Deferred" con fix posterior/preexistente (w2-ci-02): el proceso de cierre
   con boilerplate contamina FINDINGS.md; conviene cruzar comentario↔commits `fix(#n)`.
