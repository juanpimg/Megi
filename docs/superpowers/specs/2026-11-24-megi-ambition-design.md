# Diseño — Ampliar el alcance de Megi hacia la paridad con Ladybird

> **Estado**: propuesta para aprobación del dueño del proyecto. Documento de diseño; no contiene código ni cambia dependencias.
> **Fecha**: 2026-11-24 · **Fuente**: debate de cinco mesas paralelas (WPT, motor, seguridad/aislamiento, release engineering y red team de alcance) más lectura directa del repo en `main` (`5bfe2a7e`).
> **No es**: una promesa de paridad global con Ladybird. Es un plan para ganar en ejes medibles y acercarnos en compatibilidad con un método honesto.

---

## 1. Intención y por qué

Ladybird entró en alpha el 23-nov-2026: ~4–7 años, ~8 ingenieros full-time, 501(c)(3), 67.855 stars, 2.184.722 subtests WPT (~97 % de Chrome), motor propio con JIT, WASM, multiproceso sandboxeado, DevTools y distribución en Flatpak/APT/RPM/DMG con rolling updates.

Megi es pre-alpha: 17 crates, ~244.000 líneas Rust, 3.300 tests, `#![forbid(unsafe_code)]` en 17/17, gate html5lib completo y **30** comprobaciones artesanales sobre 3 ficheros de WPT CSS Syntax, sin release pública. Nuestro `coverage 100 % (75/75)` mide capacidades declaradas a mano; **no es comparable** con WPT.

El objetivo de este diseño es convertir una ambición legítima ("ser lo más grandes posible") en un plan que no mienta: atacar la compatibilidad con el **mismo método que Ladybird** (tests pequeños, agentes que preparan PRs, revisión humana), pero a nuestra escala real, y defender ejes donde nuestra arquitectura nos da ventaja estructural.

### 1.1 Veredicto honesto (de la mesa de alcance)

- La paridad global está **2–3 órdenes de magnitud** fuera de alcance: Ladybird representa ~51.000–90.000 horas-persona frente a ~170 horas de atención humana en Megi. Perseguirla destruye el proyecto.
- El cuello de botella **no es generar código** (los agentes ya comprimieron ~10–15 persona-años de LOC), sino **ser dueño de él**: comprenderlo, triarlo, revisarlo.
- Sí es legítimo y ambicioso: ganar en seguridad de memoria del código propio, determinismo byte a byte, transparencia verificable de métricas, huella/arranque reproducibles y un subconjunto WPT declarado que pasamos entero.

**Frase pública que debemos poder repetir sin avergonzarnos:**

> Megi no es aún el navegador que más sitios abre: es el motor web en Rust que mejor demuestra lo que sí hace —cero `unsafe` en nuestro código, los mismos píxeles siempre, y un subconjunto de estándares declarado que pasamos entero con la lista de lo que falta publicada.

---

## 2. Metas, no-metas y anti-objetivos

### 2.1 Metas (12 meses)

| # | Meta | Métrica verificable |
|---|---|---|
| G1 | North star externo: WPT real + sitios reales | `WPT <sha>: 17.000 / 25.000` sobre subconjunto fijado; `real-sites.html` 4 PASS / 0 DEGRADED, 0 errores JS, < 2,5 s |
| G2 | Motor usable en tareas reales | 10 fixtures E2E (login, búsqueda, POST, timers, fetch+CORS, storage, acordeón, modal…) pasan con servidor determinista |
| G3 | Ventaja de seguridad defendible | 0 `unsafe` en `megi-*` (CI), 31 targets de fuzz, 0 fugas cross-site (T1/T2), watchdog < 6 s, matriz de sandbox por plataforma |
| G4 | Ventaja de determinismo defendible | replay 20/20: mismo MD5 de PNG y mismos stats en 3 SO × 2 TZ × 2 locale |
| G5 | Producto creíble | release `continuous` diario + `beta` semanal, checksums + cosign + attestation, crash report y "reportar sitio roto" con consentimiento |

### 2.2 No-metas explícitas

- No paridad global de compatibilidad (2,18 M subtests).
- No "el navegador más rápido": hoy las webs reales tardan 1,4–5,4 s. Solo reclamaremos ventaja de arranque cuando esté medida en el mismo corpus.
- No fundación, 501(c)(3), contratación ni tiendas de aplicaciones en 12 meses.

