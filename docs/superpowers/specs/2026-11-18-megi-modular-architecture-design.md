# Megi — Auditoría y propuesta de arquitectura modular

> **Archivado**: la restructuración descrita ya se completó. Las reglas vigentes viven en `AGENTS.md`; este documento se conserva como registro de la decisión.

**Fecha:** 2026-11-18 · **Estado:** borrador para aprobación · **Autor:** arquitectura/asistente
**Ámbito:** solo análisis y planificación. **No se ha modificado ningún archivo de código.**
**Fuente:** lectura directa de `Cargo.toml`, 387 archivos `.rs`, documentación y CI, más cinco auditorías paralelas por subsistema.

---

## 0. Resumen ejecutivo

Megi **ya no es un monolito**: es un workspace Cargo con 14 crates y un DAG estricto, sin `unsafe`, con arena DOM indexada y con gates de conformidad reales. La petición de "dividir en crates" no debe interpretarse como empezar de cero, sino como **cuatro movimientos concretos**:

1. **Renombrar** los 14 crates a `megi-*` (nombres genéricos como `image`, `net`, `shell`, `css` colisionan en crates.io y dificultan forks).
2. **Extraer dos crates nuevos**: `megi-display` (contrato puro de display list) y `megi-raster` (rasterizador + PNG), separando hoy `paint` (10 archivos, ~9,1 k líneas, con estado mutable oculto en `thread_local`).
3. **Crear `megi-webview`**: una API embebible sin winit/softbuffer, extrayendo la lógica pura de `shell` (navegación, historial, zoom, scroll, eventos, frame) de `WindowApp` (estructura de ~100 campos en un archivo de 3.200 líneas).
4. **Invertir `net → js`** (PAC) y **eliminar `Rc<RefCell<Host>>`** del runtime JS mediante un bus de mensajes; hoy es el único foco real de acoplamiento peligroso para agentes.

**Salud verificada hoy:** `cargo check --workspace --all-targets` ✅ (5,09 s) y `cargo check --workspace --no-default-features` ✅. 2.831 tests, gate 4/4 suites PASS, 14/14 crates con `#![forbid(unsafe_code)]`.

**Riesgo inmediato para vibecoding:** `docs/GAPS.md` está desactualizado (afirma que XHR, WebSocket, `querySelector`, `transform`, CSP, SRI, HSTS y mixed-content *no existen*, cuando el código sí los implementa). Un agente que lea esa tabla tomará decisiones erróneas. Es deuda a corregir en la fase 0.

> **Decisión por defecto (mi call, pendiente de confirmación):** objetivo de plataforma = **escritorio primero** (winit + softbuffer, como hoy), con `megi-webview` platform-agnostic para que móvil/WASM puedan añadir frontends después sin tocar el núcleo. Ver §5.

---

## 1. Auditoría del estado actual

### 1.1 Qué hay implementado y qué funciona

| Subsistema | Crate actual | Evidencia de que funciona |
|---|---|---|
| Núcleo de datos | `dom` (1 archivo, 4.011 líneas) | Arena contigua `NodeId(NonZeroU32)` 4 B (20 bits slot + 12 generación), `free_list` LIFO, `O(1)` splice, `validate_invariants`, 65 tests. Cero `Rc`/`RefCell`/`unsafe`. |
| HTML | `html` (~12,7 k) | Tokenizador 84 estados **7.050/7.050**; tree builder 21 modos **3.904/3.904** sin allowlist (incl. `scripted_*` con JS real); serializador **230/230**; ~104 tests. |
| CSS | `css` (~19,5 k) | Syntax L3, Selectors L4 (`:is/:where/:not/:has`), cascade layers, `var()`/`calc()`, Container Queries, Color 4/5, gradientes, `@font-face`; WPT CSS 30/30 + ~201 tests. |
| Layout | `layout` (~8,6 k) | BFC/IFC, flex MVP, grid completo, tablas §17, bidi UAX#9 subset, sticky, `Arc<ComputedStyle>` internado; ~274 tests + presupuesto ≤43 ms. |
| Tipografía | `fonts` (~2,1 k) | `fontdb`/`ttf-parser`/`rustybuzz`/`ab_glyph`, WOFF2, DB `Send+Sync`; 29 tests. |
| Pintado y raster | `paint` (~9,1 k) | Display list ~20 variantes, orden Apéndice E, AA, clipping, sombras, filtros, PNG propio determinista, hit-testing; ~470 tests. |
| Imagen | `image` (~8,6 k) | PNG/JPEG/GIF/WebP (VP8L + VP8), inflate/DEFLATE propios, EXIF, anti-DoS; ~73 tests. Sin dependencias. |
| SVG | `svg` (~5,7 k) | Parser XML, shapes/paths/gradientes/pattern/text/clip/mask/filter/SMIL, raster AA; ~117 tests. |
| Red | `net` (~20 k) | URL, HTTP/1.1 + H2 (HPACK, multiplexación, push), TLS rustls, caché, cookies+PSL, CORS, CSP/SRI/HSTS/mixed-content, proxies HTTP/SOCKS5/PAC, WebSocket; ~290 tests. |
| JS | `js` (~13,9 k) | Boa 0.22, bindings DOM, eventos, timers con reloj virtual, módulos ES/TLA/import maps, workers cooperativos, fetch/XHR/WebSocket; 312 tests. |
| Orquestación | `render` (~6,5 k) | `URL → PNG` E2E con scripts y event loop determinista, hit regions, stats; 47 archivos de tests de integración. |
| Shell | `shell` (~19,3 k) | winit+softbuffer, pestañas, chrome, find, formularios, sesión, descargas, `--screenshot`; ~250 unit + 60 integración. |
| Herramientas | `bench`, `test_harness`, `fuzz/` | Gate html5lib/WPT, 28 targets fuzz, benchmarks con presupuesto. |

