# Handoff de sesión — ronda 21 (recuperación de presupuestos de rendimiento + progress status generado)

**Fecha**: 2026-11-10 (Europe/Madrid)
**Rama**: `sprint/robustez-fase4` — HEAD `a8ce8ba` (merge del commit del dueño `23deef9`, que cambió el título del README; se conserva `# Megi Web Engine 🌐`).
**Estado**: **1345 tests, 0 fallos**; gate **PASS** con **árbol 3.904/3.904 (100 %)**; **presupuestos recuperados sin subirlos** — pipeline ≤140 ms ✅ (~132) y layout ≤43 ms ✅ (36,4); F4 ≈99 %, F5 ≈97 %, F6 ≈90 %, F7 ≈80 % (overall 95 % en el status generado).
**CI/Repo**: GitHub Actions en verde, con el paso **Status freshness**; `main` y `sprint/robustez-fase4` sincronizadas en `a8ce8ba`; fuzz PR build-only desde la ronda 18; suite determinista.
**Estado de fases**: fuente de verdad generada en [`PROGRESS.md`](PROGRESS.md) + [`docs/status/`](docs/status) (mapa/badges), regenerable con `python3 tools/update_progress.py --write`; **no editar a mano** los ficheros generados.

## Qué se hizo en la ronda 21 (5 commits + merge del dueño)

| Commit | Bloque | Resultado |
|---|---|---|
| `26560cc` `perf(html)` | **Fast paths del tokenizer** | Bulk para tag/atributos/nombres/valores, runs de texto UTF-8 byte a byte, preprocesado byte-oriented con hint `has_wide` y dedupe híbrido de atributos. A/B: `html/realistic-1mb` 20,0→**16,4 ms** (**−18/−20 %**), tag-40k 11,7→9,2, nested-div 20,1→17,1, pretty-5k 11,0→8,8, massive-formatting 12,3→10,9, massive-end-p 19,3→18,4; gate exacto intacto. |
| `0ea54b5` `perf(css)` | **Cascada FNV + caché de estilos** | Mapa de ganadores con hasher FNV y claves `&str` prestadas + **caché de `ComputedStyle`** keyed por 128 bits (identidad de declaraciones ganadoras + NodeId del padre; sin inline style, umbral 2 avistamientos, cap 1024). A/B css resolve 38,84→**33,91 ms** (**−12,7 %**, ≤35 ✅) y pipeline 154,13→**132,39 ms** (−14,1 %). Descartadas medidas: Vec O(n²) (+15 %), búsqueda binaria + ordenación (+39 %), cache UA clonada (+16 %), pointer-cache sin umbral (+11 %). |
| `d0249ce` `perf(layout)` | **Interning de `ComputedStyle`** | tree_builder interná por fingerprint barato + `eq` exacta antes de compartir `Arc`; estilos anónimos perezosos/cacheados por padre, fast path de spacing cero en IFC, `#[inline]` en resolvers y grid por índice en vez de `Arc::ptr_eq`. Perfil previo: tree-build 59,5 % del layout con 13,0 ms de clones. A/B layout 46,6→**36,4 ms** (**−21,8 %**, ≤43 ✅), sticky 42,8→32,3, pipeline 141→131 (−10). |
| `fd2c471` `perf(paint)` | **Display list fast paths** | Retorno barato por caja en `paint_background_border` (28.191 cajas sin fondo/borde/sombra/radio), `DisplayList::with_capacity(16384)` (API aditiva), skip de text-decoration vacío, `first_text_baseline` sin `Vec` y stacks reservados. A/B build-display 8,62→**7,84 ms** (**−9,1 %**); goldens intactos. |
| `aa0ae74` `chore(status)` | **Refresco del status generado** | Actualiza `PROGRESS.md`, `docs/status/progress.svg`, badges y `docs/status/progress.toml` al HEAD con 1345 tests; snapshot: overall 95 %, F1 100/F2 100/F3 100/F4 99/F5 97/F6 90/F7 80, 14 crates, ~108.972 líneas Rust, 12/14 `forbid(unsafe_code)`. |
| `a8ce8ba` merge | **Sincronización con `main`** | Integra `23deef9` del dueño ("Update project title in README"); el título del README se conserva. |

## Evidencia verificada

- `cargo test --workspace` → **1345 tests, 0 fallos**: css **175**, layout **197**, paint **175**, render **136**, shell **140**, net **207**, svg **73**, html **60**, js **71**, dom **28**, image 45 + 1 doctest, fonts 17 + 1 doctest, bench **8**, test_harness **11**.
- Gate (`cargo run -p test_harness -- --summary`, exit 0): tokenizer **7.050/7.050**, árbol **3.904/3.904**, serializer **230/230**, CSS **30/30**; allowlists tokenizer/tree **(none)**.
- **A/B de rendimiento** (binarios pre/post del mismo árbol, intercalado, `taskset`): html realistic-1mb 20,0→16,4 ms; css resolve 38,84→33,91 ms; layout 46,6→36,4 ms (sticky 42,8→32,3); build-display 8,62→7,84 ms; pipeline 160,1→~130-133 ms (A/B 154,13→132,39). **Los presupuestos ≤140 (pipeline) y ≤43 (layout) son gates y NO se subieron.**
- **Goldens byte-idénticos** (showcase `c95c7eff…`, basic.rgba `60f8984e…`, PNG `9e957177…`, basic `7324e851…`) y gate sin allowlist de tree.
- **Progress status**: `PROGRESS.md`/`docs/status/*` generados desde el repo; CI incluye **Status freshness**. Regenerar con `python3 tools/update_progress.py --write`; validar con `--check --no-gate` (o el paso de CI).

