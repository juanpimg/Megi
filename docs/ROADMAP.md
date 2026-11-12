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
Fase 4: Paint, PNG & Tipografía      [██████████] 100% (conic/hints/OKLab, calc mixto, sticky/tipografía, grid avanzado, clipping+clip:text, decoraciones)
Fase 5: Red, Render & Imágenes       [██████████] 100% (HTTP/2, JPEG/GIF/WebP, SameSite/PSL/Referrer/Range, RenderCache)
Fase 6: Integración JS & Bindings    [██████████] 100% (Boa 0.22, ES modules, TLA, fetch, DOM/Storage/MutationObserver)
Fase 7: Ventana OS & Shell           [█████████░]  98% (chrome con fuente real, tabs, find, forms, reader mode, session)
```

### Auditoría Pre-Alpha (`v0.1.0`) — Navegación en webs reales (2026-11-12)

Tras publicar la Pre-Alpha `v0.1.0` (`2.300+` tests, gate `PASS`, `14/14` crates `#![forbid(unsafe_code)]`), se ejecutó una auditoría manual y con `render --json` sobre sitios reales:

- **Qué funciona hoy**:
  - `https://example.com`, `https://es.wikipedia.org/wiki/Rust` y `https://www.rust-lang.org` cargan por HTTPS/TLS, decodifican gzip/SVG/imágenes y permiten clic en enlaces y edición en la barra de direcciones (con esquema `https://` automático para dominios sin prefijo).
- **Problemas reales detectados y registrados en GitHub**:
  1. **`#1011` (`layout`)**: `https://news.ycombinator.com` colapsa a **`display_items: 0`** (página en blanco por maquetación `<center>` / `<table>`).
  2. **`#1013` (`layout`)**: En `es.wikipedia.org/wiki/Rust`, pestañas/elementos de navegación Vector (`Artículo`/`Discusión`, `Leer`/`Editar`/`Ver historial`) se solapan, `<h1>` se alinea a la derecha y se pierde un espacio entre nodos inline (`a:Quinto`).
  3. **`#1012` (`shell`)**: La barra de chrome (`58px`) tapa los primeros `58px` del contenido cuando `scroll_y == 0`.
  4. **`#1010` (`render`)**: `<img>` sin `crossorigin` aplica validación CORS estricta y descarta imágenes cross-origin.
  5. **`#1014` (`perf`)**: Latencia alta en webs reales (`2.55 s` en Wikipedia, `4.12 s` en HN): `styles_pre_script` ~695 ms y descarga secuencial de imágenes ~1.4 s.
  6. **`#1015` (`fonts`) + `#987`**: Texto CJK/emoji muestra `[tofu]` sin fallback de fuentes del sistema, y el árabe carece de shaping contextual.
  7. **Alpha pendiente**: Animaciones CSS `@keyframes`/`transition` en runtime (`#941`/`#942`), diálogo visual de descargas (`#988`), widgets nativos/accesibilidad/aislamiento por pestaña (`#989`/`#946`).

