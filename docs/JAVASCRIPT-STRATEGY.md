# ADR-001 — Estrategia JavaScript para la Fase 6 (`docs/JAVASCRIPT-STRATEGY.md`)

- **Estado**: **PROPUESTO — decisión pendiente del dueño**
- **Fecha**: 2026-11-10 (Europe/Madrid)
- **Rama / HEAD**: `sprint/robustez-fase4` @ `c4cc170`
- **Ámbito**: motor JavaScript de la Fase 6, bindings DOM↔JS, ejecución de `<script>`/`document.write` y event loop mínimo.
- **No es**: una implementación; este documento no contiene código ni cambia `Cargo.toml`.
- **Documentos relacionados**: [`ARCHITECTURE.md`](ARCHITECTURE.md) §2.1 (arena DOM e identidad), §3.1 (política de dependencias); [`ROADMAP.md`](ROADMAP.md) §«FASE 6»; [`HANDOFF.md`](HANDOFF.md); [`FUZZING.md`](FUZZING.md); [`SECURITY.md`](../SECURITY.md).

---

## 1. Contexto

Megi ha cerrado funcionalmente la Fase 4 (renderizado) y tiene un pipeline de navegador real en la Fase 5 (`crates/render`: URL → net → HTML → CSS externo → layout → imagen `<img>` → PNG). La **Fase 6 (JS) es la última decisión estratégica grande**: ejecutar scripts de terceros dentro de un motor de navegador propio implica elegir un motor ECMAScript, definir una frontera DOM↔JS segura y mantener los invariantes del proyecto.

### 1.1 Qué necesita Megi de JavaScript (alcance mínimo verificable)

1. **`document.write` durante el parseo** para cerrar los **4 suites `scripted_*`** que siguen en la allowlist del gate (6 mode-runs): `scripted_adoption01.dat` (1), `scripted_ark.dat` (1), `scripted_foster01.dat` (2) y `scripted_webkit01.dat` (2). Hoy el gate pasa con `3.898/3.904` por esa allowlist documentada; con `document.write` correcto el árbol debería llegar a `3.904/3.904`.
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

**Estado real verificado a fecha del ADR**: **546 tests, 0 fallos**; gate de 4 suites PASS (tokenizer 7.050/7.050, árbol 3.898/3.904, serializador 230/230, CSS 30/30); Fase 6 al **0 %** de implementación (no existe `crates/js` todavía); todos los crates del motor con `#![forbid(unsafe_code)]`; MSRV Rust 1.88.

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
| Tamaño de build y tiempo de compilación | Medio | El build por defecto no debe crecer si hay feature `js` opcional. |
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

## 4. Recomendación

**Adoptar la Opción B1: Boa (`boa_engine` 0.21.x) como motor JS de la Fase 6, con feature `js` opcional (apagada por defecto), en un crate nuevo `crates/js`, sin delegar el núcleo DOM/CSS/layout/paint.**

Razones:

1. **Encaje con la filosofía**: Megi sigue implementando desde cero el núcleo del navegador (DOM/HTML/CSS/layout/paint/render/shell). El motor ECMAScript es un **componente de lenguaje**, no el núcleo del navegador; usar un motor existente evita años de trabajo y es coherente con la política §3.1, que ya autoriza componentes de bajo nivel (tipografía, red, ventanas).
2. **Pure Rust y `#![forbid(unsafe_code)]`**: no añade toolchain C ni FFI a nuestro código; el build por defecto puede seguir siendo std-only si la feature `js` está apagada.
3. **Licencia**: Unlicense OR MIT, compatible con `MIT OR Apache-2.0` eligiendo MIT; debe declararse en §3.1.
4. **MSRV**: `boa_engine 0.21.1` declara **Rust 1.88.0**, exactamente el MSRV del workspace; fijar `0.21.x` hasta que se decida subir la MSRV (0.22 exige 1.91).
5. **Cobertura suficiente para el alcance**: >90 % de la spec actual cubre las páginas reales y los `scripted_*` (que necesitan ES5 + `document.write`); si en el futuro la compatibilidad resulta insuficiente, la arquitectura de bindings permite **migrar a QuickJS/rquickjs** sin reescribir el DOM (ver §9).

### 4.1 Reglas de integración propuestas

- **Crate nuevo `crates/js`** con `#![forbid(unsafe_code)]`, dependencia opcional de `boa_engine`; `render`/`shell` la consumen con feature `js`.
- **Feature `js` apagada por defecto** en el workspace: `cargo test --workspace` y el gate siguen sin depender de Boa; CI añade un job `--features js` (build+tests) y el job MSRV sólo se ejecuta con la feature encendida si el dueño acepta la dependencia.
- **Registrar en `ARCHITECTURE.md` §3.1** una fila nueva: *Motor JS (Fase 6): `boa_engine` (Unlicense OR MIT, pure Rust)*; y actualizar la lista de lo que Megi implementa desde cero (bindings, event loop, política de ejecución y sandbox lógica).
- **Sin snapshots ni binarios precompilados**; todo compilado desde el registry, reproducible en CI offline tras el primer `cargo fetch`.
- **Revisión de `unsafe`**: Boa usa `unsafe` internamente; nuestro crate se mantiene libre de `unsafe` y la frontera es la API safe de `boa_engine`. Documentarlo en `SECURITY.md`.
- **Determinismo**: el motor no debe introducir `Date`/`Math.random` en golden tests; los tests de bindings usan scripts deterministas y el render con JS queda fuera de los goldens por ahora.

### 4.2 Riesgos y mitigaciones

