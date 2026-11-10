# Megi Web Engine 🌐🦀

[![Rust](https://img.shields.io/badge/Rust-Edition%202024%20(MSRV%201.88%2B)-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![CI](https://github.com/juanpimg/Megi/actions/workflows/ci.yml/badge.svg)](https://github.com/juanpimg/Megi/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-862%20passing-success.svg)]()
[![Fuzz nightly](https://github.com/juanpimg/Megi/actions/workflows/fuzz-nightly.yml/badge.svg)](https://github.com/juanpimg/Megi/actions/workflows/fuzz-nightly.yml)
[![Clippy](https://img.shields.io/badge/Clippy-0%20warnings%20(-D%20warnings)-brightgreen.svg)]()
[![HTML5 Conformance](https://img.shields.io/badge/HTML5-tokenizer%207050%2F7050%20·%20tree%203904%2F3904%20(100%25)%20·%20serializer%20230%2F230-success.svg)]()

> **Megi** es un motor de navegación web moderno, open source e independiente desarrollado desde cero en **Rust** (Edición 2024), sin bifurcar (*fork*) Blink, WebKit ni Gecko. Todo su diseño se rige estrictamente por las especificaciones formales vivas de **WHATWG** y **W3C**, con verificación continua en bucle cerrado (*closed-loop testing*) contra **Web Platform Tests (WPT)** y **`html5lib-tests`**.

---

## 📊 Estado del Proyecto y Barra de Progreso por Módulos

🎉 **HITO de conformidad HTML5 (ronda 11)**: el árbol DOM pasa a **3.904/3.904 (100 %)** — incluidas las 4 suites `scripted_*` — y el gate deja la **allowlist de tree vacía**. Fases 0–3 completadas. La **Fase 4 (paint + tipografía)** está **funcionalmente completa**: tablas avanzadas, imágenes `<img>`, **background-image/list-style-image pintados**, **texto de celdas de tabla corregido**, PNG comprimido y painter iterativo; pendientes menores `background-repeat/size/position`, block-in-inline y colores de `border-collapse`. La **Fase 5 (red/render)** está al **~95 %**: pipeline URL→PNG, imágenes + **backgrounds E2E**, TLS/caché/`data:`, `@import`/media, **cookies RFC 6265, CORS y gzip/deflate con anti-bomba ya integrados en el pipeline** (`--cookies`/`--origin`) y showcase con golden determinista; pendientes HTTP/2, streaming y SameSite/PSL. La **Fase 6 (JS)** está al **~75-80 %**: **WP-6.1**, **WP-6.2 host**, **WP-A**, **WP-B** y **WP-6.4 HECHOS** — `crates/js` ejecuta scripts, expone `EventTarget` (con `document` como target), `Event`/`CustomEvent`, timers con reloj inyectable y el DOM completo de WP-B (`CSSStyleDeclaration`, `getComputedStyle` con provider, `NodeList`, `matches`/`closest`, `insertAdjacentHTML/Text`, identidad de wrappers); el **parser** ejecuta scripts parser-inserted y re-tokeniza `document.write` en la posición actual (gate 3.904/3.904) y `render` corre un **event loop presupuestado** (reloj virtual, `DOMContentLoaded`/`load`, timers, un solo re-render). Pendientes: `onclick`/`once`/`passive`, módulos ES, memoria, cablear el provider de estilos al pipeline y event loop vivo en el shell. La **Fase 7 (shell)** está al **~65-70 %**: ventana usable con scroll, entrada, HiDPI, worker, **navegación real con historial/título** y **hit regions nativos** (`RenderOutput.hit_regions` + `paint::find_link_at`); pendientes formularios, pestañas, zoom, JS en el host y verificación de ventana real. **Rendimiento (ronda 11)**: regresión de `html/tag-40k-attrs` corregida (**~12,4 ms**, A/B siempre por debajo del baseline pre-r10) manteniendo las ganancias de la ronda 10; pipeline intercalado **~154-158 ms**. El detalle está en [`docs/PHASE4-SUPPORT.md`](docs/PHASE4-SUPPORT.md), [`docs/ROADMAP.md`](docs/ROADMAP.md) y [`docs/JAVASCRIPT-STRATEGY.md`](docs/JAVASCRIPT-STRATEGY.md).

| Módulo / Crate | Barra de Progreso | Estado | Tests | Capacidades Implementadas |
|---|:---:|:---:|---|---|
| [`crates/dom`](crates/dom) | `[██████████] 100%` | **COMPLETO** | 28 | Arena contigua con `NodeId` de 4 bytes (**20 bits slot + 12 bits generación**), free list LIFO con reutilización de slots, `remove_node`/`remove_subtree`, detección de handles stale/foráneos (`owns`, `try_node`), `descendants`/`ancestors` iterativos con guardas de ciclo, `validate_invariants()`, `estimate_bytes()/allocated()/free_slots()`; **API de navegador**: `document_element/head/body`, `get_element_by_id`, `get_elements_by_tag_name/class_name`, factorías de nodos, atributos, `text_content`, `remove_child`/`insert_before_child`/`replace_all_children`, `clone_node_into` y `DomError`. |
| [`crates/html`](crates/html) | `[██████████] 100%` | **COMPLETO** | 60 (14 unit + 7 scripting + 6 rendimiento + 13 robustez + 20 browser API) | Tokenizador de 84 estados (**7.050/7.050** en el gate), 21 modos de árbol (**3.904/3.904 — 100 %**, allowlist vacía), 2.231 entidades, AAA y *foster parenting*, `innerHTML`, **serializador público WHATWG HTML §13.3** (`serialize_html`, `serialize_fragment`, `serialize_outer_html`, `*_with_options`) con **230/230** casos de `html5lib-tests/serializer/`; límites anti-DoS `ParseOptions`, serializador iterativo y **DOM de navegador**: `parse_fragment_into`, `set_inner_html`/`set_outer_html`, `document_write` (contextos table/select/template/rawtext, sin fugas de slots). **WP-6.4**: hook `html::scripting::{ScriptExecutor, ScriptExecution}` + `TreeBuilder::with_script_executor` y `parse_document_with_options_and_executor`; los scripts parser-inserted se ejecutan al `</script>`/EOF y el `document.write` se **re-tokeniza en la posición actual** con `Tokenizer::insert_input` (scripts anidados incluidos); externos `src` los ejecuta el host y errores son no fatales (`ParseOutcome::script_errors`); camino sin scripting zero-cost. Incluye el fix de la regresión `tag-40k-attrs` (~12,4 ms). |
| [`crates/css`](crates/css) | `[██████████] 100%` | **COMPLETO (Fase 2+)** | 57 + 30 WPT | CSS Syntax L3, **Selectores L4 funcionales** (`:is/:where/:not/:has`) y estructurales (`:nth-of-type`, `:first/last/only-of-type`, `:nth-last-*`), especificidad correcta, expansión de shorthands antes de la cascada, `font` shorthand, `@media` básico (`resolve_document_styles_with_media`), índice de reglas (**2,1×** en resolve-styles con 10k nodos) y bases de Fase 4 (bordes por lado, `border-style: none`, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`), **tablas CSS 2.1** (`display: table-*`/`inline-table`/caption, `border-spacing`, `border-collapse`, `table-layout`, `caption-side`, `empty-cells`) y hoja UA §15 con reglas de tabla; **`@import`** (`Stylesheet.imports`) y **Media Queries** completas (`parse_media_query_list`, `media::evaluate`: and/or/not/only, listas, all/screen/print, width/height/min/max en px/em/rem/pt/pc/in/cm/mm, orientation). El `render` ya consume `@import` y media (ronda 7). Añade **query API** `css::query::{query_selector, query_selector_all, matches}` (+ variantes `try_*`) y parseo de `background-image`/`list-style-image` (`url(...)`, `none`, shorthand color+imagen; gradientes → `None`; aún no se pintan). |
| [`crates/layout`](crates/layout) | `[██████████] 100%` | **Fase 4 completa** | 133 | Box Tree con campos usados de posicionamiento (`position`, `z_index`, `opacity`, stacking context, `clip`, `is_float`); BFC con colapso de márgenes (§8.3.1) e ICB; **IFC** con saltos UAX #14, soft hyphen de ancho cero, `line-height` como factor, `vertical-align` §10.8.1 y `justify`; **`position: relative/absolute/fixed`** (containing blocks, posición estática, shrink-to-fit; `sticky` documentado como relative), **floats/`clear`** §9.5 (BFC contiene floats §10.6.7) y **`overflow`** con BFC y clip rect. **Tablas CSS 2.1 §17** (auto layout min/max-content, `border-spacing`, `colspan`, fixups anónimos, `vertical-align` de celdas, caption y row-groups), **`inline-block`** (shrink-to-fit, baseline, `vertical-align`) y **cajas reemplazadas** (`BoxType::Replaced`, tamaños intrínsecos `img` 300×150 / `input` 20×20 / `textarea` 160×40, API `layout_document_with_replaced_sizes`). **`LayoutBox::Drop` iterativo** (50.000 niveles, pila de 256 KiB) y **−43 %** en `layout/realistic-1mb` (100,6→57,2 ms) por `Arc<ComputedStyle>`, line breaking en streaming y una sola pasada de `format_line`. **Tablas avanzadas**: `rowspan`, `table-layout: fixed`, `inline-table` atómico y `border-collapse` con resolución de conflictos, con fast path para separate/auto/sin rowspan (`layout/realistic-1mb` ~59 ms). Modelo de celdas fijado por tests (`table > rowgroup > row > TableCell > LineBox > TextNode`). Pendiente: `background-repeat/size/position`, block-in-inline y colores de `border-collapse`. |
| [`crates/fonts`](crates/fonts) | `[█████████░] 90%` | **COMPLETO (base LTR)** | 13 + 1 doctest | Motor tipográfico real (`fontdb` + `ttf-parser` + `rustybuzz` + `ab_glyph`): descubrimiento/fallback, métricas OpenType, shaping LTR (kern/ligaduras), rasterización de glifos a máscaras alfa y fuente DejaVu determinista. **Integrado en layout y paint** (texto real en el PNG). Limitaciones documentadas: bidi/scripts complejos/fuentes variables fuera de alcance. |
| [`crates/paint`](crates/paint) | `[██████████] 100%` | **Fase 4 completa** | 108 | Display lists con **orden completo del Apéndice E** (stacking contexts y `z-index`), `OpacityGroup` (grupo offscreen compuesto una vez), `PushClip`/`PopClip` desde `overflow`, `text-decoration` (underline/overline/line-through), `visibility: hidden` y marcadores `disc`/`circle`/`square`/`decimal`; rasterizador con AA exacto, alfa straight, **culling** y cachés por pasada; bordes por lado y fondo bajo el *border box*; pipeline HTML → PNG con codificador PNG propio y golden determinista (`basic.rgba` `md5 60f8984e…`, PNG `md5 9e957177…`). **Compresión PNG propia**: DEFLATE con LZ77 (hash-chains 32 KiB + Huffman fijo) y zlib/Adler, filtros PNG adaptativos; PNG del fixture **1.920.813 → 42.853 B (44,8×)** con `basic.rgba` idéntico. `build_display_list` es **totalmente iterativo** (pila explícita; caps stacking 256/floats 64/items 5M). **Hit-testing aditivo** (`hit_test`/`hit_test_all`, `HitRegion` en orden de pintado con clip, `HitLink`/`find_link_at` y builders `build_display_list_with_hit_regions`; PNG/golden byte-idénticos, coste opt-in). **Imágenes E2E**: `ImageStore = HashMap<NodeId, Arc<image::RgbaImage>>`, `DisplayItem::Image` y `build_display_list_with_images`; raster **bilineal clamp-to-edge** con AA/alfa/clip y placeholder gris; **fix de underflow** de cobertura parcial en `fill_rect`; **background-image** (`BackgroundImages`, estirada al padding box, color debajo), **list-style-image** como marcador (~1em) y **fix del texto de celdas de tabla** (`paint_inline_content` ya desciende en cajas de tabla); 9 tests nuevos. |
| [`crates/js`](crates/js) | `[████████░░] 75%` | **Fase 6 (WP-6.1/6.2/6.4 + WP-A/WP-B HECHOS)** | 46 | **Boa 0.21.1** embebido (`crates/js`, `#![forbid(unsafe_code)]`, feature `js` **ON por defecto** con stub `--no-default-features`): `JsRuntime`/`JsRuntime::with_document` (eval persistente, console capture, `Budget`/`RuntimeLimits` anti-bucle, `JsError`), bindings DOM (`document.write`, búsquedas, `querySelector(All)`, creación/mutación de nodos, `innerHTML`/`outerHTML`, atributos, `style`/`classList`, wrappers con handle lateral y **NodeId stale ⇒ TypeError**), **eventos (WP-A)**: `EventTarget` (`addEventListener`/`removeEventListener`/`dispatchEvent`) en nodos/document/window con captura window→document→ancestros→target y burbuja de vuelta, `Event`/`CustomEvent` (`bubbles`/`cancelable`/`detail`/`defaultPrevented`, prevent/stop/stopImmediate) y **timers con reloj inyectable** (`setTimeout`/`setInterval`/clear, orden por `(due, id)`, `advance`, `run_until`, `now_ms`, `pending_timers`; `run_tasks` ejecuta vencidos por reloj real). **WP-A**: `document` como `EventTarget`. **WP-B**: `CSSStyleDeclaration` completa (`cssText`/`length`/`item`/`getPropertyValue`/`setProperty`/`removeProperty`), `getComputedStyle` con provider inyectable, `NodeList` snapshot, `matches`/`closest`, `insertAdjacentHTML`/`Text` y **caché de identidad wrapper↔nodo**. **APIs Rust**: `eval_on_document`, `dispatch_simple_event`, `take_document`. **Ya ejecutado desde `render` y desde el parser** (WP-6.4). Pendientes: `onclick`/`once`/`passive`, módulos ES, presupuesto de memoria, cablear el provider de computed styles al pipeline y event loop vivo en el shell. |
| [`crates/net`](crates/net) | `[████████░░] 90%` | **Fase 5** | 180 | URL WHATWG (subset `http`/`https`/`file` + `join`), HTTP/1.1 `GET` (timeouts, `Content-Length`, *chunked*, redirects con límite, caps), **TLS 1.2/1.3 con `rustls` 0.23 (ring) + `webpki-roots`** (feature `tls` por defecto, SNI y validación de certificado/host; `rcgen` solo en tests), **caché HTTP RFC 9111 subset** (`max-age`/`Expires`/`ETag`/`Last-Modified`/`304`/`no-store`/`Vary`), **`data:` URLs** con Base64 propio, `file://`, MIME sniffing y `ResourceLoader` con caché. `#![forbid(unsafe_code)]`; tests 100 % offline. El pipeline de navegador lo consume `crates/render`. **Añade**: cookies RFC 6265 subset (`CookieJar` con dominio/path/secure/httpOnly/expiry e integración en cada hop), **CORS básico** (Origin, ACAO exacto/*, credentials, preflight OPTIONS, `FetchError::Cors`) y **gzip/deflate** (`Accept-Encoding: gzip`, gzip wrapper RFC 1952 con CRC32/ISIZE, inflate compartido de `crates/image`, caps anti-bomba 64 MiB/ratio 1000:1). Pendientes: HTTP/2, CORS completo (SameSite/PSL no), streaming. |
| [`crates/render`](crates/render) | `[██████████] 100%` | **Fase 5 (pipeline de navegador)** | 88 | Pipeline **URL → PNG** de extremo a extremo: carga con `net` (`file`/`data`/HTTP/HTTPS con TLS y caché), decodifica charset (UTF-8/Windows-1252), respeta `<base>`, recolecta `<style>` + `<link rel=stylesheet>` externos (dedup y caps), resuelve la cascada UA+autor, hace layout con fuentes bundled, display list y **PNG determinista**; CLI `render <entrada> <salida.png> [--width/--height/--base/--no-network/--json]` y stats por etapa; **imágenes `<img>`**: carga por `net` (respeta `--no-network`), decodifica PNG (`crates/image`; otros formatos ⇒ warning + placeholder no fatal), inyecta tamaños intrínsecos y pinta (caps 64 imágenes/8 MiB/32 MiB). Fixture `images.html` 800×600 → 15.625 B `md5 1de31699…`. **`@import` recursivo** (resolución relativa, dedup/ciclos, profundidad 8 y caps) y **media real** en `<link>`/`<style>`/`@import`; **backgrounds E2E** (resolución relativa al documento, dedupe, caps 32/8 MiB/16 MiB, stats y warnings no fatales) y showcase 1024×768 → **80.220 B `md5 c95c7eff…`** con 80 display items y texto de tabla/background/list-style ya pintados. **Ejecución de JavaScript (WP-6.2 host)**: `ScriptingMode`/`RenderOptions.scripting` (default Enabled con feature `js`; `--no-scripting`), descubre `<script>` clásicos en orden (inline/`src`, módulos avisados y saltados, `nomodule` ignorado), carga externos con caps (8 MiB/script, 32 MiB total, 64 scripts) y `--no-network`, ejecuta con `JsRuntime::with_document` con budget por script+total, drena microtasks y hasta 10.000 timers, captura console y stats (`scripts_*`, `timings.scripting`), y **recalcula estilos→layout→paint sobre el DOM mutado** (mutaciones visibles en el PNG). 7 fixtures `js_*` y 12 tests. Límite: ejecución **solo post-parseo** (sin re-tokenización de `document.write` ⇒ WP-6.4). **Red avanzada E2E (ronda 10)**: `RenderOptions.cookies` (jar por render) o `cookie_jar` compartido de sesión propagando `Set-Cookie` entre documento/redirects/subrecursos/renders, `RenderOptions.origin`/`--origin` (o derivado del document URL) con validación CORS y credentials cuando hay jar, denegaciones CORS como warning no fatal, y gzip/deflate E2E (HTML y CSS gzip aplican estilos). `RenderStats` gana `hit_regions`/`cookies_stored`/`content_encoding`; CLI `--cookies`/`--origin`. **`RenderOutput.hit_regions`** se construye con `paint::build_display_list_with_hit_regions` (píxeles sin cambios, coste +0,6 µs). **Event loop presupuestado (ronda 11)**: reloj virtual, `DOMContentLoaded`/`load` despachados por el `EventTarget` del documento, timers con delay (`max_tasks` 10.000, `max_virtual_time` 1000 ms), un solo re-render tras drenar la cola y stats `script_*`/`timings.event_loop`; 6 fixtures `js_event_*` y 11 tests. |
| [`crates/image`](crates/image) | `[██████████] 100%` | **Soporte de imágenes (Fase 4/5)** | 45 + 1 doctest | Decodificador **PNG propio** (std puro, `#![forbid(unsafe_code)]`): inflate zlib/DEFLATE completo (stored/fixed/dynamic, ventana 32 KiB, Adler-32), `IHDR/PLTE/tRNS/IDAT/IEND` con CRC, color types 0/2/3/4/6, bit depth 1–16, filtros 0–4, Adam7 y límites (`PngLimits`); decodifica el PNG del pipeline y lo compara byte a byte con `basic.rgba`. GIF/JPEG/WebP solo detectados. |
| [`crates/bench`](crates/bench) | `[██████████] 100%` | **COMPLETO (tooling)** | 8 | Arnés de benchmarks reproducible sin dependencias (`std`), cargas sintéticas deterministas, grupos de parseo/estilos/layout/paint/PNG y baseline publicada en [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md). |
| [`crates/test_harness`](crates/test_harness) | `[██████████] 100%` | **COMPLETO** | 11 | Gate de conformidad con **4 suites**: tokenizer (**7.050/7.050**), árbol (**3.904/3.904 — 100 %**, `scripted_*` incluidos y **sin allowlist de tree**), **serializador HTML (230/230)** y CSS Syntax WPT vendorizado (30/30); `--summary` exit 0 y `--strict` exit 1 solo por las diferencias de parse-error documentadas. **WP-6.4**: `js_executor::JsScriptExecutor` sobre `JsRuntime::eval_on_document` con preludio/polyfills para ejecutar los scripts parser-inserted del corpus (3 tests unit + test de las 4 suites en `closed_loop`). |
| [`crates/shell`](crates/shell) | `[██████░░░░] 65%` | **Fase 7 (en curso)** | 83 | Ventana **usable** con **`winit` 0.30 + `softbuffer` 0.4** (X11/Wayland dlopen, sin GPU): **scroll vertical** (rueda/teclado/paginación, cap 8192 CSS px con marca `[recortado]`), **entrada** (ratón/teclado, clic izquierdo sigue enlaces con `Url::join`, Alt+Left/Backspace/b atrás, Alt+Right/n adelante, r recarga, q/Esc cierra), **HiDPI** (viewport CSS = physical/scale, nearest-neighbour), **render en worker** con generación/cancelación/coalescing (debounce 120 ms, `EventLoopProxy`), **navegación con historial** (`NavigationState` puro: back/forward/replace/reload, cap 256, truncado forward; fallos conservan la entrada) y **título dinámico** (`<title>` + URL corta + % scroll + `[recortado]`, errores «Megi — error: …») usando **`RenderOutput.hit_regions` + `paint::find_link_at`** (fallback layout+DOM documentado) y título del mismo render; fixture `file://` con enlaces relativos/absolutos; CLI `--cookies`/`--origin`; **`--screenshot` headless** determinista (640×400 `md5 7324e851…`) y `tools/megi.sh`. Pendientes: formularios, pestañas, zoom, JS en el bucle del host y eventos/timers del DOM en el host. |

**Total del workspace: 862 tests, 0 fallos.** Reparto: `bench` 8, `css` 57, `dom` 28, `fonts` 13 + 1 doctest, `html` 60, `image` 45 + 1 doctest, `js` 46, `layout` 133, `net` 180, `paint` 108, `render` 88, `shell` 83, `test_harness` 11. Todos los crates están marcados `#![forbid(unsafe_code)]`.

---

## 🖼️ Resultado actual: HTML → PNG

![Render real de Fase 4: HTML → PNG con tipografía, fondo propagado, bordes por lado, tarjeta con z-index y badge con opacity, viñetas y enlace subrayado](docs/images/phase4-basic.png)

*Render determinista 800×600 (26 display items) de [`tests/fixtures/phase4/basic.html`](tests/fixtures/phase4/basic.html), generado por `cargo run --release -p paint --example html_to_png`: tipografía real (DejaVu), fondo de canvas propagado (CSS 2.2 §14.2), **bordes por lado** (sólido + discontinuo), viñetas de lista, enlace subrayado y un *stage* con `position: relative`, `#under` (`z-index: 1`) y `#badge` (`z-index: 3`, `opacity: 0.7`) solapados. El PNG del pipeline es **comprimido por el codificador propio** (42.853 B tras DEFLATE, `md5 9e957177…`) y se valida con `tools/png_check.py`; la captura vive en [`docs/images/phase4-basic.png`](docs/images/phase4-basic.png).*

**Pipeline de navegador real (`crates/render`, Fase 5):**

![Render del fixture con hoja de estilos externa resuelta por <base> y canvas verde](docs/images/render-fixture.png)

*`cargo run -p render -- tests/fixtures/render/basic.html docs/images/render-fixture.png --width 640 --height 400`: el documento carga `sub/theme.css` como hoja externa a través de `<base href="sub/">` (canvas verde), ignora la hoja `media="print"` y aplica el `<style>` inline. 640×400, 15.498 B (`md5 7324e851…`), 4 display items, 1 hoja externa + 1 inline.*

**Imágenes `<img>` de extremo a extremo (`crates/paint` + `crates/image` + `crates/render`):**

![Render del fixture de imágenes: PNG escalado con muestreo bilineal y placeholder gris para una imagen ausente](docs/images/images-e2e.png)

*`cargo run -p render -- tests/fixtures/render/images.html docs/images/images-e2e.png`: un PNG de 4×4 (`assets/px.png`) escalado a 40×40 con **muestreo bilineal clamp-to-edge** y una imagen ausente que se sustituye por un **placeholder gris** con warning no fatal (stats `images_loaded: 1`, `images_failed: 1`). 800×600, 15.625 B, `md5 1de316995f56f7c72eefeb7e38a4e184`.*

**Showcase de navegador real (`crates/render`, 1024×768):**

![Showcase render: CSS externo con @import y media queries, imágenes PNG decodificadas, tabla, stage posicionado y listas](docs/images/showcase.png)

*`cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768`: página real sin JS que ejercita `file://`/HTTP, hoja externa + `@import` recursivo + media queries (la hoja `media="print"` se omite), dos PNG decodificados, tabla, elementos posicionados y listas. Render determinista **80.220 B**, `md5 c95c7eff83e7bd08f815631aa42afdb0`, **80 display items**; incluye texto de celdas de tabla, `background-image` en hero/stage y `list-style-image`. La captura versionada es el PNG del pipeline (80 KB).*

---

## 🏆 Hitos Principales Logrados

- [x] **🎉 HITO: conformidad HTML5 completa — árbol 3.904/3.904 (100 %)**: el corpus `html5lib-tests/tree-construction` pasa al 100 %, incluidas las 4 suites `scripted_*` (adoption01 1/1, ark 1/1, foster01 2/2, webkit01 2/2), y el gate queda **sin allowlist de tree** (`KNOWN_TREE_FAILURES=[]`, `TREE_EXPECTED_PASSED=3904`). Tokenizador **7.050/7.050**, **serializador HTML 230/230** (5 suites upstream) y CSS Syntax WPT **30/30**; `cargo run -p test_harness -- --summary` → **PASS, exit 0**; `--strict` → exit 1 solo por las diferencias de parse-error documentadas (order-only y `xmlViolation`).
- [x] **WP-6.4 — scripts parser-inserted y `document.write` re-tokenizado (ronda 11)**: `html::scripting::{ScriptExecutor, ScriptExecution}` + `TreeBuilder::with_script_executor`/`parse_document_with_options_and_executor`; el script se ejecuta al `</script>` (o EOF) con el DOM parcial visible, `document.write` se inserta en el flujo del tokenizer en la posición actual (scripts anidados incluidos, `line_starts`/UTF-16 ajustados), externos `src` los ejecuta el host y los errores son no fatales; sin scripting el parser es **zero-cost**. El harness implementa `js_executor::JsScriptExecutor` sobre `JsRuntime::eval_on_document` (preludio/polyfills, sin tocar `crates/js`) y el gate alcanza 3.904/3.904.
- [x] **Pipeline HTML → PNG con tipografía real y determinismo**: `crates/fonts` integrado en layout/paint (shaping + máscaras de glifo), bordes por lado (`solid`/`dashed`/`dotted`/`double`), antialiasing exacto, fondo bajo el *border box* y golden byte a byte regenerado (`basic.rgba` `md5 60f8984e…`; PNG `md5 9e957177…`, 26 display items).
- [x] **Tablas CSS 2.1, `inline-block` y cajas reemplazadas**: layout de tablas §17 (auto min/max-content, `border-spacing`, `colspan`, celdas/caption/row-groups), `inline-block` con shrink-to-fit y baseline, y `BoxType::Replaced` con tamaños intrínsecos (API `layout_document_with_replaced_sizes`).
- [x] **Compresión PNG propia (44,8×)**: DEFLATE LZ77+Huffman fijo con zlib/Adler, filtros adaptativos y **1.920.813 → 42.853 B** manteniendo `basic.rgba` bit a bit; `build_display_list` totalmente iterativo (20.000 niveles, caps documentados).
- [x] **Fase 6: JS en el pipeline, el parser y el event loop (WP-6.1/6.2/6.4 + WP-A/WP-B HECHOS)**: Boa 0.21.1 con feature `js` ON por defecto (stub `--no-default-features`), bindings DOM con handles laterales/generaciones y `Budget`/`RuntimeLimits` anti-DoS. `render` descubre y ejecuta los `<script>` (inline/`src`) con budgets y caps, y **recalcula estilos→layout→paint sobre el DOM mutado** (mutaciones visibles en el PNG; `--no-scripting`). **WP-B**: `CSSStyleDeclaration` completa, `getComputedStyle` con provider, `NodeList` snapshot, `matches`/`closest`, `insertAdjacentHTML`/`Text` e **identidad de wrappers**; **`document` como `EventTarget`** y APIs Rust `eval_on_document`/`dispatch_simple_event`/`take_document`. **Event loop en `render`**: reloj virtual, `DOMContentLoaded`/`load`, timers con delay (`max_tasks` 10.000, `max_virtual_time` 1000) y un solo re-render (6 fixtures `js_event_*`, 11 tests). **46 tests js y 88 render**; pendientes `onclick`/`once`/`passive`, módulos ES, memoria y cablear el provider de estilos/event loop vivo al shell.
- [x] **Red avanzada (Fase 5, E2E en la ronda 10)**: cookies RFC 6265 (jar por render o sesión, `--cookies`), CORS con `Origin`/credentials (`--origin`, denegación no fatal) y gzip/deflate con **anti-bomba** (64 MiB, ratio 1000:1) usando el inflate propio de `crates/image`; verificado en loopback con HTML/CSS gzip aplicando estilos.
- [x] **Rendimiento de parseo+layout (ronda 10)**: pipeline 800×600 **167,6 → 131,2 ms (−21,7 %)** y `html/realistic-1mb` **55,0 → 19,2 ms (−65 %)** por batching de caracteres in-body (`consume_data_run`, una inserción por run, preprocesado en una pasada, hasher interno, buffer de tokens reutilizado); layout 56,1→54,5 ms sin clones calientes; gate y golden intactos.
- [x] **Fix de la regresión del parser (ronda 11)**: `html/tag-40k-attrs` vuelve a **~12,4 ms** (dedupe de atributos con `std::HashSet`, `FastHasher` word-at-a-time solo en bookkeeping y guard de batching con `state == Data` primero), **A/B siempre por debajo del baseline pre-r10** y conservando las ganancias de la ronda 10 (pipeline intercalado ~154-158 ms). Perf suite de `html` verde.
- [x] **Backgrounds y tablas en paint**: `background-image`/`list-style-image` E2E (paint+render) y fix del **texto de celdas de tabla**, con golden nuevo del showcase.
- [x] **Showcase de navegador real (`render`, Fase 5/7)**: `tests/fixtures/render/showcase/` con CSS externo + `@import` recursivo + media queries + 2 PNG decodificados + tabla + stage posicionado → **1024×768, 80.220 B `md5 c95c7eff…`**, 80 display items, determinista (file:// y HTTP); captura en `docs/images/showcase.png`.
- [x] **Shell navegable (Fase 7, WP-7.5)**: scroll vertical (cap 8192 CSS px), entrada ratón/teclado, HiDPI y render off-thread; **clic en enlaces** (`Url::join`) con **historial** (`NavigationState`: back/forward/replace/reload, cap 256), Alt+←/Backspace/b y Alt+→/n, `r` recarga y **título dinámico** (`<title>` + URL + % scroll/[recortado]); `--screenshot` headless intacto; **hit regions nativos** (`RenderOutput.hit_regions` + `paint::find_link_at`) y CLI `--cookies`/`--origin`. 83 tests.
- [x] **Tablas CSS 2.1 avanzadas**: `rowspan`, `table-layout: fixed`, `inline-table`, `border-collapse` con resolución de conflictos y fast path (layout/realistic-1mb ~59 ms).
- [x] **CSS query API + `background-image` parsing**: `css::query::{query_selector, query_selector_all, matches}` y campos `background_image`/`list_style_image` en `ComputedStyle` (pintado pendiente).
- [x] **Hit-testing en paint (ronda 9)**: módulo `hit_test` con `HitKind`, `HitRegion { rect, node_id, kind }` en orden de pintado con clip aplicado, `hit_test`/`hit_test_all` (topmost gana), `HitLink`/`find_link_at` (sube a `<a href>` HTML) y builders aditivos `build_display_list_with_hit_regions`/`build_display_list_and_hit_regions` (entradas clásicas byte-idénticas; coste opt-in). 11 tests nuevos (108); en la ronda 10 `render` los construye en `RenderOutput.hit_regions` (+0,6 µs) y el shell los usa con `find_link_at`.
- [x] **ADR-001 de JavaScript (Fase 6) ACEPTADO (2026-11-10)**: motor **Boa 0.21.x**, feature `js` **ON por defecto** con job CI mínimo `--no-default-features` y alcance completo; **WP-6.1, WP-6.2 (lado host), WP-A, WP-B y WP-6.4 HECHOS** (46 tests js). Detalle en [`docs/JAVASCRIPT-STRATEGY.md`](docs/JAVASCRIPT-STRATEGY.md).
- [x] **Imágenes `<img>` de extremo a extremo**: `paint` con `ImageStore`/`DisplayItem::Image` y rasterización bilineal clamp-to-edge con AA/alfa/clip, `render` cargando/decodificando PNG con caps y placeholders no fatales; fixture determinista 800×600 15.625 B (`md5 1de31699…`).
- [x] **Fase 7 arrancada (`crates/shell`)**: ventana winit+softbuffer y **modo headless `--screenshot`** determinista (640×400 `md5 7324e851…`), CLI `megi` + `tools/megi.sh`.
- [x] **CSS `@import` + Media Queries completas** y **DOM de navegador**: `innerHTML`/`outerHTML`/`document.write`, búsquedas por id/tag/clase, manipulación de hijos/atributos (`DomError`).
- [x] **Layout −43 %** (`layout/realistic-1mb` 100,6→57,2 ms) con `Arc<ComputedStyle>`, line breaking en streaming y `format_line` de una pasada; `LayoutBox::Drop` iterativo a 50.000 niveles.
- [x] **Pipeline de navegador real (`crates/render` + `crates/image`) con ejecución de scripts**: `URL → net → HTML → CSS externo → layout → PNG` con CLI y stats por etapa; decodificador PNG/inflate propio (`crates/image`) que valida el PNG del motor contra el golden RGBA; el render ejecuta los `<script>` del documento y re-renderiza el DOM mutado (ver hito de Fase 6). **DOM generacional** (slot+generación, free list, handles stale/foráneos rechazados, `validate_invariants`).
- [x] **Pintado CSS 2.2 Apéndice E completo (caso in-flow)**: stacking contexts y `z-index` (positivo/negativo/auto), `opacity` compuesta como grupo offscreen, `PushClip`/`PopClip` desde `overflow`, `visibility: hidden`, `text-decoration` y marcadores de lista; posicionamiento `relative`/`absolute`/`fixed`, floats/`clear` y BFC que contiene floats en layout.
- [x] **Serializador HTML público (WHATWG HTML §13.3)**: `html::serialize_html`, `serialize_fragment` (`innerHTML`), `serialize_outer_html` y variantes `*_with_options(HtmlSerializeOptions { scripting_enabled })`, con las 5 suites oficiales de serialización en el gate.
- [x] **Rendimiento medido y regresiones detectables**: tree builder de complejidad **lineal** (eliminada la CPU cuadrática), índice de reglas CSS (**2,1×** en resolve-styles), rasterizador con **culling** y cachés por pasada → raster 800×600 **1598,8 → 0,9 ms**, render-to-PNG **1628 → 7,7 ms** y pipeline completo **1842 → 170,7 ms** (**10,8×**). Metodología y baseline en [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md).
- [x] **Núcleo CSS/IFC de Fase 4**: Selectores L4 funcionales y estructurales, `@media` básico, `font` shorthand, `line-height` numérico heredado como factor, soft hyphen invisible de ancho cero (guion sólo al romper), `vertical-align: top/bottom` conforme a §10.8.1 y `text-align: justify`.
- [x] **Robustez anti-DoS**: parsear 100.000/200.000 `<div>` anidados ya **no aborta** (límite de profundidad 4.096 reportado en `LimitViolations`), 40.000 atributos en un start-tag bajan de ~1.276 ms a ~10 ms; serializador iterativo y DOM con `try_node`/validación de ciclos.
- [x] **Fase 5 en marcha (`crates/net`)**: **TLS 1.2/1.3 con `rustls` 0.23 (ring) + `webpki-roots`** (SNI y validación; `rcgen` solo en tests), **caché HTTP RFC 9111 subset**, **`data:` URLs** con Base64 propio, URL/HTTP1.1/`file://`/MIME/`ResourceLoader` y **180 tests offline**; pendientes HTTP/2, CORS completo (SameSite/PSL) y streaming.
- [x] **Fuzzing continuo**: `fuzz/` (workspace aparte con `cargo-fuzz`, 4 targets y corpus semilla versionado) + `.github/workflows/fuzz-nightly.yml` (cron diario); campaña local documentada de ~1,06 M de ejecuciones ASan con **0 crashes**. Ver [`docs/FUZZING.md`](docs/FUZZING.md).
- [x] **Infraestructura de proyecto**: CI (`.github/workflows/ci.yml`: fmt, clippy `-D warnings`, tests, doc, gate de conformidad y job MSRV) + **fuzz nocturno** (`fuzz-nightly.yml`), licencias MIT/Apache, `rust-version = "1.88"` verificado, `#![forbid(unsafe_code)]` en todos los crates y `./tools/dev.sh all` como puerta única de calidad.

---

## 🏗️ Arquitectura del Espacio de Trabajo (DAG)

Megi está diseñado como un **Grafo Acíclico Dirigido (DAG)** sin dependencias circulares ni cajas negras:

```mermaid
graph TD
    dom["crates/dom<br/>(Arena DOM, NodeId 4B, O(1) mutations)"]
    html["crates/html<br/>(Tokenizer 84 estados, TreeBuilder 21 modos, Serializer §13.3)"]
    css["crates/css<br/>(CSS Syntax L3, Selectors L4, Cascade & Styles)"]
    layout["crates/layout<br/>(Box Tree, BFC/IFC, margin collapsing)"]
    fonts["crates/fonts<br/>(FontDatabase, shaping, glyph masks)"]
    paint["crates/paint<br/>(Display List, raster AA, HTML→PNG)"]
    net["crates/net (Fase 5)<br/>(URL, HTTP/1.1 + TLS, caché, data:, file://, MIME, loader)"]
    image["crates/image<br/>(PNG decoder + inflate propio)"]
    js["crates/js (Fase 6)<br/>(Boa runtime, DOM bindings)"]
    render["crates/render (Fase 5)<br/>(pipeline URL → PNG + scripts)"]
    shell["crates/shell (Fase 7)<br/>(ventana winit + screenshot headless)"]
    bench["crates/bench<br/>(benchmarks reproducibles)"]
    test_harness["crates/test_harness<br/>(Gate de conformidad: 4 suites)"]

    dom --> html
    dom --> css
    dom --> layout
    css --> layout
    layout --> paint
    fonts --> layout
    fonts --> paint
    dom --> net
    dom --> image
    net --> render
    html --> render
    css --> render
    layout --> render
    paint --> render
    fonts --> render
    dom --> render
    dom --> js
    html --> js
    css --> js
    js --> render
    render --> shell
    paint --> shell
    net --> shell
    html --> test_harness
    css --> test_harness
    layout --> bench
    paint --> bench
    fonts -.-> bench
```

`crates/net` no depende de crates internos; sus únicas dependencias externas son `rustls` + `webpki-roots` (§3.1). `crates/render` es el punto de integración (URL → PNG, con ejecución opcional de los `<script>` del documento vía `crates/js`), `crates/image` (std puro) decodifica PNG, `crates/js` embebe Boa para la Fase 6 (feature `js` ON por defecto) y `crates/shell` presenta el `Pixmap` en ventana o screenshot headless (winit/softbuffer, §3.1). `fuzz/` es un workspace **aparte** (herramienta de fuzzing, no forma parte del motor).

---

## 🚀 Inicio Rápido y Flujo de Trabajo

### Requisitos
- **Rust 1.88+ (Edition 2024)**; MSRV declarada en `Cargo.toml` y verificada en CI (`rustup toolchain install 1.88`).
- **Git** y **Python 3** (scripts de tooling: especificaciones, inspección de tests y `png_check.py`).

### Compilar y Verificar Todo el Workspace

```bash
./tools/dev.sh all
```

Ejecuta, en orden: `cargo fmt --check` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo test --workspace` (**862 tests**) → `cargo doc --workspace --no-deps` → **gate de conformidad** (`test_harness --summary`). Ruta de comprobación equivalente, paso a paso:

```bash
cargo +1.88 check --workspace --all-targets   # MSRV
cargo test --workspace                         # 862 tests (con JS por defecto)
cargo run -p test_harness -- --summary         # gate: PASS, exit 0
```

### Gate de Conformidad (4 suites)

```bash
# Tokenizer + árbol DOM + serializador HTML + CSS Syntax
# Esperado: 7050/7050 tokenizer, 3904/3904 árbol (100 %), 230/230 serializer, 30/30 CSS; exit 0
cargo run -p test_harness -- --summary

# Modo estricto: cualquier fallo (incluidos los de la allowlist) hace exit 1
cargo run -p test_harness -- --summary --strict

# Ejecutar una suite concreta
cargo run -p test_harness -- template.dat
cargo run -p test_harness -- test3.test
```

### Navegar de URL a PNG (pipeline de Fase 5)

```bash
# Documento + CSS externo resuelto por <base> (fixture offline)
cargo run -p render -- tests/fixtures/render/basic.html /tmp/render-fixture.png --width 640 --height 400 --json
python3 tools/png_check.py /tmp/render-fixture.png

# Equivalente con URL (requiere red/TLS para http(s); --no-network para offline)
cargo run -p render -- https://example.com/ /tmp/example.png
bash tools/render_fixture.sh /tmp/megi-render-fixture.png    # fixture + validación + md5

# Ejecución de JavaScript del documento (feature `js` ON por defecto; --no-scripting para desactivar)
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_on.png
cargo run -p render -- tests/fixtures/render/js_text.html /tmp/js_off.png --no-scripting
md5sum /tmp/js_on.png /tmp/js_off.png    # e1da51d4… (15.876 B) vs 53f67e77… (15.499 B)
# Otros fixtures: js_console.html, js_create.html, js_document_write.html, js_external.html, js_loop.html, js_throw.html

# Red avanzada en el pipeline: cookies y CORS (flag --cookies / --origin)
cargo run -p render -- https://example.com/ /tmp/page.png --cookies --origin https://example.com --json

# Eventos/timers y event loop presupuestado (WP-A/WP-B; reloj virtual en tests)
cargo test -p js                                   # 46 tests (WP-A/WP-B/identidad)
cargo run -p render -- tests/fixtures/render/js_event_timeout.html /tmp/timer.png --json
cargo run -p render -- tests/fixtures/render/js_event_domcontentloaded.html /tmp/dcl.png
cargo test -p render --test scripting_tests
cargo test -p render --test event_loop_tests
```

`crates/render` encadena `net` (file/data/HTTP/HTTPS con TLS y caché), `html`, `css` (cascada UA+autor), `layout` (fuentes bundled), `paint` y el codificador PNG. `crates/image` decodifica PNG con un inflate propio. Ver [`docs/PHASE4-SUPPORT.md`](docs/PHASE4-SUPPORT.md) y [`docs/TOOLING.md`](docs/TOOLING.md).

### Abrir una ventana usable o capturar un screenshot (Fase 7, `crates/shell`)

```bash
# Screenshot headless (CI-friendly, no necesita display)
cargo run -p shell -- tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png --width 640 --height 400
bash tools/megi.sh tests/fixtures/render/basic.html --screenshot /tmp/megi-shot.png

# Ventana nativa usable (requiere X11 o Wayland)
tools/megi.sh tests/fixtures/render/showcase/index.html --cookies --origin https://localhost
# controles: clic izq = seguir enlace (historial); Alt+←/Backspace/b = atrás; Alt+→/n = adelante;
#            rueda/↑↓/PageUp/PageDown/Home/End = scroll; r = recargar; q/Esc = salir
# título de ventana = <title> + URL + % scroll (+ [recortado] si >8192 CSS px)

# Showcase de navegador real (CSS externo + @import + media + imágenes)
cargo run -p render -- tests/fixtures/render/showcase/index.html docs/images/showcase.png --width 1024 --height 768
```

El binario `megi` presenta el `Pixmap` del pipeline como framebuffer `0RGB` con `softbuffer`; `--screenshot` usa el mismo pipeline sin `EventLoop`. La ventana no se ha verificado visualmente en este host (sin servidor gráfico); compila y el modo headless está testeado. Ver [`docs/TOOLING.md`](docs/TOOLING.md).

### Renderizar HTML a PNG (pipeline de Fase 4)

```bash
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png
python3 tools/png_check.py /tmp/basic.png
cargo test -p paint --test end_to_end_html
```

El ejemplo acepta `[width] [height]` (por defecto 800×600) y `--rgba <ruta>` para volcar la imagen RGBA canónica usada por los golden tests. Ver [`tests/fixtures/phase4/README.md`](tests/fixtures/phase4/README.md). El codificador PNG propio ya comprime (DEFLATE) y el decoder vive en `crates/image`; ver [`docs/PHASE4-SUPPORT.md`](docs/PHASE4-SUPPORT.md) y [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) §3.2.

### Rendimiento (benchmarks reproducibles)

```bash
./tools/bench.sh                    # suite completa en release -> target/bench-latest.txt
./tools/bench.sh --scale 0.25       # smoke test rápido
./tools/bench.sh --filter html      # un solo grupo
```

Metodología, cargas sintéticas, tabla de baseline y límites de medición en [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md).

### Fuzzing (tooling separado, nightly)

```bash
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
cd fuzz && cargo +nightly fuzz run html_parse -- -max_total_time=30
# targets: html_parse, css_parse, selectors, net_url
```

Guía completa (targets, corpus, triaje de crashes y CI nocturna): [`docs/FUZZING.md`](docs/FUZZING.md). El fuzzing es *tooling*: `cargo-fuzz`/`libfuzzer-sys` no son dependencias de los crates del motor.

### Motor Tipográfico (crate `fonts`)

```bash
cargo run -p fonts --example report    # métricas, shaping y glifo con DejaVu Sans @16px
cargo test -p fonts
```

### Motor JavaScript (Fase 6, `crates/js`)

```bash
cargo test -p js                                   # 46 tests (Boa 0.21.1, WP-A/WP-B)
cargo test -p js --no-default-features             # stub sin motor (build mínimo)

# JS del documento ejecutado de extremo a extremo en el pipeline de render (WP-6.2 host)
cargo run -p render -- tests/fixtures/render/js_create.html /tmp/js_create.png
cargo run -p render -- tests/fixtures/render/js_console.html /tmp/js_console.png --json
```

`render` descubre los `<script>` clásicos en orden (inline/`src`; módulos avisados y saltados), los ejecuta con `JsRuntime::with_document` bajo `ScriptingLimits`/`ScriptBudget` (budget por script y total), drena microtasks y hasta 10.000 timers, captura `console` en `stats` y **recalcula estilos→layout→paint sobre el DOM mutado**, de modo que las mutaciones aparecen en el PNG (`--no-scripting` desactiva todo). Límite actual: ejecución **solo post-parseo** (sin re-tokenización de `document.write` durante el parsing ⇒ WP-6.4 para los `scripted_*`), sin `async`/`defer` real, módulos ES ni eventos/event-loop con reloj. ADR y plan en [`docs/JAVASCRIPT-STRATEGY.md`](docs/JAVASCRIPT-STRATEGY.md).

### Consultar las Especificaciones Oficiales desde Terminal

```bash
python3 tools/whatwg_spec.py html "#data-state"
python3 tools/whatwg_spec.py html "#the-after-after-body-insertion-mode"
```

---

## 📖 Documentación del Proyecto

- 🏛️ [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): modelo de crates y DAG, arena DOM, política de dependencias externas (§3) y decisión de codificador PNG propio (§3.2).
- 🗺️ [`docs/ROADMAP.md`](docs/ROADMAP.md): hoja de ruta por fases y micro-paquetes `WP-*`, con **Fase 4 (`WP-4.0`…`WP-4.13`)** y el arranque de la **Fase 5**.
- 🧭 [`docs/PHASE4-SUPPORT.md`](docs/PHASE4-SUPPORT.md): matriz de soporte CSS 2.2 de la Fase 4 (implementado / parcial / pendiente).
- 📈 [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md): arnés `crates/bench`, metodología y baseline de rendimiento.
- 🛡️ [`docs/FUZZING.md`](docs/FUZZING.md): targets de fuzzing, corpus, workflow nocturno y política de triaje.
- 🔐 [`SECURITY.md`](SECURITY.md): modelo de seguridad, límites anti-DoS, TLS/trust roots y cómo reportar vulnerabilidades.
- 📦 [`docs/HANDOFF.md`](docs/HANDOFF.md): estado de la última ronda, comandos de verificación y deuda técnica priorizada.
- 🧠 [`docs/JAVASCRIPT-STRATEGY.md`](docs/JAVASCRIPT-STRATEGY.md): **ADR-001 de la Fase 6 (JS), ACEPTADO (2026-11-10)**: Boa 0.21.x, feature `js` ON por defecto, alcance completo, diseño de bindings DOM↔JS y plan WP-6.1–6.7 en implementación.
- 🛠️ [`docs/TOOLING.md`](docs/TOOLING.md): scripts CLI, fixtures vendorizadas, validación PNG, benchmarks y skills de agentes.
- 🤝 [`CONTRIBUTING.md`](CONTRIBUTING.md): filosofía, convenciones de código/commits, CI y flujo de PR.
- 📝 [`CHANGELOG.md`](CHANGELOG.md): historial de cambios por ronda/fase.

---

## 📄 Licencia

Megi se publica bajo doble licencia:

- **Licencia MIT** ([`LICENSE-MIT`](LICENSE-MIT))
- **Licencia Apache 2.0** ([`LICENSE-APACHE`](LICENSE-APACHE))

a tu elección. La fuente de test DejaVu Sans empaquetada en `crates/fonts/tests/fixtures/` conserva su propia licencia (ver `LICENSE-DejaVu.txt`).

---

> **Estado**: proyecto en desarrollo activo, sin releases publicadas. **Conformidad HTML5: árbol 3.904/3.904 (100 %)**; Fase 4 funcionalmente completa, Fase 5 ≈95 % (red avanzada E2E), Fase 6 ≈75-80 % (JS en parser+render+event loop, WP-A/B/6.4 HECHOS) y Fase 7 ≈65-70 % (shell navegable con historial y hit regions); consulta [`docs/PHASE4-SUPPORT.md`](docs/PHASE4-SUPPORT.md) y [`docs/ROADMAP.md`](docs/ROADMAP.md) antes de asumir soporte de una propiedad CSS o una capacidad de red.