**Pre-Alpha `v0.1.0`** (`96787a6` → `1e563ec`): `bidi` UAX #9 (`css`+`layout`), `RenderCache`, fuzz targets ampliados (`dom_tree`, `layout_tree`, `paint_display_list`, `fonts_shape`), fix `column flex` auto-height (`#696`), fix binario `megi` en `release.yml`, fuente bundled en `chrome.rs` + `https://` automático en barra de direcciones + prioridad de URL CLI sobre `session.json`.
**Ronda 32**: **perf CSS** ancestor-class Bloom pruning + fast paths @container/`has_revert_layer` ⇒ resolve **28,58→14,14 ms (−50,5 %)**; **layout** **40,18→37,35 ms (−7,0 %)**; **PNG decode 6,739→2,536 ms (−62,4 %)** y **encode byte-safe −18 %/−22 %** con render-to-png **−27,7 %**; **render fast paths** backgrounds 7,0→0,8 ms e inline_svg 2,8→0,6 ms (hallazgo: fila pipeline del bench no usa render; CLI A/B −6,5 %); **TLA real + live bindings** con módulos nativos Boa (`eval_module_async`, await Promise/fetch/timer, let/var vivas, const inmutable, ciclos; js **168**/render **172**); **h1 streaming real + push prioritization** (status/headers incremental, redirects antes del body, CL/chunked/read-to-EOF; PADDED/PRIORITY, `ServerWeights|Fifo`, desalojo RST; net **325**). Total **1929 tests**; CI verde y repo sincronizado en `3e69968`.
**Ronda 31**: **Cascade Layers** (`@layer` nombres/anónimas/anidadas/compuestas, orden por primera aparición, `!important` invierte, `revert-layer`, @media/@container dentro, fast path; css **260**; fixtures `ddeafab9…`/`5944fb8a…`); **PNG single-encode** (`RenderOptions.encode_png`/`ensure_png` perezoso byte-idéntico; shell `--chrome/--find/--chrome-menu` con encode_png=false + overlay + encode único ⇒ +8,57→+1,13 ms 640, +21,67→+1,75 1024, showcase +91,20→+2,32; shell **238**/render **170**); **fetch() subset** (Headers/Request/Response + provider, GET/POST, CORS same-origin, `--no-network`, fix HostScope; js **158**/render **170**); **proxies HTTP/HTTPS** (`ProxyConfig{http,https,no_proxy}`, env, matching host/sufijo/IP/host:port/`*`, absolute-form+Basic, CONNECT+TLS h1/h2, errores tipados; net **315**); **perf VP8** (allocs 25.305→9, A/B 13,96→**10,91 ms = −22,2 %**, bench 10,815 ms; image **90**). Total **1907 tests**; CI verde y repo sincronizado en `aba05f8`.
**Ronda 30**: **decoder VP8 keyframe completo** para WebP lossy (boolean/segmentación/loop filters/particiones/intra+B_PRED/dequant/IDCT/IWHT/YUV420 fancy+BT.601; tolerancias vs PIL y decode 53,6 ms; image **90**/render **165**); **Container Queries** (`container-type/name`, `@container` min/max width/height/inline/block con and, ancestro compatible, fast path; css **241**/layout **224**; fixtures `c358c11f…`/`57b09283…`); **PSL real vendorizada** (MPL-2.0, reglas normal/wildcard/exception) y **request trailers** h2/h1 (net **299**); **Web Workers cooperativos deterministas** (realm sin DOM, FIFO JSON-safe, onerror, caps 8/256; js **147**/render **164**); **historial navegable** (HistoryLog cap 64/dedupe, about:history, Ctrl+H, ←/→/⟳) y **chrome fast path** (−89,5 % componente no-PNG, píxeles idénticos; shell **236**). Total **1853 tests**; CI verde y repo sincronizado en `f7dad8d`.
**Ronda 29**: **streaming + keep-alive** (`get_streaming`/`request_streaming` con DataCallback, h1 fallback buffered, loader `load_streaming`; `H1Pool` por origin cap 4/idle/reuse ⇒ 2 loads = 1 TCP y h2 `Session` reutilizada; net **287**/render **162**); **SVG textPath + SMIL** (`<textPath>`, `values`+`keyTimes`+`calcMode` spline/keySplines y animación opacity/fill/stroke/color sRGB+alpha; svg **109**); **import maps + import() dinámico** (prefijo más largo/null bloqueado/ImportMapBlocked, literales eager, TLA gap tipado; js **137**); **WebP lossy header experimental** (parser RIFF/VP8 + `UnsupportedFeature`; pixel decoder pendiente; image **90**); **shell** (directorios file://, páginas de error con Reintentar R, favicon/dot; shell **224**; md5 dir `d0b66f74…`, error `58a65985…`, chrome+dot `4b9033c7…`); **fuzz 14 targets** y **bench r28** pipeline **115,67/140 = 82,6 %** con todos los presupuestos ✅. Total **1796 tests**; CI verde y repo sincronizado en `8f9cfa9`.
**Ronda 28**: **reduced IDCT + EXIF thumbnails** (4/2/1 para escalas 2/4/8, full intacto; A/B −57/−82/−90 %; image **84**); **server push consumido + uploads streaming** (opt-in, PUSH_PROMISE/HEADERS/DATA con flow-control propio, `H2PushedResponse`, caps/RST; `RequestBody::Stream` pull-based con backpressure, cap 16 MiB, h1 chunked/100-continue; net **279**); **geometría JS** (`LayoutProvider`, `getBoundingClientRect`/offset/client, viewport innerWidth/Height/DPR; render pre-script; js **130**/render **157**); **blur/PNG byte-safe** (encode −23,5 %, E2E −13,2 %, blur −4,7 %, floor ~1,54 ms documentado; paint **208**); **shell contextual** (hover/status bar, Ctrl+clic/clic medio, menú contextual con Save image/Open/Copy link/Reload/Save page; shell **204**; `--chrome --chrome-menu` md5 `b59202c1…`). Total **1738 tests**; CI verde y repo sincronizado en `cb7af43`.
**Ronda 27**: **fix regresión PNG** (`#[inline(always)]` en decode_pass/unfilter/render_row/pixel_from_row; r25 6,62 / r26 7,97 / fix **6,54 ms**; escalado full 6,27–6,43, ×2 5,53–5,69, ×8 5,02–5,13); **HTTP/2 PUSH_PROMISE/prioridades/h2c** (push deshabilitado por defecto + RST/HPACK; `Priority` y scheduler por peso; `allow_h2c`/`h2c_request` con fallback h1; net **259**); **gaps DOM/JS cerrados** (URL/SearchParams, dataset, innerText, attributes, normalize, compareDocumentPosition, queueMicrotask, idle, Mouse/PointerEvent, focus/blur, atob/btoa, structuredClone; js **128**/dom **36**); **perf efectos** (`compute_effects` de un barrido; filter E2E −75,7 % a 0,036 ms; backdrop 0,692→0,684 por blur+PNG); **chrome pulido** (address bar, autocompletado, tab overflow, `<select>`; shell **195**; `--chrome` md5 `1a29d6b3…`). Total **1695 tests**; CI verde y repo sincronizado en `9d2298b`.
**Ronda 26**: **`<script type=module>` cableado** en render (inline/externos con resolver/loader reales, file/http(s)/data, caps por módulo/grafo, fallo no fatal; render **153**); **decode escalado** (`decode_scaled`/`--max-image-dim`, PNG unfilter+Adam7 sin buffer completo; PNG ×2/−28 %, ×8/−36 %; image **76**); **HTTP/2 request bodies/trailers/100-continue + h1 POST** (`H2Request`/`request_many`, DATA/END_STREAM, cap 16 MiB, trailers h2/h1, `Expect: 100-continue`, POST CL/chunked; net **249**); **bug-bash DOM/JS** (fragmentos, atributos case-insensitive, childNodes snapshot, propiedades reflejadas, createDocumentFragment, cloneNode deep; dom **33**/js **111**); **filtros SVG completos + SMIL básica** (`feColorMatrix`/`feBlend`/`feMerge`/`feComposite` arithmetic con cadenas; `<animate>/<set>/<animateTransform>` determinista con `render_svg_with_time`; svg **97**); **bench aislado** (layer-effects/object-fit/filter/backdrop; `save-png-encode` 3,76 ms; pareja 30 vs 29 ms). Total **1653 tests**; CI verde y repo sincronizado en `f42b9b9`.
**Ronda 25**: **CSS Color 4/5** (`color()` con srgb/srgb-linear/display-p3/a98-rgb/prophoto-rgb/rec2020/xyz, `lab/lch/oklab/oklch`, `hwb()` y `color-mix()`; css **230**/paint **208**); **ES modules** en js (`eval_module` con resolver/loader, default/named/namespace/side-effect imports, export const/let/var/function/class/default/{as}, `import.meta.url`, orden topológico, ciclos `ModuleCycle`; js **96**); **WebP VP8L completo** (predictor 14, color transform 3.5, subtract-green, color indexing, Huffman complejo/meta, LZ77+overlap; fixtures reales píxel-exactas; image **69**/render **145**); **providers JS cableados en render** (`history_actions`/`location_actions`, storage compartido/session, `host_events`, checkpoints de microtasks y E2E MutationObserver DOM→PNG `7411a495…`; render **145**); **save-png directo** del shell (+1,7 ms en vez de +200,1; shell **183**). Total **1594 tests**; CI verde y repo sincronizado en `4718408`.
**Ronda 24**: **apis de navegador en js** (History API con providers, `location` getters/assign/replace/reload, `localStorage`/`sessionStorage` con cap 1 MiB/`QuotaExceededError`/StorageProvider, `MutationObserver` subset con records al drenar microtasks; js **83**); **css/paint** (`aspect-ratio`, `object-fit/object-position`, `backdrop-filter` + alias `-webkit`; `DisplayItem::BackdropFilter` y object-fit con clip/bilineal; css **210**/paint **205**; fixtures aspect_ratio `a1e4d950…`/object_fit `b54ddebe…`/backdrop_filter `ff04fac9…`); **layout consume aspect-ratio** (w/h derivados, ambos definidos gana definido, replaced auto intrínseco, flex/grid; fixture 400×400 `6f021d43…`; layout **215**); **imagen** (JPEG progresivo de color completo Δmáx ≤2/gris Δ0 y WebP VP8L subset con literales ARGB; transforms/LZ77/VP8 lossy ⇒ `UnsupportedFeature`; image **67**/render **139**); **shell** (sesión persistente JSON v1 con tabs/historial/zoom/scroll caps 32/64, escritura atómica+lock, XDG/`--no-session`/`--session-file`, y descargas `Ctrl+S` PNG/`Ctrl+Shift+S` HTML con `--save-png`/`--save-html`; shell **180**); **fuzz 10 targets** (`image_decode`/`exif_parse`/`hpack_decode`, corpus 11/9/21, CI 300 s + PR build-only; build 10/10, smoke 300 runs sin crashes). Total **1547 tests**; CI verde y repo sincronizado en `0b3adee`.
**Ronda 23**: **presupuestos recuperados con margen**: **CSS** (cfad971) clave de caché por identidad de estilo del padre + move-on-miss + AST UA en `OnceLock` ⇒ css resolve 35,81→**26,54 ms (−25,9 %)** y pipeline 142,64→**105,47 ms (−26,1 %)**; **paint** (3f5ce3e) análisis agregado `effects_present` + fast path r21-equivalente + DisplayItem 20480 ⇒ build-display **7,34–7,72 ms ≤8** (−4,8 %/−9,6 %); **imagen** (1e8e078) **EXIF orientation completa** (APP1/TIFF little+big, 1–8 en decode, `exif_orientation()`, fixtures exif_o1..o8) y **JPEG progresivo subset** (SOF2 multiscan DC/AC first+refine, spectral selection, successive approximation, EOB runs, restart; gris exacto; color refinement ⇒ `UnsupportedFeature`), **WebP detectado** (RIFF/VP8L) pero sin decoder (aviso no fatal), image **60**/render **138**; **HTTP/2** (a2fb055) **flow control** (ventanas conexión/stream, WINDOW_UPDATE/FLOW_CONTROL_ERROR, delta 0 inválido), **multiplexado ≥2 streams** (`get_many`, Session/scheduler, RST aislado, cap 8) y **SETTINGS** avanzados (HEADER_TABLE_SIZE/MAX_CONCURRENT_STREAMS/INITIAL_WINDOW_SIZE/MAX_FRAME_SIZE/ENABLE_PUSH con ACK, HEADERS+CONTINUATION, HPACK compartido), net **237**. Total **1468 tests**; CI verde y repo sincronizado en `d1f3121`.
**Ronda 22**: **JPEG y GIF desde cero** (JPEG baseline con IDCT separable, 4:4:4/4:2:2/4:2:0 nearest, grayscale/YCbCr/Adobe RGB y errores tipados; GIF87a/89a con LZW 12-bit, paletas, transparencia e interlace; `render` decodifica PNG|JPEG|GIF por sniffing; decode PNG 800×600 **6,65 ms p50 ~290 MB/s**; image **54**/render **137**); **efectos SVG** (`clipPath` shapes/paths/use con units/anidados/ciclos sin panic, `mask` por luminancia con región/units y gradientes, filtros `feGaussianBlur`/`feOffset`/subset `feFlood`+`feComposite`; caps 4096 shapes/16 primitivas/blur 64/64M muestras; svg **85**); **HTTP/2 MVP** (HPACK completo con static 61/dynamic eviction/literals/Huffman, framing HEADERS/CONTINUATION/DATA/SETTINGS/WINDOW_UPDATE/PING/RST/GOAWAY con caps 16 KiB frame/64 KiB headers/2 streams, ALPN h2 sobre rustls con fallback h1, `FetchError::Http2`; net **225**); **chrome del shell** (`chrome.rs`: address bar Ctrl+L/caret/Enter/Esc, tab strip con títulos/activa/cerrar/+/overflow y hit-testing, dropdown `<select>` cap 8, overlay 58 px y `--screenshot --chrome`; shell **160**); **grid completo** (grid-auto-flow row/column+dense, `grid-template-areas`/`grid-area`, líneas nombradas y placement por nombre, justify/align-content con stretch, `repeat(auto-fit)`; layout **206**/css **195**; fixtures grid_areas `8f9a9eba…`/grid_dense `1789daf8…`/grid_content `123d1a2b…`); **filtros CSS/mix-blend/image-set** (filter blur/brightness/contrast/grayscale/invert/opacity/sepia/saturate/hue-rotate/drop-shadow + alias `-webkit`, mix-blend-mode normal/multiply/screen/overlay/darken/lighten, image-set() 1x; paint `DisplayItem::EffectGroup` contra backdrop, caps 16,7 Mpx/blur 64/funciones 16; css **195**/paint **190**; fixtures filter_effects `ffe06406…`, blend_modes `32788e6d…`, image_set `dc638211…`). Total **1449 tests**; **F5 ≈99 %**, **F7 ≈90 %**; manifiesto `docs/status/progress.toml` actualizado (F4/F5/F7 y next_up) para regenerar `PROGRESS.md`; CI verde y repo sincronizado en `00d8388`.
**Ronda 21**: **recuperación de presupuestos sin subirlos** (los presupuestos son gates: pipeline ≤140 ms, layout ≤43 ms). **HTML** (26560cc): fast paths bulk del tokenizer, runs de texto UTF-8 byte a byte, preprocesado byte-oriented (`has_wide`) y dedupe híbrido de atributos ⇒ `html/realistic-1mb` 20,0→16,4 ms (−18/−20 %). **CSS** (0ea54b5): hasher FNV + claves `&str` prestadas + caché de `ComputedStyle` 128-bit (umbral 2, sin inline, cap 1024) ⇒ css resolve 38,84→33,91 ms (−12,7 %; ≤35 ✅). **Layout** (d0249ce): interning de `ComputedStyle` (fingerprint + `eq` exacta antes de compartir `Arc`), estilos anónimos perezosos, spacing cero en IFC y grid por índice ⇒ layout 46,6→36,4 ms (−21,8 %; ≤43 ✅), sticky 42,8→32,3. **Paint** (fd2c471): fast path por caja en `paint_background_border`, `DisplayList::with_capacity(16384)`, baseline sin `Vec` ⇒ build-display 8,62→7,84 ms (−9,1 %). **Pipeline 160,1→~130-133 ms** (A/B 154,13→132,39, −14,1 %; ≤140 ✅). Estado de fases generado en [`PROGRESS.md`](PROGRESS.md) (`tools/update_progress.py --write`, check de frescura en CI): overall 95 %, 1.345 tests, gate PASS. Total **1345 tests**; CI verde y repo sincronizado en `a8ce8ba`.
**Ronda 20**: **fix bloqueante de custom properties** (`Option<Arc<HashMap>>` lazy/COW + `needs_resolution` fast path; A/B css resolve 46,8→39,8 ms = −14,9 % quiet/−9,4 % carga, pipeline −3,4 %/−6,1 %, UA-cache revertida, presupuestos intactos); **conic-gradient**/`repeating-conic` (`from`/`at`, stops angulares, hard stops, alpha, wrap) + **color hints** + **espacios `in srgb|srgb-linear|oklab`** (default sRGB byte-idéntico); **calc() mixto %+px** (`CalcLengths` en CSS + resolución en layout, base indefinida ⇒ %=0); **tipografía** (`text-transform` en shaping, `word-spacing`/`letter-spacing`, `white-space: pre-wrap/pre-line`) y **`position: sticky`** (modelo headless); **pestañas en shell** (`tabs.rs`, Ctrl+T/W/Tab/Shift+Tab, Alt+1..9, `--tabs` repetible, MAX 32; shell **140**); **filas E2E del bench** (`pipeline/gradients`, `pipeline/grid`, `css/vars-calc`, `paint/clipping`, `layout/sticky-text` SKIP). css **175**/layout **196**/paint **175**/render **136**/shell **140**; total **1344 tests**; **F7 ≈80 %**; CI verde y repo sincronizado en `c6b66b1`.
**Ronda 19**: **gradientes CSS E2E** (`linear-gradient` con ángulos deg/grad/rad/turn y `to side/corner`, `radial-gradient` circle/ellipse con radios y posición, `repeating-*` en capas de `background-image` mezclables con `url()`; stops alpha/currentColor/hard stops, caps 64 stops/256 gradientes; paint `DisplayItem::Gradient` por tile respetando size/position/repeat(space/round)/origin/clip/radius/clip:text/opacidad; css **142**/paint **162**; fixture `gradient_linear` 240×220 md5 `6a04fdc6…`, goldens intactos); **grid avanzado** (placement explícito con líneas 1-based/negativas y `span N`, auto-placer que salta ocupadas y crece tracks implícitos, `minmax()` px/%/fr/auto y `repeat(auto-fill|auto-fit)` con cap 1024, `order` estable en grid/flex; layout **173**; fixtures `grid_span` md5 `0c376406…` y `grid_autofill` `d2255ca3…`); **render** instala `ReferrerConfig` (header>meta>default) en el loader ⇒ `Referer` y SameSite reales, `RenderStats.referrer_policy` en `--json`, y resuelve **estado CSS real** con `css::state::with_state` pre/post-script (`:checked/:disabled/:enabled/:read-only/:required/:placeholder-shown`, `data-megi-*`, fixture `css_state.html`; **136** tests, +2 de gradientes); **shell find-in-page** (`Ctrl+F` case-insensitive sobre nodos Text, Enter/Shift+Enter con wrap, Esc, scroll al hit, highlight amarillo en framebuffer y `--find QUERY --screenshot`, `find.rs` puro, navegación F5/g/Alt+Home/Ctrl+D; shell **125** tests, +17). Total **1260 tests**; **F7 ≈78 %**; CI verde y repo sincronizado en `d9e07e0`.
**Ronda 18**: **fuzz 7 targets** (+`render_pipeline`/`js_eval`/`svg_parse`, seeds 4/6/5 y job PR build-only; allocation one-time de Boa documentada); **CSS moderno**: custom properties/`var()` (herencia/fallback/ciclos, 32 niveles), `calc()` (px/em/rem/%/vh/vw; mezcla %+px invalid), pseudo-clases de estado (`:focus`, `:focus-visible`, `:hover`, `:active`, `:checked`, `:disabled`, `:enabled`, `:read-only`, `:read-write`, `:required`, `:placeholder-shown`) con `StateContext`/`data-megi-*` y `resolve_document_styles_with_media_and_state`, parseo de grid y alias `-webkit-background-clip` (css **128**); **paint**: `overflow:hidden`+radius recorta el subárbol (padding box, radios internos, AA 4×4, cap 16,7 Mpx) y `background-clip:text` (máscara de glifos, multlínea, cap 4096 runs; display list byte-idéntica sin clipping; paint **150**); **layout**: **Grid MVP** (tracks fixed/%/fr/auto/repeat, gaps, auto-placement row-major, justify/align items/self, auto-rows; sin spans/minmax/auto-fill/subgrid/colocación explícita; layout **158**, 3 fixtures E2E md5 `2d6714bc…`/`c411ff6d…`/`93d1126d…`); **net**: SameSite+PSL reducida (~150 sufijos), Referrer-Policy (8), Range/206 (`ByteRange`/`ContentRange`, sin multi-range/If-Range) y cache (`no-cache`/`must-revalidate`/`s-maxage`/`Age`/`requires_revalidation`), net **207**; integración del referrer en render HECHA en la ronda 19. Total **1189 tests**; F5 ≈97 %, F6 ≈90 %, F7 ≈75 %; goldens intactos.
**Ronda 17**: **Flexbox MVP** (`display:flex/inline-flex` con flag, direction 4, justify 5 incl. space-evenly, align stretch/flex-*/center/baseline, wrap, gap, grow/shrink/basis + shorthand; layout **146**/css **94** tests, fixtures E2E md5 `c28ce538…`/`8d06d66c…`/`7fbf998e…`); **`border-radius`/`box-shadow`/`text-shadow`** con DisplayItems redondeados, blur determinista de 3 pasadas, caps 64 px/32 sombras y goldens intactos (css 94/paint **141**); **formularios** UA styles + shell editable (FocusState/TextEdit UTF-8/snapshot re-render/Tab/checkbox; `forms.html` 640×480 md5 `b8b9a20…`; shell **108**); **SVG `<pattern>` y `<text>` básico** (dep interna `svg→fonts`, caps 10k tiles/1.000 texts; svg **73**). Total **1107 tests**; **F7 ≈75 %**. CI verde y repo sincronizado.
**Ronda 16**: **SVG real** (CSS interno `<style>`/class/id/descendientes + gradientes lineales/radiales completos con stops/units/spread/gradientTransform/href; `pattern` pendiente; svg **60 tests**), **backgrounds multicapa** con `background-origin`/`clip`/`attachment` y `background_layers` top-first (`space`/`round` reales, legacy flag; css **76**/paint **130**), **APIs de navegador en js** (`matchMedia`, viewport/DPR/scroll, `requestAnimationFrame` 16 ms, `onerror`, `readyState`, consola; **71 tests**) y **cableado real en render** (`getComputedStyle` pre-script con `styles_pre_script`, matchMedia/viewport, rAF con **epoch virtual fijo 48 ms** sin flake, errores/console ⇒ warnings no fatales `console_errors`, multi-capa E2E `background_layers.html` md5 `c357cc2c…`; **123 tests**). Total **1046 tests**; **F6 ≈90 %**. CI verde con deps de sistema y suite determinista. Estados: F4 prácticamente completa, F5 ≈95 %, **F6 ≈90 %**, **F7 ≈70 %**.
**Ronda 15**: **background-* geometry** (`repeat/size/position` + shorthand, tiling con clip y cap 4096, legacy `background_geometry_explicit` para goldens; css 69/paint 122); **SVG inline** en `render` (pre-proceso a `<img>` sintético con base64, límites 256/1 MiB/8 MiB/depth 256, stat `inline_svgs`, 7 fixtures y 10 tests ⇒ render 114); **IFC −28 %** (layout/realistic-1mb **50,5 → 36,3 ms**, pipeline **132,9 → 120,5 ms**, geometría byte-idéntica gracias a pool de texto, ids de estilo, breaking incremental y fast path de `format_line`); y **fix de determinismo** del event loop (`b808d7b`: sólo reloj virtual, fixture `js_event_timeout` md5 `07682088…`, CI verde). Estados: F5 ≈95 %, **F6 ≈80-85 %**, **F7 ≈70 %**.
**Ronda 13**: **fuentes web @font-face + WOFF2 E2E** (`css` parsea `@font-face`, `fonts` decodifica WOFF2→sfnt con `woff2-patched 0.4` Apache-2.0 pure Rust — única dep externa de `fonts` — y `render` construye una DB por render para layout+paint con fetch/dedupe/caps 8/8 MiB/32 MiB, stats y warnings no fatales; evidencia md5 webfont `f38aed03…` vs fallback `e2d76dee…`); nuevo **crate `svg`** (parser XML+modelo SVG+rasterizador AA desde cero, 41 tests, límites anti-DoS) ya **cableado E2E en `render`** para `<img src=*.svg>` y `background-image` (detección Content-Type/sniffing, intrínseco width/height/viewBox con 300×150 por defecto, cap 16 Mpx/64 MiB RGBA, corrupto ⇒ warning no fatal, navegación directa en página sintética, stat `svgs_rendered`; 11 tests ⇒ render 104); **cierre de ronda**: fix `html/fast_hash` recupera `html/massive-end-p` (~19 ms; antes 21,4) y deja el **pipeline ~127-134 ms** con el gate intacto; **shell** con **zoom** (`--zoom` 0.5–3.0), **auto-refresh opt-in** (`--refresh-ms`) y título enriquecido (100 tests; `--zoom 2` md5 `c11903cf…`). CI verde con deps de sistema. Estados: F5 ≈95 %, **F6 ≈80-85 %**, **F7 ≈70 %**.
**🎉 HITO Ronda 11**: **árbol HTML5 3.904/3.904 (100 %)** — las 4 suites `scripted_*` pasan y el gate queda **sin allowlist de tree** (`scripted_*` 6/6 mode-runs). **WP-6.4 HECHO**: hook `ScriptExecutor` en el parser, scripts parser-inserted al `</script>`/EOF y `document.write` re-tokenizado en la posición actual (`Tokenizer::insert_input`, anidados incluidos), harness `JsScriptExecutor` sobre `eval_on_document`. **WP-B HECHO**: `CSSStyleDeclaration` completa, `getComputedStyle` con provider, `NodeList`, `matches`/`closest`, `insertAdjacentHTML/Text`, identidad de wrappers y `document` como `EventTarget`. **Event loop presupuestado en `render`**: reloj virtual, `DOMContentLoaded`/`load`, timers con delay (`max_tasks` 10k, `max_virtual_time` 1000), un solo re-render y stats (`script_*`, `timings.event_loop`); 6 fixtures `js_event_*`. **Parser**: regresión `tag-40k-attrs` corregida (~12,4 ms, A/B por debajo del baseline pre-r10) manteniendo las ganancias de r10. Estados: F5 ≈95 %, **F6 ≈75-80 %**, **F7 ≈65-70 %**. **Ronda 12**: **rendimiento** — `LayoutBox.style` → `Option<Arc<ComputedStyle>>` (+ `style()`/`with_shared_style`), estilo anónimo compartido por padre y tokens `Space` con `char` ⇒ **LayoutBox 496→176 B**, `layout/realistic-1mb` **−14,6 %** (51-53 ms) y pipeline **134-137 ms**; **encoder PNG −30,5 %** (render-to-PNG 13,30→9,24 ms) con **bitstream byte a byte idéntico** (goldens sin regenerar). **JS eventos avanzados HECHO** (`currentTarget`/`eventPhase`, `once`/`passive`, `onclick`/`onload`/`onerror`, `document.close()`, `estimated_heap_bytes()`, excepciones capturadas; 58 tests). **CI GitHub en verde** (3 jobs) con paquetes de sistema Wayland/X11/xcb añadidos al workflow; `main` y la rama sincronizadas. Estados: F5 ≈95 %, **F6 ≈80 %**, **F7 ≈65-70 %**.
**Ronda 10**: JS **WP-A HECHO** (`EventTarget` con captura+burbuja, `Event`/`CustomEvent` y timers con reloj inyectable `advance`/`run_until`/`now_ms`/`pending_timers`; 26 tests js); **rendimiento** pipeline 800×600 **167,6 → 131,2 ms (−21,7 %)** y `html/realistic-1mb` **55,0 → 19,2 ms (−65 %)** (gate y golden intactos); **`RenderOutput.hit_regions` cableado** en el pipeline (píxeles sin cambios, +0,6 µs) y el shell usa `paint::find_link_at`; **red avanzada E2E** en `render`/`shell` (jar de cookies por render o sesión, `--cookies`, `--origin` con CORS/credentials y denegación no fatal, gzip/deflate en HTML/CSS). Estados: F5 ≈95 %, F6 ≈40 %, F7 ≈65 %. **Ronda 9 (cierre)**: el pipeline `render` **ejecuta JavaScript** (`ScriptingMode`, WP-6.2 host: scripts inline/externos post-parseo con budgets, microtasks/timers, console y **re-render del DOM mutado** visible en el PNG; `--no-scripting`), `paint` gana **hit-testing** aditivo (`HitRegion`/`hit_test`/`find_link_at`; PNG/golden byte-idénticos) y el `shell` es **navegable**: clic en enlaces con `Url::join`, historial (`NavigationState`), Alt+←/→, `r` y título dinámico. La Fase 4 sigue funcionalmente cerrada (pendientes menores: `background-repeat/size/position`, colores de `border-collapse`, block-in-inline). Fase 5 ≈95 % (HTTP2/streaming/SameSite pendientes). Fase 6 ≈40 % (WP-B, WP-6.4 `scripted_*` e integración de eventos/timers en el host pendientes). Fase 7 ≈65 % (formularios, pestañas, zoom, JS en el host y eventos del DOM pendientes).
> **1929 tests** y gate de 4 suites PASS (**árbol 3.904/3.904, 100 %**); estado de fases y badges generados en [`PROGRESS.md`](PROGRESS.md) (`python3 tools/update_progress.py --write`, check de frescura en CI); rendimiento r23: css resolve **26,54 ms**, pipeline **~105,5 ms** en A/B (presupuestos ≤140/≤43 sin subirlos), build-display **7,34–7,72 ms ≤8**; suite determinista (reloj virtual, epoch fijo de rAF).
```


| Fase / Bloque | Barra | Estado | Cobertura Verificada | Hito Clave Alcanzado |
|---|:---:|:---:|---|---|
| **Fase 0: Tooling, Arena DOM & Closed-Loop Harness** | `[██████████] 100%` | **COMPLETO** | 28 tests en `crates/dom` | Arena contigua indexada (`NodeId(NonZeroU32)` de 4 bytes) sin `Rc<RefCell<Node>>`, scripts CLI de inspección WHATWG en vivo. |
| **Fase 1A: Tokenizador HTML5 (84 Estados & 2.231 Entidades)** | `[██████████] 100%` | **COMPLETO** | 7.050 / 7.050 estados en el gate (allowlist vacía) | 13 suites estándar de `html5lib-tests/tokenizer` al 100%; `ParseOptions` anti-DoS y deduplicación de atributos en tiempo lineal. |
| **Fase 1B: Árbol DOM, Serializador §13.3 & DOM de navegador** | `[██████████] 100%` | **COMPLETO** | **3.904/3.904 árbol (100 %)** + 230/230 serializer + **60 tests html** | 21 modos de inserción, AAA, *foster parenting*, `innerHTML`; **serializador público WHATWG HTML §13.3** (`serialize_html`/`serialize_fragment`/`serialize_outer_html` + opciones) con las 5 suites `html5lib-tests/serializer` en el gate. **WP-6.4 (ronda 11)**: hook `ScriptExecutor` + scripts parser-inserted + `document.write` re-tokenizado ⇒ las 4 suites `scripted_*` pasan y la allowlist de tree queda vacía; fix de la regresión `tag-40k-attrs` (~12,4 ms). |
| **Fase 2: Motor CSS (Sintaxis L3, Selectores L4, Cascada L4, Hoja UA §15)** | `[██████████] 100%` | **COMPLETO** | 230/230 unit + 30/30 WPT | Selectores L4 funcionales (`:is/:where/:not/:has`) y estructurales, especificidad, expansión de shorthands antes de la cascada, `font` shorthand, `@media` básico y índice de reglas (2,1× en resolve-styles 10k nodos); bases de Fase 4 (bordes por lado, `border-style: none`, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`, `line_height_factor`). |
| **Fase 3+4: Motor de Layout (BFC/IFC, flexbox, grid completo, calc mixto, sticky, tipografía, posicionamiento, floats, tablas)** | `[██████████] 100%` | **COMPLETO** | 215 tests | Box Tree, cajas anónimas *inherit-only*, colapso de márgenes, ICB, `box-sizing`/min-max/%, sobre-restringido; **IFC**: oportunidades de salto reales, soft hyphen de ancho cero, `line-height` como factor, `vertical-align: top/bottom` (§10.8.1) y `text-align: justify`. |
| **Fase 4: Display Lists, Pipeline HTML → PNG & Rasterización 2D** | `[██████████] 100%` | *COMPLETA* | 208 tests en `crates/paint` | Pipeline `HTML → CSS → layout → display list → PNG` con texto real, **orden completo del Apéndice E** (stacking contexts/`z-index`), `OpacityGroup` (grupo offscreen), `PushClip`/`PopClip` desde `overflow`, `text-decoration`, `visibility`, marcadores de lista, bordes por lado, fondo bajo el *border box*, antialiasing exacto y culling/cachés; codificador PNG propio y golden determinista (PNG `md5 9e957177…`). PNG comprimido propio (**44,8×**), painter iterativo, imágenes `<img>` (ImageStore/bilineal), **fix de underflow** en cobertura parcial, **hit-testing aditivo** (`HitRegion`/`hit_test`/`find_link_at`, 11 tests), DOM generacional y **encoder PNG −30,5 % con bitstream invariante** (ronda 12). **Ronda 15**: **`background-repeat/size/position`** reales (`background_tiles`, cover/contain centrado, clip/tiling al padding box, cap 4096) con legacy `background_geometry_explicit`. **Ronda 16**: `background-origin`/`clip`/`attachment` y **multi-capa**. **Ronda 17**: **`border-radius`/`box-shadow`/`text-shadow`** (DisplayItems redondeados, blur 3 pasadas determinista, caps 64 px/32 sombras) y **Flexbox MVP** en layout. |
| **Fase 4: Motor tipográfico (`crates/fonts`)** | `[██████████] 100%` | **COMPLETO (integrado + fuentes web)** | 17 + 1 doctest | `FontDatabase` (fontdb), métricas OpenType (ttf-parser), shaping LTR con `rustybuzz` (kern/ligaduras), rasterización de glifos (`ab_glyph`) y fallback; **integrado en layout y paint** (texto real en el pipeline). **Ronda 13**: **fuentes web `@font-face`+WOFF2 E2E** (dep `woff2-patched 0.4`, alias CSS, DB por render en layout/paint, caps y stats). Bidi/scripts complejos/fuentes variables quedan fuera de Fase 4. |
| **Fase 5: Red, Render Pipeline & Imágenes** | `[██████████] 100%` | *COMPLETA* | 325 (net) + 172 (render) + 90 (image) | URL WHATWG (subset http/https/file + join), HTTP/1.1 `GET`, **TLS 1.2/1.3 con `rustls` 0.23 (ring) + `webpki-roots`** (SNI/validación, feature `tls` por defecto), **caché HTTP RFC 9111 subset** (max-age/Expires/ETag/Last-Modified/304/no-store/Vary), **`data:` URLs** con Base64 propio, `file://`, MIME sniffing y `ResourceLoader` con caché; `#![forbid(unsafe_code)]`. **`crates/render`**: pipeline URL→PNG con CSS externo, CLI y **`<img>` E2E**; **`crates/image`**: decoder PNG/inflate propio; CSS con `@import`/media queries. **`@import` recursivo + media real** en `render`, **showcase 1024×768** (80.220 B, `md5 c95c7eff…`), **backgrounds E2E**, **cookies RFC 6265/CORS/gzip** y **ejecución JS en render** (`ScriptingMode`), **red avanzada E2E** (cookies/CORS/gzip con `--cookies`/`--origin`) **hit_regions** en `RenderOutput` y **event loop presupuestado** (reloj virtual, lifecycle, timers, re-render único) y **fuentes web E2E** (`@font-face`/WOFF2, DB por render, stats/timing) y **SVG E2E** en `render` (`<img>`/background-image, cap 16 Mpx/64 MiB RGBA, `svgs_rendered`; 5 fixtures `svg_*`), **SVG inline** (pre-proceso a `<img>` sintético, caps 256/1 MiB/8 MiB, `inline_svgs`; 7 fixtures y 10 tests) y **event loop determinista** (sólo reloj virtual; fixture `js_event_timeout` md5 `07682088…`), **APIs de navegador** (`getComputedStyle` pre-script, matchMedia/viewport, rAF epoch fijo 48 ms, onerror/consola ⇒ warnings) y multi-capa backgrounds E2E (`c357cc2c…`); 123 tests render. **Ronda 18**: SameSite+PSL reducida, Referrer-Policy (8), Range/206 y cache refinada (net 207). **Ronda 19**: `ReferrerConfig` instalado en el loader de `render` (header>meta>default) ⇒ `Referer`/SameSite reales en subrecursos + `RenderStats.referrer_policy`, estado CSS real con `with_state` en render; render **136**. **Ronda 23**: HTTP/2 **flow control** (ventanas conexión/stream, WINDOW_UPDATE/FLOW_CONTROL_ERROR), **multiplexado ≥2 streams** (`get_many`, scheduler, RST aislado, cap 8) y **SETTINGS** avanzados con ACK; EXIF/JPEG progresivo/WebP detectado en imagen. Pendientes push/prioridades/h2c/trailers/request bodies, streaming, PSL completa y multi-range. |
| **Fase 6: Integración JS & DOM Bindings** | `[█████████░] 90%` | *EN CURSO (WP-A/B/6.4 + eventos + APIs de navegador HECHOS)* | 71 tests `crates/js` + scripting E2E en `render` y en el parser | **Boa 0.21.1** embebido (feature `js` ON por defecto, stub `--no-default-features`) con `Budget`/`RuntimeLimits`, bindings DOM y scheduler base. **`render` ejecuta los `<script>`** (inline/`src`) post-parseo con caps/budgets, drena microtasks/timers, captura console y re-renderiza el DOM mutado (mutaciones visibles en el PNG; `--no-scripting`); 7 fixtures `js_*`. **WP-A HECHO**: `EventTarget` con captura+burbuja, `Event`/`CustomEvent` y timers con reloj inyectable (`setTimeout`/`setInterval`, `advance`/`run_until`/`now_ms`/`pending_timers`). **WP-B HECHO** (`CSSStyleDeclaration`, `getComputedStyle` provider, `NodeList`, `matches`/`closest`, `insertAdjacentHTML/Text`, identidad de wrappers) y **WP-6.4 HECHO** (parser-inserted + `document.write` ⇒ árbol 3.904/3.904). Event loop presupuestado en `render` (reloj virtual, `DOMContentLoaded`/`load`, timers). **Eventos avanzados (ronda 12) HECHOS**: `currentTarget`/`eventPhase`, `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror`, `document.close()`, `estimated_heap_bytes()` y excepciones de listeners capturadas. Pendientes: `document.onerror`, módulos ES, memoria real, provider cableado al pipeline y event loop vivo en el shell. [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md). |
| **Fase 7: Ventana OS & Shell** | `[█████████░] 98%` | *EN CURSO* | 238 tests en `crates/shell` | Ventana usable (`winit`+`softbuffer`, sin GPU) con scroll (cap 8192 CSS px), entrada, HiDPI, render en worker y `--screenshot` headless (640×400 `md5 7324e851…`). **Navegación real (WP-7.5)**: clic en enlaces (`Url::join`), historial (`NavigationState`: back/forward/replace/reload, cap 256), Alt+←/→, `r`, título dinámico (`<title>`+URL+% scroll) y errores de navegación; **hit regions nativos** (`RenderOutput.hit_regions` + `paint::find_link_at`, fallback layout+DOM), CLI `--cookies`/`--origin`, **zoom** (`Ctrl+=/-/0`, `--zoom` 0.5–3.0) **auto-refresh opt-in** (`--refresh-ms`) y **formularios editables (ronda 17)**: UA styles, `src/forms.rs` con FocusState/TextEdit UTF-8/snapshot `data:` URL y patch+re-render (clic enfoca, teclado/Backspace/Tab/Esc, checkbox/radio toggle, Enter nunca envía; scripts off al editar); fixture `forms.html` 640×480 md5 `b8b9a20…`. **Ronda 19**: **find-in-page** (`Ctrl+F`, `find.rs` puro, Enter/Shift+Enter, Esc, scroll al hit, overlays amarillos, `--find` headless), F5/g reload, Alt+Home y Ctrl+D; básic golden `7324e851…` intacto. **Ronda 20**: **pestañas** (`tabs.rs` puro: estado por pestaña navigation/title/url/scroll/zoom/find, MAX 32, Ctrl+T/W/Tab/Shift+Tab, Alt+1..9, `--tabs URL` repetible, título `[N/M]`, frames por generación); basic golden `7324e851…` intacto. Pendientes: barra visual de pestañas, reordenar/sesión/reopen, widgets nativos/select, validación/submit y event loop vivo. |

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