### 2.3 Anti-objetivos (decididos, no negociables en este ciclo)

| Anti-objetivo | Razón |
|---|---|
| Motor JS propio ni JIT | ADR-001 ya lo descartó; un JIT añade codegen (no determinismo, `unsafe`, MSRV) y 5–8 persona-años. |
| WebGL / GPU / Skia | Rompen render CPU determinista y `forbid(unsafe_code)`; `getContext('webgl') === null` es conforme. |
| Vídeo con FFmpeg | C/FFI + licencias + 80–120 días. Solo audio tardío con `symphonia` es plausible. |
| Service Workers completos | push/background sync/notificaciones requieren integración OS y un event loop que hoy no existe. Solo fase 1 (Cache + intercepción). |
| HTTP/3/QUIC, DevTools completos, Test262 completo, ICU completo | coste alto, impacto marginal en el MVP. |
| Copiar la arquitectura de Ladybird en Rust | coevolucionó con 70.000 h; importar su complejidad sin su plantilla destruye G4. El DAG de 17 crates funciona: no se rearquitectura por paridad. |
| Perseguir stars | no compilan tests ni arreglan `:link`. |

---

## 3. Estado real (verificado, para no planificar sobre fantasmas)

- `docs/GAPS.md` es histórico (11-12): muchas filas `absent` ya están implementadas (`forms/submit.rs`, `frames.rs`, `canvas.rs`, `transform.rs`, `net/src/{csp,integrity,hsts,mixed_content}.rs`, `xhr.rs`, `websocket.rs`, `shadow_dom.rs`, `a11y_backend.rs`, `image/src/avif.rs`). **No usar como plan de ataque.**
- Brecha real: **profundidad, no anchura**. Casi todo existe como subset acotado con caps.
- Event loop de `render` es virtual y acotado por render (`max_tasks` 10.000); el shell solo tiene `--refresh-ms` opt-in. No hay bucle vivo por pestaña.
- Ausencias duras: ServiceWorker, EventSource, WebAssembly, WebGL, vídeo/audio, HTTP/3, DevTools, JIT (0 coincidencias).
- Seguridad: `SiteKey` puro, pool de hilos y de procesos por sitio, IPC con framing y cap antes de asignar, `catch_unwind`, E2E real. **Pero**: jar de cookies global copiado a cada renderer (incluye HttpOnly de otros sitios); el renderer devuelve cookies y el padre las aplica sin validar contra su sitio (inyección de cookies en cualquier dominio); el child hereda entorno y cwd completos; sin watchdog, RSS cap ni sandbox de SO; iframes cross-site comparten renderer.

**Catálogo de amenazas (IDs T, referenciados en G3 y Q1)** — los mismos de la mesa de seguridad, con su bloqueo por fase:

| ID | Amenaza | Bloqueo |
|---|---|---|
| T1 | Obtener cookies de otros sitios, incl. HttpOnly (jar global sembrado en el child) | F0: jar por sitio + sembrado filtrado |
| T2 | Inyectar cookies en cualquier dominio devolviendo `IpcCookie` manipulado | F0: quitar `cookies` del response + validar dominio en el padre |
| T3 | Leer archivos locales (`file://` sin confinamiento para scripts/módulos) | F0/F1: `FileGrantTable` canonicalize + symlink-safe; F2: Landlock |
| T4 | Exfiltrar por red (renderer con sockets, TLS y entorno completos) | F2: seccomp sin `connect`; F3: broker con token de sitio |
| T5 | DoS: sin deadline ni RSS cap; child colgado ocupa el slot | F0: watchdog 5 s + kill de grupo + rlimit/cgroup |
| T6 | OOM por asignaciones gigantes en decodificadores (sin corrupción de memoria) | F4: image process + RSS cap + caps existentes |
| T7 | Iframe cross-site compartiendo realm | F2/F4: `SiteInstance` por frame cross-site |
| T8 | Falsificar mensajes IPC (`document_url`, hit-regions, `png_bytes`) | F1: sobre versionado + `Validated<T>` + caps + fuzz |
| T9 | Leer secretos del entorno del padre (`Command::new` sin `env_clear`) | F0: `env_clear` + whitelist |
| T10 | Panic de renderer | Ya mitigado (`catch_unwind`); conservar E2E |