| Riesgo | Prob. | Mitigación |
|---|:---:|---|
| Cobertura ES de Boa insuficiente para páginas reales | Media | Empezar con bindings; medir con un corpus de scripts; plan de migración a QuickJS (§9) |
| Incremento de tiempo de compilación/binary size con `js` | Alta | Feature opcional; perfil de build separado; medir en CI (target del bench) |
| Licencia Unlicense no deseada | Baja | Elegir MIT explícitamente; declarar en §3.1 |
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

## 6. Plan propuesto WP-6.x

Estimaciones en días de un agente full-time, con tests y revisión; dependencia en cadena salvo lo indicado.

| WP | Objetivo | Dep. | Días | Criterio de aceptación |
|---|---|---|---:|---|
| **WP-6.1** | Crate `crates/js` + feature `js` opcional; realm Boa (`Context`), `console` mínimo, presupuesto de instrucciones y errores tipados; CI job `--features js` | — | 3 | `cargo test -p js`; `cargo test --workspace` sin feature sigue verde (build std-only); MSRV 1.88 con feature JS |
| **WP-6.2** | Bindings DOM core: tabla lateral por realm, wrappers `Node`/`Document`/`Element`/`Text`, generación/stale, `DomError`→excepción JS | 6.1 | 6 | Micro-tests JS↔DOM (get/set `id`, atributos, `textContent`, hijos); wrappers invalidados tras `remove_node`; sin `unsafe` en `crates/js` |
| **WP-6.3** | Ejecución de `<script>` parser-inserted + `document.write`/`innerHTML` usando la API existente | 6.2 | 5 | **`cargo run -p test_harness -- --summary`**: suites `scripted_*` dejan de estar en la allowlist; árbol **3.904/3.904**; gate PASS con allowlist sólo de tokenizer |
| **WP-6.4** | `EventTarget` mínimo + timers/microtareas + integración con el event loop de `render`/`shell` | 6.2, 6.3 | 5 | `setTimeout`/`Promise` ordenados; tests de eventos sintéticos; un script con timer acaba en el pixel esperado |
| **WP-6.5** | `querySelector`/`querySelectorAll` (reutiliza el motor de selectores de `css`), `classList`/`style` (CSSOM básico) | 6.2 | 6 | Tests de selector JS; estilos aplicados tras mutación DOM (render cambia) |
| **WP-6.6** | Endurecimiento: presupuestos de tiempo/memoria, fuzz de bindings, aislamiento entre realms | 6.3–6.5 | 3 | Fuzz target de bindings en CI; script malicioso (bucle/alloc) interrumpido sin tumbar el proceso |
| **WP-6.7** | Integración con `shell` (ventana con JS) y documentación final de Fase 6 | 6.4–6.6 | 3 | `megi` ejecuta una página con JS en ventana y en `--screenshot`; docs + gate verdes |

**Total orientativo: 30–31 días-persona.** Camino mínimo hasta `scripted_*` verdes: **6.1 → 6.2 → 6.3 = ≈14 días-persona** (Boa 0.21.1). Si se elige QuickJS/rquickjs, sumar ~2–3 días por toolchain C y MSRV/CI.

### 6.1 Qué NO hacer en Fase 6

- No reimplementar un motor ECMAScript propio.
- No delegar en JS el parseo HTML/CSS, layout ni pintado: JS **muta el DOM**; el pipeline existente se re-ejecuta (o se invalida selectivamente) como hoy.
- No exponer red/filesystem por defecto; no `eval` de código remoto; no JIT.
- No prometer compatibilidad Test262 completa; el objetivo es páginas reales + `scripted_*` verdes.
- No meter Boa en el build por defecto: feature `js` opcional y CI específico.

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

## 8. DECISIÓN PENDIENTE DEL DUEÑO

**Este ADR no se implementa hasta que el dueño responda:**

1. **Motor**: ¿**Boa** (recomendado: pure Rust, MSRV 1.88 con 0.21.x, licencia Unlicense OR MIT, sin C) o **QuickJS/rquickjs** (MIT, casi 100 % Test262, pero añade toolchain C + FFI) u otra opción? ¿Se acepta la entrada «Unlicense OR MIT» en la política §3.1?
2. **Activación**: ¿feature `js` **apagada por defecto** (recomendado: build/gate/CI actuales intactos, job específico `--features js`) o encendida por defecto (más simple para el usuario, más peso de build y MSRV)?
3. **Alcance inicial**: ¿sólo **`document.write` + bindings DOM básicos** para cerrar los `scripted_*` y demostrar navegador real (**WP-6.1–6.3**, ≈14 días), o incluir desde el inicio eventos/timers/CSSOM/`querySelector` (**WP-6.1–6.5**, ≈25 días)?

**Mientras no haya decisión**: `crates/js` no se crea, la Fase 6 permanece al 0 %, el gate mantiene la allowlist `scripted_*` documentada y este ADR queda enlazado desde ARCHITECTURE/ROADMAP/HANDOFF. Cualquier cambio de decisión debe actualizar este documento y la matriz de §3.1 de [`ARCHITECTURE.md`](ARCHITECTURE.md).

### 8.1 Criterios para revisar la decisión

- Si Boa no alcanza la compatibilidad necesaria para un corpus realista → reevaluar QuickJS/rquickjs (la capa de bindings es reutilizable).
- Si el dueño exige Test262 completo o JIT → la opción correcta es V8/SpiderMonkey, con una decisión de producto distinta (navegador de máxima compatibilidad) y no una feature de Fase 6.
- Si el build sin C es un requisito duro → Boa (o propio) es la única familia válida.
- Si se sube la MSRV del workspace (≥1.91) → se puede actualizar Boa 0.22+; si no, fijar 0.21.x.