**Estado ~100 %** para el objetivo «HTML real → PNG». Implementados: pipeline, tipografía real (**+ fuentes web `@font-face`/WOFF2 E2E desde la ronda 13**), bordes por lado, posicionamiento/floats/overflow, stacking/`opacity`, tablas CSS 2.1, `inline-block`, cajas reemplazadas, **imágenes `<img>` E2E**, PNG comprimido propio, DOM generacional y painter iterativo. **Crate `svg` propio (rondas 13/16/17, 73 tests) ya cableado E2E en `render`** para `<img src=*.svg>`, `background-image` **e inline `<svg>`** (pre-proceso a `<img>` sintético con caps 256/1 MiB/8 MiB, `svgs_rendered`/`inline_svgs`; 11+10 tests). **Ronda 15**: `background-repeat/size/position` reales en paint (tiling/cover/contain, cap 4096, legacy flag) y **IFC −28 %** con geometría byte-idéntica. **Ronda 16**: backgrounds multicapa/origin/clip/attachment y CSS+gradientes internos en SVG. **Ronda 17**: **Flexbox MVP** (`layout/flex.rs`), **`border-radius`/`box-shadow`/`text-shadow`** con blur determinista, **formularios UA** y **SVG `<pattern>`/`<text>` básico**. **Ronda 18**: **Grid MVP**, **clipping real `overflow:hidden`+radius**, **`background-clip:text`** y **CSS vars/calc/pseudo-clases**. **Ronda 19**: **gradientes CSS E2E** (linear/radial/repeating en capas de background-image, stops/caps, `DisplayItem::Gradient` por tile), **grid avanzado** (placement explícito con spans/negativas, `minmax`, `repeat(auto-fill)`, `order`) y **find-in-page en shell**. **Ronda 20**: **conic-gradient** + color hints + espacios OKLab/sRGB-linear, **calc() mixto %+px** (`CalcLengths`), **sticky/typography** y **pestañas**. **Ronda 21 (rendimiento, presupuestos recuperados y NO subidos)**: html fast paths/tokenizer bytes (realistic-1mb 20,0→16,4 ms), css cascade FNV + caché de estilos (33,91 ms), layout interning de `ComputedStyle` (36,4 ms), paint fast path por caja (build-display 7,84 ms), pipeline ~132 ms (≤140 ✅) y layout ≤43 ✅. **Ronda 22**: **JPEG/GIF desde cero**, **efectos SVG** (clipPath/mask/filtros), **grid completo** (áreas/líneas nombradas/dense/auto-fit/content distribution) y **filtros CSS/mix-blend/image-set** con `DisplayItem::EffectGroup` offscreen. **Ronda 23**: **EXIF orientation completa** (1–8) y **JPEG progresivo subset** (gris exacto; color refinement documentado `UnsupportedFeature`), **WebP detectado sin decoder**, y perf recuperada. **Ronda 24**: **JPEG progresivo color completo** y **WebP VP8L subset**, **aspect-ratio/object-fit/backdrop-filter** en css/paint/layout; pendientes VP8 lossy/transforms+LZ77 completos, display-p3, SVG animation. `background-image`/`list-style-image` y el **texto de celdas de tabla** ya se pintan (render E2E). **Rendimiento (ronda 12)**: layout `-14,6 %` con `LayoutBox.style: Arc<ComputedStyle>` (LayoutBox 496→176 B) y encoder **PNG −30,5 % con bitstream byte a byte idéntico**; el cierre de ronda 13 arregla `html/fast_hash` ⇒ `html/massive-end-p` ~19 ms; **ronda 15**: IFC **−28 %** (layout 50,5→36,3 ms) y **pipeline 132,9→120,5 ms** con goldens byte-idénticos. Pendientes menores: `background-clip:text`, `background-attachment` local, grid/`order`, kerning/italic en `<text>`, block-in-inline y colores de `border-collapse`.

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