**Conclusión:** la base funciona y está bien probada. Cualquier migración debe preservar tres invariantes byte-exactos: **goldens PNG**, **gate de conformidad** y **presupuestos de bench**.

### 1.2 Grafo de dependencias real (as-is)

```text
dom ──┬─► html ─────┐
      ├─► css ──┐    │
      └─► layout ┤    ├─► js ──────────┐
css ────────────┘    │                 │
fonts ──► layout ──► paint ──► render ─┤──► shell
image ──► paint      ▲        ▲        │
image ──► svg ───────┘        │        │
fonts ──► svg ────────────────┘        │
image ──► net ─────────────────────────┤
net ──(feature pac)──► js  ⚠ INVERSIÓN │
js ──► test_harness ───────────────────┘
```
- `dom`, `image`, `fonts`: hojas puras. `css` solo depende de `dom`. `layout` de `dom+css+fonts`.
- `paint` depende de `dom+css+layout+fonts+image`. `svg` de `image+fonts`. `render` de 9 crates. `shell` de 9 crates + winit/softbuffer.
- **Única inversión:** `net --features pac → js` (`net/src/pac.rs:270`, un solo callsite, desactivada por defecto).

### 1.3 Acoplamiento excesivo y archivos gigantes

**Archivos > 1.500 líneas (top; Fuente: `wc -l`):**

| Archivo | Líneas | Problema |
|---|---:|---|
| `js/src/runtime.rs` | 9.783 | Un único `impl JsRuntime` + ~150 callbacks nativos agrupados solo por comentarios `// ---`. No es descomponible por un agente sin leerlo entero. |
| `css/src/properties.rs` | 6.029 | `ComputedStyle` (~100 campos) + ~60 tipos de valor + `inherit_from`. |
| `html/src/tree_builder.rs` | 5.036 | 21 modos de inserción, AAA, foster parenting, hooks de script. |
| `paint/src/rasterizer.rs` | 4.454 | Raster + blur + caches `thread_local` (`BLUR_PLANES`, `DROP_SHADOW_MASK`, `BACKDROP_BUFFER`, `BLUR_OUTPUT`). |
| `dom/src/lib.rs` | 4.011 | Arena + API de navegador + iteradores + 54 tests, todo en un archivo. |
| `html/src/tokenizer.rs` | 3.602 | 84 estados + entidades + posiciones de error. |
| `shell/src/window.rs` | 3.200 | `WindowApp` con ~100 campos; añadir un campo obliga a leer el archivo entero. |
| `shell/src/chrome.rs` | 3.187 | Tab strip, address bar, dropdown, menú contextual, overlay. |
| `svg/src/document.rs` | 2.936 | Modelo SVG + estilos + gradientes + filtros. |
| `net/src/http.rs` | 2.888 | H1/H2/TLS/redirects/streaming en un archivo. |
| `render/src/lib.rs` | 2.429 | Orquestador de pipeline con 15 responsabilidades. |
| `layout/src/block.rs` / `inline.rs` | 2.310 / 2.169 | BFC e IFC con `Cell` de profundidad y pools de texto. |
| `paint/src/painter.rs` | 2.161 | Lee `LayoutBox` directamente; mapa `HashMap<*const LayoutBox, …>` para stacking contexts (punteros crudos, aunque seguros). |

**Patrones de acoplamiento (medidos):**

- `Rc<`/`Weak<`: **0** en `dom`, `html`, `css`, `layout`, `paint`, `image`, `svg`, `render`, `net`, `shell`. El diseño de arena ya cumplió el objetivo anti-`Rc<RefCell>`.
- `Rc<RefCell<...>>`: **concentrado en `js`** — `host.rs` (6 usos), `runtime.rs` (15 usos): `JsRuntime { host: Rc<RefCell<Host>> }`, `CURRENT_HOST: RefCell<Option<Rc<RefCell<Host>>>>`, `HostScope::enter`, fan-out a workers (`worker_parent`, `to_main`) y loader de módulos `Rc<Self>` + `&RefCell<&mut Context>`. Es el único sitio con re-entrancia y riesgo de ciclo fuerte padre↔worker.
- `RefCell` restante: cachés `thread_local` de estilo (`css/selector.rs:490`), scratch de PNG/DEFLATE/blur (`paint/{png,deflate,rasterizer}.rs`) y `&RefCell<RegionCollector>` en `paint/painter.rs`. Son aceptables, pero son **estado ambiental invisible** para un agente.
- `Arc<`: 108 usos. Compartir inmutable (`Arc<ComputedStyle>`, `Arc<RgbaImage>`, `Arc<Mutex<CookieJar>>`) es correcto y no debe confundirse con `Rc<RefCell>`.
- `thread::spawn`: 3 (worker del shell + helpers de test en net). `net` usa `thread::scope` en `load_many`. Los workers de JS son cooperativos, sin hilos.
- `unsafe`: **0 en todo `src/`**; los 14 crates tienen `#![forbid(unsafe_code)]`.

**Deuda de documentación:** `docs/ARCHITECTURE.md` tiene párrafos de miles de caracteres en una sola línea (p. ej. línea 3 y filas de la tabla §1.2) — daña el contexto de los modelos. `docs/GAPS.md` (2026-11-12) contradice el código actual. `CONTRIBUTING.md` y `docs/AI-QUICKSTART.md` siguen enumerando 14 crates y nombres antiguos.

### 1.4 Deuda técnica ordenada por impacto a corto plazo

