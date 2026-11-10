# Hoja de Ruta Maestra y Micro-Paquetes Atómicos (`docs/ROADMAP.md`)

Este documento desglosa la planificación y el estado de ejecución del motor web **Megi** en **micro-paquetes de trabajo atómicos** (`WP-*`) de **máximo 1 a 2 días de duración cada uno**.

Cada paquete de trabajo está vinculado de forma explícita y trazable a:
1. **Sección exacta y ancla (`#id`) de la especificación formal WHATWG HTML / DOM o W3C CSS**.
2. **Archivo(s) de prueba oficial(es) de `html5lib-tests` / WPT** que lo validan.
3. **Crate y módulo responsable** dentro del Cargo Workspace.
4. **Criterio de aceptación verificable automáticamente por comando `cargo test`**.

---

## 📊 Tablero de Progreso Global por Módulos y Fases

```text
Fase 0: Tooling & Arena DOM         [██████████] 100% (COMPLETADA)
Fase 1: Tokenizador & Árbol HTML5   [██████████] 100% (COMPLETADA - gate 7.050/7.050 + árbol 3.904/3.904)
Fase 2: Motor CSS                    [██████████] 100% (COMPLETADA)
Fase 3: Motor Layout (BFC & IFC)     [██████████] 100% (COMPLETADA)
Fase 4: Paint, PNG & Tipografía      [██████████] 100% (backgrounds/SVG inline E2E; pendientes menores)
Fase 5: Red, Render & Imágenes       [██████████]  95% (red avanzada E2E: cookies/CORS/gzip en el pipeline)
Fase 6: Integración JS & Bindings    [████████░░]  80-85% (eventos avanzados HECHOS; event loop en render)
Fase 7: Ventana OS & Shell           [███████░░░]  70% (navegación + zoom + auto-refresh + hit regions)

**Ronda 15**: **background-* geometry** (`repeat/size/position` + shorthand, tiling con clip y cap 4096, legacy `background_geometry_explicit` para goldens; css 69/paint 122); **SVG inline** en `render` (pre-proceso a `<img>` sintético con base64, límites 256/1 MiB/8 MiB/depth 256, stat `inline_svgs`, 7 fixtures y 10 tests ⇒ render 114); **IFC −28 %** (layout/realistic-1mb **50,5 → 36,3 ms**, pipeline **132,9 → 120,5 ms**, geometría byte-idéntica gracias a pool de texto, ids de estilo, breaking incremental y fast path de `format_line`); y **fix de determinismo** del event loop (`b808d7b`: sólo reloj virtual, fixture `js_event_timeout` md5 `07682088…`, CI verde). Estados: F5 ≈95 %, **F6 ≈80-85 %**, **F7 ≈70 %**.
**Ronda 13**: **fuentes web @font-face + WOFF2 E2E** (`css` parsea `@font-face`, `fonts` decodifica WOFF2→sfnt con `woff2-patched 0.4` Apache-2.0 pure Rust — única dep externa de `fonts` — y `render` construye una DB por render para layout+paint con fetch/dedupe/caps 8/8 MiB/32 MiB, stats y warnings no fatales; evidencia md5 webfont `f38aed03…` vs fallback `e2d76dee…`); nuevo **crate `svg`** (parser XML+modelo SVG+rasterizador AA desde cero, 41 tests, límites anti-DoS) ya **cableado E2E en `render`** para `<img src=*.svg>` y `background-image` (detección Content-Type/sniffing, intrínseco width/height/viewBox con 300×150 por defecto, cap 16 Mpx/64 MiB RGBA, corrupto ⇒ warning no fatal, navegación directa en página sintética, stat `svgs_rendered`; 11 tests ⇒ render 104); **cierre de ronda**: fix `html/fast_hash` recupera `html/massive-end-p` (~19 ms; antes 21,4) y deja el **pipeline ~127-134 ms** con el gate intacto; **shell** con **zoom** (`--zoom` 0.5–3.0), **auto-refresh opt-in** (`--refresh-ms`) y título enriquecido (100 tests; `--zoom 2` md5 `c11903cf…`). CI verde con deps de sistema. Estados: F5 ≈95 %, **F6 ≈80-85 %**, **F7 ≈70 %**.
**🎉 HITO Ronda 11**: **árbol HTML5 3.904/3.904 (100 %)** — las 4 suites `scripted_*` pasan y el gate queda **sin allowlist de tree** (`scripted_*` 6/6 mode-runs). **WP-6.4 HECHO**: hook `ScriptExecutor` en el parser, scripts parser-inserted al `</script>`/EOF y `document.write` re-tokenizado en la posición actual (`Tokenizer::insert_input`, anidados incluidos), harness `JsScriptExecutor` sobre `eval_on_document`. **WP-B HECHO**: `CSSStyleDeclaration` completa, `getComputedStyle` con provider, `NodeList`, `matches`/`closest`, `insertAdjacentHTML/Text`, identidad de wrappers y `document` como `EventTarget`. **Event loop presupuestado en `render`**: reloj virtual, `DOMContentLoaded`/`load`, timers con delay (`max_tasks` 10k, `max_virtual_time` 1000), un solo re-render y stats (`script_*`, `timings.event_loop`); 6 fixtures `js_event_*`. **Parser**: regresión `tag-40k-attrs` corregida (~12,4 ms, A/B por debajo del baseline pre-r10) manteniendo las ganancias de r10. Estados: F5 ≈95 %, **F6 ≈75-80 %**, **F7 ≈65-70 %**. **Ronda 12**: **rendimiento** — `LayoutBox.style` → `Option<Arc<ComputedStyle>>` (+ `style()`/`with_shared_style`), estilo anónimo compartido por padre y tokens `Space` con `char` ⇒ **LayoutBox 496→176 B**, `layout/realistic-1mb` **−14,6 %** (51-53 ms) y pipeline **134-137 ms**; **encoder PNG −30,5 %** (render-to-PNG 13,30→9,24 ms) con **bitstream byte a byte idéntico** (goldens sin regenerar). **JS eventos avanzados HECHO** (`currentTarget`/`eventPhase`, `once`/`passive`, `onclick`/`onload`/`onerror`, `document.close()`, `estimated_heap_bytes()`, excepciones capturadas; 58 tests). **CI GitHub en verde** (3 jobs) con paquetes de sistema Wayland/X11/xcb añadidos al workflow; `main` y la rama sincronizadas. Estados: F5 ≈95 %, **F6 ≈80 %**, **F7 ≈65-70 %**.
**Ronda 10**: JS **WP-A HECHO** (`EventTarget` con captura+burbuja, `Event`/`CustomEvent` y timers con reloj inyectable `advance`/`run_until`/`now_ms`/`pending_timers`; 26 tests js); **rendimiento** pipeline 800×600 **167,6 → 131,2 ms (−21,7 %)** y `html/realistic-1mb` **55,0 → 19,2 ms (−65 %)** (gate y golden intactos); **`RenderOutput.hit_regions` cableado** en el pipeline (píxeles sin cambios, +0,6 µs) y el shell usa `paint::find_link_at`; **red avanzada E2E** en `render`/`shell` (jar de cookies por render o sesión, `--cookies`, `--origin` con CORS/credentials y denegación no fatal, gzip/deflate en HTML/CSS). Estados: F5 ≈95 %, F6 ≈40 %, F7 ≈65 %. **Ronda 9 (cierre)**: el pipeline `render` **ejecuta JavaScript** (`ScriptingMode`, WP-6.2 host: scripts inline/externos post-parseo con budgets, microtasks/timers, console y **re-render del DOM mutado** visible en el PNG; `--no-scripting`), `paint` gana **hit-testing** aditivo (`HitRegion`/`hit_test`/`find_link_at`; PNG/golden byte-idénticos) y el `shell` es **navegable**: clic en enlaces con `Url::join`, historial (`NavigationState`), Alt+←/→, `r` y título dinámico. La Fase 4 sigue funcionalmente cerrada (pendientes menores: `background-repeat/size/position`, colores de `border-collapse`, block-in-inline). Fase 5 ≈95 % (HTTP2/streaming/SameSite pendientes). Fase 6 ≈40 % (WP-B, WP-6.4 `scripted_*` e integración de eventos/timers en el host pendientes). Fase 7 ≈65 % (formularios, pestañas, zoom, JS en el host y eventos del DOM pendientes).
> **990 tests** y gate de 4 suites PASS (**árbol 3.904/3.904, 100 %**); rendimiento en `docs/PERFORMANCE.md` (**layout ~36,3 ms, pipeline ~120,5 ms, `html/massive-end-p` ~19 ms, PNG −30,5 %**) y suite determinista (reloj virtual).
```


