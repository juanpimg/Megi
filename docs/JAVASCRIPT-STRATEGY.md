# ADR-001 — Estrategia JavaScript para la Fase 6 (`docs/JAVASCRIPT-STRATEGY.md`)

- **Estado**: **ACEPTADO (ADR-001) — decisión del dueño registrada el 2026-11-10; implementación en curso (`WP-6.1`)**
- **Fecha**: 2026-11-10 (Europe/Madrid)
- **Decisor**: dueño del proyecto · **Implementación**: Fase 6, plan WP-6.1–6.7
- **Rama / HEAD**: `sprint/robustez-fase4` @ `c4cc170`
- **Ámbito**: motor JavaScript de la Fase 6, bindings DOM↔JS, ejecución de `<script>`/`document.write` y event loop mínimo.
- **No es**: una implementación; este documento no contiene código ni cambia `Cargo.toml`.
- **Documentos relacionados**: [`ARCHITECTURE.md`](ARCHITECTURE.md) §2.1 (arena DOM e identidad), §3.1 (política de dependencias); [`ROADMAP.md`](ROADMAP.md) §«FASE 6»; [`HANDOFF.md`](HANDOFF.md); [`FUZZING.md`](FUZZING.md); [`SECURITY.md`](../SECURITY.md).

---

## 1. Contexto

Megi ha cerrado funcionalmente la Fase 4 (renderizado) y tiene un pipeline de navegador real en la Fase 5 (`crates/render`: URL → net → HTML → CSS externo → layout → imagen `<img>` → PNG). La **Fase 6 (JS) es la última decisión estratégica grande**: ejecutar scripts de terceros dentro de un motor de navegador propio implica elegir un motor ECMAScript, definir una frontera DOM↔JS segura y mantener los invariantes del proyecto.

### 1.1 Qué necesita Megi de JavaScript (alcance mínimo verificable)

1. **`document.write` durante el parseo** para cerrar los **4 suites `scripted_*`** (6 mode-runs: `scripted_adoption01.dat` 1, `scripted_ark.dat` 1, `scripted_foster01.dat` 2, `scripted_webkit01.dat` 2). **Resuelto en la ronda 11 con WP-6.4**: el parser ejecuta los scripts parser-inserted y re-tokeniza `document.write`; el gate pasa con **3.904/3.904 (100 %)** y la allowlist de tree está vacía.
2. **DOM bindings**: exponer a JS el árbol DOM que ya existe (`Document`, `Element`, `Text`, atributos, `innerHTML`/`textContent`) con wrappers estables y GC-friendly.
3. **Modelo de ejecución**: scripts *parser-inserted* síncronos (los que importan para `document.write`), `defer`/`async` y microtareas/`Promise` con un **event loop mínimo** (macrotareas: timers y eventos de usuario de la Fase 7).
4. **Aislamiento y límites**: JS de terceros no puede bloquear ni tumbar el navegador (presupuesto de tiempo/instrucciones, memoria, pila), ni acceder por defecto a red/FS.
5. **EventTarget mínimo** (`addEventListener`/`removeEventListener`/`dispatchEvent`) para enlazar con la ventana y el DOM.

### 1.2 Estado actual del stack DOM (lo que ya existe)

- **Arena contigua** (`crates/dom`): `NodeId` de 4 bytes con **20 bits de slot + 12 de generación**, *free list* LIFO con reutilización, `remove_node`/`remove_subtree`, rechazo de handles *stale*/foráneos (`try_node`/`owns`), `validate_invariants()`, `estimate_bytes()` y presupuesto anti-DoS ya en el parser (`ParseOptions`).
- **API de navegador**: `document_element`/`head`/`body`, `get_element_by_id`, `get_elements_by_tag_name`/`class_name`, factorías, atributos, `text_content`, manipulación de hijos y `DomError`; en `crates/html`: `parse_fragment_into`, `set_inner_html`, `set_outer_html` y `document_write`.
- **Esto es exactamente lo que un binding necesita**: el ADR no requiere rediseñar el DOM, sino construir una **capa lateral** entre `NodeId` y los valores JS.

### 1.3 Por qué es una decisión estratégica