| # | Deuda | Evidencia | Consecuencia |
|---|---|---|---|
| D1 | Docs desincronizados con el código | `GAPS.md:28,57,60-61,76-80` vs `js/src/xhr.rs`, `js/src/websocket.rs`, `css/src/query.rs`, `css/src/transform.rs`, `net/src/{csp,integrity,hsts,mixed_content}.rs` | Agente implementa dos veces una feature existente o "arregla" un falso bug. |
| D2 | Nombres de crates genéricos | `Cargo.toml:29-40` | Imposible publicar/fork limpio; `use megi_image::…` ambiguo; difusión de la comunidad. |
| D3 | `paint` mezcla display-list + raster + PNG | `paint/src/{painter,rasterizer,png,deflate,inflate}.rs`; caches `thread_local` | No se puede usar solo el pipeline de cómputo de display list en un embebido ligero sin arrastrar 4.454 líneas de raster. |
| D4 | Sin API `WebView` reutilizable | `shell/src/window.rs:165-216` solo `run_window*` bloqueante; `WindowApp` privado | Un fork debe copiar el shell completo; no hay contrato de eventos/frame. |
| D5 | `Rc<RefCell<Host>>` + TLS en JS | `js/src/host.rs:492-496,799-821`; `runtime.rs:167,8644-8660` | Re-entrancia frágil, ciclos padre↔worker, imposible razonar localmente al editar un binding. |
| D6 | `net → js` por PAC | `net/src/pac.rs:270`, `net/Cargo.toml:20` | Activar proxy-autoconfig arrastra Boa a un build mínimo. |
| D7 | Huecos de features | `test_harness` depende de `js` sin opcional; `shell` sin `[features]` (siempre winit+Boa vía `render`) | `--no-default-features` del workspace **no** produce un motor sin JS/UI; CI lo documenta como limitación. |
| D8 | God-files (§1.3) | 29 archivos de `src/` >1.500 líneas | Un cambio pequeño exige cargar demasiado contexto; alta probabilidad de romper otra sección. |
| D9 | Duplicación inflate/PNG | `paint/src/inflate.rs` (329) subset de `image/src/inflate.rs` (1.669); CRC/adler duplicados | Dos implementaciones que pueden divergir; `_with_capacity` distinto. |
| D10 | Sin reglas de arquitectura ejecutables | No hay `AGENTS.md`, no hay linter de DAG/tamaño; CI no comprueba límites | Las reglas viven en prosa y se incumplen sin aviso. |

**Nota de integridad (árbol en movimiento):** al iniciar la auditoría había cambios sin commitear en `crates/net/src/websocket.rs` y su test (+127/−7). **Durante el análisis aparecieron además cambios *staged* de otra línea de trabajo**: `js/src/{events,host,lib,runtime}.rs`, `js/tests/iframe_tests.rs`, `net/src/csp.rs`, `render/src/{frames,lib,scripts}.rs`, `render/tests/iframe_tests.rs` (+2.484/−78) y artefactos regenerados (`PROGRESS.md`, `docs/status/*`). Es exactamente el caso que `docs/AI-QUICKSTART.md` advierte: "the tree may be moving in parallel". **No se ha tocado ninguno de esos archivos.** La Fase 0 exige decidir su destino (commit/stash) y congelar la base de goldens *después* de que ese trabajo aterrice: dividir `runtime.rs` o `render/lib.rs` mientras se está añadiendo iframe es la receta perfecta para conflictos.

---

## 2. Arquitectura propuesta

### 2.1 Principios

1. **Un crate = una responsabilidad = un contrato de salida.** Si un crate produce dos cosas conceptualmente distintas (p. ej. display list *y* píxeles), se divide.
2. **DAG unidireccional, verificado por código**, no por documentación. Ningún crate de capa baja importa de una capa alta.
3. **Contratos de datos puros en la frontera:** IDs `Copy`, enums propios, structs con datos *owned*. Prohibido `Rc`, `RefCell` y lifetimes propios en tipos públicos que cruzan crates.
4. **Inversión por trait, no por dependencia:** la capa baja define el trait; la capa alta lo implementa (PAC, providers de JS, host de red).
5. **Mutación explícita por canal o `&mut`, nunca por estado ambiental.** `Arc` solo para payload inmutable; `mpsc` para eventos/hilos.
6. **Ligereza por feature flags, no por forks:** una sola base de código, `megi-webview` con `default = []`.

### 2.2 Grafo objetivo

```text
                       ┌──────────── megi-core ────────────┐
                       │ (NodeId, arena, Rect/Color/Limits) │
                       └─┬──────┬──────┬──────┬──────┬──────┘
                         │      │      │      │      │
        ┌────────────────┘      │      │      │      └──────────────┐
        ▼                       ▼      ▼      ▼                     ▼
  megi-html             megi-css  megi-net  megi-js          megi-layout
  (tokens/AST)          (styles)  (loader)  (Boa+eventos)     (LayoutBox)
                                     ▲         │                    │
  megi-image ──┐                     │         │ (traits)           │
  megi-fonts ──┼──► megi-display ◄────┘         │                    ▼
               │   (DisplayItem/List/HitRegion) │             megi-paint
               │            ▲                   │             (DisplayList builder)
               └──► megi-svg │                   │                    │
                            └───────────────────┼────────────────────┤
                                                ▼                    ▼
                                  megi-raster  ◄────────────── megi-render
                                  (DisplayList → frame/PNG)    (orquestador, sin UI)
                                                                     │
                                                              megi-webview
                                                        (API embebible, sin OS)
                                                                     │
                                                              megi-browser
                                                           (winit/softbuffer/chrome)

  tooling (no entra en el DAG del motor): megi-test-harness, megi-bench, fuzz/
```

