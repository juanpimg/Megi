# ADR — Estrategia WebGL: rasterizador software en Rust puro

- **Estado**: ACEPTADO — decisión (a) del plan vigente, ejecutada en el WP `WP-DEPTH-m6-webgl-software`.
- **Fecha**: 2026-11-24 (plan), registrado 2026-11-25 con código, tests y WPT medidos.
- **Hito**: M6, criterio «WebGL 1 por software» (`docs/status/progress.toml:157` antes del cierre).
- **Referencias**: spec v2 §4.1 (`docs/superpowers/specs/2026-11-24-megi-ambition-design.md:76`); master plan M6 (`docs/superpowers/plans/2026-11-24-megi-ambition-master-plan.md:96`); plan de gráficos, Tasks 1 y 3 (`docs/superpowers/plans/2026-11-24-graphics-media-plan.md:27-37`, `:51-58`); medición WPT (`docs/status/wpt.json`, `docs/status/wpt.md`).

## Contexto

El plan exige un spike y una decisión ADR entre (a) Rust puro, (b) WASM aislado y (c) librería C en proceso separado, más un triángulo dibujado por software y WPT WebGL con allowlist 0. Antes de este WP existía un foundation real (`crates/js/src/canvas/webgl.rs`) con `clearColor`, `clear`, `viewport`, `enable`, `readPixels` y `drawArrays(POINTS)`, pero el propio archivo declaraba «no shaders, no buffers, no textures… not WebGL 1». No existía este ADR, ni buffer objects, ni primitivas de triángulo, ni ningún fixture WebGL en `tests/wpt/`.

La barra de producción de spec §2 no permite rebajar WPT ni contar `skip` como PASS. Por tanto el cierre tiene que apoyarse en un caso upstream ejecutado de verdad, con providencia SHA-pinneada y allowlist vacía.

## Decisión

Se adopta la **rama (a): rasterizador software en Rust puro**, dentro de `CanvasSurface` y detrás de las mismas *capabilities* que ya aíslan el contexto 2D. Concretamente:

1. `crates/js/src/canvas/webgl.rs` rasteriza `TRIANGLES` con una función de borde determinista sobre coordenadas de framebuffer, con origen bottom-left y sin antialiading ni blending. `POINTS` sigue disponible y comparte la conversión de viewport.
2. `crates/js/src/runtime/gap_apis/webgl_buffers.rs` añade `createBuffer`, `bindBuffer`, `bufferData`, `bufferSubData` y `getError` con almacenamiento de bytes ownado dentro del estado del canvas; las constantes de interfaz (`WebGLRenderingContext.NO_ERROR`, etc.) se instalan en `gap_apis/install.rs`. No se introduce FFI, `unsafe`, `Rc`, `Weak`, lifetimes propios ni dependencias externas.
3. El pin WPT `ab2ef95d3a7c512a6e7321688c80e428086af426` aporta `webgl/bufferSubData.html` y su helper `webgl/common.js`, importados byte-idénticos mediante `tools/vendor_wpt.py --files`. El subtest se ejecuta en el harness con `getContext('experimental-webgl')`, buffers reales y conversión WebIDL de `NaN → 0` para el desplazamiento.
4. El cambio visual queda fijado por `tests/fixtures/render/webgl_triangle.html` y `tests/fixtures/render/webgl_triangle/golden.png`, con dos renders byte-idénticos verificados por `tools/render_fixture.sh`.

**No se eligen** la rama (b) ni la (c) en este WP: no hay evidencia de que el rasterizador actual incumpla la barra M6, y ambas añadirían toolchain, licencias y una frontera de aislamiento sin necesidad medible. Quedan diferidas, no descartadas.

## Evidencia medida