La aceptación "0 fugas cross-site (T1/T2)" significa: tests automatizados que demuestran que un renderer de `site-a.test` **no puede** obtener ni escribir cookies de `bank.example`.
- Documentación que miente: ADR-004 dice "modo por defecto" pero `--ipc-render` es opt-in; los E2E de procesos están tras `#![cfg(feature = "a11y")]`; `PROGRESS.md` dice 14/16 targets de fuzz cuando son 28.
- Release: `release.yml` publica con `needs: [build, sbom]` (sin esperar CI) y el tarball incrusta mtimes (rompe determinismo).

---

## 4. Arquitectura y decisiones tomadas

### 4.1 WPT: shim JS sobre Boa, vendorizado por directorios completos

- **Own** el runner en `crates/test_harness/src/wpt/`; **adoptar** fixtures WPT con licencia BSD-3-Clause y pin por SHA de commit (nunca `master`).
- Reimplementar testharness.js en Rust sería caro y frágil. Boa ya ejecuta JS contra un `Document` prestado y devuelve un `String` (`JsRuntime::eval`). Escribir `megi-testharness.js` (~300–500 líneas) que implemente el subconjunto de API que usen los directorios adoptados y devuelva JSON.
- Anti-falsos positivos: golden de ~30 archivos con resultados upstream conocidos (incluidos FAIL); prohibido relajar aserciones.
- Denominador = **todos** los subtests descubiertos en los directorios adoptados, incluidos FAIL/TIMEOUT/NOTRUN/harness errors. Métrica `P/T` con SHA pegado; nunca "% de WPT".
- Allowlist monotónica por **época** `(WPT-REVISION.commit, sha256(MANIFEST.json))`; un fix elimina entradas, un fallo nuevo rompe CI. Rebaseline = PR etiquetado `wpt-rebaseline` con revisión humana.
- Comparación con Ladybird solo sobre **intersección de claves con el mismo SHA**; si no hay artefacto ajeno, "sin comparación disponible".
- Directorios iniciales por ROI: `css/css-syntax/` (3 → directorio completo), `dom/events/`, `html/webappapis/timers/` + `microtask-queuing/`, `dom/nodes/`, `css/css-selectors/`. Diferir flexbox/grid/fonts hasta que exista puente `getComputedStyle` robusto.
- Objetivos: 3 meses 1.700/2.500; 6 meses 6.000/9.000; 12 meses 17.000/25.000 (~0,8 % de Ladybird, publicado como tal).
- **Crecimiento del denominador `T` (aclaración)**: `T` se congela dentro de una época y **crece solo por trimestre**, añadiendo directorios completos en este orden (el plan confirma los nombres exactos contra el SHA importado):
  - **Época 1 (Q1)**: `css/css-syntax`, `dom/events`, `html/webappapis/timers`, `html/webappapis/microtask-queuing`, `dom/nodes`, `css/css-selectors` → ~2.500.
  - **Época 2 (Q2)**: `+ html/dom/documents`, `html/semantics/document-metadata`, `html/webappapis/atob`, `html/webappapis/scripting`, `css/css-cascade`, `css/css-values`, `dom/ranges` → ~9.000. Requiere el puente `getComputedStyle` limitado a propiedades ya soportadas.
  - **Época 3 (Q3–Q4)**: `+ css/css-flexbox`, `css/css-grid`, `css/css-fonts`, `html/semantics/forms`, `html/webappapis/fetch` (con servidor local) → ~25.000.
  - Cada salto es un PR `wpt-rebaseline` con revisión humana; el denominador **nunca se reduce**. Añadir un directorio dispara una época nueva aunque el SHA de WPT no cambie.

### 4.2 Motor: mantener Boa, cerrar profundidad y el event loop

- **Seguir con Boa 0.22, sin JIT.** Invertir en corpus de scripts reales + corrección de bindings + subconjunto de Test262 como gate. Cláusula de salida única: si >10 % de fallos del corpus son semántica del motor (no bindings), migrar una vez a `rquickjs` (capa de bindings reutilizable, ~20–30 días, asumiendo toolchain C y rompiendo "Rust puro").
- **Event loop persistente por pestaña** en `render` + `browser`, con cola propia; retirar `--refresh-ms` como único mecanismo.
- **Formularios completos**: `multipart/form-data`, `FormData` real y evento `submit`.
- **EventSource/SSE** reutilizando el cliente WebSocket (3–5 días, rentabilidad altísima).
- **Bidi/CJK**: adoptar `unicode-bidi` y `unicode-linebreak` (Rust puro); mantener rustybuzz. El problema no es el shaping.
- **Canvas 2D útil** compartiendo primitivas con `crates/raster`.
- **WASM**: `wasmi` (intérprete Rust puro), no `wasmtime`, no propio. Después del event loop y del corpus JS. Sin WASI/SIMD/threads.
- **Audio** tardío con `symphonia`+`rodio`; vídeo fuera.
- Mantener `crates/raster` + `crates/paint`; `skia-safe` y `wgpu` quedan prohibidos por política.