**Orden de capas:** `core → (html|css|net|js|image|fonts) → display → layout → paint → raster → render → webview → browser`.

### 2.3 Responsabilidades y prohibiciones de importación

| Crate (nuevo ← actual) | Responsabilidad exacta | Puede importar | **Prohibido importar** | Contrato que entrega |
|---|---|---|---|---|
| `megi-core` ← `dom` | `NodeId`, `Document`/arena, `NodeKind`, `Namespace`, tipos base `Rect`/`EdgeSizes`/`Color`/`Length`, `Limits`, `DomError`. | Solo `std`. | **Todo lo demás**, cualquier dependencia externa. | `NodeId`, `&Document`, valores `Copy`. |
| `megi-html` ← `html` | WHATWG input stream, tokenizador, tree builder, serializador, `ScriptExecutor`. | `megi-core`. | `css`, `layout`, `paint`, `js`, `net`, `render`. | `Token`, `ParseError`, `ParseOptions`, `ParseOutcome`, `Document`. |
| `megi-css` ← `css` | Sintaxis L3, selectores L4, cascada, `ComputedStyle`, UA sheet, media/state/container, `query`. | `megi-core`. | `html`, `layout`, `paint`, `js`, `net`, `render`. | `HashMap<NodeId, ComputedStyle>`, `PseudoStyles`, `MediaContext`. |
| `megi-fonts` ← `fonts` | Descubrimiento, métricas, shaping, máscaras de glifo, WOFF2. | Dependencias de bajo nivel autorizadas. | Cualquier crate `megi-*` (incluido `core`). | `FontMetrics`, `ShapedRun`, `GlyphMask`, `FontDatabase`. |
| `megi-image` ← `image` | Decodificadores PNG/JPEG/GIF/WebP, inflate, EXIF, límites. | Solo `std`. | Cualquier crate `megi-*`. | `RgbaImage`, `detect_format`, `decode_scaled`. |
| `megi-net` ← `net` | URL, H1/H2, TLS, caché, cookies, CORS/CSP/SRI/HSTS, loader, WebSocket. | `std`, `rustls`/`webpki-roots` (feature). | **`megi-js`**, `render`, `html`, `css`, `layout`, `paint`, UI. | `Url`, `Resource`, `FetchResult`, trait `PacEvaluator`. |
| `megi-js` ← `js` | Boa, bindings DOM, eventos/timers, módulos, workers, fetch/XHR/WS *shim*. | `megi-core`, `megi-html`, `megi-css`, `boa_engine` (feature). | `megi-net`, `megi-layout`, `megi-paint`, `megi-render`, UI. | `JsRuntime`, `JsEvent`, `HostEffect` (mensajes). |
| `megi-display` **nuevo** | Tipos del contrato de pintado: `DisplayItem`, `DisplayList`, `HitRegion`, `GradientSpec`, `TextMaskRun`. Sin lógica de layout ni de raster. | `megi-core`, `megi-fonts`. | `css`, `layout`, `paint`, `raster`, `render`, `image`. | `DisplayList` owned, clonable, sin lifetimes. |
| `megi-layout` ← `layout` | Box tree, BFC/IFC, flex, grid, tablas, bidi, positioning. | `megi-core`, `megi-css`, `megi-fonts`. | `html`, `display`, `paint`, `raster`, `render`, `net`, `js`. | `LayoutBox` (árbol owned, Drop iterativo). |
| `megi-paint` ← `paint` (parte) | Construir `DisplayList` desde `LayoutBox` + estilos + imágenes; hit-testing. | `core`, `css`, `layout`, `fonts`, `image`, `display`. | **`megi-raster`**, `render`, `net`, `js`, UI. | `DisplayList`, `Vec<HitRegion>`. |
| `megi-raster` **nuevo** ← `paint/{rasterizer,png,deflate}` | `DisplayList → RgbaFrame → PNG`; blur, AA, culling, encoder. | `megi-core`, `megi-display`, `megi-fonts`. | `dom` traversal, `css`, `layout`, `paint`, `render`, dependencias externas. | `RgbaFrame`, `Vec<u8>` PNG, `RasterStats`. |
| `megi-svg` ← `svg` | Parser XML, modelo SVG, raster vectorial, SMIL. | `megi-image`, `megi-fonts`, `megi-display`. | `core`, `css`, `layout`, `paint`, `render`. | `RgbaImage` / `DisplayList` opcional. |
| `megi-render` ← `render` | Orquestación `URL → RenderOutput`; scripts, event loop, fuentes, imágenes, stats. | Todos los crates de motor excepto `webview`/`browser`; `megi-js` opcional. | winit, softbuffer, `megi-webview`, `megi-browser`. | `RenderOutput` (DOM + display list + frame + PNG + hits + stats). |
| `megi-webview` **nuevo** | API embebible: `WebView::new/load/render_on/tick/hit_test`, cola de `WebEvent`, callback de frame, navegación, zoom, sesión opcional. | `megi-render`, `megi-core`, `megi-display`, `megi-net` (vía render). | **winit, softbuffer, glutin, wgpu, cualquier crate de UI o ventana**. | `Frame`, `WebEvent`, `WebCommand`, callbacks. |
| `megi-browser` ← `shell` | Ventana de escritorio, chrome, pestañas, sesión, descargas, accesibilidad. | `megi-webview` + winit/softbuffer. | Acceso directo a `html/css/layout/paint/render/net/js/fonts/image` (todo pasa por `webview`). | Binario `megi`. |
| `megi-test-harness` / `megi-bench` | Gate de conformidad y benchmarks. | Pueden depender de todo. | Ningún crate del motor puede depender de ellos. | Gate PASS / JSON de métricas. |