- **Esfuerzo**: un motor ECMAScript completo es un proyecto de años; el objetivo es integrar uno existente, no reimplementarlo.
- **Licencias**: Megi se distribuye como `MIT OR Apache-2.0`; el motor elegido debe ser compatible y declarado.
- **Tamaño y compilación**: afecta al build por defecto, al MSRV (1.88) y al CI (el proyecto exige `cargo +1.88 check --workspace --all-targets`).
- **Política del repo** ([`ARCHITECTURE.md`](ARCHITECTURE.md) §3): prohibido delegar el **núcleo** (HTML/CSS/layout/pintado) a motores de terceros; un motor JS es un **componente** distinto y puede ser delegado, pero la decisión debe ser explícita.
- **Seguridad**: ejecutar JS no confiable es la mayor superficie de ataque del navegador; requiere frontera, presupuestos y fuzzing desde el primer WP.
- **Compatibilidad**: el objetivo no es pasar Test262 completo, sino renderizar páginas reales y cerrar los tests `scripted_*` del corpus; hay que decidir cuánto ES/DOM se exige.

**Estado real verificado en la decisión (ronda 7)**: **602 tests, 0 fallos**; gate de 4 suites PASS (tokenizer 7.050/7.050, árbol 3.898/3.904, serializador 230/230, CSS 30/30); Fase 6 con **decisión ACEPTADA el 2026-11-10**, implementación arrancando por `WP-6.1`.

**Estado actual (`main`, post-v0.1.0)**: **2.383 tests, 0 fallos**; gate PASS; **175+ tests js**; **Boa 0.22** embebido con **TLA real y live bindings** (`eval_module_async` con `ModuleLoader`, await Promise/fetch/timer, orden topológico, ciclos nativos), **`fetch()` + `AbortController`/`AbortSignal`**, **Web Workers cooperativos deterministas** (`with_document_realm(..., is_worker)` con realm aislado sin DOM), **`localStorage`/`sessionStorage`** (orden de inserción + WebIDL index/boolean conformance), `MutationObserver` y reutilización acotada de slots de listeners (`MAX_EVENT_LISTENERS = 16_384`). Todos los crates con `#![forbid(unsafe_code)]`; MSRV Rust 1.88+.

---

## 2. Fuerzas y criterios de decisión

| Criterio | Peso | Comentario |
|---|:---:|---|
| Encaje con la filosofía «sin forks / sin delegar el núcleo» | Alto | JS es componente, no núcleo; el motor no puede copiar código de Servo/Ladybird/Blitz, sólo usar crates/componentes externos permitidos. |
| Licencia compatible con `MIT OR Apache-2.0` | Alto | Debe permitir redistribución bajo esas licencias y declararse en `ARCHITECTURE.md` §3.1. |
| Seguridad de ejecución y ausencia de `unsafe` en *nuestro* código | Alto | Los crates del motor están `#![forbid(unsafe_code)]`; un motor con FFI C queda encapsulado en una dependencia, no en nuestro código. |
| MSRV 1.88 y CI existente | Medio-alto | No romper el job MSRV; si un motor exige >1.88, hay que subir la MSRV del workspace de forma explícita o fijar una versión anterior. |
| Cobertura ECMAScript y madurez | Medio | Para páginas reales hace falta ES2015+ básico; para `scripted_*` basta ES5 + `document.write`. |
| Rendimiento | Medio | El cuello de botella actual es el render; JS debe ser «suficiente» al principio. |
| Tamaño de build y tiempo de compilación | Medio | **Decisión**: el build por defecto incluye JS (feature `js` ON); se mantiene un job `--no-default-features` para el build mínimo. |
| Determinismo/CI offline | Medio | Tests y fuzz sin red; el motor no debe descargar binarios en tiempo de test. |

---

## 3. Opciones evaluadas

### Opción A — Escribir un intérprete/VM JS propio

- **Qué implica**: lexer/parser (ECMAScript es un lenguaje enorme), bytecode/árbol, objetos/prototipos, GC, *built-ins*, `Promise`/microtareas, semántica de *sloppy/strict*, Annex B… Un subconjunto ES5 útil ya son meses-persona; compatibilidad real, años.
- **Encaje**: máxima independencia, pero **no es viable a corto plazo** y distraería del objetivo (renderizar páginas reales).
- **Riesgo**: cobertura insuficiente y mantenimiento permanente del lenguaje.
- **Veredicto**: **descartada** para Fase 6; podría reconsiderarse sólo si el proyecto decidiera que la independencia total del lenguaje también es un objetivo de producto.

### Opción B — Embeber un motor JS mantenido

#### B1. **Boa** (pure Rust) — `boa_engine`