## Comandos de verificación

```bash
./tools/dev.sh all                                  # fmt, clippy, 1345 tests, doc y gate
cargo test --workspace                              # 1345 tests (JS por defecto)
cargo run -p test_harness -- --summary              # gate: 3904/3904 árbol, PASS, exit 0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo doc --workspace --no-deps
cargo +1.88 check --workspace --all-targets         # MSRV

# Ronda 21 (A/B de referencia; usar binarios pre/post para comparar)
cargo run --release -p bench -- --filter html/realistic-1mb
cargo run --release -p bench -- --filter resolve-styles-10k-nodes
cargo run --release -p bench -- --filter layout/realistic-1mb
cargo run --release -p bench -- --filter build-display
cargo run --release -p bench -- --filter pipeline/html-to-png

# Progress status generado
python3 tools/update_progress.py --check --no-gate    # frescura (CI)
python3 tools/update_progress.py --write              # regenera PROGRESS.md + docs/status/*
```

## Estado actual por crate

| Crate | Estado | Tests |
|---|---|---|
| `dom` / `html` | COMPLETOS (arena + parser 3.904/3.904); html r21 con fast paths byte-oriented | 28 / 60 |
| `css` | F2+F4 + var/calc/pseudos + gradientes + customs lazy/COW + **cascada FNV/caché de estilos** | **175** |
| `layout` | F4 completa + flex/grid + calc mixto/sticky/tipografía + **interning de `ComputedStyle`** | **197** |
| `fonts` | COMPLETO LTR + fuentes web | 17 + 1 doctest |
| `paint` | F4 completa + backgrounds/decoraciones/clipping/gradientes + **fast path de display list** | **175** |
| `net` / `image` | F5 (TLS/caché/SameSite/PSL/Referrer/Range) / decoder PNG | **207** / 45 + 1 doctest |
| `js` | F6: DOM/eventos/timers + APIs de navegador (sin cambios r21) | 71 |
| `svg` | Parser/modelo/rasterizador + CSS/gradientes/pattern/text | 73 |
| `render` | Pipeline URL→PNG + imágenes/SVG/webfonts + event loop | **136** |
| `shell` | F7 + zoom/refresh/forms/find-in-page/pestañas | **140** |
| `bench` / `test_harness` | Tooling (filas E2E) / gate 4 suites | 8 / 11 |
| `fuzz` (workspace aparte) | 7 targets + corpus + job PR build-only (sin cambios r21) | build 7/7 |

## Deuda priorizada

1. **Perf**: re-baseline en calma (otro carril) para confirmar absolutos; si aparece margen, siguientes palancas son layout (resto del tree-build) y render/paint; no tocar los presupuestos.
2. **CSS/paint**: `display-p3`/espacios de color avanzados, `image-set()`, `border-image`, filtros; `calc()` mixto en más propiedades.
3. **Grid/layout**: `dense`, zonas/`grid-template-areas`, auto-fit real, solapes explícitos; block-in-inline y colores de `border-collapse`; sticky con scroll real.
4. **Shell**: barra visual de pestañas, reordenar/sesión/reopen, aislamiento, widgets nativos/select, validación/submit, event loop vivo multi-frame.
5. **Red/JS**: HTTP/2, streaming, PSL completa, multi-range; módulos ES, memoria real, eventos resize/scroll/change.
6. **Status/fuzz/bench**: mantener el snapshot generado al cerrar cada ronda (`tools/update_progress.py --write`); campañas de fuzz más largas en CI; activar `layout/sticky-text-1mb`.

## Reglas del repositorio (recordatorio)

- Sin *forks* de motores ni crates de parsing/layout/imagen de terceros. Núcleo (`dom`, `html`, `css`, `layout`) sólo `std`; `fonts` usa la pila tipográfica autorizada **y `woff2-patched` 0.4 (Apache-2.0, pure Rust)**; `net` usa `rustls`/`webpki-roots`/`rcgen` (tests); `image`/`svg` std (+`image`/`fonts` internos); `js` usa **Boa 0.21.1**; `render` no añade deps externas; `shell` usa `winit`/`softbuffer`; `test_harness` usa `js`.
- `#![forbid(unsafe_code)]` en todos los crates; `fuzz/` workspace aparte (nightly + ASan).
- Cero advertencias: `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo doc`.
- Tests en el mismo commit; *Conventional Commits*; no commit/push desde agentes.
- **Presupuestos = gates**: pipeline ≤140 ms y layout ≤43 ms; recuperarlos, nunca subirlos.
- `tests/html5lib-tests` versionado; de `tests/wpt/` sólo `tests/wpt/css/css-syntax/`; `.agents/`/`.cache/` ignorados.
- `docs/PERFORMANCE.md` lo mantiene el carril de rendimiento; `PROGRESS.md`/`docs/status/*` son generados (no editar a mano).
