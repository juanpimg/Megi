# Auditoría ola 2 — F2 assets (`fonts` / `image` / `svg`)

Prefijo IDs: `w2-assets`. Alcance del brief: todos los `.rs` bajo `crates/fonts/src/`,
`crates/image/src/` y `crates/svg/src/`, leídos línea por línea.
Ejecución en worktree limpio `/tmp/opencode/megi-verify` (HEAD `3c06db8`);
informe escrito en el árbol principal. `crates/fonts`, `crates/image` y `crates/svg`
no tienen cambios sin commitear (verificado con `git status`), por lo que ningún
hallazgo depende de WIP del usuario (`excluir: WIP del usuario` no aplica).

## Alcance

29 archivos (20 557 líneas), leídos completos de arriba abajo:

- `crates/image/src/`: `inflate.rs` (1669), `png.rs` (1079), `gif.rs` (467),
  `jpeg.rs` (1737), `exif.rs` (331), `webp.rs` (1437), `vp8.rs` (182),
  `vp8_decoder.rs` (1855), `vp8_tables.rs` (500), `lib.rs` (665), `crc32.rs` (65),
  `error.rs` (93).
- `crates/fonts/src/`: `db.rs` (1256), `font.rs` (388), `types.rs` (183),
  `web.rs` (198), `lib.rs` (81), `error.rs` (65).
- `crates/svg/src/`: `document.rs` (2928), `raster.rs` (2169), `path.rs` (601),
  `transform.rs` (355), `xml.rs` (460), `css.rs` (532), `color.rs` (192),
  `gradient.rs` (494), `pattern.rs` (101), `text.rs` (123), `lib.rs` (351).