**Nota sobre tipos duplicados:** `megi-core` solo aporta primitivas mínimas (`Rect`, un `Color` RGBA8, escalares). `megi-css` conserva sus tipos ricos (`Length`, `Color` con espacios Color 4/5, `CalcLengths`) y `megi-display` traduce a primitivas al emitir items. No se migra `ComputedStyle` a `core`.

**Reglas de oro para agentes:** si un archivo necesita `use megi_render::…` desde `megi-paint`, la arquitectura está rota: el tipo debe moverse a `megi-display` o `megi-core`, o la dependencia debe invertirse con un trait definido en el crate de abajo.

### 2.4 Contratos puros entre capas

Todos son tipos *owned*, `Clone + Debug`, sin lifetimes propios, sin `Rc`/`RefCell`. `Arc` solo para payloads inmutables y `Send + Sync`.

```rust
// megi-core
pub struct NodeId(NonZeroU32);                 // Copy, 4 B, generacional
pub struct Document { /* nodes: Vec<Node>, free_list, … */ }
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 } // Copy

// megi-html (AST/tokens, sin referencias al Document)
pub enum Token { Doctype{..}, StartTag{name, attrs, ..}, Character(u32), Eof, .. }
pub struct ParseOptions { /* caps anti-DoS */ }
pub struct ParseOutcome { pub document: Document, pub violations: Vec<LimitViolation> }

// megi-css
pub struct ComputedStyle { /* campos owned; Arc interno para gradientes/custom props */ }
pub type StyleMap = HashMap<NodeId, ComputedStyle>;

// megi-layout
pub struct LayoutBox { /* dimensions, style: Option<Arc<ComputedStyle>>, children, … */ }

// megi-display — contrato entre layout/paint y raster
pub enum DisplayItem {
    SolidColor { rect: Rect, color: Color },
    Text { /* runs owned, baseline, fuente */ },
    Image { image: Arc<RgbaImage> },         // Arc inmutable permitido
    Gradient { spec: Arc<GradientSpec> },
    PushClip(Rect), PopClip,
    OpacityGroup { items: Vec<DisplayItem> }, /* … ~20 variantes */
}
pub struct DisplayList { pub items: Vec<DisplayItem> }
pub struct HitRegion { pub rect: Rect, pub node_id: NodeId, pub kind: HitKind }

// megi-raster
pub struct RgbaFrame { pub width: u32, pub height: u32, pub pixels: Vec<u8> }

// megi-render
pub struct RenderOutput {
    pub document: Document,
    pub display_list: DisplayList,
    pub frame: RgbaFrame,
    pub png: Option<Vec<u8>>,
    pub hit_regions: Vec<HitRegion>,
    pub stats: RenderStats,
    pub warnings: Vec<Warning>,
}

// megi-webview — frontera con el frontend
pub enum WebEvent { Pointer{..}, Key{..}, Wheel{..}, Resize{..}, Focus{..}, TextInput(String) }
pub enum WebCommand { Navigate(Url), Reload, Back, Forward, Stop, SetZoom(f32) }
pub trait HostCallbacks { fn on_navigate(&mut self, url: &Url); fn on_title(&mut self, title: &str); }
```

**Reglas del contrato:** (a) nada de `&'a` en tipos públicos; (b) nada de `Rc`/`RefCell`; (c) el consumidor de un servicio define el trait, el productor lo implementa; (d) la mutaciónllega por `&mut self` o por `mpsc::Sender<...>`; (e) la serialización (serde) se decide después y no se deriva por defecto.

**Caso específico JS:** sustituir `Rc<RefCell<Host>>` + `CURRENT_HOST` por `JsRuntime { host: HostState }` y un `mpsc::Sender<HostEffect>`/`Receiver<JsEvent>`. Los ~150 callbacks nativos reciben el estado por el canal, no por TLS. `FetchProvider`, `WebSocketProvider`, `LayoutProvider` y los providers de storage/history se agrupan en un único `enum HostCall` con respuesta tipada. Esto elimina la re-entrancia y los ciclos padre↔worker en un solo movimiento.

**Caso específico PAC:** `megi-net` define `pub trait PacEvaluator: Send + Sync { fn evaluate(&self, url: &str, host: &str) -> String; }` (o un `Fn(&str,&str)->String` inyectado). `megi-webview`/`megi-browser` inyecta un adaptador sobre `megi-js` solo cuando el feature `pac` está activo. `net/Cargo.toml` pierde la dependencia opcional `js`.

### 2.5 Feature flags (ligero vs completo)

| Feature | Dónde | Default en `megi-webview` | Default en `megi-browser` | Arranca |
|---|---|---|---|---|
| `html`, `css`, `layout`, `fonts`, `image`, `display`, `paint`, `raster` | crates respectivos | siempre on (núcleo) | on | ~ n/d (std + tipografía) |
| `net` | `megi-webview`, `megi-net` | **off** | on | rustls/webpki, HTTP |
| `tls` | `megi-net` | **off** | on | `net` + `rustls` |
| `h2` | `megi-net` | **off** | on | HPACK/framing |
| `pac` | `megi-net` | **off** | on | trait evaluator + JS |
| `js` (renombrar `js`→`boa` en `megi-js`, `render`) | `megi-js`, `render`, `webview` | **off** | on | `boa_engine` + MSRV 1.91 |
| `svg` | `megi-svg`, `render`, `webview` | **off** | on | parser/raster SVG |
| `webfonts` | `megi-render` | **off** | on | WOFF2 |
| `full` | `megi-webview` | — | on | `net,tls,h2,pac,js,svg,webfonts` |

`megi-webview` con `default = []` = **modo WebView ligero**: `file:`/`data:` + HTML/CSS/layout/paint/raster, sin Boa, sin TLS, sin winit. Es el objetivo de "embebible y forkable".