## FASE 5: Red, Render Pipeline & Imágenes (EN CURSO, ~97%)

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

## FASE 6: Integración JS & DOM Bindings (EN CURSO — WP-A/B/6.4 + eventos + APIs navegador HECHOS, 2026-11-10)

**Estado**: **WP-6.1 HECHO** (`crates/js` con **Boa 0.21.1**, feature `js` ON por defecto), **WP-6.2 lado host HECHO** (`render` ejecuta `<script>` inline/`src` con `ScriptingMode`/`ScriptingLimits`, caps y re-render del DOM mutado; 7 fixtures `js_*`), **WP-A HECHO** (ronda 10: `EventTarget` captura+burbuja, `Event`/`CustomEvent`, timers con reloj inyectable y `document` como target), **WP-B HECHO** (ronda 11: `CSSStyleDeclaration` completa —`cssText`/`length`/`item`/`getPropertyValue`/`setProperty`/`removeProperty`—, `getComputedStyle` con provider inyectable, `NodeList` snapshot, `matches`/`closest`, `insertAdjacentHTML/Text` e **identidad de wrappers**) y **WP-6.4 HECHO** (ronda 11: hook `html::scripting::{ScriptExecutor, ScriptExecution}`; scripts parser-inserted al `</script>`/EOF; `document.write` re-tokenizado con `Tokenizer::insert_input` en la posición actual, anidados incluidos; externos `src` los ejecuta el host; errores no fatales; camino sin scripting zero-cost ⇒ **árbol 3.904/3.904 y allowlist de tree vacía**). **APIs de navegador y cableado real (ronda 16)**: `matchMedia`, viewport/DPR/scroll, `requestAnimationFrame` (16 ms virtuales), `onerror`, `readyState`, consola ampliada, `getComputedStyle` real pre-script; en `render`: `RenderTimings::styles_pre_script`, MediaContext/viewport reales, rAF con epoch fijo 48 ms y warnings `console_errors`. **Event loop presupuestado y determinista en `render`** (ronda 15, `b808d7b`): sólo reloj virtual (`run_until` en pasos de 1 ms, sin reloj de pared), `DOMContentLoaded`/`load`, timers con delay (`max_tasks` 10.000, `max_virtual_time` 1000), un solo re-render, stats `script_*`/`timings.event_loop`; 6 fixtures `js_event_*` y 11 tests (fixture `js_event_timeout` md5 `07682088…`). **Eventos avanzados (ronda 12) HECHO**: `currentTarget` real + `eventPhase` 1/2/3 (capture en el target), `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror` (elemento/document/window), `document.close()` no-op, `estimated_heap_bytes()` aproximado, dispatch no-`Event` ⇒ `TypeError` y excepciones de listeners capturadas (`last_event_error()`) sin abortar el resto. **168 tests js** y **1929 tests** de workspace; **Fase 6 ≈100 %**; **fuzz `js_eval`** añadido en la ronda 18 y el estado CSS real (`with_state`) integrado en render en la ronda 19.