- Repo: <https://github.com/boa-dev/boa> · crate: <https://crates.io/crates/boa_engine> · docs: <https://docs.rs/boa_engine>
- **Versión evaluada**: `0.21.1` — **MSRV Rust 1.88.0**, exactamente el MSRV de Megi. (`0.22.0` ya exige Rust 1.91.0: si se sube la MSRV, se puede actualizar.)
- **Licencia**: **Unlicense OR MIT** (`LICENSE-MIT` + `LICENSE-UNLICENSE` en el repo; crates.io lo declara `Unlicense OR MIT`). Compatible con la doble licencia de Megi eligiendo MIT, pero la opción Unlicense debe declararse en la política §3.1.
- **Madurez/cobertura**: motor *experimental* pero activo, **>90 % de la última spec ECMAScript** según su README; conformance pública en <https://boajs.dev/conformance>; ast/engine/string/gc separados (`boa_ast`, `boa_engine`, `boa_string`, `boa_gc`).
- **Tamaño/compilación**: pure Rust, sin toolchain C/C++ ni binarios precompilados; añade árbol de crates (boa_gc, icu, etc.) y tiempo de compilación, pero es **reproducible offline** una vez en el registry y no requiere *link-time* de librerías del sistema.
- **`unsafe`**: el crate usa `unsafe` internamente; Megi encapsula Boa en `crates/js` con `#![forbid(unsafe_code)]` en nuestro código. La frontera es *safe Rust*.
- **Event loop**: Boa expone cola de *jobs* para promesas/microtareas (`JobQueue`/`run_jobs`); las macrotareas (timers, eventos) las aporta Megi.
- **Encaje**: el mejor equilibrio para el objetivo; sin FFI C, licencia laxa, MSRV exacta y pure Rust.

#### B2. **QuickJS** vía `rquickjs` (C con FFI)

- QuickJS: <https://bellard.org/quickjs/> · repo oficial <https://github.com/bellard/quickjs> · `rquickjs`: <https://github.com/DelSkayn/rquickjs>, crate <https://crates.io/crates/rquickjs>.
- **Versión evaluada**: `rquickjs 0.14.0` — MSRV Rust 1.87 (compatible), **licencia MIT**; QuickJS: **MIT**, fuente pequeña (~620 KB el tarball `quickjs-2026-06-04.tar.xz` según Content-Length del sitio oficial), **casi 100 % de Test262** según bellard.org.
- **Coste**: requiere compilar C (cc) con el `build.rs` de `rquickjs`/`quickjs-sys`, herramienta C en todas las plataformas y **FFI `unsafe`** en la frontera. El binding `rquickjs` sigue siendo un crate seguro para nuestro código, pero el build deja de ser «Rust puro» y el MSRV/CI se complica (compilador C, target Windows).
- **Encaje**: alternativa fuerte si la cobertura ES de Boa no alcanza; mejor compatibilidad y tamaño de runtime, peor reproducibilidad de build.
- **Veredicto**: **segunda opción / plan de migración**.

### Opción C — Motor del sistema / navegador (V8, SpiderMonkey)