**Matriz CI mínima:** (1) `cargo test --workspace` (full), (2) `cargo test -p megi-webview --no-default-features`, (3) `cargo check -p megi-render --no-default-features -p megi-net --no-default-features -p megi-js --no-default-features`, (4) gate de conformidad, (5) MSRV `--locked`, (6) goldens PNG + bench. Hoy el job `minimal` solo cubre `js`/`render` porque `shell`/`test_harness` arrastran Boa; con este diseño ese caveat desaparece.

### 2.6 Lo que **no** debe cambiar

- **No separar `css/properties.rs` de `cascade.rs`/`shorthand.rs`**: el orden de expansión de shorthands y `custom_properties: Arc<HashMap>` asume visibilidad de un solo crate. Solo dividir en submódulos *dentro* de `megi-css`.
- **No convertir BFC/IFC/flex/grid/table en crates**: comparten `LayoutBox`, `Arc<ComputedStyle>` y recursión `&mut LayoutBox`. Dividirlos obligaría a exponer internos y `Item<'a>` con lifetimes.
- **No añadir serde a los contratos del núcleo** salvo que se necesite persistencia real; los goldens exigen determinismo byte a byte y serde podría cambiar el orden.
- **No tocar el DAG de `image`/`fonts`**: son hojas correctas; `layout/text.rs:FontTextMeasurer` es el seam previsto.
- **No mover el fuzz al workspace principal**: es deliberadamente aparte (nightly + ASan).

---

## 3. Plan de migración por fases

Cada fase termina **verde** con los mismos comandos. No se acumulan deudas entre fases.

### Fase 0 — Verdad y guardarraíles (sin cambios de comportamiento)
- Congelar baseline: `cargo test --workspace`, gate `--summary`, md5 de goldens y bench actuales.
- Corregir `docs/GAPS.md`: marcarlo como *histórico (2026-11-12)* o regenerarlo con evidencia vigente; añadir nota en `docs/ARCHITECTURE.md` de que es la fuente canónica del DAG. Partir líneas gigantes de `ARCHITECTURE.md`.
- Añadir `tools/check_arch.py`: valida el DAG desde los `Cargo.toml`, detecta importaciones prohibidas, `unsafe`, `Rc<RefCell` y límites de tamaño; cablearlo en CI `quality`.
- Añadir `[workspace.lints]` en `Cargo.toml` (`unsafe_code = "forbid"`) y `AGENTS.md` (borrador §4).
- **Gate:** mismos tests + `python3 tools/check_arch.py` → 0 violaciones.

### Fase 1 — Renombrar a `megi-*` (mecánico, un solo PR por capa)
- Orden hoja→raíz: `dom→megi-core`, `image→megi-image`, `fonts→megi-fonts`, `html→megi-html`, `css→megi-css`, `net→megi-net`, `js→megi-js`, `layout→megi-layout`, `svg→megi-svg`, `paint→megi-paint`, `render→megi-render`, `shell→megi-browser`, `test_harness→megi-test-harness`, `bench→megi-bench`.
- Actualizar `[workspace.dependencies]`, claves de dependencia, todos los `use megi_*::`, docs, workflows, `.cargo/config.toml` aliases y scripts.
- **Gate:** `cargo check --workspace --all-targets`, gate 4/4, goldens idénticos. No cambia ni una línea de lógica.

### Fase 2 — Extraer primero `megi-display` + `megi-raster` ⭐
**Es lo que hay que extraer primero** porque el contrato ya está limpio (display list owned) y los goldens PNG dan una verificación byte-exacta inmediata.
1. Crear `megi-display` con `DisplayItem`, `DisplayList`, `HitRegion`, `GradientSpec`, `TextMaskRun` desde `paint/src/display_list.rs`; `megi-paint` y `megi-render` importan de él.
2. Crear `megi-raster` moviendo `rasterizer.rs`, `png.rs`, `deflate.rs` y el scratch `thread_local` a un módulo `scratch.rs` documentado. `megi-paint` **deja** de tener rasterizador.
3. `megi-render` llama `megi_raster::rasterize(&display_list, &font_db)`.
4. Opcional: unificar CRC/Adler con `megi-image` sin invertir el DAG (mover a `megi-core` o dejar duplicado con commentario).
- **Gate:** PNG byte-idíntico para los ~20 fixtures de `paint` y los 47 de `render`; `cargo test -p megi-paint -p megi-raster`.

### Fase 3 — Invertir `net → js` (PAC)
- Definir `PacEvaluator` en `megi-net`; mover `pac.rs` a un adaptador dentro de `megi-webview`/`megi-browser` tras la fase 5 (interinamente en `megi-render`).
- Quitar `js` de `[dependencies]` de `megi-net`. `cargo tree -p megi-net` sin Boa.
- **Gate:** tests de proxy/PAC + `cargo check -p megi-net --no-default-features`.

### Fase 4 — Desacoplar el runtime JS
- Sustituir `Rc<RefCell<Host>>`/`CURRENT_HOST` por `HostState` + `mpsc<HostEffect/JsEvent>` (§2.4).
- Dividir `runtime.rs` (9.783 líneas) en `js/src/runtime/{globals,dom_bindings,events_timers,modules,network,workers,geometry}.rs` **sin cambiar comportamiento**, agrupando los bloques ya marcados con banners.
- Renombrar feature `js` → `boa`; stub `--no-default-features` ya existente se mantiene.
- **Gate:** 312 tests de `megi-js`, fixtures WPT, y re-render E2E idéntico en `megi-render`.