Focos del brief verificados: inflate/DEFLATE propio, PNG filters/Adam7/bit depths,
GIF LZW/disposal, JPEG progresivo, WebP VP8L/VP8X/ALPH/ANIM, EXIF orientation,
`decode_scaled`, límites anti-DoS; fuentes WOFF2, alias/generic family order,
fallback DB global, `scan_faces_for_char`, uncached failures; SVG path/arcs,
miter joins (#1324), pattern cycles, clip/mask budgets (#1267), gradient stop
búsqueda O(n) por píxel (#1270), `<switch>`, `currentColor`, text/tspan.

## Cobertura

- `image/inflate.rs` — leído sin hallazgo (límites por escritura, Huffman canónico
  con chequeo de sobresuscripción, streaming con checksum; ver Concern C3).
- `image/png.rs` — leído sin hallazgo publicable (Adam7, filtros 0–4, Paeth,
  sub-byte scaling, tRNS/PLTE validados; comentario rancio en `decode_png_scaled`
  recogido en w2-assets-11).
- `image/gif.rs` — leído sin hallazgo publicable (LZW con KwKwK y guarda cíclica,
  disposal documentado primer-frame, tablas acotadas; comentario MSB/LSB en
  w2-assets-11).
- `image/jpeg.rs` — leído: 1 hallazgo (w2-assets-10, doc vs código en refines
  entrelazados). Huffman/DC/AC/IDCT/reduced-IDCT revisados sin hallazgo.
- `image/exif.rs` — leído sin hallazgo (offsets con `checked_add`, count>512
  rechazado, SHORT count>=3 por offset).
- `image/webp.rs` — leído: 1 hallazgo (w2-assets-09, ALPH filter 2/3).
  VP8L (transforms, Huffman, LZ77, colour cache), VP8X, ANIM/ANMF con budget
  agregado, trailing-bits validados: sin hallazgo adicional.
- `image/vp8.rs`, `image/vp8_decoder.rs`, `image/vp8_tables.rs` — leídos sin
  hallazgo (keyframe-only, particiones acotadas, `filter_limits` equivale al
  spec pese a la condición redundante `level >= 20 || level >= 15`; el tip miter
  del loop filter no se audita aquí — es VP8, no SVG).
- `image/lib.rs` — leído: parte de w2-assets-11 (docs rancios). `decode_scaled`
  progresivo con fallback correcto; `apply_orientation`/`resize_nearest_dims`/
  `downsample_nearest` acotados por construcción.
- `image/crc32.rs`, `image/error.rs` — leídos sin hallazgo.
- `fonts/db.rs` — leído sin hallazgo publicable (dedup por hash+comparación,
  caps 256 caras/64 MiB, fallback con caché invalidada en cargas; ver Concerns
  C1/C2).
- `fonts/font.rs` — leído sin hallazgo (raster cap 16 Mpx/máscara, `size<=4096`,
  `u16` ids con error tipado #1409, métricas saneadas #1237).
- `fonts/web.rs` — leído sin hallazgo (WOFF2 validado antes del decoder,
  `catch_unwind`, residual Brotli ya documentado en el propio código).
- `fonts/types.rs`, `fonts/lib.rs`, `fonts/error.rs` — leídos sin hallazgo.
- `svg/path.rs` — leído sin hallazgo (budget por punto, anti-bucle `Z`,
  arcos con `lambda` F.6.6, recursión acotada a profundidad 12).
- `svg/transform.rs`, `svg/xml.rs`, `svg/css.rs` — leídos sin hallazgo
  (caps 32 funciones/64 valores, budget global de selectores, XXE imposible:
  no hay fetch externo/DTD).
- `svg/gradient.rs` — leído: 1 hallazgo (w2-assets-08, dedup #1270).
- `svg/pattern.rs` — leído sin hallazgo (bucle de tiles analizado: el span en
  `f32` y el `as i64` saturante no pueden exceder el chequeo más que por
  redondeo marginal; budgets global y por píxel aplicados).
- `svg/text.rs` — leído: origen de w2-assets-06 (parte del split naíf).
- `svg/lib.rs` — leído: parte de w2-assets-11.
- `svg/document.rs` — leído: 4 hallazgos (w2-assets-01, 05, 06, 07).
- `svg/raster.rs` — leído: 1 hallazgo (w2-assets-02, dedup #1324) + parte de
  w2-assets-08.

Verificado-fijo (no es hallazgo): `currentColor` + propiedad `color` (#1276/#1331):
`color="red"` + `fill="currentColor"` pinta rojo (400 px, repro `currentcolor.svg`).
`gradient.rs`/`raster.rs`/`document.rs` ya contienen disclaimers de los diferidos.

## Hallazgos

### w2-assets-01 — `<switch>` pinta todos los hijos en vez del primero válido
- `archivo:línea`: `crates/svg/src/document.rs:1067`
- `lente`: contrato vs especificación
- `severidad`: P2 — `categoría`: bug
- `title`: la rama `"g" | "a" | "switch"` trata `switch` como grupo y pinta todos
  los hijos; SVG 1.1 §5.8 exige pintar sólo el primero que pase
  `requiredFeatures`/`requiredExtensions`/`systemLanguage`.
- `evidence`: render con `svg_to_png` (worktree 3c06db8) del repro `switch.svg`
  (hijo 1 con `systemLanguage="xx-invalid"` rojo + hijo 2 azul sin tests):
  `painted= 5000 top= [((255, 0, 0, 255), 2500), ((0, 0, 255, 255), 2500)]`
  (ambos rectángulos completos; esperado sólo el azul). Incluso sin atributos de
  test, el spec pinta sólo el primer hijo.
- `impact`: contenido alternativo/condicional de `<switch>` se duplica en pantalla.
- `expected`: SVG 1.1 §5.8 (`switch`: render the first child whose tests pass).
- `fix`: evaluar por hijo `systemLanguage` (lista del usuario; `required*` →
  no pasar) y pintar sólo el primero válido; sin tests → primero.
- `repro`: `docs/audit/findings/wave2/repro-w2-assets/switch.svg` +
  `cargo run -p svg --example svg_to_png -- switch.svg out.png` + conteo de píxeles.
- `dedup`: #1329 (y #1273) — evidencia nueva de que sigue vivo en HEAD.

### w2-assets-02 — `stroke-linejoin="miter"` deja un hueco en vez del tip
- `archivo:línea`: `crates/svg/src/raster.rs:1883-1920` (`stamp_join`, rama `Miter`)
- `lente`: aritmética
- `severidad`: P1 — `categoría`: bug
- `title`: la intersección de las líneas de offset está mal planteada: el código
  intersecta las rectas `{v + t·n0}` y `{v + u·n1}` (ambas pasan por `v`), de modo
  que `t ≡ -1` siempre y el "tip" es un punto fijo del lado interior; el
  `miter_limit` nunca depende del ángulo y falta el pico exterior.
- `evidence`: `miter.svg` (polilínea 10,90→100,10→190,90, width 12, miterlimit 10):
  columna x=100 con alfa 0 en filas 0–8 (`[(0,0),(1,0),…,(8,0),(9,159),…]`),
  mientras `miter-bevel.svg` pinta filas 5+ (`(5,128),(6,255),…`). El tip
  geométrico correcto es (100, 1.97) — cálculo pitagórico en el informe de
  ejecución — y está vacío. Topmost ink fila 5 en ambos renders.
- `impact`: picos en miter quedan huecos/recortados como bevel.
- `expected`: SVG 1.1 §11.4 (miter join = intersección de las líneas de offset;
  fallback a bevel sólo si `miterLength > miterlimit × stroke-width`).
- `fix`: resolver `v + h·n0 + s·d0 = v + h·n1 + t·d1` y usar ese tip;
  `distance = |tip − v|/half` para el chequeo del límite.
- `repro`: `repro-w2-assets/miter.svg`, `miter-bevel.svg` + columna x=100.
- `dedup`: #1324 — evidencia nueva (píxeles + derivación `t ≡ -1`) en HEAD.

### w2-assets-03 — `fill` con 132 de los 148 colores CSS con nombre → sin paint
- `archivo:línea`: `crates/svg/src/color.rs:138-163` (`named_color`)
- `lente`: contrato vs especificación
- `severidad`: P2 — `categoría`: bug
- `title`: sólo 16 colores básicos + `transparent`; p.ej. `rebeccapurple`,
  `orange` sí está pero `coral`/`darkblue`/`lightgray` no. `parse_color` → `None`
  → `Paint::None` (transparente) en vez del color.
- `evidence`: `namedcolor.svg` (`fill="rebeccapurple"`) renderiza `painted= 0`
  (esperado `(102, 51, 153)` opaco). El fix del crate `css` (#1129, 148 colores)
  no se aplicó al crate `svg`.
- `impact`: SVGs válidos con colores comunes quedan invisibles.
- `expected`: CSS Color 3 / SVG 1.1 §4.2 (147 nombres; `rebeccapurple` es CSS Color 4).
- `fix`: tabla completa de 148 nombres (reutilizar la del crate css o una tabla
  compartida).
- `repro`: `repro-w2-assets/namedcolor.svg`.
- `dedup`: new (relacionado con #1129 pero de otro crate; en `svg` sigue vivo).

### w2-assets-04 — sin soporte `hsl()` en paints SVG
- `archivo:línea`: `crates/svg/src/color.rs:68-71`
- `lente`: contrato vs especificación
- `severidad`: P2 — `categoría`: bug
- `title`: `parse_color("hsl(0, 100%, 50%)")` → `None` → `Paint::None`; la forma
  funcional más usada de CSS Color 3 pinta transparente.
- `evidence`: `hsl.svg` renderiza `painted= 0` (esperado rojo `(255,0,0,255)`).
- `impact`: SVGs válidos con `hsl()` quedan invisibles.
- `expected`: CSS Color 3 §4.2.4 / SVG (functional notation).
- `fix`: parser `hsl()`/`hsla()` (h en grados, s/l en %) con clamp, como el del
  crate css (#1130).
- `repro`: `repro-w2-assets/hsl.svg`.
- `dedup`: new.

### w2-assets-05 — `visibility="inherit"` oculta en vez de heredar
- `archivo:línea`: `crates/svg/src/document.rs:613-616`
- `lente`: contrato vs especificación
- `severidad`: P3 — `categoría`: bug
- `title`: `self.visibility = value == "visible"` convierte `inherit` (y
  cualquier valor no-`visible`) en oculto; con padre visible, `inherit` debe
  pintar.
- `evidence`: `vis-inherit.svg` (rect rojo con `visibility="inherit"` bajo raíz
  visible) renderiza `painted= 0` (esperado 400 px rojos).
- `impact`: subárboles con `visibility="inherit"` explícito desaparecen.
- `expected`: SVG 1.1 §5.3 / CSS2 `visibility` (heredable; `inherit` toma el valor
  del padre).
- `fix`: no tocar `visibility` cuando el valor es `inherit` (conservar el del
  padre clonado); mantener `hidden|collapse → false`.
- `repro`: `repro-w2-assets/vis-inherit.svg`.
- `dedup`: new.

### w2-assets-06 — `first_number` ignora unidades: `x="10px"` en `<text>` = 0
- `archivo:línea`: `crates/svg/src/document.rs:2813-2821` (`first_number`),
  usado en `push_text` (`document.rs:1221-1224`), `stdDeviation`, `dx/dy` de
  filtros, `pattern x/y`, `k1..k4`
- `lente`: contrato vs especificación
- `severidad`: P2 — `categoría`: bug
- `title`: `first_number` hace `parse::<f32>()` directo del token, por lo que
  `"10px"` falla → `0.0`; además es inconsistente con `rect`/`length_attr` (que
  sí acepta `px`). Efecto grave: `stdDeviation="2px"` → `0.0` → el primitivo
  `feGaussianBlur` se descarta entero (`document.rs:2621-2628`).
- `evidence`: `text-px.svg` vs `text-num.svg` (mismo texto, `x="10px"` vs `x="10"`):
  tinta más a la izquierda en x=1 frente a x=11 (mismo nº de glifos, 70 px).
- `impact`: texto desplazado al origen; filtros con unidades en `stdDeviation`
  se pierden en silencio.
- `expected`: SVG 1.1 §4 (las longitudes aceptan sufijos de unidad; `px` es la
  unidad nula).
- `fix`: usar `parse_length`-con-referencia o desnudar sufijo `px` en
  `first_number`.
- `repro`: `repro-w2-assets/text-px.svg`, `text-num.svg`.
- `dedup`: new.

### w2-assets-07 — `opacity="50%"` en la raíz se ignora (sólo decimal)
- `archivo:línea`: `crates/svg/src/document.rs:419-422`
- `lente`: contrato vs especificación — `severidad`: P3 — `categoría`: bug
- `title`: la opacidad raíz usa `v.parse::<f32>()` en vez de `parse_opacity`
  (que sí acepta `%`); `opacity="50%"` → parse falla → `unwrap_or(1.0)`.
  El resto de elementos sí acepta `%` (`parse_opacity_attribute`).
- `evidence`: `root-opacity-pct.svg` (`opacity="50%"`, rect rojo) pinta centro
  `(255, 0, 0, 255)` (esperado `(255, 0, 0, 128)`); con `opacity="0.5"` sí da 128.
- `impact`: inconsistencia raíz vs resto; opacidad porcentual raíz sin efecto.
- `expected`: SVG 1.1 §12 (`opacity` acepta number o percentage).
- `fix`: `parse_opacity_attribute(root, "opacity").unwrap_or(1.0)`.
- `repro`: `repro-w2-assets/root-opacity-pct.svg` (+ `root-opacity.svg` control).
- `dedup`: new.

### w2-assets-08 — gradiente: búsqueda lineal de stops por píxel cubierto
- `archivo:línea`: `crates/svg/src/gradient.rs:225-242` (`evaluate`), llamado en
  `crates/svg/src/raster.rs:1583` (`composite_gradient`, por píxel)
- `lente`: límites y DoS (perf) — `severidad`: P2 — `categoría`: perf
- `title`: `evaluate` recorre linealmente hasta 256 stops por píxel; los stops
  están ordenados (`sort_by` en `parse_stops`, `gradient.rs:365`) y admiten
  bisección.
- `evidence`: rect 1600×1600 a pantalla completa, build debug: 2 stops
  1049–1315 ms vs 256 stops 1848–2567 ms (3 pasadas cada uno; mismo `svg_to_png`).
  El coste escala con nº de stops × píxeles.
- `impact`: SVGs con muchos stops degradan el render (en release el factor por
  píxel persiste aunque el absoluto baje).
- `expected`: rendimiento proporcional a `log(stops)` con `partition_point`
  (patrón ya usado en `segment_at`, `raster.rs:951`).
- `fix`: bisección sobre `stops` por `offset` (ordenado) en `evaluate`.
- `repro`: `repro-w2-assets/g2-16.svg`, `g256-16.svg` + cronometraje.
- `dedup`: #1270 (y #1416 de paint es otro crate) — evidencia nueva en HEAD.

### w2-assets-09 — WebP `ALPH` con filtro vertical/gradient se rechaza
- `archivo:línea`: `crates/image/src/webp.rs:1025-1029` (`apply_alpha_plane`)
- `lente`: contrato vs especificación — `severidad`: P2 — `categoría`: bug
- `title`: sólo métodos 0 (none) y 1 (horizontal); 2 (vertical) y 3 (gradient),
  válidos según el spec del contenedor WebP (byte 0 de `ALPH`), devuelven
  `UnsupportedFeature` y la imagen entera falla.
- `evidence`: `vp8x_alpha_lossy.webp` (16×16, ALPH sin comprimir) con los bits
  de filtro cambiados: `filter=0: OK 16x16`, `filter=1: OK 16x16`,
  `filter=2: ERR unsupported feature: WebP ALPH filtering method`,
  `filter=3: ERR …` (harness `image::decode`, mismos bytes salvo el byte 0).
- `impact`: WebP extended válidos (codificadores emiten 2/3) no abren.
- `expected`: WebP Container Spec ("ALPH chunk", filtering method 0–3).
- `fix`: implementar filtros vertical y gradient (espejos del horizontal por
  columnas / predictor), o decodificarlos a plano sin filtrar equivalente.
- `repro`: `repro-w2-assets/alph-f1.webp` (control OK), `alph-f2.webp`,
  `alph-f3.webp` + `image::decode`.
- `dedup`: new.

### w2-assets-10 — el doc de JPEG progresivo promete rechazar refines entrelazados, pero los acepta
- `archivo:línea`: `crates/image/src/jpeg.rs:8-11` (doc) vs `881-953`
  (`parse_sos`: sólo rechaza `Ns>1 && ss!=0`)
- `lente`: contrato vs documentación — `severidad`: P3 — `categoría`: docs
- `title`: el doc dice que "colour refinement scans (`Ah > 0` con más de un
  componente) retornan `UnsupportedFeature`", pero un scan DC-refine entrelazado
  (`Ss==0, Ah>0, Ns>1`) se decodifica por `decode_dc_refine` sin error.
- `evidence`: `crates/image/tests/fixtures/jpeg/progressive.jpg` contiene el
  scan `Ns=3 Ss=0 Se=0 Ah=1 Al=0` (tabla de SOS extraída por parseo propio) y
  `decode`/`decode_scaled` tienen éxito (tests `jpeg_scaled` y `scaled` en verde).
- `impact`: discrepancia doc/código; el comportamiento real (decodificar) es el
  razonable, pero el contrato escrito miente.
- `expected`: el doc debe decir "sólo se rechazan AC entrelazados (`Ss>0`)".
- `fix`: corregir el docstring (líneas 8–11 y 26) o añadir el rechazo si el
  spec lo exigiera.
- `repro`: parseo SOS del fixture + `cargo test -p image progressive`.
- `dedup`: new.

### w2-assets-11 — ómnibus de docs rancios (contrato escrito ≠ código)
- `archivo:línea`: `crates/image/src/lib.rs:10-12,40-43,122-123`;
  `crates/image/src/png.rs:358`; `crates/image/src/gif.rs:315`
- `lente`: contrato vs documentación — `severidad`: P3 — `categoría`: docs
- `title`: (a) `lib.rs` dice "typed errors for progressive" y "VP8X …
  stay unsupported" cuando ambos se decodifican; (b) `png.rs:358` dice
  "accumulate per-output-pixel RGBA sums" pero el scaled es nearest
  (`decode_png_scaled` doc correcto en 149–155); (c) `gif.rs:315` dice
  "MSB-first bit reader" pero el LZW es LSB-first (línea 323 lo confirma).
- `evidence`: inspección + éxito de decode progresivo/VP8X (suite verde);
  (c) es auto-evidente en el propio archivo.
- `impact`: lectores del contrato esperan errores que ya no existen.
- `expected`: docs = comportamiento.
- `fix`: actualizar los tres docstrings.
- `repro`: lectura + `cargo test -p image -p svg`.
- `dedup`: new.

## Comandos ejecutados

Desde `/tmp/opencode/megi-verify` (HEAD 3c06db8; el árbol principal no compilaba
por cambios WIP en `crates/js`/`crates/render`, fuera de este scope):

- `cargo test -p fonts` → `3, 19, 10, 1 passed; 0 failed` (33 tests).
- `cargo test -p image` → `32, 34, 5, 9, 8, 6 … passed; 0 failed` (123 tests).
- `cargo test -p svg` → 147 passed, 0 failed.
- Total: 303 passed, 0 failed. (Un intento inicial de `cargo test -p fonts -p image -p svg`
  en paralelo falló el linkado por carrera de `rlib` en el `target` compartido;
  reejecutado por crate en serie: verde.)
- `cargo run -p svg --example svg_to_png -- <repro>.svg /tmp/w2-*.png` (10 fixtures):
  salidas `rendered … (100x50/200x100/20x20/100x30 …)`; análisis de píxeles con
  decodificador PNG propio (`zlib` stdlib): switch `red 2500 + blue 2500`;
  namedcolor/hsl/vis-inherit `painted= 0`; text-px ink-x=1 vs text-num ink-x=11;
  root-opacity-pct centro `(255,0,0,255)`; currentcolor 400 px rojos;
  miter col x=100 filas 0–8 alfa 0 vs bevel filas 5+ pintadas; tip correcto
  (100, 1.97).
- Gradientes 1600² (debug): g2 1315/1158/1049 ms; g256 2567/1848/2042 ms.
- ALPH (harness `image::decode`): `filter=0: OK 16x16`, `filter=1: OK 16x16`,
  `filter=2: ERR unsupported feature: WebP ALPH filtering method`,
  `filter=3: ERR …`.
- SOS de `progressive.jpg`: `Ns=3 Ss=0 Se=0 Ah=0 Al=1`, `Ns=1 …` (×5),
  `Ns=3 Ss=0 Se=0 Ah=1 Al=0`, `Ns=1 Ss=1 Se=63 Ah=1 Al=0` (×3) — refine
  entrelazado presente y decodificado.
- Dedup: `FINDINGS.md` + `gh issue list` revisados; #1270/#1273/#1275/#1276/
  #1324/#1329/#1331 cerrados-diferidos pero vivos → dedup con evidencia nueva;
  #1129/#1130 son del crate `css`, no cubren a `svg`.

## Concern

- **C1 (needs-repro, no se publica)**: `<text font-size="5000">` se salta en
  silencio (`raster.rs:1016-1018`, `size > 4096 → return Ok`). No pude renderizar
  la prueba (fallo de bus con `/tmp` al 100% durante el experimento). Pregunta si
  el truncado silencioso es aceptable o debe ser `SvgError::Limit`.
- **C2 (needs-repro)**: `svg::text::families` parte por comas sin respetar
  comillas y desnuda genéricos entrecomillados, divergiendo de
  `fonts::parse_font_families` y del fix #1239 (`"serif"` citado → genérico).
  Sólo evidencia a nivel código (`text.rs:108-123` vs `db.rs:193-301`).
- **C3 (needs-repro)**: `StreamingInflater::feed` sólo drena `input` si
  `bit == 0` (`inflate.rs:811-815`): con feeds pequeños el buffer retiene todo
  el comprimido consumido (memoria O(input) en una API streaming). El harness
  RSS no pudo correrse (disco `/tmp` lleno por `target/` compartido).
- **C4 (nota)**: `FontDatabase::identity_digest` hashea TODOS los bytes de las
  caras (`db.rs:327-331`) y se llama por render (`render/src/lib.rs:1262`):
  coste O(bytes totales) por página. Considerar digest incremental o con tope.
- **C5 (nota)**: `SYSTEM_FALLBACK_DB` (`db.rs:1069-1073`) + `face_data` cachean
  bytes de fuentes del sistema para siempre en el proceso; crecimiento
  monótono ante misses de glifos. Sin repro de daño.
- **C6 (nota)**: JPEG Huffman over-subscribed leve se acepta en silencio
  (`from_raw` sólo rechaza `> 1<<16`); `libjpeg` avisa. Sin Panic/OOB
  (`values.get` acotado) — robustez menor, sin repro de píxel erróneo.
- Sin vector `security`/`advisory`: ningún crasheo remoto, sandbox bypass,
  path traversal o symlink race en el scope; todo lo hostil termina en
  `ImageError`/`FontError`/`SvgError` o `Limit`.