### 4.3 Seguridad: cerrar primero los explotos lógicos, después sandbox

Orden obligatorio: **F0 (lógicos) → F1 (IPC) → F2 (sandbox Linux) → F3 (network process) → F4 (image/iframe/plataformas)**. No al revés.

- F0: jar de cookies **por sitio**, nunca sembrar HttpOnly de otros sitios; **eliminar** `cookies` del `IpcRenderResponse` y validar dominio en el padre; `env_clear()`, `current_dir`, `process_group`, watchdog con deadline y RSS cap; `FileGrantTable` con canonicalize y symlink-safe. Enmendar ADR-004 (modo opt-in) y el gate del feature en E2E.
- **Semántica de "sitio" (decisión tomada, no dejar al constructor)**: **registrable domain (eTLD+1) con la PSL ya vendorizada** en `crates/net/data/public_suffix_list.dat` (MPL-2.0, snapshot real, `tools/update_psl.sh` como única vía de actualización). No origen estricto (rompería subdominios legítimos) y no `SameSite` como frontera de proceso (es una política de envío de cookies, no una clave de aislamiento). Reglas concretas:
  - `SiteKey` para **renderer/proceso/token IPC** = `scheme + eTLD+1 + puerto` (heredando la política actual `can_share(a,b) = a==b`).
  - La partición del **jar de cookies** usa la misma eTLD+1, con `SameSite=Strict|Lax|None` aplicado **dentro** del jar por el network process (F3); `HttpOnly` nunca cruza al renderer.
  - `file:` y `data:` siguen siendo orígenes opacos, pero **ya no se agrupan todos juntos**: `file:` se particiona por directorio canonicalizado (FileGrantTable) y `data:` por documento heredero; se elimina el `Opaque("file")` global actual.
  - Esta decisión reutiliza la PSL existente: **no añade dependencia nueva ni dato nuevo**. El plan debe incluir tests con `foo.example.co.uk` vs `bar.example.co.uk` (misma eTLD+1, distinto host) y con `example.com` vs `example.org`.
- F1: sobre IPC versionado (`magic|version|kind|request_id|site_token|len|payload`), `Validated<T>` con `TryFrom<WireMessage>`, `deny_unknown_fields`, caps semánticos; 28 → 31 targets de fuzz (nuevos `ipc_wire`, `ipc_response`, `net_file_url`).
- F2: crate `megi-sandbox` solo con APIs seguras: Landlock + `no_new_privs` + seccomp + `PR_SET_DUMPABLE(0)`; fallback `bwrap`. **`forbid(unsafe_code)` se mantiene en los 17 crates**; los unsafe de dependencias se listan con `cargo-geiger` y `deny.toml`, nunca se niega su existencia.
- F3: `network-<site>` como RequestServer; el renderer no tiene sockets ni cookies; token de sitio validado en cada fetch.
- F4: image-decoder process, `SiteInstance` por frame cross-site, perfiles macOS/Windows con degradación **explícita** (`enforced | best-effort | unsupported`).
- **Alcance de sandbox por plataforma (decisión tomada)**: en los 12 meses se implementa sandbox real **solo en Linux** (F2: Landlock + `no_new_privs` + seccomp; fallback `bwrap`). En **macOS y Windows se reporta `unsupported`** en la matriz `docs/status/security.html`, sin implementar `sandbox-exec`/AppContainer: hacerlo exigiría FFI/`unsafe` o toolchain nativo, y chocaría con `forbid(unsafe_code)` y "Rust puro". La degradación es **visible en la UI** (welcome page) y en `--json`, no silenciosa. La matriz distingue tres estados por plataforma y se genera con un check de CI; publicar `unsupported` es una decisión honesta, no un fallo del plan.
- Marketing defendible: "0 unsafe en los 17 crates propios (verificado en CI), frontera IPC versionada y fuzzed, sandbox progresivo por plataforma". **No** decir "más seguro que Ladybird" hasta F2–F3 medidos.