### Fase 5 — Nacer `megi-webview`
- Extraer de `shell` los módulos puros (`navigation`, `history`, `links`, `forms`, `find`, `zoom`, `scroll`, `refresh`, `session`, `worker` sin winit) a `megi-webview` con `WebView`, `WebEvent`, `WebCommand` y callback de frame.
- Contrato de frame versionado (reemplaza `worker::RenderFrame` privado con `Arc<Document>` + PNG).
- `megi-webview` con `default = []` y flag `full`; publicar ejemplos `embed_minimal` y `embed_full`.
- **Gate:** los 60 tests de integración del shell pasan contra el nuevo `WebView` (sin UI), más un test de frame determinista.

### Fase 6 — `shell` → `megi-browser` como capa fina
- `WindowApp` se convierte en `WinitFrontend` que solo: traduce eventos winit → `WebEvent`, dibuja `Frame` en softbuffer y delega estado en `WebView`.
- Eliminar de su `Cargo.toml` las dependencias directas sobre `html/css/layout/paint/render/net/js/fonts/image`.
- **Gate:** capturas `--screenshot` byte-idénticas a las de la fase 5; tests de chrome/tabs/session.

### Fase 7 — Features y CI
- `megi-test-harness` con feature `js` opcional; `megi-browser` activa `full`.
- Matriz CI de la §2.5; medir tamaño de binario/tiempo de link del build ligero y publicarlo en `PROGRESS.md`.
- **Gate:** los seis comandos de CI en verde, incluido `no-default-features` sin Boa en ningún crate.

### Fase 8 — Reducir god-files (Incremental, 1 archivo por PR)
- Orden sugerido: `dom/src/lib.rs` → `id.rs/arena.rs/query.rs/iter.rs`; `runtime.rs` ya dividido en F4; `css/properties.rs` → submódulos por familia de propiedades; `tree_builder.rs` → `modes/{in_body,in_table,…}`; `window.rs` → `frontend/{window,input,event_loop}.rs`; `chrome.rs` → `chrome/{tabs,address_bar,menu,overlay}.rs`.
- Límite: **ningún archivo nuevo > 400 líneas**; los 29 existentes >1.500 líneas se reducen con test de no-regresión.
- **Gate:** diffs solo de movimiento de código; `git diff --stat` sin cambios de comportamiento.

---

## 4. Borrador de `AGENTS.md` (listo para promover a `/AGENTS.md`)