- `cargo +1.99.1 test -p megi-js --test webgl_tests` → **5 passed / 0 failed**, EXIT=0. Incluye `webgl_triangle_rasterises_a_real_surface_region` y `webgl_buffer_sub_data_accepts_nan_offset_without_error`; el primero falló en rojo con `left: "255,0,0,255|255,0,0,255"`, el segundo con `TypeError: not a callable function`.
- Harness `cargo +1.99.1 run --release -p megi-test-harness -- --summary` → `webgl Total: 1 / 1 tests passed`, sin `[FAIL]`, `Conformance gate: PASS`, EXIT=0.
- `tools/wpt_report.py --write` → `wpt.json/wpt.md (596/596)`; `tools/wpt_gate.py --rebaseline` → `rebaselined 596/596` (manifest cambió al adoptar dos archivos nuevos y hubo **0 FAIL**); `tools/wpt_gate.py --check` → `PASS — 596/596 subtests, allowlist clean`; `tools/vendor_wpt.py --check` → `135 declared file(s)`.
- `tools/render_fixture.sh` → dos md5 `3d079f295aa37f644196272a7bdf12f8`, «byte-identical across two runs» y coincidencia byte-for-byte con el golden, EXIT=0.

## Coste y consecuencias

- **Coste inmediato**: un módulo JS nuevo, extensiones acotadas en el estado WebGL, un fixture WPT con su helper, un fixture de render con golden y este ADR. No hay cambios de DAG ni nueva dependencia; MSRV y `#![forbid(unsafe_code)]` quedan intactos.
- **Beneficio**: se puede cerrar M6#4 con un subtest upstream real y una imagen determinista, sin simular PASS ni elevar presupuestos.
- **Coste continuo**: el subset es deliberadamente estrecho; cualquier ampliación WebGL (shaders, texturas, profundidad, más targets) debe pasar por un WP nuevo con su propio ADR si cambia la estrategia.

## Límites explícitos

- Esto **no es WebGL 1 completo**: no hay shaders, programas, texturas, depth buffer, VAO, `TRIANGLE_STRIP`/`TRIANGLE_FAN`, blending ni antialiasing. `drawArrays(TRIANGLES)` rasteriza las triples de la propiedad determinista `vertices`, no un `ARRAY_BUFFER` vía `vertexAttribPointer`.
- El WPT adoptado es **un caso real** (`bufferSubData`), no la suite WebGL completa; la barra cerrada es «WebGL WPT» con allowlist 0 sobre el subset revisado, no conformance completa de WebGL 1.
- El color del triángulo es fijo (`POINT_COLOR`); no hay programación de fragmentos.
- El golden cubre el rasterizador y el helper de buffers se ejecuta en el fixture, pero el contenido de los buffers no es visible en la imagen: su comportamiento se verifica en el test JS y en el subtest WPT.
- La decisión (a) se revisaría sólo si aparece un requisito de la barra §2 que el rasterizador software no pueda satisfacer (por ejemplo, un WPT WebGL que exija shaders/texturas, o un presupuesto de render superado con A/B reproducible). En ese caso se reevalúan (b) y (c) con datos, no por dogma.

## Reproducción

```bash
PATH=/home/jp/.cargo/bin:$PATH TMPDIR=/var/tmp CARGO_FORK_AUTO_UPDATE=0 \
  CARGO_BUILD_JOBS=1 nice -n 19 cargo +1.99.1 test -p megi-js --test webgl_tests
PATH=/home/jp/.cargo/bin:$PATH TMPDIR=/var/tmp CARGO_FORK_AUTO_UPDATE=0 \
  CARGO_BUILD_JOBS=1 nice -n 19 cargo +1.99.1 run --release -p megi-test-harness -- --summary
python3 tools/vendor_wpt.py --check
python3 tools/wpt_gate.py --check
python3 tools/wpt_report.py --check
PATH=/home/jp/.cargo/bin:$PATH TMPDIR=/var/tmp CARGO_FORK_AUTO_UPDATE=0 \
  CARGO_BUILD_JOBS=1 nice -n 19 bash tools/render_fixture.sh
python3 tools/check_milestone.py M6 --match 'WebGL 1 por software'
```