**Decisiones registradas** (detalle y racional en [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md)):
1. **Motor = Boa `boa_engine` 0.21.x** (pure Rust, MSRV Rust 1.88, licencia Unlicense OR MIT; se elige MIT). Se descartan motor propio, QuickJS/rquickjs (queda como plan de migración), V8/SpiderMonkey y el stub.
2. **Feature `js` ENCENDIDA POR DEFECTO**: build y CI compilan con JS; se mantiene un job **`cargo test --workspace --no-default-features`** para el build mínimo std-only, y el job MSRV cubre ambas configuraciones.
3. **Alcance inicial COMPLETO**: `document.write` + bindings DOM + eventos + timers + CSSOM + `querySelector` (WP-6.1–6.7, **≈25–30 días-persona**).

**Implementado (WP-6.1 + WP-6.2 host + WP-A + WP-B + WP-6.4 + eventos avanzados)**: realm Boa persistente + `JsRuntime::with_document`; handles laterales por `NodeId` con comprobación de generación (`NodeId` stale ⇒ `TypeError`); `document.write`, búsquedas, `querySelector(All)`, creación/mutación de nodos, `innerHTML`/`outerHTML`, atributos, `style`/`classList`; **eventos** `EventTarget`/`Event`/`CustomEvent` con captura y burbuja en nodos/document/window; **timers** en el scheduler Rust con reloj inyectable (`advance`/`run_until`/`now_ms`/`pending_timers`, `run_tasks` solo vencidos por reloj real); `Budget`/`RuntimeLimits` anti-DoS y sin red/FS; **host en `render`**: `ScriptingMode`, `RenderOptions::{scripting, scripting_limits}`, `ScriptingLimits`/`ScriptBudget::SMALL`, ejecución en orden con budget por script+total, stats (`scripts_executed/failed/bytes/console`, `timings.scripting`) y re-render del DOM mutado. Evidencia: `js_text.html` con scripting 15.876 B `md5 e1da51d4…` vs `--no-scripting` 15.499 B `md5 53f67e77…`; showcase golden intacto `c95c7eff…`. **WP-B**: `CSSStyleDeclaration` completa, `getComputedStyle` con provider inyectable, `NodeList`, `matches`/`closest`, `insertAdjacentHTML`/`Text`, caché de identidad wrapper↔nodo. **WP-6.4 y event loop**: el parser ejecuta scripts parser-inserted (`ScriptExecutor` + re-tokenización de `document.write`) y `render` corre el event loop presupuestado. **Eventos avanzados (ronda 12)**: `currentTarget`/`eventPhase` reales, `once`/`passive`, `removeEventListener` por identidad, handlers inline `onclick`/`onload`/`onerror`, `document.close()`, `estimated_heap_bytes()` y excepciones de listeners capturadas. Límites vigentes: `document.onerror` para errores de scripts pendiente, sin módulos ES, sin contabilidad de memoria real, provider de computed styles aún no cableado al pipeline y event loop no vivo en el shell.