### 4.4 Release engineering: arreglar lo que existe antes de añadir formatos

1. **P0 (semana 1–2)**: `verify` job en `release.yml` con `test_harness --summary` + tests release; `needs: [build, sbom, verify]`; `SOURCE_DATE_EPOCH` + `tar --sort=name --mtime --owner=0` + `gzip -n`; cosign keyless sobre `SHA256SUMS` + `actions/attest-build-provenance`; documentar `sha256sum -c` / `cosign verify-blob` / `gh attestation verify`.
2. **P1 (semanas 3–6)**: canales `continuous` (diario, 03:17 UTC) y `beta` (domingo), sin auto-update en alpha; `docs/CHANNELS.md`; job `artifact-smoke` que descarga el tarball y ejecuta `--version` + `--json`.
3. **P2/P3 (meses 2–4)**: AppImage x86-64; `.deb`/`.rpm` como artefactos adjuntos, **sin repos firmados**.
4. **P4/P5 (meses 6–12, solo con tracción)**: Flatpak/Flathub y DMG notarizado/Windows firmado. Regla de decisión: si a 3 meses hay <100 descargas/mes, no hacerlos.
5. Producto: crash report con consentimiento (default OFF, sin URL ni contenido), "reportar sitio roto" desde la UI (reutilizando `render --json` y `real_sites.toml`), welcome page `about:megi`, release notes con `alpha-blocker` reales.
   - **Destino de los informes (decisión tomada, sin backend propio en 12 meses)**: el botón "Revisar y enviar" abre un **borrador de GitHub Issue** pre-rellenado (`https://github.com/juanpimg/Megi/issues/new?template=crash_report.yml&…` / `broken_site.yml&…`) para que el usuario lo envíe; el crash se guarda primero en `~/.local/state/megi/crashes/<timestamp>.txt`. No hay endpoint propio, ni retención, ni dashboards. El welcome page y los release notes se sirven desde archivos locales del binario (`about:megi` generado por el propio motor, dogfooding). métrica derivada: `crash-free sessions` solo sobre consentimiento, publicando siempre el denominador y la tasa de consentimiento.
6. Organización: `docs/GOVERNANCE.md` (BDFL + lazy consensus), CODEOWNERS con owners vivos, labels declarativos, `tools/git_hygiene.sh` semanal, SLA de triaje 72 h, sponsors/Ko-fi. ~10–15 h/semana nuevas; 100–300 USD/año obligatorios.

---

## 5. Secuencia (doce meses)

Las fases se ejecutan en paralelo donde no comparten archivos; el orden dentro de cada eje es obligatorio. **Regla de parada: no pasar al siguiente paso del eje si el anterior no está verde.**

### Q1 (meses 1–3) — Medir, asegurar y cerrar lo que está a medias
1. Fase 0 de seguridad (T1/T2/T3/T5/T9) y corrección de documentación mentirosa.
2. Fase 0 de WPT: `vendor_wpt.py`, `WPT-REVISION.json`, `MANIFEST.json`, shim, job `wpt` en CI, allowlist vacía.
3. P0/P1 de release: `verify` job, archivado reproducible, cosign, canales.
4. Event loop persistente + corpus JS (triage de `example.com/s.js` y Wikipedia) + formularios + EventSource.
5. **Aceptación**: gate PASS + ≥500 subtests WPT ejecutados; 0 fugas cross-site; watchdog <6 s; `real-sites` sin `console_errors` en example.com y HN; POST a servidor loopback devuelve 200.