```markdown
# AGENTS.md — Reglas de arquitectura de Megi

Eres un agente que trabaja en un motor web 100 % Rust, desde cero, con
vocación de ser embebible. Estas reglas son obligatorias. Si no puedes
cumplirlas, para y pregunta. Mantén este archivo por debajo de 200 líneas.

## 1. Orientación antes de tocar nada
1. Lee `README.md`, `docs/ARCHITECTURE.md` (DAG y §3.1) y este archivo.
2. `git status --short`; NO pises cambios ajenos sin commitear. Trabaja en worktree.
3. Verifica el código, no solo la documentación: `docs/GAPS.md` es histórico.
   Si un doc y el código discrepian, el código + sus tests son la verdad; abre issue para el doc.
4. No edites artefactos generados: `PROGRESS.md`, `docs/status/*`, `docs/audit/FINDINGS.md`.

## 2. Vetos absolutos
- `#![forbid(unsafe_code)]` en los 14 crates. Cero `unsafe`.
- Cero `Rc<`, `Rc<RefCell<`, `Weak<` en código nuevo. Usa `NodeId` + arena,
  `Arc<T>` solo para payload inmutable `Send + Sync`, o `mpsc` para mutación.
- Cero lifetimes propios ('a) en tipos públicos que crucen crates. Préstamos
  cortos dentro de un módulo están bien.
- Cero dependencias que implementen parseo HTML/CSS, layout o pintado
  (html5ever, cssparser, selectores, stylo, taffy, blitz…). Consulta §3.1.
- Cero dependencias circulares entre crates. Un edge nuevo requiere PR de
  arquitectura + actualización del DAG.

## 3. DAG: quién puede importar a quién
    core → html, css, net, js, image, fonts
    css + fonts → layout → paint → raster → render → webview → browser
    image + fonts → svg → render
    display (core + fonts) es el contrato de paint y raster.
Prohibiciones clave:
- `net` NUNCA importa `js` (PAC usa el trait `PacEvaluator`).
- `js` NUNCA importa `net`, `layout`, `paint` ni `render` (usa providers).
- `layout` NUNCA importa `paint`, `display` ni `render`.
- `paint` NUNCA importa `raster` ni `render`; `raster` NUNCA importa `layout`.
- `webview` NUNCA importa winit/softbuffer/UI; `browser` no importa crates de motor
  directamente (solo `webview`).
- `test_harness`/`bench` son herramientas: nadie del motor depende de ellos.

## 4. Tamaño y forma de archivos
- Archivo `.rs` nuevo: **≤ 400 líneas** (objetivo) / **500 líneas** (tope duro).
- Modificar un archivo > 500 líneas: divide primero la sección que tocas en un
  submódulo, salvo excepción documentada con
  `// ARCH-EXCEPTION(#issue): motivo; plan de reducción en <issue>`.
- Crate: objetivo ≤ 12.000 líneas de `src/`; > 20.000 requiere plan de escisión.
- Un módulo público expone uno o pocos tipos relacionados; `lib.rs` solo
  re-exporta (sin `impl` de más de 20 líneas).
- Contratos entre crates: enums/structs owned, `Copy` cuando sea posible, sin
  `Rc`/`RefCell`/`&'a`. Traits se definen en el consumidor.

## 5. TDD — ley de hierro
1. Un test mínimo que compruebe comportamiento observable (no un mock).
2. `cargo test -p <crate> <nombre>` y confirmar que FALLA por la razón esperada.
3. Implementar el arreglo más pequeño; refactorizar solo en verde.
4. Citar la norma: `// WHATWG HTML §13.2.6.4.7` / `// RFC 6455 §5.2`.
5. Cambios visuales: golden PNG byte-idéntico salvo decisión explícita con
   md5 antiguo y nuevo en el commit.

## 6. Verificación antes de reportar éxito (pega la salida)
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo check --workspace --all-targets
    cargo check --workspace --no-default-features
    cargo test --workspace
    cargo test -p megi-webview --no-default-features
    cargo run --release -p megi-test-harness -- --summary   # debe decir PASS
    python3 tools/check_arch.py                             # DAG + tamaños
    python3 tools/check_docs_links.py
    python3 tools/update_progress.py --check
    python3 tools/update_coverage.py --check
    python3 tools/audit_coverage.py --check --strict
Cero advertencias. Reporta cada comando con su código de salida. Nunca digas
"verde" sin salida pegada.

## 7. Dependencias, métricas y entrega
- Diseño de specs vivas (WHATWG/W3C/RFC); prohíbido inventar algoritmos.
- Nueva dependencia externa: solo transporte/cripto/ventana/tipografía de bajo
  nivel, previa lectura de `docs/ARCHITECTURE.md` §3.1 y aprobación.
- Si cambia el inventario de archivos: `tools/audit_coverage.py --generate` y
  marcar filas como covered SOLO tras leerlas. Ledger en 0 pendientes.
- Commits atómicos, Conventional Commits en inglés, DCO (`git commit -s`).
- Entrega: (a) qué cambió y por qué, (b) comandos + códigos de salida,
  (c) salida del gate, (d) limitaciones conocidas por nombre.
```

---

## 5. Decisiones tomadas (mis llamadas) y preguntas abiertas

**Decisiones asumidas para que el documento sea accionable:**

1. **Plataforma:** escritorio primero, con `megi-webview` sin dependencias de OS. Móvil/WASM serían frontends nuevos sobre la misma API. *(Si el primer consumidor es móvil/WASM, cambia el diseño del worker y del callback de frame; ver pregunta 1.)*
2. **Crates adicionales conservados:** `megi-image`, `megi-fonts`, `megi-svg`, `megi-render`, `megi-test-harness`, `megi-bench`. La lista del usuario era ilustrativa ("por ejemplo"); eliminar estos sería regresivo.
3. **`megi-core` = `dom` renombrado**, con tipos geométricos/color añadidos. No crear `megi-geom` todavía: `dom/lib.rs` ya es grande y añadir un crate más eleva el coste de migración; se puede extraer en Fase 8.
4. **Sin serde en contratos del núcleo** por ahora (determinismo).
5. **Límites de tamaño 400/500 líneas** son objetivos nuevos; los ~18 archivos actuales >1.500 líneas se reducen en Fase 8, no antes.
6. **No publicar en crates.io** sin decidirlo antes; el rename es igualmente necesario para forks.

**Preguntas que solo tú puedes responder (cambian el plan; ordenadas por impacto).**
*Las preguntas 1–3 y 6–7 salieron del "builder check" de un arquitecto fresco que leyó el documento sin contexto.*

1. **Primer consumidor del WebView y modelo de hilos/frame** — ¿escritorio (winit/softbuffer), móvil, WASM o una app concreta? ¿Necesita frame async/GPU o `RgbaFrame` CPU basta? Define el contrato de frame, la propiedad (`Vec` vs `Arc`) y si Fase 5 empieza con worker async. **Un error aquí obliga a reescribir la API pública nueva.**
2. **¿Existen consumidores externos de los nombres actuales?** Si alguien fuera del repo depende de `image`, `net`, `shell`, etc., Fase 1 necesita crates-alias o periodo de deprecación; si no, el rename es flag-day limpio.
3. **¿Publicar `megi-*` en crates.io, con qué licencia y owners?** Afecta a urgencia del rename, auditoría de metadata/docs.rs y disciplina de estabilidad de API.
4. **Versionado: ¿lockstep `0.1.0` (como hoy) o independiente por crate?** Afecta todos los `Cargo.toml` y cómo los forks consumen subsets.
5. **¿Feature freeze durante Fases 0–7?** Recomiendo congelar producto (iframe/canvas) hasta Fase 7. **Esto ya no es hipotético:** hay un cambio staged de +2.484 líneas añadiendo iframe sobre `runtime.rs`/`render/lib.rs`, justo los god-files que la Fase 4/5 quiere dividir.
6. **¿Qué debe hacer exactamente `megi-webview --no-default-features`?** He asumido `file:`/`data:` sin Boa/TLS/H2/SVG. Si el modo ligero debe soportar `http:` sin TLS, imágenes o SVG, cambia el grafo de features y la matriz CI de Fase 7.
7. **Destino del árbol sucio y dureza de las gates** — ¿commit/stash de los cambios concurrentes? ¿Son bloqueadores duros o advisory el PNG byte-idéntico, el gate 4/4 y el presupuesto ≤43 ms de layout? Define qué significa "verde" en cada fase.

**Aceptación implícita si no respondes:** seguiré con los defaults de esta sección (escritorio-first, flag-day rename, sin publicación, versionado lockstep, freeze de features, modo ligero `file:`/`data:`, gates como bloqueadores duros).

---

## 6. Siguiente paso (tras aprobación)

Con este documento aprobado, el siguiente paso es `superpowers:writing-plans` para convertir las fases en planes de trabajo atómicos (comandos, tests y criterios de aceptación por tarea), y después ejecutar con TDD. **No se ha escrito código todavía.**