**Siguientes WP**: `document.onerror` para errores de scripts; módulos ES y `async`/`defer` externos; contabilidad de memoria real y aislamiento de realms; cablear el provider de `getComputedStyle` al pipeline (cascada real) y event loop vivo en el shell; endurecimiento/fuzz de `js`/`render`, bench E2E con JS y caché de Boa en CI (+25 s de build). Total **≈25–30 días-persona**.

**Qué NO se hace**: motor JS propio, delegar parseo HTML/CSS/layout/paint a JS, JIT, exponer `fetch`/filesystem por defecto ni entrar en el build mínimo.

## FASE 7: Ventana del Sistema & Shell (EN CURSO, ~75%)

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

Limitaciones actuales: sin pestañas/descargas ni widgets nativos/select dropdown, formularios editables ya HECHO (ronda 17: foco, teclado con caret UTF-8, Tab/checkbox, snapshot re-render), fragmentos = nueva navegación, >8192 CSS px recortado, sin JS ni event loop vivo en el bucle del host (el pipeline `render` sí ejecuta el event loop; el auto-refresh repite el primer frame de animaciones con reloj virtual); ventana no verificada visualmente en CI (headless), pero compila y el modo headless, `NavigationState`, la resolución de enlaces con hit regions, zoom, auto-refresh y formularios están testeados (108 tests).