### Q2 (meses 4–6) — Profundidad visible
1. WPT Fase 2 (6.000/9.000), allowlist época 1 reducida ≥40 %.
2. Bidi/CJK, transiciones/animaciones reales sobre reloj virtual, Canvas 2D útil.
3. Sandbox Linux (F2) con 12 pruebas negativas.
4. **Aceptación**: solape Vector (#1013) cerrado; warning de fuentes eliminado; fixtures `bidi_cjk`/`canvas_paths`/`animation_real`/`sse` con md5 estable; `cargo geiger` 0 unsafe en `megi-*`.

### Q3 (meses 7–9) — Componentes y persistencia
1. Shadow DOM encapsulamiento + slots + `:host`/`::part` + customElements.
2. Service Workers fase 1 (CacheStorage + intercepción, sin push).
3. Network process (F3) con token de sitio; E2E site-a no puede pedir site-b.
4. WASM mínimo con `wasmi` si Q1–Q2 van holgados.
5. **Aceptación**: ≥100 subtests WPT de shadow-dom/custom-elements; fixture `sw_offline` funciona con red cortada; `network_broker_tests` pasa.

### Q4 (meses 10–12) — "Se parece a un navegador" y ventaja hardening
1. Accesibilidad (foco real, `label[for]`, ARIA live) con AccessKit.
2. Interacción: portapapeles, diálogos, descargas, widgets nativos.
3. Replay 20/20, huella (binario ≤40 MB, RSS ≤100 MB, p95 ≤250 ms en render frío), matrix de sandbox.
4. AppImage + .deb/.rpm; Flatpak/macOS solo si hay tracción medida.
5. **Aceptación final**: G1–G5 de la tabla de metas verdes.

**Coste total estimado**: ~320–400 días-persona de trabajo de motor + ~200 h/año de capacidad de entrega. Con 1 persona a tiempo completo es un plan de 12 meses **ajustado**; cualquier desviación en Q1 debe desplazar Q3, nunca Q2 (lo que el usuario ve).

---

## 6. Riesgos y mitigaciones

| Riesgo | Mitigación |
|---|---|
| El shim WPT de falsos positivos | golden de 30 archivos con resultados upstream (incluye FAIL); tests unitarios del shim; diff de baseline |
| Boa insuficiente | gate en Q1: si >10 % de fallos son semántica del motor, migración única a rquickjs |
| 75/75 como falsa compatibilidad | WPT `P/T` + 4/4 sitios como north star; `coverage.toml` pasa a inventario, no KPI; badge WPT independiente |
| Determinar "rápido" sin medir | publicar p50/p95 y RSS sobre corpus declarado; no reclamar ventaja hasta conseguirlo |
| Regresiones de cookies al separar el jar | decisión de spec (PSL/SameSite) + tests; no solo mover código |
| Latencia IPC | `load_many` por lote y keep-alive; medir con `crates/bench` antes/después |
| Fragmentación de sandbox | publicar `enforced/best-effort/unsupported`, nunca fingir paridad |
| Burnout del mantenedor | los agentes preparan PRs, el humano revisa; límite de 10 WPs activos; rebaseline trimestral |
| Ladybird sigue avanzando | no perseguir paridad general; defender los cinco ejes medibles |

---

## 7. Lo que queda al constructor del plan

Este documento decide **qué** y **en qué orden**. El plan (`writing-plans`) debe decidir los **cómo** ejecutables: nombres de funciones, firmas, rutas exactas, tests concretos y comandos por tarea. En particular:

- El diseño fija los directorios WPT iniciales pero no los archivos exactos del SHA (se confirman al importar).
- El diseño fija "sandbox con APIs seguras" pero no elige entre `landlock` directo y `bwrap` como mecanismo primario; el plan debe fijarlo con un spike.
- El diseño fija canales de release pero no la sintaxis final de `continuous.yml`; el plan la escribe.
- El diseño fija la cláusula de salida de Boa pero no el Corpus JS concreto; el plan propone los fixtures.
- Todo lo demás (métricas, anti-objetivos, orden de fases, criterios de aceptación) está decidido aquí y no debe reabrirse sin aprobación.

### 7.1 Respuestas al builder check (incorporadas, no pendientes)

| # | Pregunta del revisor | Respuesta ahora en el documento |
|---|---|---|
| 1 | ¿Qué son T1/T2/T3/T5/T9? | Catálogo T1–T10 en §3, con amenaza y fase de bloqueo |
| 2 | ¿Crece `T` por trimestre? | §4.1: épocas 1/2/3 con directorios concretos y denominador nunca decreciente |
| 3 | ¿Semántica de "sitio" (PSL/SameSite/origen)? | §4.3: eTLD+1 con la PSL ya vendorizada; SameSite dentro del jar; opacos por directorio/documento |
| 4 | ¿Destino de crash y reportes? | §4.4: borrador de GitHub Issue + archivo local; sin backend propio |
| 5 | ¿Sandbox real en macOS/Windows? | §4.3: solo Linux en 12 meses; macOS/Windows reportan `unsupported` de forma visible |