| Fase / Bloque | Barra | Estado | Cobertura Verificada | Hito Clave Alcanzado |
|---|:---:|:---:|---|---|
| **Fase 0: Tooling, Arena DOM & Closed-Loop Harness** | `[██████████] 100%` | **COMPLETO** | 28 tests en `crates/dom` | Arena contigua indexada (`NodeId(NonZeroU32)` de 4 bytes) sin `Rc<RefCell<Node>>`, scripts CLI de inspección WHATWG en vivo. |
| **Fase 1A: Tokenizador HTML5 (84 Estados & 2.231 Entidades)** | `[██████████] 100%` | **COMPLETO** | 7.050 / 7.050 estados en el gate (allowlist vacía) | 13 suites estándar de `html5lib-tests/tokenizer` al 100%; `ParseOptions` anti-DoS y deduplicación de atributos en tiempo lineal. |
| **Fase 1B: Árbol DOM, Serializador §13.3 & DOM de navegador** | `[██████████] 100%` | **COMPLETO** | **3.904/3.904 árbol (100 %)** + 230/230 serializer + **60 tests html** | 21 modos de inserción, AAA, *foster parenting*, `innerHTML`; **serializador público WHATWG HTML §13.3** (`serialize_html`/`serialize_fragment`/`serialize_outer_html` + opciones) con las 5 suites `html5lib-tests/serializer` en el gate. **WP-6.4 (ronda 11)**: hook `ScriptExecutor` + scripts parser-inserted + `document.write` re-tokenizado ⇒ las 4 suites `scripted_*` pasan y la allowlist de tree queda vacía; fix de la regresión `tag-40k-attrs` (~12,4 ms). |
| **Fase 2: Motor CSS (Sintaxis L3, Selectores L4, Cascada L4, Hoja UA §15)** | `[██████████] 100%` | **COMPLETO** | 69/69 unit + 30/30 WPT | Selectores L4 funcionales (`:is/:where/:not/:has`) y estructurales, especificidad, expansión de shorthands antes de la cascada, `font` shorthand, `@media` básico y índice de reglas (2,1× en resolve-styles 10k nodos); bases de Fase 4 (bordes por lado, `border-style: none`, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`, `line_height_factor`). |
| **Fase 3+4: Motor de Layout (BFC/IFC, posicionamiento, floats, tablas)** | `[██████████] 100%` | **COMPLETO** | 135 tests | Box Tree, cajas anónimas *inherit-only*, colapso de márgenes, ICB, `box-sizing`/min-max/%, sobre-restringido; **IFC**: oportunidades de salto reales, soft hyphen de ancho cero, `line-height` como factor, `vertical-align: top/bottom` (§10.8.1) y `text-align: justify`. |
| **Fase 4: Display Lists, Pipeline HTML → PNG & Rasterización 2D** | `[██████████] 100%` | *COMPLETA* | 122 tests en `crates/paint` | Pipeline `HTML → CSS → layout → display list → PNG` con texto real, **orden completo del Apéndice E** (stacking contexts/`z-index`), `OpacityGroup` (grupo offscreen), `PushClip`/`PopClip` desde `overflow`, `text-decoration`, `visibility`, marcadores de lista, bordes por lado, fondo bajo el *border box*, antialiasing exacto y culling/cachés; codificador PNG propio y golden determinista (PNG `md5 9e957177…`). PNG comprimido propio (**44,8×**), painter iterativo, imágenes `<img>` (ImageStore/bilineal), **fix de underflow** en cobertura parcial, **hit-testing aditivo** (`HitRegion`/`hit_test`/`find_link_at`, 11 tests), DOM generacional y **encoder PNG −30,5 % con bitstream invariante** (ronda 12). **Ronda 15**: **`background-repeat/size/position`** reales (`background_tiles`, cover/contain centrado, clip/tiling al padding box, cap 4096) con legacy `background_geometry_explicit`. |
| **Fase 4: Motor tipográfico (`crates/fonts`)** | `[██████████] 100%` | **COMPLETO (integrado + fuentes web)** | 17 + 1 doctest | `FontDatabase` (fontdb), métricas OpenType (ttf-parser), shaping LTR con `rustybuzz` (kern/ligaduras), rasterización de glifos (`ab_glyph`) y fallback; **integrado en layout y paint** (texto real en el pipeline). **Ronda 13**: **fuentes web `@font-face`+WOFF2 E2E** (dep `woff2-patched 0.4`, alias CSS, DB por render en layout/paint, caps y stats). Bidi/scripts complejos/fuentes variables quedan fuera de Fase 4. |
| **Fase 5: Red, Render Pipeline & Imágenes** | `[██████████] 95%` | *EN CURSO* | 180 (net) + 114 (render) + 45+1 (image) | URL WHATWG (subset http/https/file + join), HTTP/1.1 `GET`, **TLS 1.2/1.3 con `rustls` 0.23 (ring) + `webpki-roots`** (SNI/validación, feature `tls` por defecto), **caché HTTP RFC 9111 subset** (max-age/Expires/ETag/Last-Modified/304/no-store/Vary), **`data:` URLs** con Base64 propio, `file://`, MIME sniffing y `ResourceLoader` con caché; `#![forbid(unsafe_code)]`. **`crates/render`**: pipeline URL→PNG con CSS externo, CLI y **`<img>` E2E**; **`crates/image`**: decoder PNG/inflate propio; CSS con `@import`/media queries. **`@import` recursivo + media real** en `render`, **showcase 1024×768** (80.220 B, `md5 c95c7eff…`), **backgrounds E2E**, **cookies RFC 6265/CORS/gzip** y **ejecución JS en render** (`ScriptingMode`), **red avanzada E2E** (cookies/CORS/gzip con `--cookies`/`--origin`) **hit_regions** en `RenderOutput` y **event loop presupuestado** (reloj virtual, lifecycle, timers, re-render único) y **fuentes web E2E** (`@font-face`/WOFF2, DB por render, stats/timing) y **SVG E2E** en `render` (`<img>`/background-image, cap 16 Mpx/64 MiB RGBA, `svgs_rendered`; 5 fixtures `svg_*`), **SVG inline** (pre-proceso a `<img>` sintético, caps 256/1 MiB/8 MiB, `inline_svgs`; 7 fixtures y 10 tests) y **event loop determinista** (sólo reloj virtual; fixture `js_event_timeout` md5 `07682088…`); 114 tests render. Pendientes HTTP2/streaming y SameSite/PSL. |
| **Fase 6: Integración JS & DOM Bindings** | `[████████░░] 80-85%` | *EN CURSO (WP-A/B/6.4 + eventos avanzados HECHOS)* | 58 tests `crates/js` + scripting E2E en `render` y en el parser | **Boa 0.21.1** embebido (feature `js` ON por defecto, stub `--no-default-features`) con `Budget`/`RuntimeLimits`, bindings DOM y scheduler base. **`render` ejecuta los `<script>`** (inline/`src`) post-parseo con caps/budgets, drena microtasks/timers, captura console y re-renderiza el DOM mutado (mutaciones visibles en el PNG; `--no-scripting`); 7 fixtures `js_*`. **WP-A HECHO**: `EventTarget` con captura+burbuja, `Event`/`CustomEvent` y timers con reloj inyectable (`setTimeout`/`setInterval`, `advance`/`run_until`/`now_ms`/`pending_timers`). **WP-B HECHO** (`CSSStyleDeclaration`, `getComputedStyle` provider, `NodeList`, `matches`/`closest`, `insertAdjacentHTML/Text`, identidad de wrappers) y **WP-6.4 HECHO** (parser-inserted + `document.write` ⇒ árbol 3.904/3.904). Event loop presupuestado en `render` (reloj virtual, `DOMContentLoaded`/`load`, timers). **Eventos avanzados (ronda 12) HECHOS**: `currentTarget`/`eventPhase`, `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror`, `document.close()`, `estimated_heap_bytes()` y excepciones de listeners capturadas. Pendientes: `document.onerror`, módulos ES, memoria real, provider cableado al pipeline y event loop vivo en el shell. [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md). |
| **Fase 7: Ventana OS & Shell** | `[███████░░░] 70%` | *EN CURSO* | 100 tests en `crates/shell` | Ventana usable (`winit`+`softbuffer`, sin GPU) con scroll (cap 8192 CSS px), entrada, HiDPI, render en worker y `--screenshot` headless (640×400 `md5 7324e851…`). **Navegación real (WP-7.5)**: clic en enlaces (`Url::join`), historial (`NavigationState`: back/forward/replace/reload, cap 256), Alt+←/→, `r`, título dinámico (`<title>`+URL+% scroll) y errores de navegación; **hit regions nativos** (`RenderOutput.hit_regions` + `paint::find_link_at`, fallback layout+DOM), CLI `--cookies`/`--origin`, **zoom** (`Ctrl+=/-/0`, `--zoom` 0.5–3.0) y **auto-refresh opt-in** (`--refresh-ms`). Pendientes: formularios, pestañas, event loop vivo en el host y verificación real de ventana. |

---

## FASE 0: Tooling, Arena DOM y Arnés de Auto-Testeo en Bucle Cerrado

### `WP-0.1` — Inicialización del Workspace, Tooling de Especificaciones y Arena DOM Intrusiva
- **Duración estimada**: 1 día
- **Sección exacta de la Spec WHATWG**:
  - WHATWG DOM Living Standard §4.2 *Node tree* (`https://dom.spec.whatwg.org/#concept-node-tree`)
  - WHATWG DOM §4.4 *Interface Node* (`#interface-node`) y §4.5 *Interface Document* (`#interface-document`, `#concept-document-quirks`)
  - WHATWG HTML §4.12.3 *The `template` element* (`https://html.spec.whatwg.org/multipage/scripting.html#the-template-element`)
- **Archivo(s) de test que lo validan**:
  - Tests unitarios de invariantes de arena y mutaciones `O(1)` en `crates/dom/src/lib.rs`.
- **Crate y módulo responsable**:
  - `Cargo.toml`, `.cargo/config.toml`, `tools/whatwg_spec.py`, `tools/test_inspector.py`, `tools/dev.sh`
  - `crates/dom/src/lib.rs` (`NodeId(NonZeroU32)`, `Document`, `Node`, `NodeKind`, `ElementData`, `Attribute`, `Namespace`, `QuirksMode`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p dom` pasa al 100% verificando `size_of::<Option<NodeId>>() == 4`, `append_child`, `insert_before`, `detach`, `reparent_children`, coalescencia de nodos de texto (`append_text` / `insert_text_before`) y creación del `DocumentFragment` inerte para `<template>`.

---

### `WP-0.2` — Preprocesador del Flujo de Entrada (`InputStream`) y Detección de Errores de Código de Punto
- **Duración estimada**: 1 día
- **Sección exacta de la Spec WHATWG**:
  - WHATWG HTML §13.2.3.5 *Preprocessing the input stream* (`https://html.spec.whatwg.org/multipage/parsing.html#preprocessing-the-input-stream`)
  - WHATWG HTML §13.2.2 *Parse errors* (`#parse-error-surrogate-in-input-stream`, `#parse-error-noncharacter-in-input-stream`, `#parse-error-control-character-in-input-stream`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/unicodeChars.test`
  - `tests/html5lib-tests/tokenizer/unicodeCharsProblematic.test`
- **Crate y módulo responsable**:
  - `crates/html/src/input_stream.rs` (`preprocess_code_points`, `preprocess_str`, `is_noncharacter`, `is_surrogate`, `is_control_character_in_stream`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p html input_stream` pasa al 100%, normalizando secuencias `\r\n` y `\r` aislados a `\n` (`U+000A`) y registrando los errores de flujo de entrada sin descartar códigos de punto ni provocar pánicos ante sustitutos UTF-16 (`0xD800..=0xDFFF`).

---

### `WP-0.3` — Arnés en Bucle Cerrado para el Tokenizador (`tokenizer_harness`)
- **Duración estimada**: 1 día
- **Sección exacta de la Spec WHATWG**:
  - WHATWG HTML §13.2.5 *Tokenization* (`#tokenization`)
  - WHATWG HTML §13.2.9 *Coercing an HTML DOM into an infoset* (`#coercing-an-html-dom-into-an-infoset`)
- **Archivo(s) de test que lo validan**:
  - Los 14 archivos JSON en `tests/html5lib-tests/tokenizer/*.test` (6.824 objetos de test, 7.050 ejecuciones combinando `initialStates`).
- **Crate y módulo responsable**:
  - `crates/test_harness/src/tokenizer_harness.rs` (`load_tokenizer_test_file`, `decode_test_string`, `coalesce_normalized_tokens`, `run_tokenizer_suite_file`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p test_harness test_all_14_tokenizer_json_files_parse_cleanly_with_6824_cases` pasa al 100%, soportando las claves raíz `"tests"` y `"xmlViolationTests"`, desescapado hexadecimal `\uHHHH` cuando `"doubleEscaped": true`, ejecución multi-estado según `"initialStates"`, configuración de `"lastStartTag"`, coalescencia de tokens `Character` adyacentes y tuplas de 3 elementos `["ProcessingInstruction", target, data]`.

---

### `WP-0.4` — Arnés en Bucle Cerrado para Construcción del Árbol (`tree_harness`) y Serializador Canónico `#document`
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - WHATWG HTML §13.2.6 *Tree construction* (`#tree-construction`)
  - WHATWG HTML §13.4 *Parsing HTML fragments* (`#parsing-html-fragments`)
- **Archivo(s) de test que lo validan**:
  - Los 62 archivos `.dat` en `tests/html5lib-tests/tree-construction/*.dat` y sus 62 equivalentes JSON en `tests/html5lib-tests/tree-construction-json/*.json` (1.973 casos de prueba, incluyendo 206 casos con `#document-fragment`).
- **Crate y módulo responsable**:
  - `crates/html/src/serializer.rs` (`serialize_dom_for_test`)
  - `crates/test_harness/src/tree_harness.rs` (`parse_dat_str`, `load_tree_dat_file`, `load_tree_json_file`, `execute_tree_case`, `run_tree_suite_file`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p test_harness test_all_62_tree_construction_dat_and_json_files_parse_with_1973_cases` pasa al 100%, garantizando paridad exacta entre el parser `.dat` (que respeta nodos de texto multilínea con líneas en blanco `\n\n`) y los archivos `.json` exportados.

---

## FASE 1A: Tokenizador HTML5 — Máquina de 84 Estados y Referencias de Caracteres

### `WP-1.1` — Estados Fundamentales de Datos, Apertura/Cierre de Etiquetas y Nombre de Etiqueta
- **Duración estimada**: 1 día
- **Sección exacta de la Spec WHATWG**:
  - §13.2.5.1 *Data state* (`#data-state`)
  - §13.2.5.6 *Tag open state* (`#tag-open-state`)
  - §13.2.5.7 *End tag open state* (`#end-tag-open-state`)
  - §13.2.5.8 *Tag name state* (`#tag-name-state`)
  - §13.2.5.40 *Self-closing start tag state* (`#self-closing-start-tag-state`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/test1.test`
  - `tests/html5lib-tests/tokenizer/test2.test`
- **Crate y módulo responsable**:
  - `crates/html/src/tokenizer.rs` (`TokenizerState::{Data, TagOpen, EndTagOpen, TagName, SelfClosingStartTag}`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p html test_tokenize_basic_tags_and_attributes` y `cargo test -p test_harness test_closed_loop_tokenizer_all_13_standard_suites_100_percent_green` pasan al 100%.

---

### `WP-1.2` — Estados de Atributos (Con Comillas Dobles, Simples, Sin Comillas y Deduplicación)
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.5.32 *Before attribute name state* (`#before-attribute-name-state`)
  - §13.2.5.33 *Attribute name state* (`#attribute-name-state`) — incluye regla de deduplicación que conserva la primera ocurrencia y registra `duplicate-attribute`
  - §13.2.5.34 *After attribute name state* (`#after-attribute-name-state`)
  - §13.2.5.35 *Before attribute value state* (`#before-attribute-value-state`)
  - §13.2.5.36 *Attribute value (double-quoted) state* (`#attribute-value-(double-quoted)-state`)
  - §13.2.5.37 *Attribute value (single-quoted) state* (`#attribute-value-(single-quoted)-state`)
  - §13.2.5.38 *Attribute value (unquoted) state* (`#attribute-value-(unquoted)-state`)
  - §13.2.5.39 *After attribute value (quoted) state* (`#after-attribute-value-(quoted)-state`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/test1.test`
  - `tests/html5lib-tests/tokenizer/test2.test`
  - `tests/html5lib-tests/tokenizer/test3.test`
  - `tests/html5lib-tests/tokenizer/test4.test`
- **Crate y módulo responsable**:
  - `crates/html/src/tokenizer.rs` (`PendingTag`, `TokenizerState::BeforeAttributeName`..`AfterAttributeValueQuoted`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- test3.test` reporta `1802 / 1802 state-runs passed`.

---

### `WP-1.3` — Declaraciones de Marcado, Comentarios HTML y Estados deInstrucciones de Procesamiento (PI)
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.5.41 *Bogus comment state* (`#bogus-comment-state`)
  - §13.2.5.42 *Markup declaration open state* (`#markup-declaration-open-state`)
  - §13.2.5.43 *Comment start state* (`#comment-start-state`)
  - §13.2.5.44 *Comment start dash state* (`#comment-start-dash-state`)
  - §13.2.5.45 *Comment state* (`#comment-state`)
  - §13.2.5.46 *Comment less-than sign state* (`#comment-less-than-sign-state`)
  - §13.2.5.47 *Comment less-than sign bang state* (`#comment-less-than-sign-bang-state`)
  - §13.2.5.48 *Comment less-than sign bang dash state* (`#comment-less-than-sign-bang-dash-state`)
  - §13.2.5.49 *Comment less-than sign bang dash dash state* (`#comment-less-than-sign-bang-dash-dash-state`)
  - §13.2.5.50 *Comment end dash state* (`#comment-end-dash-state`)
  - §13.2.5.51 *Comment end state* (`#comment-end-state`)
  - §13.2.5.52 *Comment end bang state* (`#comment-end-bang-state`)
  - §13.2.5.72 *Processing instruction open state* (`#processing-instruction-open-state`)
  - §13.2.5.73 *Processing instruction target state* (`#processing-instruction-target-state`)
  - §13.2.5.74 *After processing instruction target state* (`#after-processing-instruction-target-state`)
  - §13.2.5.75 *Processing instruction data state* (`#processing-instruction-data-state`)
  - §13.2.5.76 *Processing instruction questionable state* (`#processing-instruction-questionable-state`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/pendingSpecChanges.test`
  - `tests/html5lib-tests/tokenizer/test2.test`
  - `tests/html5lib-tests/tokenizer/test3.test`
  - `tests/html5lib-tests/tree-construction/comments01.dat`
  - `tests/html5lib-tests/tree-construction/processing-instructions.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tokenizer.rs` (`TokenizerState::BogusComment`..`CommentEndBang` y `ProcessingInstructionOpen`..`ProcessingInstructionQuestionable`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p html test_processing_instruction_states` y `cargo run -p test_harness -- test2.test` (`46 / 46 state-runs passed`).

---

### `WP-1.4` — Máquina de Estados DOCTYPE (`PUBLIC`, `SYSTEM` y `force-quirks`)
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.5.53 *DOCTYPE state* (`#doctype-state`)
  - §13.2.5.54 *Before DOCTYPE name state* (`#before-doctype-name-state`)
  - §13.2.5.55 *DOCTYPE name state* (`#doctype-name-state`)
  - §13.2.5.56 *After DOCTYPE name state* (`#after-doctype-name-state`)
  - §13.2.5.57 *After DOCTYPE public keyword state* (`#after-doctype-public-keyword-state`)
  - §13.2.5.58 *Before DOCTYPE public identifier state* (`#before-doctype-public-identifier-state`)
  - §13.2.5.59 *DOCTYPE public identifier (double-quoted) state* (`#doctype-public-identifier-(double-quoted)-state`)
  - §13.2.5.60 *DOCTYPE public identifier (single-quoted) state* (`#doctype-public-identifier-(single-quoted)-state`)
  - §13.2.5.61 *After DOCTYPE public identifier state* (`#after-doctype-public-identifier-state`)
  - §13.2.5.62 *Between DOCTYPE public and system identifiers state* (`#between-doctype-public-and-system-identifiers-state`)
  - §13.2.5.63 *After DOCTYPE system keyword state* (`#after-doctype-system-keyword-state`)
  - §13.2.5.64 *Before DOCTYPE system identifier state* (`#before-doctype-system-identifier-state`)
  - §13.2.5.65 *DOCTYPE system identifier (double-quoted) state* (`#doctype-system-identifier-(double-quoted)-state`)
  - §13.2.5.66 *DOCTYPE system identifier (single-quoted) state* (`#doctype-system-identifier-(single-quoted)-state`)
  - §13.2.5.67 *After DOCTYPE system identifier state* (`#after-doctype-system-identifier-state`)
  - §13.2.5.68 *Bogus DOCTYPE state* (`#bogus-doctype-state`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/test1.test` – `test4.test`
  - `tests/html5lib-tests/tree-construction/doctype01.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tokenizer.rs` (`PendingDoctype`, `TokenizerState::Doctype`..`BogusDoctype`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- doctype01.dat` reporta `74 / 74 mode-runs passed`.

---

### `WP-1.5` — Estados RCDATA, RAWTEXT, PLAINTEXT, CDATA Section y Verificación de `appropriate end tag token`
- **Duración estimada**: 1 día
- **Sección exacta de la Spec WHATWG**:
  - §13.2.5.2 *RCDATA state* (`#rcdata-state`)
  - §13.2.5.3 *RAWTEXT state* (`#rawtext-state`)
  - §13.2.5.5 *PLAINTEXT state* (`#plaintext-state`)
  - §13.2.5.9 *RCDATA less-than sign state* (`#rcdata-less-than-sign-state`)
  - §13.2.5.10 *RCDATA end tag open state* (`#rcdata-end-tag-open-state`)
  - §13.2.5.11 *RCDATA end tag name state* (`#rcdata-end-tag-name-state`)
  - §13.2.5.12 *RAWTEXT less-than sign state* (`#rawtext-less-than-sign-state`)
  - §13.2.5.13 *RAWTEXT end tag open state* (`#rawtext-end-tag-open-state`)
  - §13.2.5.14 *RAWTEXT end tag name state* (`#rawtext-end-tag-name-state`)
  - §13.2.5.69 *CDATA section state* (`#cdata-section-state`)
  - §13.2.5.70 *CDATA section bracket state* (`#cdata-section-bracket-state`)
  - §13.2.5.71 *CDATA section end state* (`#cdata-section-end-state`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/contentModelFlags.test`
  - `tests/html5lib-tests/tokenizer/domjs.test`
- **Crate y módulo responsable**:
  - `crates/html/src/tokenizer.rs` (`is_appropriate_end_tag`, `flush_temp_buffer_as_end_tag_fallback`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- contentModelFlags.test` reporta `24 / 24 state-runs passed`.

---

### `WP-1.6` — Estados de Datos de Script (`Script data`, `Escaped` y `Double-Escaped`)
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.5.4 *Script data state* (`#script-data-state`)
  - §13.2.5.15 *Script data less-than sign state* (`#script-data-less-than-sign-state`)
  - §13.2.5.16 *Script data end tag open state* (`#script-data-end-tag-open-state`)
  - §13.2.5.17 *Script data end tag name state* (`#script-data-end-tag-name-state`)
  - §13.2.5.18 *Script data escape start state* (`#script-data-escape-start-state`)
  - §13.2.5.19 *Script data escape start dash state* (`#script-data-escape-start-dash-state`)
  - §13.2.5.20 *Script data escaped state* (`#script-data-escaped-state`)
  - §13.2.5.21 *Script data escaped dash state* (`#script-data-escaped-dash-state`)
  - §13.2.5.22 *Script data escaped dash dash state* (`#script-data-escaped-dash-dash-state`)
  - §13.2.5.23 *Script data escaped less-than sign state* (`#script-data-escaped-less-than-sign-state`)
  - §13.2.5.24 *Script data escaped end tag open state* (`#script-data-escaped-end-tag-open-state`)
  - §13.2.5.25 *Script data escaped end tag name state* (`#script-data-escaped-end-tag-name-state`)
  - §13.2.5.26 *Script data double escape start state* (`#script-data-double-escape-start-state`)
  - §13.2.5.27 *Script data double escaped state* (`#script-data-double-escaped-state`)
  - §13.2.5.28 *Script data double escaped dash state* (`#script-data-double-escaped-dash-state`)
  - §13.2.5.29 *Script data double escaped dash dash state* (`#script-data-double-escaped-dash-dash-state`)
  - §13.2.5.30 *Script data double escaped less-than sign state* (`#script-data-double-escaped-less-than-sign-state`)
  - §13.2.5.31 *Script data double escape end state* (`#script-data-double-escape-end-state`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/escapeFlag.test`
  - `tests/html5lib-tests/tokenizer/domjs.test`
  - `tests/html5lib-tests/tree-construction/scriptdata01.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tokenizer.rs` (`TokenizerState::ScriptData`..`ScriptDataDoubleEscapeEnd`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- escapeFlag.test` (`9 / 9`) y `cargo run -p test_harness -- scriptdata01.dat` (`52 / 52`).

---

### `WP-1.7` — Referencias de Caracteres Numéricas (Decimal/Hex) y Con Nombre (2.231 Entidades Oficiales)
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.5.77 *Character reference state* (`#character-reference-state`)
  - §13.2.5.78 *Named character reference state* (`#named-character-reference-state`)
  - §13.2.5.79 *Ambiguous ampersand state* (`#ambiguous-ampersand-state`)
  - §13.2.5.80 *Numeric character reference state* (`#numeric-character-reference-state`)
  - §13.2.5.81 *Hexadecimal character reference start state* (`#hexadecimal-character-reference-start-state`)
  - §13.2.5.82 *Hexadecimal character reference state* (`#hexadecimal-character-reference-state`)
  - §13.2.5.83 *Decimal character reference state* (`#decimal-character-reference-state`)
  - §13.2.5.84 *Numeric character reference end state* (`#numeric-character-reference-end-state`)
  - §13.5 *Named character references* (`https://html.spec.whatwg.org/multipage/named-characters.html#named-character-references`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tokenizer/entities.test`
  - `tests/html5lib-tests/tokenizer/namedEntities.test`
  - `tests/html5lib-tests/tokenizer/numericEntities.test`
  - `tests/html5lib-tests/tree-construction/entities01.dat`
  - `tests/html5lib-tests/tree-construction/entities02.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/entities.rs` (`NAMED_ENTITIES`)
  - `crates/html/src/tokenizer.rs` (`NUMERIC_REPLACEMENT_TABLE`, `TokenizerState::CharacterReference`..`NumericCharacterReferenceEnd`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- namedEntities.test` (`4210 / 4210`), `numericEntities.test` (`336 / 336`), `entities.test` (`80 / 80`), `entities01.dat` (`150 / 150`) y `entities02.dat` (`52 / 52`).

---

## FASE 1B: Construcción del Árbol DOM (`TreeBuilder`) — 21 Modos de Inserción (COMPLETA — 3.904/3.904, 100 %)

> **Estado (ronda 11)**: gate del árbol **3.904/3.904 (100 %)**, 62 suites `html5lib-tests/tree-construction` incluidas las 4 `scripted_*` (`script-on`), **sin allowlist de tree**. La ronda 11 añade `WP-6.4` (hook `ScriptExecutor` del parser, scripts parser-inserted y `document.write` re-tokenizado en la posición actual) y corrige la regresión de rendimiento `tag-40k-attrs` (~12,4 ms).

### `WP-1.8` — Infraestructura del Estado de Parseo, Algoritmos de Alcance (*Scope*) y Modos Iniciales (`Initial` a `AfterHead`)
- **Duración estimada**: 2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.4.1 *The insertion mode* (`#the-insertion-mode`, `#reset-the-insertion-mode-appropriately`)
  - §13.2.4.2 *The stack of open elements* (`#the-stack-of-open-elements`, `#has-an-element-in-the-specific-scope`, `#has-an-element-in-scope`, `#has-an-element-in-list-item-scope`, `#has-an-element-in-button-scope`, `#has-an-element-in-table-scope`)
  - §13.2.6.4.1 *The "initial" insertion mode* (`#the-initial-insertion-mode`)
  - §13.2.6.4.2 *The "before html" insertion mode* (`#the-before-html-insertion-mode`)
  - §13.2.6.4.3 *The "before head" insertion mode* (`#the-before-head-insertion-mode`)
  - §13.2.6.4.4 *The "in head" insertion mode* (`#parsing-main-inhead`)
  - §13.2.6.4.5 *The "in head noscript" insertion mode* (`#parsing-main-inheadnoscript`)
  - §13.2.6.4.6 *The "after head" insertion mode* (`#the-after-head-insertion-mode`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/doctype01.dat`
  - `tests/html5lib-tests/tree-construction/noscript01.dat`
  - `tests/html5lib-tests/tree-construction/quirks01.dat`
  - `tests/html5lib-tests/tree-construction/comments01.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`TreeBuilder`, `InsertionMode::{Initial, BeforeHtml, BeforeHead, InHead, InHeadNoscript, AfterHead}`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- doctype01.dat` (`74/74`), `noscript01.dat` (`18/18`), `comments01.dat` (`32/32`) y `quirks01.dat` (`8/8`).

---

### `WP-1.9` — Modo `"in body"`: Elementos de Bloque, Encabezados, Listas (`li`, `dd`, `dt`), Cierre Implícito de `<p>` y Elementos Vacíos
- **Duración estimada**: 2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.6.3 *Closing elements that have implied end tags* (`#closing-elements-that-have-implied-end-tags`, `#generate-implied-end-tags`, `#generate-all-implied-end-tags-thoroughly`)
  - §13.2.6.4.7 *The "in body" insertion mode* (`#parsing-main-inbody`, `#close-a-p-element`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/inbody01.dat`
  - `tests/html5lib-tests/tree-construction/blocks.dat`
  - `tests/html5lib-tests/tree-construction/main-element.dat`
  - `tests/html5lib-tests/tree-construction/search-element.dat`
  - `tests/html5lib-tests/tree-construction/void-in-phrasing.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`process_in_body`, `close_p_element`, `generate_implied_end_tags`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- inbody01.dat` (`8/8`), `void-in-phrasing.dat` (`26/26`), `blocks.dat` (`96/96`), `main-element.dat` (`6/6`) y `search-element.dat` (`16/16`).

---

### `WP-1.10` — Lista de Elementos de Formato Activo, Cláusula del Arca de Noé y el *Adoption Agency Algorithm* (AAA)
- **Duración estimada**: 2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.4.3 *The list of active formatting elements* (`#the-list-of-active-formatting-elements`, `#push-onto-the-list-of-active-formatting-elements`, `#reconstruct-the-active-formatting-elements`, `#clear-the-list-of-active-formatting-elements-up-to-the-last-marker`)
  - §13.2.6.4.7 *The "in body" insertion mode — Adoption agency algorithm* (`#adoption-agency-algorithm`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/adoption01.dat`
  - `tests/html5lib-tests/tree-construction/adoption02.dat`
  - `tests/html5lib-tests/tree-construction/tricky01.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`FormattingEntry`, `push_active_formatting`, `reconstruct_active_formatting_elements`, `run_adoption_agency_algorithm`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- adoption01.dat` (`36/36`), `adoption02.dat` (`8/8`) y `tricky01.dat` (`18/18`).

---

### `WP-1.11` — Modos de Inserción de Tablas (`InTable` a `InCell`) y Algoritmo de *Foster Parenting*
- **Duración estimada**: 2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.6.1 *Creating and inserting nodes* (`#creating-and-inserting-nodes`, `#appropriate-place-for-inserting-a-node`, `#foster-parent`)
  - §13.2.6.4.9 *The "in table" insertion mode* (`#parsing-main-intable`, `#clear-the-stack-back-to-a-table-context`)
  - §13.2.6.4.10 *The "in table text" insertion mode* (`#parsing-main-intabletext`)
  - §13.2.6.4.11 *The "in caption" insertion mode* (`#parsing-main-incaption`)
  - §13.2.6.4.12 *The "in column group" insertion mode* (`#parsing-main-incolgroup`)
  - §13.2.6.4.13 *The "in table body" insertion mode* (`#parsing-main-intbody`, `#clear-the-stack-back-to-a-table-body-context`)
  - §13.2.6.4.14 *The "in row" insertion mode* (`#parsing-main-intr`, `#clear-the-stack-back-to-a-table-row-context`)
  - §13.2.6.4.15 *The "in cell" insertion mode* (`#parsing-main-intd`, `#close-the-cell`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/tables01.dat`
  - `tests/html5lib-tests/tree-construction/tests9.dat`
  - `tests/html5lib-tests/tree-construction/webkit01.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`appropriate_place_for_inserting_node`, `process_in_table`, `process_in_caption`, `process_in_column_group`, `process_in_table_body`, `process_in_row`, `process_in_cell`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p html test_foster_parenting_places_misnested_element_before_table` y `cargo run -p test_harness -- tables01.dat` (`38/38`).

---

### `WP-1.12` — Elementos de Formulario (`<form>`, `<select>`, `<option>`, `<optgroup>`, `<button>`) y Anotaciones `<ruby>`
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.4.4 *The element pointers* (`#the-element-pointers`, `#form-element-pointer`)
  - §13.2.6.4.7 *The "in body" insertion mode* (`#parsing-main-inbody` — reglas modernas para `form`, `button`, `select`, `option`, `optgroup`, `rb`, `rp`, `rt`, `rtc`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/ruby.dat`
  - `tests/html5lib-tests/tree-construction/isindex.dat`
  - `tests/html5lib-tests/tree-construction/menuitem-element.dat`
  - `tests/html5lib-tests/tree-construction/tests16.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`form_element`, manejo de `<ruby>` y `<select>` en `process_in_body`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- isindex.dat` (`8/8`), `menuitem-element.dat` (`40/40`) y `ruby.dat` (`42/42`).

---

### `WP-1.13` — Modo `"in template"`, Pila de Modos de Plantilla y Fragmentos Inertes `<template>`
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.4.1 *The stack of template insertion modes* (`#stack-of-template-insertion-modes`, `#current-template-insertion-mode`)
  - §13.2.6.4.16 *The "in template" insertion mode* (`#parsing-main-intemplate`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/template.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`template_modes`, `InsertionMode::InTemplate`)
  - `crates/html/src/serializer.rs` (serialización de `|   content`)
- **Criterio de aceptación verificable por test**:
  - `cargo test -p html test_template_contents_serialization` y `cargo run -p test_harness -- template.dat` (`254/254`).

---

### `WP-1.14` — Contenido Foráneo (MathML y SVG): Puntos de Integración, Normalización CamelCase y Salida a HTML
- **Duración estimada**: 2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.2.6 *Tree construction* (`#tree-construction-dispatcher`, `#mathml-text-integration-point`, `#html-integration-point`)
  - §13.2.6.1 *Adjusting MathML/SVG/Foreign attributes* (`#adjust-mathml-attributes`, `#adjust-svg-attributes`, `#adjust-foreign-attributes`)
  - §13.2.6.5 *The rules for parsing tokens in foreign content* (`#parsing-main-inforeign`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/svg.dat`
  - `tests/html5lib-tests/tree-construction/math.dat`
  - `tests/html5lib-tests/tree-construction/foreign-fragment.dat`
  - `tests/html5lib-tests/tree-construction/namespace-sensitivity.dat`
  - `tests/html5lib-tests/tree-construction/tests10.dat`
  - `tests/html5lib-tests/tree-construction/tests11.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (despachador de contenido foráneo, tablas de ajuste de 37 etiquetas SVG y 60 atributos SVG + 11 atributos `xlink`/`xml`/`xmlns`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- svg.dat` (`16/16`), `math.dat` (`16/16`), `namespace-sensitivity.dat` (`2/2`) y `foreign-fragment.dat` (`132/132`).

---

### `WP-1.15` — Modos Finales del Documento (`Text`, `AfterBody`, `InFrameset`, `AfterFrameset`, `AfterAfterBody`, `AfterAfterFrameset`)
- **Duración estimada**: 1 día
- **Sección exacta de la Spec WHATWG**:
  - §13.2.6.2 *Parsing elements that contain only text* (`#generic-raw-text-element-parsing-algorithm`, `#generic-rcdata-element-parsing-algorithm`)
  - §13.2.6.4.8 *The "text" insertion mode* (`#parsing-main-incdata`)
  - §13.2.6.4.17 *The "after body" insertion mode* (`#parsing-main-afterbody`)
  - §13.2.6.4.18 *The "in frameset" insertion mode* (`#parsing-main-inframeset`)
  - §13.2.6.4.19 *The "after frameset" insertion mode* (`#parsing-main-afterframeset`)
  - §13.2.6.4.20 *The "after after body" insertion mode* (`#the-after-after-body-insertion-mode`)
  - §13.2.6.4.21 *The "after after frameset" insertion mode* (`#the-after-after-frameset-insertion-mode`)
  - §13.2.7 *The end* (`#the-end`, `#stop-parsing`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/scriptdata01.dat`
  - `tests/html5lib-tests/tree-construction/tests1.dat` – `tests26.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`InsertionMode::{Text, AfterBody, InFrameset, AfterFrameset, AfterAfterBody, AfterAfterFrameset}`)
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- scriptdata01.dat` (`52/52`), `tests14.dat` (`14/14`) y `tests24.dat` (`16/16`).

---

### `WP-1.16` — Algoritmo de Parseo de Fragmentos HTML (`innerHTML` / `#document-fragment`) y Serialización HTML
- **Duración estimada**: 1–2 días
- **Sección exacta de la Spec WHATWG**:
  - §13.3 *Serializing HTML fragments* (`#serialising-html-fragments`, `#html-fragment-serialisation-algorithm`)
  - §13.4 *Parsing HTML fragments* (`#parsing-html-fragments`, `#html-fragment-parsing-algorithm`, `#fragment-case`)
- **Archivo(s) de test que lo validan**:
  - `tests/html5lib-tests/tree-construction/tests_innerHTML_1.dat`
  - `tests/html5lib-tests/tree-construction/tests4.dat`
  - `tests/html5lib-tests/tree-construction/foreign-fragment.dat`
- **Crate y módulo responsable**:
  - `crates/html/src/tree_builder.rs` (`parse_fragment`)
  - `crates/html/src/serializer.rs`
- **Criterio de aceptación verificable por test**:
  - `cargo run -p test_harness -- tests_innerHTML_1.dat` (`162/162`) y `tests4.dat` (`18/18`).

---

## FASE 4: Display Lists, Pipeline HTML → PNG y Tipografía (CERRADA funcionalmente)

**Estado ~100 %** para el objetivo «HTML real → PNG». Implementados: pipeline, tipografía real (**+ fuentes web `@font-face`/WOFF2 E2E desde la ronda 13**), bordes por lado, posicionamiento/floats/overflow, stacking/`opacity`, tablas CSS 2.1, `inline-block`, cajas reemplazadas, **imágenes `<img>` E2E**, PNG comprimido propio, DOM generacional y painter iterativo. **Crate `svg` propio (ronda 13, 41 tests) ya cableado E2E en `render`** para `<img src=*.svg>`, `background-image` **e inline `<svg>`** (pre-proceso a `<img>` sintético con caps 256/1 MiB/8 MiB, `svgs_rendered`/`inline_svgs`; 11+10 tests). **Ronda 15**: `background-repeat/size/position` reales en paint (tiling/cover/contain, cap 4096, legacy flag) y **IFC −28 %** con geometría byte-idéntica. `background-image`/`list-style-image` y el **texto de celdas de tabla** ya se pintan (render E2E). **Rendimiento (ronda 12)**: layout `-14,6 %` con `LayoutBox.style: Arc<ComputedStyle>` (LayoutBox 496→176 B) y encoder **PNG −30,5 % con bitstream byte a byte idéntico**; el cierre de ronda 13 arregla `html/fast_hash` ⇒ `html/massive-end-p` ~19 ms; **ronda 15**: IFC **−28 %** (layout 50,5→36,3 ms) y **pipeline 132,9→120,5 ms** con goldens byte-idénticos. Pendientes menores: `background-origin/clip`, múltiples capas, block-in-inline y colores de `border-collapse`.

| WP | Estado |
|---|---|
| `WP-4.0`–`WP-4.7` | **HECHO** |
| `WP-4.8` (tablas) | **HECHO** (avanzado pendiente) |
| `WP-4.9` (`inline-block`/listas/decoraciones/**imágenes**) | **HECHO** (`background-image` pendiente) |
| `WP-4.10` (`overflow`/clip) | **HECHO** |
| `WP-4.11` (AA/culling/compresión PNG) | **HECHO** |
| `WP-4.12` (robustez/DOM generacional) | **PARCIAL**: falta drop iterativo de `LayoutBox` en construcción profunda, presupuestos de layout y `max_nodes` vs `MAX_SLOTS` |
| `WP-4.13` (goldens/fixtures/matriz) | **PARCIAL** |

Resultados visuales: [`images/phase4-basic.png`](images/phase4-basic.png), [`images/render-fixture.png`](images/render-fixture.png), [`images/images-e2e.png`](images/images-e2e.png). Matriz: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md). Rendimiento: [`PERFORMANCE.md`](PERFORMANCE.md) (layout −43 % en la ronda 6).

---

## FASE 5: Red, Render Pipeline & Imágenes (EN CURSO, ~95%)

| WP | Objetivo | Estado |
|---|---|---|
| `WP-5.0`–`WP-5.4` | URL/HTTP1.1/TLS/caché/`data:`/file/MIME/loader (`net`) | **HECHO** (180 tests con cookies/CORS/gzip) |
| `WP-5.5` | Decoder PNG + inflate propio (`image`) | **HECHO** (45 tests + 1 doctest) |
| `WP-5.6` | Pipeline de navegador URL→PNG con CSS externo (`render`) | **HECHO** (114 tests en ronda 15, incluye scripting/red avanzada/event loop determinista/webfonts/SVG inline) |
| `WP-5.7` | **Imágenes `<img>` E2E** (fetch/decode/tamaño intrínseco/pintado/caps/placeholder) | **HECHO** (fixture 15.625 B `md5 1de31699…`) |
| `WP-5.8` | `@import` + media queries en CSS / integración en `render` | **HECHO**: parser/evaluador + integración recursiva en `render` (dedup/ciclos/profundidad 8/media real) |
| `WP-5.9` | Cookies RFC 6265, CORS básico y gzip/deflate con anti-bomba | **HECHO** (parcial: SameSite/PSL, CORS completo y HTTP/2/streaming pendientes) |
| `WP-5.10` | Backgrounds E2E (`background-image`/`list-style-image`) en `render` | **HECHO** (ronda 8; caps 32/8 MiB/16 MiB) |
| `WP-5.11` | Red avanzada E2E en el pipeline: cookies (jar por render o sesión), CORS/credentials con denegación no fatal, gzip/deflate en HTML/CSS; `--cookies`/`--origin` y stats (`cookies_stored`/`content_encoding`) | **HECHO** (ronda 10; render 114 tests tras ronda 15) |

Criterio global: `cargo test -p net -p image -p render` + `cargo run -p render -- tests/fixtures/render/images.html /tmp/images.png` + `bash tools/render_fixture.sh`.

---

## FASE 6: Integración JS & DOM Bindings (EN CURSO — WP-A/B/6.4 + eventos avanzados HECHOS, 2026-11-10)

**Estado**: **WP-6.1 HECHO** (`crates/js` con **Boa 0.21.1**, feature `js` ON por defecto), **WP-6.2 lado host HECHO** (`render` ejecuta `<script>` inline/`src` con `ScriptingMode`/`ScriptingLimits`, caps y re-render del DOM mutado; 7 fixtures `js_*`), **WP-A HECHO** (ronda 10: `EventTarget` captura+burbuja, `Event`/`CustomEvent`, timers con reloj inyectable y `document` como target), **WP-B HECHO** (ronda 11: `CSSStyleDeclaration` completa —`cssText`/`length`/`item`/`getPropertyValue`/`setProperty`/`removeProperty`—, `getComputedStyle` con provider inyectable, `NodeList` snapshot, `matches`/`closest`, `insertAdjacentHTML/Text` e **identidad de wrappers**) y **WP-6.4 HECHO** (ronda 11: hook `html::scripting::{ScriptExecutor, ScriptExecution}`; scripts parser-inserted al `</script>`/EOF; `document.write` re-tokenizado con `Tokenizer::insert_input` en la posición actual, anidados incluidos; externos `src` los ejecuta el host; errores no fatales; camino sin scripting zero-cost ⇒ **árbol 3.904/3.904 y allowlist de tree vacía**). **Event loop presupuestado y determinista en `render`** (ronda 15, `b808d7b`): sólo reloj virtual (`run_until` en pasos de 1 ms, sin reloj de pared), `DOMContentLoaded`/`load`, timers con delay (`max_tasks` 10.000, `max_virtual_time` 1000), un solo re-render, stats `script_*`/`timings.event_loop`; 6 fixtures `js_event_*` y 11 tests (fixture `js_event_timeout` md5 `07682088…`). **Eventos avanzados (ronda 12) HECHO**: `currentTarget` real + `eventPhase` 1/2/3 (capture en el target), `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror` (elemento/document/window), `document.close()` no-op, `estimated_heap_bytes()` aproximado, dispatch no-`Event` ⇒ `TypeError` y excepciones de listeners capturadas (`last_event_error()`) sin abortar el resto. **58 tests js** y **990 tests** de workspace; **Fase 6 ≈80-85 %**.

**Decisiones registradas** (detalle y racional en [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md)):
1. **Motor = Boa `boa_engine` 0.21.x** (pure Rust, MSRV Rust 1.88, licencia Unlicense OR MIT; se elige MIT). Se descartan motor propio, QuickJS/rquickjs (queda como plan de migración), V8/SpiderMonkey y el stub.
2. **Feature `js` ENCENDIDA POR DEFECTO**: build y CI compilan con JS; se mantiene un job **`cargo test --workspace --no-default-features`** para el build mínimo std-only, y el job MSRV cubre ambas configuraciones.
3. **Alcance inicial COMPLETO**: `document.write` + bindings DOM + eventos + timers + CSSOM + `querySelector` (WP-6.1–6.7, **≈25–30 días-persona**).

**Implementado (WP-6.1 + WP-6.2 host + WP-A + WP-B + WP-6.4 + eventos avanzados)**: realm Boa persistente + `JsRuntime::with_document`; handles laterales por `NodeId` con comprobación de generación (`NodeId` stale ⇒ `TypeError`); `document.write`, búsquedas, `querySelector(All)`, creación/mutación de nodos, `innerHTML`/`outerHTML`, atributos, `style`/`classList`; **eventos** `EventTarget`/`Event`/`CustomEvent` con captura y burbuja en nodos/document/window; **timers** en el scheduler Rust con reloj inyectable (`advance`/`run_until`/`now_ms`/`pending_timers`, `run_tasks` solo vencidos por reloj real); `Budget`/`RuntimeLimits` anti-DoS y sin red/FS; **host en `render`**: `ScriptingMode`, `RenderOptions::{scripting, scripting_limits}`, `ScriptingLimits`/`ScriptBudget::SMALL`, ejecución en orden con budget por script+total, stats (`scripts_executed/failed/bytes/console`, `timings.scripting`) y re-render del DOM mutado. Evidencia: `js_text.html` con scripting 15.876 B `md5 e1da51d4…` vs `--no-scripting` 15.499 B `md5 53f67e77…`; showcase golden intacto `c95c7eff…`. **WP-B**: `CSSStyleDeclaration` completa, `getComputedStyle` con provider inyectable, `NodeList`, `matches`/`closest`, `insertAdjacentHTML`/`Text`, caché de identidad wrapper↔nodo. **WP-6.4 y event loop**: el parser ejecuta scripts parser-inserted (`ScriptExecutor` + re-tokenización de `document.write`) y `render` corre el event loop presupuestado. **Eventos avanzados (ronda 12)**: `currentTarget`/`eventPhase` reales, `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror`, `document.close()`, `estimated_heap_bytes()` y excepciones de listeners capturadas. Límites vigentes: `document.onerror` para errores de scripts pendiente, sin módulos ES, sin contabilidad de memoria real, provider de computed styles aún no cableado al pipeline y event loop no vivo en el shell.

**Siguientes WP**: `document.onerror` para errores de scripts; módulos ES y `async`/`defer` externos; contabilidad de memoria real y aislamiento de realms; cablear el provider de `getComputedStyle` al pipeline (cascada real) y event loop vivo en el shell; endurecimiento/fuzz de `js`/`render`, bench E2E con JS y caché de Boa en CI (+25 s de build). Total **≈25–30 días-persona**.

**Qué NO se hace**: motor JS propio, delegar parseo HTML/CSS/layout/paint a JS, JIT, exponer `fetch`/filesystem por defecto ni entrar en el build mínimo.

## FASE 7: Ventana del Sistema & Shell (EN CURSO, ~70%)

| WP | Objetivo | Estado |
|---|---|---|
| `WP-7.1` | Ventana nativa `winit` 0.30 + presentación `softbuffer` 0.4 (X11/Wayland dlopen, sin GPU) | **HECHO** |
| `WP-7.2` | Modo headless `--screenshot` (sin `EventLoop`, CI-friendly) + CLI `megi` + `tools/megi.sh` | **HECHO** (640×400 `md5 7324e851…`) |
| `WP-7.3` | Ventana usable: render en worker (generación/cancelación/coalescing), resize, errores y título | **HECHO** |
| `WP-7.4` | Scroll vertical, inputs ratón/teclado y **HiDPI** | **HECHO** |
| `WP-7.5` | **Navegación real**: clic en enlaces (`Url::join`), historial (`NavigationState`: back/forward/replace/reload, cap 256, truncado forward), Alt+←/→, `r`, título dinámico (`<title>`+URL+% scroll+`[recortado]`), errores y redirects replace | **HECHO** (34 tests nuevos → 80) |
| `WP-7.5b` | Cablear `paint::find_link_at` en `render` (`RenderOutput.hit_regions`) y adoptarlo en el shell | **HECHO** (ronda 10; `RenderOutput.hit_regions` + `find_link_at`, fallback layout+DOM; píxeles sin cambios, +0,6 µs) |
| `WP-7.7` | **Zoom** (`Ctrl+=/-/0`, `--zoom` 0.5–3.0, escalones 50–300 %, viewport CSS = físico/(scale·zoom)), **auto-refresh opt-in** (`--refresh-ms` 250 ms, worker generacional, scroll conservado, sin bucles) y **título enriquecido** (zoom/scroll/warnings/errores) | **HECHO** (ronda 13; 83→100 tests; `--zoom 2` 320×200 `md5 c11903cf…`) |
| `WP-7.6` | Formularios, pestañas, accesibilidad mínima y pruebas de ventana reales en CI con Xvfb | **PENDIENTE** |

Limitaciones actuales: sin formularios/pestañas/descargas, fragmentos = nueva navegación, >8192 CSS px recortado, sin JS ni event loop vivo en el bucle del host (el pipeline `render` sí ejecuta el event loop; el auto-refresh repite el primer frame de animaciones con reloj virtual); ventana no verificada visualmente en CI (headless), pero compila y el modo headless, `NavigationState` la resolución de enlaces con hit regions, el zoom y el auto-refresh están testeados (100 tests).

### Fuera del alcance inicial de Fase 7

GPU/compositor, pestañas, extensiones, descargas, impresión, sandbox de procesos por sitio (se diseñará con la Fase 6) y accesibilidad completa.