### Fuera del alcance inicial de Fase 7

GPU/compositor, pestañas, extensiones, descargas, impresión, sandbox de procesos por sitio (se diseñará con la Fase 6) y accesibilidad completa.

---

## 🗺️ Plan hacia la primera versión Pre-Alpha

### Estado actual (HEAD `e454d5c`, 2026-11-12)

- **Tests**: 2.213 pasando, 0 fallos
- **Gate**: PASS (tokenizer 7.050/7.050, tree 3.904/3.904, serializer 230/230, CSS 30/30)
- **Clippy/fmt**: limpios
- **PRs abiertos**: 0 (35 PRs de seguridad fusionados)
- **Issues abiertos**: 30 (fuzz/tooling/docs/CI)
- **Binario**: `megi` (26 MB, ELF x86-64) — renderiza páginas HTML/CSS a PNG headless

### Fases de ejecución hacia Pre-Alpha

#### Fase A: Seguridad (1-2 semanas) — PRIORIDAD 0

| # | Tarea | Área | Esfuerzo | Issue |
|---|-------|------|----------|-------|
| A1 | cargo audit + cargo deny en CI | CI/Security | Bajo | #951 |
| A2 | SBOM (Software Bill of Materials) | Security | Medio | #952 |
| A3 | ASan/UBSan en fuzz jobs | Fuzz/Security | Medio | #953 |
| A4 | cargo geiger para unsafe audit | Security | Bajo | #954 |
| A5 | Dependabot para actualizaciones automáticas | Security | Bajo | #955 |
| A6 | SECURITY.md con proceso CVE y timeline | Security | Bajo | #956 |

