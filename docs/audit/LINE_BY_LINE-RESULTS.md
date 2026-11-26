# Resultados de la auditoría línea por línea (2026-11-26)

> Índice de hallazgos publicados. Plan: `docs/superpowers/plans/2026-11-26-line-by-line-audit-plan.md`.
> Estado: **auditoría completada en dos pasadas**; los hallazgos están como issues, no como cambios de código.

## Cobertura

- **18 crates** de `crates/`, **270.986 líneas Rust** leídas línea por línea por 8 subagentes.
- **80 hallazgos** confirmados con `archivo:línea`, norma e impacto.
- **79 issues creadas** (#2315–#2393); 1 duplicado omitido.
- Segunda pasada: `tools/*.py`, `fuzz/`, `.github/workflows/`, `tests/` y `docs/` leídos línea por línea por 5 subagentes: **61 hallazgos**, **61 issues** (#2394–#2454).
- Total: **140 issues** (#2315–#2454). Sin `cargo`, sin cambios de código del motor.

### Segunda pasada (herramientas, fuzz, CI, tests y docs)

| Informe | Hallazgos publicados | Rango |
|---|---:|---|
| `ci.md` | 12 | #2394–#2405 |
| `docs.md` | 12 | #2406–#2417 |
| `fuzz.md` | 11 | #2418–#2428 |
| `tests.md` | 11 | #2429–#2439 |
| `tools.md` | 15 | #2440–#2454 |

> Los rangos son orientativos; la fuente de verdad es `issues-created.json` de cada pasada.
> Los listados completos de la segunda pasada están en `/tmp/opencode/audit-line2/`.

## Desglose

| Informe | Hallazgos publicados |
|---|---:|
| `core-html.md` | 6 |
| `css.md` | 12 |
| `js.md` | 12 |
| `layout.md` | 5 |
| `net.md` | 11 |
| `paint.md` | 11 |
| `render.md` | 10 |
| `shell.md` | 12 |

| Esfuerzo | Hallazgos |
|---|---:|
| S | 39 |
| M | 36 |
| L | 4 |

| Hito | Hallazgos |
|---|---:|
| M0 | 1 |
| M1 | 12 |
| M2 | 13 |
| M3 | 9 |
| M4 | 7 |
| M5 | 17 |
| M6 | 4 |
| M7 | 3 |
| M8 | 3 |
| M9 | 10 |

## Issues por área

### `core-html.md`

- https://github.com/juanpimg/Megi/issues/2315 — `crates/html/src/tree_builder/dispatch.rs:300` — Script sin cierre se ejecuta en EOF
- https://github.com/juanpimg/Megi/issues/2316 — `crates/core/src/query/mutation.rs:57` — insertBefore(nodo, nodo) no reubica el nodo
- https://github.com/juanpimg/Megi/issues/2317 — `crates/core/src/node.rs:219` — Atributos con namespace invisibles para getAttribute/hasAttribute
- https://github.com/juanpimg/Megi/issues/2318 — `crates/html/src/serializer/fragment.rs:344` — El serializador HTML emite PUBLIC/SYSTEM del DOCTYPE
- https://github.com/juanpimg/Megi/issues/2319 — `crates/html/src/tree_builder/quirks.rs:34` — System ID vacío clasifica mal el modo quirks
- https://github.com/juanpimg/Megi/issues/2320 — `crates/core/src/query/text.rs:108` — textContent de <template> expone el contenido inerte

### `css.md`

- https://github.com/juanpimg/Megi/issues/2321 — `crates/css/src/cascade/chunk2.rs:232` — CSS Nesting se parsea pero nunca entra en la cascada (`child_rules` se descartan)
- https://github.com/juanpimg/Megi/issues/2322 — `crates/css/src/transform.rs:73` — `translate()` rechaza porcentajes: `transform: translate(-50%, -50%)` deja de aplicar
- https://github.com/juanpimg/Megi/issues/2323 — `crates/css/src/shorthand/chunk1.rs:66` — `all: unset` y las palabras clave CSS-wide `initial`/`unset` no se computan
- https://github.com/juanpimg/Megi/issues/2324 — `crates/css/src/parser/chunk2.rs:300` — `@import ... layer(...)` / `supports(...)` se descartan por completo
- https://github.com/juanpimg/Megi/issues/2325 — `crates/css/src/properties/values/length.rs:40` — Unidades `<length>` estándar ausentes: `cm`, `mm`, `in`, `pc`, `ch`, `ex`, `vmin`, `vmax`, `dvh`, `svh`, `lvh`
- https://github.com/juanpimg/Megi/issues/2326 — `crates/css/src/parser/chunk2.rs:196` — `@font-face { unicode-range: ... }` se ignora: las fuentes se aplican a todos los codepoints
- https://github.com/juanpimg/Megi/issues/2327 — `crates/css/src/properties/computed/chunk3.rs:326` — `overflow-x` / `overflow-y` no se computan; `overflow: x y` solo conserva el primer valor
- https://github.com/juanpimg/Megi/issues/2328 — `crates/css/src/media/chunk2.rs:130` — `(prefers-reduced-motion: no-preference)` evalúa siempre `false`
- https://github.com/juanpimg/Megi/issues/2329 — `crates/css/src/cascade/chunk2/supports.rs:174` — `@supports (<propiedad conocida>: <valor cualquiera>)` acepta valores inválidos o no implementados
- https://github.com/juanpimg/Megi/issues/2330 — `crates/css/src/animation_runtime/chunk1.rs:317` — `transform` no es animable ni transicionable (y las propiedades animables son una lista cerrada)
- https://github.com/juanpimg/Megi/issues/2331 — `crates/css/src/cascade/chunk6.rs:9` — Las animaciones de propiedades heredadas no se re-heredan en descendientes
- https://github.com/juanpimg/Megi/issues/2332 — `crates/css/src/properties/mask.rs:17` — `mask-image` solo acepta una URL; gradientes y listas multi-capa se descartan

### `js.md`

- https://github.com/juanpimg/Megi/issues/2333 — `crates/js/src/events/event.rs:39-82 (`build_event`), crates/js/src/events/event.rs:86-124 (`init_event_object`)` — `Event` no expone `initEvent()` y carece de `timeStamp`
- https://github.com/juanpimg/Megi/issues/2334 — `crates/js/src/events/event.rs:50-62 (`Attribute::all()` en `type`, `bubbles`, `cancelable`, `detail`, `target`, `currentTarget`, `defaultPrevented`, `eventPhase`)` — Atributos IDL de `Event` son escribibles por script
- https://github.com/juanpimg/Megi/issues/2335 — `crates/js/src/events/listeners.rs:13-19; crates/js/src/events/dispatch.rs:237-248` — `addEventListener` acepta listeners no invocables; falta `handleEvent`
- https://github.com/juanpimg/Megi/issues/2336 — `crates/js/src/events/timers.rs:22-26 (`.filter(|value| value.as_object().is_some())` + `js_error("timer callback must be a function")`)` — `setTimeout`/`setInterval` lanzan `TypeError` con handlers no función
- https://github.com/juanpimg/Megi/issues/2337 — `crates/js/src/runtime/document/parse.rs:146-157 (`wrap_list` en la línea 156; compárese `node_list` en la línea 143 para `getElementsByTagName`)` — `getElementsByClassName` devuelve un `Array` plano, no `NodeList`/`HTMLCollection`
- https://github.com/juanpimg/Megi/issues/2338 — `crates/js/src/runtime/dom_utils.rs:154-194 (la llamada en la línea 187 usa `&JsValue::undefined()` como receiver)` — `NodeList.forEach` ignora el argumento `thisArg`
- https://github.com/juanpimg/Megi/issues/2339 — `crates/js/src/runtime/history.rs:107-145 (`history_push_state`) y crates/js/src/runtime/history.rs:160-197 (`history_replace_state`)` — `history.pushState`/`replaceState` no validan same-origin
- https://github.com/juanpimg/Megi/issues/2340 — `crates/js/src/xhr/prelude_a.js:5-17; crates/js/src/runtime/service_worker/sw_prelude.js:7-13; referencias en crates/js/src/runtime/gap_apis/prelude_a.js:175,189,225` — `DOMException` es únicamente un shim de `Error`, sin interfaz nativa
- https://github.com/juanpimg/Megi/issues/2341 — `crates/js/src/runtime/document/query.rs:302-366 (`document_cookie_set`, store `cookies_fallback`)` — `document.cookie` (fallback) ignora `Path`/`Domain`/`Expires`/`Secure`/`SameSite`
- https://github.com/juanpimg/Megi/issues/2342 — `crates/js/src/runtime/gap_apis/canvas.rs:61 (`("strokeText", canvas_fill_text as NativeFn)`)` — `strokeText` está enlazado a `canvas_fill_text`
- https://github.com/juanpimg/Megi/issues/2343 — `crates/js/src/canvas/webgl.rs:220-224 (`if target != ARRAY_BUFFER || usage != STATIC_DRAW`); constantes en crates/js/src/canvas/webgl.rs:29-31` — `bufferData` rechaza `DYNAMIC_DRAW` y `STREAM_DRAW`
- https://github.com/juanpimg/Megi/issues/2344 — `crates/js/src/events/timers.rs:184 (`callback.call(&JsValue::undefined(), &arguments, context)`)` — Callbacks de timers/rAF se invocan con `this === undefined`

### `layout.md`

- https://github.com/juanpimg/Megi/issues/2345 — `crates/layout/src/flex/layout.rs:22` — `flex-wrap: wrap-reverse` no crea líneas: se trata como `nowrap`
- https://github.com/juanpimg/Megi/issues/2346 — `crates/layout/src/tree_builder/box_for_node.rs:215-226` — `display: contents` anidado genera una caja fantasma en vez de hoistear recursivamente
- https://github.com/juanpimg/Megi/issues/2347 — `crates/layout/src/grid/names.rs:102-115` — Un nombre de área grid sin sufijo `-start`/`-end` no resuelve en `grid-column-start`/`grid-row-start`
- https://github.com/juanpimg/Megi/issues/2348 — `crates/layout/src/block/absolute.rs:163-193` — Posicionamiento absoluto sobre-constraindo ignora `right` también en RTL (debería ignorar `left`)
- https://github.com/juanpimg/Megi/issues/2349 — `crates/layout/src/flex/helpers.rs:266-285` — Distribución `flex-grow` descuenta los mínimos automáticos de todos los items, no solo de los que los necesitan

### `net.md`

- https://github.com/juanpimg/Megi/issues/2350 — `crates/net/src/url/parsing.rs:178` — IPv6 hosts serialized in non-canonical form
- https://github.com/juanpimg/Megi/issues/2351 — `crates/net/src/cors.rs:159-168 (also used by http/preflight.rs:53-57)` — CORS-safelisted request headers ignore Content-Type value essence
- https://github.com/juanpimg/Megi/issues/2352 — `crates/net/src/referrer.rs:88-94` — Referrer-Policy header uses the first valid token instead of the last
- https://github.com/juanpimg/Megi/issues/2353 — `crates/net/src/h2/client/session_frames.rs:398` — HTTP/2 PUSH_PROMISE stream-ID validation is incomplete
- https://github.com/juanpimg/Megi/issues/2354 — `crates/net/src/h2/client/api.rs:286-287 (with session_core.rs:40 `next_stream_id = 1`)` — h2c upgrade opens stream 1 twice
- https://github.com/juanpimg/Megi/issues/2355 — `crates/net/src/websocket/client.rs:128,132,170` — WebSocket connect uses the bracketed host for DNS and TLS SNI
- https://github.com/juanpimg/Megi/issues/2356 — `crates/net/src/http/response.rs:180-187; crates/net/src/http/streaming.rs:197-209` — Transfer-Encoding codings before `chunked` are never decoded
- https://github.com/juanpimg/Megi/issues/2357 — `crates/net/src/h2/hpack/codec.rs:55-60` — HPACK decoder rejects legal non-UTF-8 field values
- https://github.com/juanpimg/Megi/issues/2358 — `crates/net/src/h2/client/api.rs:235-251` — `h2c_request` frames caller headers without CRLF validation
- https://github.com/juanpimg/Megi/issues/2359 — `crates/net/src/websocket/client.rs:207-210` — WebSocket handshake omits the `Origin` header
- https://github.com/juanpimg/Megi/issues/2360 — `crates/net/src/http/request.rs:314-316` — 303 redirect rewrites `HEAD` to `GET`

### `paint.md`

- https://github.com/juanpimg/Megi/issues/2361 — `crates/paint/src/painter/backgrounds.rs:133-143,178-212,215-333; crates/raster/src/rasterizer/text/effects.rs:135-140` — `background-clip: text` pierde la máscara al agotar el presupuesto y pinta el fondo opaco
- https://github.com/juanpimg/Megi/issues/2362 — `crates/svg/src/css/cascade.rs:14-23` — La cascada CSS de SVG usa la especificidad máxima de la lista de selectores, no la del selector que coincide
- https://github.com/juanpimg/Megi/issues/2363 — `crates/svg/src/css/parser.rs:100-117` — El combinador hijo `>` se trata como combinador descendiente en el CSS embebido de SVG
- https://github.com/juanpimg/Megi/issues/2364 — `crates/svg/src/raster/text.rs:101-111,190-203; crates/svg/src/text.rs:81-90` — `text-anchor` de SVG se calcula con medidas sin shaping pero se pintan glifos con avances reales
- https://github.com/juanpimg/Megi/issues/2365 — `crates/paint/src/painter/text.rs:66-72,118-159; crates/raster/src/rasterizer/text/cache.rs:224-256` — Avance uniforme en las display items de texto: `letter-spacing`/`word-spacing` se distribuyen como `ancho / chars`
- https://github.com/juanpimg/Megi/issues/2366 — `crates/image/src/jpeg/store.rs:75-100` — Submuestreo de croma nearest-neighbour en JPEG 4:2:0/4:2:2
- https://github.com/juanpimg/Megi/issues/2367 — `crates/image/src/vp8_decoder/loop_filter.rs:74-82` — Filtro de bucle simple de VP8 sólo procesa luma
- https://github.com/juanpimg/Megi/issues/2368 — `crates/image/src/webp/mod.rs:27; crates/image/src/webp/container.rs:170-176` — WebP con ALPH comprimido (método 1) se rechaza: imagen rota
- https://github.com/juanpimg/Megi/issues/2369 — `crates/paint/src/painter/text.rs:231-249; crates/painter.rs:67-74` — Decoraciones de texto con grosor/posición derivados de `font-size`, no de métricas de fuente
- https://github.com/juanpimg/Megi/issues/2370 — `crates/fonts/src/lib.rs:50-57; crates/paint/src/painter/text.rs:85-116` — Bidi y texto vertical fuera de alcance en fuentes; la pintura vertical apila glifos sin rotar
- https://github.com/juanpimg/Megi/issues/2371 — `crates/svg/src/raster/text.rs:225-238; crates/svg/src/raster/canvas.rs:334-360` — SVG `<text>` stroke approximate por dilatación de cobertura (max) en vez de contorno

### `render.md`

- https://github.com/juanpimg/Megi/issues/2372 — `crates/render/src/backgrounds/load.rs:37-46,131 (firma sin `csp`; `loader.load_no_cors_checked(url, false)`)` — `background-image`/`list-style-image`/`mask-image` eluden CSP `img-src`
- https://github.com/juanpimg/Megi/issues/2373 — `crates/render/src/frames.rs:230-231 y 305-306 (`RgbaImage::new(width, height, child.pixmap.to_rgba_bytes())`)` — `<iframe>` se descarta bajo `raster_scale != 1` (desajuste de dimensiones de pixmap)
- https://github.com/juanpimg/Megi/issues/2374 — `crates/render/src/inline_svg/current_color.rs:66 (`author_css.push_str(&text[..text.len().min(remaining)])`)` — corte de bytes UTF-8 en el substring de CSS inline puede hacer panic
- https://github.com/juanpimg/Megi/issues/2375 — `crates/render/src/scripts/network.rs:20 (`if host.enable_workers {`), 203-239 (`set_websocket_provider`), 245 (cierre del gate)` — el proveedor de WebSocket está dentro del gate `enable_workers`, contradiciendo su contrato
- https://github.com/juanpimg/Megi/issues/2376 — `crates/render/src/scripts/event_loop.rs:179-190 (`for _ in 0..100 { sleep(5ms); runtime.poll_websockets(); ... }`)` — event loop "determinista" hace `sleep` de reloj real hasta 500 ms por drenaje
- https://github.com/juanpimg/Megi/issues/2377 — `crates/render/src/pipeline/svg.rs:125-131 (`rasterize_display_list_with_font_database(..., options.viewport.width, options.viewport.height, ...)`)` — navegación directa a SVG ignora `raster_scale` (ADR-007)
- https://github.com/juanpimg/Megi/issues/2378 — `crates/render/src/charset.rs:178-188 (`find_charset_attribute` exige que `charset` esté precedido de whitespace o sea el primer byte del tag)` — prescan de `<meta charset>` no reconoce `content="text/html;charset=…"` sin espacio
- https://github.com/juanpimg/Megi/issues/2379 — `crates/render/src/images/sources.rs:92-94 (`if sources.len() >= 1024 { break; }`)` — cap fijo de 1024 imágenes ignora `RenderOptions::max_images`
- https://github.com/juanpimg/Megi/issues/2380 — `crates/render/src/scripts/classic.rs:116-120,304 (`sanitize_reserved_function_names`)` — reescritura textual de `function using(` / `function let(` altera literales y comentarios
- https://github.com/juanpimg/Megi/issues/2381 — `crates/render/src/lib.rs:189 (`//! - No forms, no iframes, no web fonts, no `srcset`.`)` — documentación de límites de `lib.rs` contradice el código (forms/iframes/web fonts/srcset)

### `shell.md`

- https://github.com/juanpimg/Megi/issues/2382 — `crates/browser/src/chrome/native_widget_overlay.rs:185-188; crates/browser/src/chrome/menus.rs:266-269, 346-349` — Overlays de framebuffer empaquetan `0xRRGGBBFF` en vez de `0RGB`
- https://github.com/juanpimg/Megi/issues/2383 — `crates/browser/src/find/overlay.rs:121-138; crates/browser/src/chrome/native_widget_overlay.rs:143` — Los overlays de find y widgets ignoran el desplazamiento real de la banda de chrome
- https://github.com/juanpimg/Megi/issues/2384 — `crates/browser/src/window/presentation.rs:341-344` — La ventana nunca usa los favicons reales: dibuja siempre el punto de fallback
- https://github.com/juanpimg/Megi/issues/2385 — `crates/browser/src/a11y_backend.rs:209-228; crates/browser/src/window/accessibility.rs:45,64` — La firma de accesibilidad omite todos los estados de widget (checked/value/expanded/live/description)
- https://github.com/juanpimg/Megi/issues/2386 — `crates/browser/src/a11y/nodes.rs:178-197` — `hidden`/`inert` con valor "false" se tratan como visibles
- https://github.com/juanpimg/Megi/issues/2387 — `crates/browser/src/devtools/ws.rs:93-116; crates/browser/src/webdriver.rs:235-244` — Endpoint DevTools/BiDi sin verificación de Origin (WebSocket loopback)
- https://github.com/juanpimg/Megi/issues/2388 — `crates/test_harness/src/wpt/summary.rs:34-36 (y :26-30)` — El gate WPT pasa en verde cuando un suite se omite por error del runner
- https://github.com/juanpimg/Megi/issues/2389 — `crates/test_harness/src/serializer_harness/serialize.rs:88-91` — `escape_lt_in_attrs` del harness de serializer es una opción inerte
- https://github.com/juanpimg/Megi/issues/2390 — `crates/browser/src/ipc/worker_stream.rs:19-21` — Política de red del sandbox del renderer falla abierta si falta la variable de entorno
- https://github.com/juanpimg/Megi/issues/2391 — `crates/sandbox/src/linux.rs:151-153 (y :92-100)` — El sandbox informa `best-effort` cuando no aplicó ninguna restricción
- https://github.com/juanpimg/Megi/issues/2392 — `crates/browser/src/ipc/worker_stream.rs:77; crates/browser/src/ipc/session.rs:106-108; crates/browser/src/window/worker.rs:293-299` — El renderer por proceso pierde POST, navegaciones por script y navegaciones de mismo documento
- https://github.com/juanpimg/Megi/issues/2393 — `crates/browser/src/chrome/tokens.rs:337-361; crates/browser/src/window/input.rs:93-125` — El sidebar de la shell oscura se pinta pero no es interactivo (`shell_hit` sin usar)

## Cómo continuar

1. Abrir la issue, confirmar la reproducción y crear un WP con `tools/wp_generate.py` o a mano.
2. TDD: test que falla → fix mínimo → gates → commit DCO.
3. Regenerar el ledger con `tools/audit_coverage.py --generate` y marcar filas nuevas tras leerlas.
4. Cerrar la issue con el hash del commit y la salida del gate.