- **V8** vía `deno_core` (<https://github.com/denoland/deno>, crate <https://crates.io/crates/deno_core>, versión evaluada `0.413.0`, licencia del wrapper MIT; V8 con licencia BSD-style): máxima compatibilidad y rendimiento, pero integra un runtime C++ enorme, binarios precompilados (`rusty_v8`), y una superficie de integración (isolates, platform, snapshots) muy grande. Inaceptable para el build/CI actual.
- **SpiderMonkey** vía `mozjs` (<https://github.com/servo/mozjs>, crate `mozjs 0.26.14`, **MPL-2.0**; sigue `mozilla-esr153`): es lo que usa Servo; requiere compilar o enlazar un SpiderMonkey enorme (el README de `mozjs` describe el archivo precompilado y lo lento que es compilarlo). La licencia MPL-2.0 es compatible en uso como dependencia, pero obliga a revisar distribución de binarios modificados.
- **Veredicto**: **descartada** para Fase 6 (escala desproporcionada); sería una decisión de producto distinta («navegador de máxima compatibilidad»).

### Opción D — Diferir JS con un *stub* de `document.write` en el parser

- Implementar únicamente un «script stub» que reconozca `document.write(...)` en los 4 suites `scripted_*` sin ejecutar JS general.
- **Coste**: días, no semanas; desbloquea **4 suites / 6 mode-runs** del gate.
- **Problema**: no aporta navegador real (ninguna página con JS funcionaría), y crea una falsa sensación de conformidad. El gate quedaría 3.904/3.904 sólo si el stub cubre exactamente los casos del corpus, lo que es un riesgo de sobreajuste.
- **Veredicto**: **no recomendada**, salvo como *fallback* temporal si el dueño decide posponer Fase 6; en tal caso debe documentarse como «stub de test, no JS real» y no contarse como Fase 6.

### 3.1 Tabla comparativa

| Criterio | A: propio | **B1: Boa** | B2: QuickJS (`rquickjs`) | C: V8 / SpiderMonkey | D: stub |
|---|---|---|---|---|---|
| Esfuerzo de integración (hasta `document.write` + DOM básico) | Meses-años | **≈3–4 semanas** | ≈3–5 semanas (C build) | ≈2–4 meses | **2–4 días** |
| Cobertura ECMAScript | Subconjunto | **>90 % spec actual** (README Boa) | **≈100 % Test262** (bellard.org) | ~100 % | 0 % (sólo stub) |
| Licencia | MIT/Apache propia | **Unlicense OR MIT** | **MIT** (QuickJS + rquickjs) | V8 BSD / mozjs MPL-2.0 | MIT/Apache |
| `unsafe` en *nuestro* código | Ninguno | **Ninguno** (safe API) | Ninguno en nuestro código; FFI unsafe encapsulado | Ninguno en nuestro código; FFI/`unsafe` en crates | Ninguno |
| Compilación / tamaño | std puro | **Pure Rust, sin C**; árbol de crates medio-grande | Requiere `cc` + C; runtime pequeño | C++/prebuilt, enorme | std puro |
| MSRV 1.88 | — | **Boa 0.21.1 = 1.88** (0.22 pide 1.91) | rquickjs 0.14 = 1.87 | variable | — |
| Madurez | 0 | Experimental, activo | Maduro (QuickJS), bindings activos | Muy maduro | N/A |
| Rendimiento | — | Medio (mejora continua) | **Alto** (QuickJS) | **Máximo** (V8/SpiderMonkey) | N/A |
| Encaje con la política del repo | Total | **Alto** (componente externo, núcleo propio) | Alto (añade C al build) | Bajo (runtime gigante) | No (no es navegador) |
| Determinismo/CI offline | Total | **Alto** | Medio (toolchain C) | Bajo (descarga/compila V8) | Total |

---

## 4. Decisión aceptada por el dueño (2026-11-10)

La opción recomendada en este ADR fue **aprobada literalmente** por el dueño. Queda registrado:

1. **Motor = Boa `boa_engine` 0.21.x** (pure Rust, MSRV Rust 1.88, licencia Unlicense OR MIT; sin C/FFI). El crate se integrará en un nuevo `crates/js`.
2. **Feature `js` ENCENDIDA POR DEFECTO** (el dueño eligió ON, no la recomendación inicial de OFF). Construcción y CI deben compilar **con JS por defecto**, y se mantiene un job `--no-default-features` para el **build mínimo** (std-only, sin Boa).
3. **Alcance inicial = COMPLETO**: `document.write` + bindings DOM + eventos + timers + CSSOM + `querySelector` (plan WP-6.1–6.7 ya definido, **≈25–30 días-persona**). No se limita al mínimo de `scripted_*`.

Fundamento (contexto de la recomendación original):

- **Encaje con la filosofía**: Megi sigue implementando desde cero el núcleo del navegador (DOM/HTML/CSS/layout/paint/render/shell). El motor ECMAScript es un **componente de lenguaje**, no el núcleo; usar un motor existente evita años de trabajo y es coherente con §3.1 (componentes de bajo nivel).
- **Pure Rust y `#![forbid(unsafe_code)]`**: sin toolchain C ni FFI en nuestro código.
- **Licencia**: Unlicense OR MIT, compatible con `MIT OR Apache-2.0` eligiendo MIT; declarada en §3.1.
- **MSRV**: `boa_engine 0.21.1` declara **Rust 1.88.0**, exactamente el MSRV del workspace; fijar `0.21.x` hasta que se decida subir la MSRV (0.22 exige 1.91).
- **Cobertura**: >90 % de la spec actual, suficiente para el alcance completo y las páginas reales; si resultara insuficiente, los bindings permiten migrar a QuickJS (§9).

**Consecuencias inmediatas de la decisión**:

- `Cargo.toml` del workspace/`crates/js`: Boa 0.21.x como dependencia **no opcional por defecto** (feature `js` default); MSRV/CI siguen en 1.88, pero el build por defecto crece y compila Boa.
- CI: el job `quality` compila/tests **con JS por defecto**; se añade un job **`minimal`** (`cargo test --workspace --no-default-features`) que garantiza el build std-only; el job MSRV cubre ambas configuraciones.
- `crates/render`/`crates/shell` habilitan la ejecución JS por defecto y deben poder deshabilitarla con `--no-default-features`.
- Fase 6 pasa a **EN IMPLEMENTACIÓN**; WP-6.1–6.4 y WP-A/WP-B ya HECHOS (árbol 3.904/3.904 sin allowlist), pendientes módulos ES, memoria real y contabilidad fina.

### 4.1 Reglas de integración propuestas

- **Crate nuevo `crates/js`** con `#![forbid(unsafe_code)]` y `boa_engine` 0.21.x como dependencia de la feature `js` (default ON); `render`/`shell` la consumen con la feature y ofrecen build mínimo sin ella.
- **Feature `js` encendida por defecto** (decisión): `cargo test --workspace` y el gate compilan con Boa; CI añade un job **`--no-default-features`** que verifica el build mínimo std-only. El job MSRV se ejecuta en ambas configuraciones (`default` y `--no-default-features`).
- **Registrar en `ARCHITECTURE.md` §3.1** una fila nueva: *Motor JS (Fase 6): `boa_engine` (Unlicense OR MIT, pure Rust)*; y actualizar la lista de lo que Megi implementa desde cero (bindings, event loop, política de ejecución y sandbox lógica).
- **Sin snapshots ni binarios precompilados**; todo compilado desde el registry, reproducible en CI offline tras el primer `cargo fetch`.
- **Revisión de `unsafe`**: Boa usa `unsafe` internamente; nuestro crate se mantiene libre de `unsafe` y la frontera es la API safe de `boa_engine`. Documentarlo en `SECURITY.md`.
- **Determinismo**: el motor no debe introducir `Date`/`Math.random` en golden tests; los tests de bindings usan scripts deterministas y el render con JS queda fuera de los goldens por ahora.

### 4.2 Riesgos y mitigaciones

| Riesgo | Prob. | Mitigación |
|---|:---:|---|
| Cobertura ES de Boa insuficiente para páginas reales | Media | Empezar con bindings; medir con un corpus de scripts; plan de migración a QuickJS (§9) |
| Incremento de tiempo de compilación/binary size con `js` (ahora default) | Alta | Job CI `--no-default-features` para el build mínimo; medir tiempos en CI/bench; documentar el coste en el README |
| Licencia Unlicense no deseada | Baja | El dueño aceptó «Unlicense OR MIT»; se elige MIT explícitamente y se declara en §3.1 |
| MSRV futura de Boa (0.22 = 1.91) | Media | Fijar 0.21.x; subir MSRV sólo con decisión explícita del dueño |
| DoS por scripts maliciosos (bucles, memoria) | Alta | Interrupt handler con presupuesto de tiempo/instrucciones, límites de memoria y pila, sin red/FS por defecto; fuzzing de la capa de bindings |
| Incompatibilidad con `#![forbid(unsafe_code)]` del workspace | Baja | `forbid` aplica a nuestros crates; la dependencia externa se documenta y audita |
| Falsa conformidad por el *stub* | Media | Descartar la opción D; el gate sólo cuenta JS real ejecutado |

---

## 5. Diseño propuesto de bindings DOM↔JS (sin implementar)

### 5.1 Mundos/realm y tabla lateral

- Cada **realm** (mundo) de ejecución tiene su propio `Context` de Boa y una **tabla lateral `HashMap<NodeId, JsValue>`** (wrapper de objeto JS), tal y como anticipa [`ARCHITECTURE.md`](ARCHITECTURE.md) §2.1 («tabla lateral asociará `NodeId <-> JSWrapper` por mundo de ejecución»).
- Los wrappers se crean *lazy* al primer acceso desde JS y se cachean; el DOM no contiene estado JS (mantiene el desacoplamiento arena/JS).
- El **invariante de generación** se comprueba en cada *entry point*: si un `NodeId` fue eliminado y el slot reutilizado, el wrapper se invalida (`DomError::StaleHandle`-equivalente a nivel JS) y la propiedad lanza/retorna el error correspondiente; nunca se accede a un nodo ajeno.
- Los objetos JS mantienen un `NodeId` **con generación** (no un puntero), por lo que un wrapper huérfano no puede corromper la arena.

### 5.2 Superficie DOM inicial (WP-6.2)

- `Node`: `parentNode`, `firstChild`/`lastChild`/`nextSibling`/`previousSibling`, `childNodes` (live o snapshot, decisión documentada), `nodeType`, `nodeName`, `textContent` (get/set), `appendChild`/`insertBefore`/`removeChild`/`replaceChild`, `cloneNode`.
- `Document`: `documentElement`, `head`, `body`, `getElementById`, `getElementsByTagName`, `getElementsByClassName`, `createElement`, `createTextNode`.
- `Element`: `tagName`, `id`, `className`/`classList`, `getAttribute`/`setAttribute`/`removeAttribute`/`hasAttribute`, `innerHTML`/`outerHTML`, `children`/`childElementCount`.
- `Text`, `Comment`, `DocumentFragment` (mínimo), `EventTarget` mínimo.
- **Sin** `querySelector` inicialmente (requiere conectar `css`; disponible como WP-6.5), sin `fetch`/`XHR`, sin `localStorage`, sin `window.location` navegable (hasta Fase 7).
  - *Nota de estado actual*: `querySelector`/`querySelectorAll`, `fetch()` (con `AbortController`, `Headers`/`Request`/`Response` y `ReadableStream`), `localStorage`/`sessionStorage`, `window.location`/`history`, `MutationObserver`, `URL`/`URLSearchParams`, módulos ES con TLA y Web Workers cooperativos ya están implementados (ver §8).

### 5.3 Modelo de ejecución y `document.write`

- **Scripts parser-inserted**: al encontrar `<script>` en `crates/html`, el *tree builder* pausa, el `render`/`shell` llama al realm y, si el script ejecuta `document.write`, el HTML generado se inserta en el punto del parser usando `html::parse_fragment_into`/la API existente (respetando los contextos `table`/`select`/`template`/`rawtext` que ya implementa el parser). Al terminar, el parser continúa.
- `document.write` fuera de parseo reemplaza el documento (semántica de la spec) o se ignora con warning en el primer WP; decisión documentada.
- **Microtareas**: ejecutar la cola de *jobs* de Boa tras cada tarea; **macrotareas** (timers/eventos) las gestiona el event loop de Megi (`render`/`shell`), no el motor.
- **`defer`/`async`**: post-parse en orden de documento/cola; el primer WP puede limitarse a síncronos + `document.write`.

### 5.4 Frontera de seguridad (anti-DoS)

- **Interrupt handler** de Boa con presupuesto de tiempo/instrucciones por script y límite por página; al agotarse, `ScriptError::Interrupted` no fatal (frame blanco/estado de error como en `shell`).
- **Límites de memoria/pila** (arena y heap JS), `stack_size` explícito por `Context`, límite de objetos/strings creados por script.
- **Sin capacidades por defecto**: no `fetch`, no filesystem, no `eval` de módulos externos; `console` mínimo a stderr.
- **Fuzzing**: además de los 4 targets actuales, un target de la capa de bindings (entrada = script + DOM sintético) procesa el motor como caja negra; no se fuzzea el motor de Boa en nuestro repo (upstream lo hace) pero sí nuestra frontera y presupuestos.

---

## 6. Plan aceptado WP-6.x (alcance completo, ≈25–30 días)

Estimaciones en días de un agente full-time, con tests y revisión; dependencia en cadena salvo lo indicado. **Alcance completo decidido por el dueño** (no sólo el mínimo `scripted_*`): eventos, timers, CSSOM y `querySelector` incluidos.

| WP | Objetivo | Dep. | Días | Criterio de aceptación |
|---|---|---|---:|---|
| **WP-6.1** | Crate `crates/js` + feature `js` **default ON**; realm Boa (`Context`), `console` mínimo, presupuesto de instrucciones y errores tipados; CI con JS por defecto + job `--no-default-features` | — | 3 | `cargo test -p js`; `cargo test --workspace` (con JS) y `cargo test --workspace --no-default-features` (mínimo) verdes; MSRV 1.88 en ambas configuraciones |
| **WP-6.2** | Bindings DOM core: tabla lateral por realm, wrappers `Node`/`Document`/`Element`/`Text`, generación/stale, `DomError`→excepción JS | 6.1 | 6 | Micro-tests JS↔DOM (get/set `id`, atributos, `textContent`, hijos); wrappers invalidados tras `remove_node`; sin `unsafe` en `crates/js` |
| **WP-6.3** | Ejecución de `<script>` parser-inserted + `document.write`/`innerHTML` usando la API existente | 6.2 | 5 | **`cargo run -p test_harness -- --summary`**: suites `scripted_*` dejan de estar en la allowlist; árbol **3.904/3.904**; gate PASS con allowlist sólo de tokenizer |
| **WP-6.4** | `EventTarget` mínimo + timers/microtareas + integración con el event loop de `render`/`shell` | 6.2, 6.3 | 5 | `setTimeout`/`Promise` ordenados; tests de eventos sintéticos; un script con timer acaba en el pixel esperado |
| **WP-6.5** | `querySelector`/`querySelectorAll` (reutiliza el motor de selectores de `css`), `classList`/`style` (CSSOM básico) | 6.2 | 6 | Tests de selector JS; estilos aplicados tras mutación DOM (render cambia) |
| **WP-6.6** | Endurecimiento: presupuestos de tiempo/memoria, fuzz de bindings, aislamiento entre realms | 6.3–6.5 | 3 | Fuzz target de bindings en CI; script malicioso (bucle/alloc) interrumpido sin tumbar el proceso |
| **WP-6.7** | Integración con `shell` (ventana con JS) y documentación final de Fase 6 | 6.4–6.6 | 3 | `megi` ejecuta una página con JS en ventana y en `--screenshot`; docs + gate verdes |

**Total: 25–30 días-persona** (alcance completo aceptado: WP-6.1–6.7). Hito intermedio: `scripted_*` verdes al cerrar WP-6.3 (≈14 días desde el inicio). Boa 0.21.x; no aplica la variante QuickJS, que queda sólo como plan de migración.

**Estado a ronda 17**: `WP-6.1` **HECHO**; `WP-6.2` **HECHO en su parte host** (bindings core en `crates/js` + integración en `render`: scripts clásicos post-parseo con budgets/caps, microtasks/timers, console y re-render del DOM mutado; 7 fixtures `js_*`). `WP-6.3`/`WP-6.4` parser-inserted/`document.write` para `scripted_*` **HECHO en ronda 11** (árbol 3.904/3.904), eventos/timers (WP-A) **HECHO en ronda 10**, CSSOM/`querySelector` (WP-B) **HECHO en ronda 11** y event loop presupuestado en `render`. Eventos avanzados (ronda 12) **HECHOS** (`currentTarget`/`eventPhase`, `once`/`passive`, handlers inline, excepciones capturadas). Siguientes: módulos ES, memoria real, eventos `resize`/`scroll`/`change`, `WP-6.6` fuzz y `WP-6.7` shell.

### 6.1 Qué NO hacer en Fase 6

- No reimplementar un motor ECMAScript propio.
- No delegar en JS el parseo HTML/CSS, layout ni pintado: JS **muta el DOM**; el pipeline existente se re-ejecuta (o se invalida selectivamente) como hoy.
- No exponer red/filesystem por defecto; no `eval` de código remoto; no JIT.
- No prometer compatibilidad Test262 completa; el objetivo es páginas reales + `scripted_*` verdes.
- Boa va en el build por defecto (feature `js` **ON**) y el CI mantiene un job `--no-default-features` para el build mínimo std-only.

---

## 7. Anexo: estado del arte (datos verificados)

| Proyecto | Cómo integra JS | Datos verificados / enlaces |
|---|---|---|
| **Servo** | Embebe **SpiderMonkey** mediante el crate `mozjs` mantenido por el equipo de Servo (bindings `mozjs-sys` + `mozjs`; sigue `mozilla-esr153`/SpiderMonkey 153) | <https://github.com/servo/mozjs> · <https://github.com/servo/servo> · crate `mozjs` 0.26.14, **MPL-2.0** (crates.io). Megi no copia código de Servo; sólo se cita como referencia. |
| **Ladybird** | Motor **LibJS propio en C++** dentro del navegador (su árbol `Libraries/LibJS` incluye parser, runtime y realms; p. ej. `Libraries/LibJS/Runtime/Realm.cpp`) | <https://github.com/LadybirdBrowser/ladybird> (rutas `Libraries/LibJS/CMakeLists.txt` y `Libraries/LibJS/Runtime/Realm.cpp` verificadas HTTP 200). Megi **no** copia su código. |
| **Blitz** (DioxusLabs) | Renderizador HTML/CSS modular; su README indica que **aún no tiene bindings JS** («We don't yet have Blitz bindings for other languages (JavaScript, Python, etc.) but would accept contributions») | <https://github.com/DioxusLabs/blitz> (README verificado). No hay `packages/blitz-script` en `main` (404). Se cita como referencia de render; no se usa. |
| **Boa** | Motor ECMAScript pure Rust, >90 % de la spec actual, experimental y activo; usado por otros proyectos Rust como librería embebida | <https://github.com/boa-dev/boa>, <https://boajs.dev/conformance>, crate 0.21.1 MSRV 1.88 (crates.io). |
| **QuickJS** | Motor C pequeño y embebible, casi 100 % Test262, licencia MIT (~620 KB de tarball fuente) | <https://bellard.org/quickjs/>, <https://github.com/bellard/quickjs>; bindings `rquickjs` 0.14, MIT, MSRV 1.87. |
| **V8** | Runtime C++ de Chromium; integrable vía `deno_core` (wrapper MIT, V8 BSD) | <https://github.com/v8/v8>, <https://github.com/denoland/deno>, crate `deno_core` 0.413.0. |

**Nota de política**: ninguna opción copia código de Servo, Ladybird ni Blitz. Se usan componentes externos permitidos (Boa o QuickJS como motores de lenguaje) o desarrollo propio (bindings, event loop, DOM), manteniendo el núcleo del navegador implementado en Megi.

---

## 8. DECISIÓN ACEPTADA (2026-11-10)

**Registrado literalmente de la decisión del dueño:**

1. **Motor**: **Boa `boa_engine` 0.21.x** (se acepta «Unlicense OR MIT» en la política §3.1; se elige MIT). Descartadas para Fase 6: propio, QuickJS/rquickjs (queda como plan de migración), V8/SpiderMonkey y el stub.
2. **Activación**: feature `js` **ENCENDIDA POR DEFECTO** en el workspace y en `render`/`shell`. El build y el CI por defecto compilan con JS; se mantiene un job **`cargo test --workspace --no-default-features`** para el build mínimo std-only, y el job MSRV cubre ambas configuraciones.
3. **Alcance inicial**: **COMPLETO** — `document.write` + bindings DOM + `EventTarget`/eventos + timers + CSSOM + `querySelector` (WP-6.1–6.7, **≈25–30 días-persona**).

**Estado de implementación actual**: **Fase 6 funcionalmente completa (~98 %)**. `crates/js` existe desde WP-6.1 (Boa, **178+ tests**) y **`render` ya ejecuta los `<script>`** clásicos y `<script type=module>` (con **TLA real y live bindings**, import maps y dynamic `import()`) con **event loop presupuestado** (reloj virtual, `DOMContentLoaded`/`load`, timers, re-render único). **WP-A HECHO**: `EventTarget` con captura+burbuja, `Event`/`CustomEvent`, timers con reloj inyectable y `document` como target. **WP-B HECHO**: `CSSStyleDeclaration`, `getComputedStyle` con provider, `NodeList`, `matches`/`closest`, `insertAdjacent*`, identidad de wrappers. **WP-6.4 HECHO**: el parser ejecuta scripts parser-inserted y re-tokeniza `document.write`, de modo que las 4 suites `scripted_*` salen de la allowlist y el **árbol llega a 3.904/3.904 (100 %)**. Se han añadido también **eventos avanzados**, **APIs de navegador** (`matchMedia`, viewport/DPR/scroll, rAF, `window.onerror`/`document.onerror`, `readyState`, consola), **geometría de layout** (`getBoundingClientRect`/offset/client), `URL`/`URLSearchParams`, `History`/`Web Storage`, `MutationObserver`, `fetch()` con `AbortController`/`ReadableStream` y **Web Workers cooperativos**. Quedan pendientes pruebas de conformance JS/DOM extendidas y contabilidad fina de memoria. No se reabre la decisión salvo por los criterios de §8.1.

### 8.1 Criterios para revisar la decisión (si cambian las premisas)

- Si Boa no alcanza la compatibilidad necesaria para un corpus realista → reevaluar QuickJS/rquickjs (la capa de bindings es reutilizable).
- Si el dueño exige Test262 completo o JIT → la opción correcta es V8/SpiderMonkey, con una decisión de producto distinta (navegador de máxima compatibilidad) y no una feature de Fase 6.
- Si el build sin C es un requisito duro → Boa (o propio) es la única familia válida.
- Si se sube la MSRV del workspace (≥1.91) → se puede actualizar Boa 0.22+; si no, fijar 0.21.x.