#### Fase B: Colaboración (1 semana) — PRIORIDAD 0

| # | Tarea | Área | Esfuerzo | Issue |
|---|-------|------|----------|-------|
| B1 | Templates de issues y PRs | Colaboración | Bajo | #957 |
| B2 | CODEOWNERS por crate | Colaboración | Bajo | #958 |
| B3 | Labels estandarizados y milestones | Colaboración | Bajo | #959 |
| B4 | CONTRIBUTING.md con guía de commits/DCO | Colaboración | Bajo | #960 |
| B5 | GitHub Projects board | Colaboración | Bajo | #970 |
| B6 | good-first-issue labels | Colaboración | Bajo | #970 |

#### Fase C: Compatibilidad web crítica (2-4 semanas) — PRIORIDAD 1

| # | Tarea | Área | Esfuerzo | Issue |
|---|-------|------|----------|-------|
| C1 | CSS animations (@keyframes, transition) | CSS/Render/Paint | 2-3 semanas | #942, #980, #981, #983 |
| C2 | WebP animado (VP8X/ALPH/ANIM) | Image/Render | 1-2 semanas | #943, #984, #985 |
| C3 | Fuentes internacionales (bidi, CJK) | Fonts/Layout/Paint | 2-3 semanas | #944, #986, #987 |

#### Fase D: UX y polish (2-3 semanas) — PRIORIDAD 1

| # | Tarea | Área | Esfuerzo | Issue |
|---|-------|------|----------|-------|
| D1 | Save-as/prompt visual, descargas, cookies/passwords | Shell/Net | 1-2 semanas | #945, #988 |
| D2 | Multi-select, native widgets, accessibility | Shell/DOM | 1-2 semanas | #946, #989 |
| D3 | Per-tab process isolation | Shell | 2 semanas | #946 |

#### Fase E: Optimización y testing (2-3 semanas) — PRIORIDAD 2

| # | Tarea | Área | Esfuerzo | Issue |
|---|-------|------|----------|-------|
| E1 | Fingerprint cache en layout | Layout | 2-3 días | #947, #990 |
| E2 | Cache de parseo HTML/CSS | HTML/CSS | 1 semana | #947, #990 |
| E3 | Fuzz campaigns largas (600s por target) | Fuzz/CI | 1-2 días | #948, #991 |
| E4 | WPT completo | Test harness | 1-2 semanas | #948, #991 |
| E5 | Bench real-page E2E | Bench | 3-5 días | #948, #991 |

#### Fase F: Documentación y release (1-2 semanas) — PRIORIDAD 3

| # | Tarea | Área | Esfuerzo | Issue |
|---|-------|------|----------|-------|
| F1 | Actualizar todos los docs | Docs | 2-3 días | #949, #992 |
| F2 | Rustdoc en GitHub Pages | Docs | 1 día | #968 |
| F3 | Getting started guide | Docs | 1 día | #969 |
| F4 | Semantic versioning + release process | Release | 1 semana | #965, #966, #967 |
| F5 | Changelog automatizado | Release | 1 día | #966 |

### Métricas de éxito para Pre-Alpha

| Métrica | Actual | Objetivo Pre-Alpha |
|---|---|---|
| Tests | 2.213 | 2.500+ |
| Issues abiertos | 30 | < 20 |
| Fuzz targets | 16 | 20+ |
| Bench filas | 19 | 25+ |
| Fases completas | F1-F4 (100%), F5-F7 (98-100%) | Todas al 100% |
| Presupuestos | Todos cumplidos | Todos cumplidos con margen |
| Gate conformidad | 100% | 100% |
| Goldens | Byte-idénticos | Byte-idénticos |
| CI jobs | 2 (quality, conformance) | 6+ |
| Security scans | 0 | cargo audit + deny + geiger |
| Release process | Ninguno | Semántico con artifacts |
| Docs publicadas | No | GitHub Pages |

### Riesgos y mitigaciones

| Riesgo | Mitigación |
|---|---|
| CSS animations complejas | Empezar con transition, luego @keyframes |
| WebP animado requiere timer | Usar reloj virtual del render |
| Bidi/CJK requiere fuentes | Priorizar shaping, luego bidi |
| Fuzz campaigns largas | Ejecutar en CI (600s por target) |
| Conflictos en merges | Resolver manualmente, verificar build |
| Cargo audit encuentra vulnerabilidades | Triaje rápido, fix o documentar |
| Coverage threshold demasiado alto | Empezar en 60%, subir gradualmente |

### Asignación de agentes sugerida

| Agente | Área | Tareas |
|---|---|---|
| paint-pipeline | Paint/Bench | Backdrop blur, PNG regions, rasterizer budgets |
| css-foundations | CSS/Net | CSS animations, HTTP/2 pooling, container queries |
| core-engine-auditor | JS/DOM | fetch streaming, import maps, conformance |
| core-robustness | HTML/DOM | Security fixes, assign_form_ids, collect_text |
| harness-integrity | Shell | Save-as, multi-select, accessibility, form ids |
| conformance-auditor | Fuzz/CI | Campañas largas, targets nuevos, corpus |
| paint-layout-auditor | Layout/Paint | Fingerprint cache, grid spans, layout bounds |

### Issues creados para colaboración

Los siguientes issues están abiertos en GitHub para que la comunidad colabore:

**Seguridad (P0)**: #951, #952, #953, #954, #955, #956
**Colaboración (P0)**: #957, #958, #959, #960, #970
**Robustez (P1)**: #961, #962, #963, #964
**Release (P2)**: #965, #966, #967
**Documentación (P3)**: #968, #969

**Pre-Alpha específicos**: #942, #943, #944, #945, #946, #947, #948, #949, #980, #981, #983, #984, #985, #986, #987, #988, #989, #990, #991, #992

### Conclusión

El proyecto Megi está en un estado muy avanzado (2.213 tests, gate PASS, todas las áreas principales cubiertas). Lo que falta para un pre-alpha funcional es:

1. **Compatibilidad web**: CSS animations, WebP animado, fuentes internacionales
2. **UX**: Save-as visual, multi-select, accessibility
3. **Infraestructura**: seguridad, colaboración, release process
4. **Testing**: fuzz campaigns, WPT completo, bench real-page

Con 4-6 semanas de trabajo enfocado, se puede tener un pre-alpha funcional. La prioridad debe ser compatibilidad (animaciones, WebP) → UX (save-as, cookies) → infraestructura (seguridad, colaboración) → testing (fuzz, WPT).
