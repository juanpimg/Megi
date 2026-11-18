# Ola 2 · F2 layout+paint — informe del auditor

Prefijo: `w2-lp`. Alcance: todos los `.rs` bajo `crates/layout/src/` y
`crates/paint/src/`, leídos línea por línea.

> Nota de alcance real: el brief indica «~68 archivos / ~38k líneas». El
> alcance asignado contiene **22 archivos y 21 108 líneas** (`wc -l` suma
> 21 108). No existen subdirectorios adicionales en `crates/layout/src/` ni
> `crates/paint/src/`. Todos los comandos cargo/test se ejecutaron desde el
> worktree limpio `/tmp/opencode/megi-verify` (HEAD `3c06db8`, sin cambios).
> Lecturas hechas en el árbol principal; `diff -r` confirma que
> `crates/layout`, `crates/paint`, `crates/css`, `crates/dom` y
> `crates/image/src` son **byte-idénticos** entre ambos árboles. Los cambios
> WIP del árbol principal (`crates/js`, `crates/render`) no afectan a este
> scope: ningún hallazgo depende de ellos (ninguno marcado
> `excluir: WIP del usuario`).

## Alcance (22 archivos)

`crates/layout/src/`: `bidi.rs` (493), `block.rs` (2310), `box_tree.rs` (313),
`calc.rs` (73), `flex.rs` (822), `geometry.rs` (155), `grid.rs` (1524),
`inline.rs` (2169), `lib.rs` (377), `table.rs` (1608), `text.rs` (310),
`tree_builder.rs` (850).
`crates/paint/src/`: `background.rs` (696), `deflate.rs` (558),
`display_list.rs` (433), `hit_test.rs` (303), `inflate.rs` (329),
`lib.rs` (257), `painter.rs` (2161), `png.rs` (710), `rasterizer.rs` (4454),
`style.rs` (203).

## Cobertura

- `calc.rs`, `geometry.rs`, `lib.rs`, `box_tree.rs`, `text.rs`, `bidi.rs`,
  `tree_builder.rs`, `block.rs`, `flex.rs`, `grid.rs`, `table.rs`,
  `inline.rs`, `style.rs`, `display_list.rs`, `background.rs`, `hit_test.rs`,
  `painter.rs`, `rasterizer.rs`, `png.rs`, `inflate.rs`, `deflate.rs`:
  **leído — con hallazgo** (ver abajo; varios archivos aportan tanto
  hallazgos vivos como verificaciones de fix).
- Ningún archivo quedó sin leer. Lentes aplicadas: aritmética/casts
  (sanitize/NaN guards verificados en `calc`, `block`, `inline`,
  `rasterizer`), panics/indexing (`expect` sólo en tests; `get`/`clamp` en
  hot paths), límites y DoS (caps verificados: `MAX_FLOATS_PER_CONTEXT`,
  `MAX_TABLE_COLUMNS`, `MAX_EXPANDED_TRACKS`, `MAX_OFFSCREEN_*`,
  `MAX_TEXT_CACHE_*`, `MAX_BACKGROUND_TILES`, `MAX_DISPLAY_LIST_ITEMS`,
  `MAX_PIXMAP_PIXELS`), UTF-8/texto (char boundaries en `style.rs`,
  `transform_text`), errores tragados (filtros `ok()?` con fallback
  documentado en `text.rs`), estado global/concurrencia (sólo
  `thread_local!` + `AtomicUsize`/`OnceLock`; sin `static mut`),
  contrato vs spec (desviaciones abajo).

## Hallazgos vivos (siguen rotos en HEAD)

### w2-lp-01 — background-clip padding/content-box ignorado para el color
- `archivo:línea`: `crates/paint/src/painter.rs:1653-1664`
- `lente`: contrato vs especificación · `severidad`: P2 · `categoría`: bug
- `title`: el color de fondo se pinta sobre el border box aunque
  `background-clip` sea `padding-box`/`content-box`
- `evidence`: repro `/tmp/opencode/w2lp/src/main.rs` (bin `w2lp`, bloque R1),
  ejecutado 2 veces desde el worktree limpio con idéntica salida:
  `R1 background-clip:padding-box red rects: [Rect { x: 0.0, y: 0.0, width: 120.0, height: 70.0 }]`
  (div 100×50 + borde 10px: el rojo cubre 120×70 = border box).
  En código: `paint_background_border` emite `SolidColor`/`RoundedRect`
  sobre `border_box` sin consultar `layer.clip` (sólo la variante `Text`
  tiene rama propia, líneas 1641-1651).
- `impact`: fondos que sangran bajo el borde; rompe diseños con bordes
  semitransparentes y `background-clip: padding-box`.
- `expected`: CSS Backgrounds 3 §2.11.2 — el color de fondo se recorta al
  `background-clip` declarado.
- `fix`: pintar el color dentro de `clip_rect` (`background_clip_rect`)
  con `PushClip`, igual que ya se hace con las capas de imagen (líneas
  1720-1751).
- `repro`: `/tmp/opencode/w2lp/src/main.rs` (R1)
- `dedup`: #1183 (deferred; evidencia nueva de que sigue vivo en HEAD)

### w2-lp-02 — object-fit:none desbordado pinta fuera del content box
- `archivo:línea`: `crates/paint/src/painter.rs:1488-1509`
- `lente`: contrato vs especificación · `severidad`: P2 · `categoría`: bug
- `title`: `object-fit: none` con imagen mayor que el content box pinta fuera
  de él, sin clip
- `evidence`: repro R2 (2 ejecuciones idénticas):
  `R2 object-fit:none image rect: Some(Rect { x: -75.0, y: -75.0, width: 200.0, height: 200.0 }), any PushClip: false`
  (img 50×50, bitmap 200×200). En código: sólo `Cover` recibe `PushClip`
  (línea 1497); `None`/`Contain`/`ScaleDown` emiten `Image` con el rect
  dibujado sin recorte. `contain`/`scale-down` nunca desbordan, pero `none`
  con intrínseco mayor sí.
- `impact`: imágenes gigantes invaden el layout vecino.
- `expected`: CSS Images 3 §5.5 — el contenido reemplazado se recorta al
  content box.
- `fix`: aplicar `PushClip(rect)` también en la rama `else` cuando
  `drawn != rect` (o siempre para `None`).
- `repro`: `/tmp/opencode/w2lp/src/main.rs` (R2)
- `dedup`: #1185 (deferred; vivo sólo el caso `none` sobredimensionado —
  `cover` ya se recorta, `contain`/`scale-down` no pueden desbordar)

### w2-lp-03 — blur_rgba difumina alfa straight (halos oscuros)
- `archivo:línea`: `crates/paint/src/rasterizer.rs:3784-3853`
- `lente`: aritmética · `severidad`: P2 · `categoría`: bug
- `title`: `filter: blur()` difumina los planos R/G/B y A por separado en
  espacio straight-alpha
- `evidence`: lectura directa — `BLUR_PLANES[0..3]` reciben R/G/B sin
  premultiplicar (líneas 3804-3807) y el plano alfa se difumina aparte
  (3815-3821); el doc del módulo (líneas 27-36) confirma almacenamiento
  straight. Comando: `cargo test -p paint` en verify (253 passed) — ningún
  test cubre franjas de borde con alfa parcial bajo blur.
- `impact`: halos oscuros alrededor de contenido semitransparente con blur.
- `expected`: difuminar en espacio premultiplicado y convertir de vuelta.
- `fix`: premultiplicar RGB por A antes de los planos y deshacer después
  (mantener el fast path de alfa uniforme).
- `repro`: código + suite existente (sin fixture de píxeles; P2 sin claim
  cuantitativo)
- `dedup`: #1184 (deferred; sigue vivo)

### w2-lp-04 — inset box-shadow recortado al border box
- `archivo:línea`: `crates/paint/src/rasterizer.rs:2711-2716`
- `lente`: contrato vs especificación · `severidad`: P2 · `categoría`: bug
- `title`: la sombra `inset` se enmascara con el border box en vez del
  padding box: invade el anillo del borde
- `evidence`: repro `/tmp/opencode/w2lp/src/bin/r5.rs` (2 ejecuciones):
  `R5 border-area(12,30)=Color { r: 0, g: 0, b: 0, a: 255 }` — píxel dentro
  del anillo de borde (border box 10,10+40×40, borde hipotético 5px ⇒
  padding desde x=15) pintado de negro con `inset` + `offset_x=10`.
  En código: `alpha *= rounded_rect_coverage(rect, …)` donde `rect` es el
  border box.
- `impact`: sombras inset visibles a través de bordes semitransparentes.
- `expected`: CSS Backgrounds 3 §6 — la inset pinta dentro del padding box.
- `fix`: enmascarar con el padding box (`rect` deflactado por
  `border_width`), igual que `overflow_rounded_clip` ya hace para el radio.
- `repro`: `/tmp/opencode/w2lp/src/bin/r5.rs`
- `dedup`: #1192 (deferred; evidencia nueva de píxeles en HEAD)

### w2-lp-05 — esquinas con mitre sólido en dashed/dotted
- `archivo:línea`: `crates/paint/src/rasterizer.rs:556-589` (`draw_corner`,
  672-749)
- `lente`: contrato vs especificación · `severidad`: P3 · `categoría`: bug
- `title`: las esquinas siempre se parten por la diagonal con color sólido,
  también con `dashed`/`dotted`
- `evidence`: `draw_border` llama a `draw_corner` incondicionalmente para
  cualquier estilo visible; `draw_corner` mezcla `color_a`/`color_b` sin
  patrón de dashes. Suite paint (253 passed) sin test de esquinas con
  patrón.
- `impact`: esquinas sólidas en bordes discontinuos (fidelidad visual).
- `expected`: CSS Backgrounds 3 §4 — el patrón continúa por la esquina.
- `fix`: recortar `draw_corner` al patrón del lado (o documentar como
  limitación en `display_list.rs`, cuyo doc ya promete mitre).
- `repro`: código + suite
- `dedup`: #1191 (deferred; sigue vivo)

### w2-lp-06 — cap de 16.7 Mpx salta rounded/gradients en silencio
- `archivo:línea`: `crates/paint/src/rasterizer.rs:2493-2496`,
  `2537-2540`, `3409-3412`
- `lente`: límites y DoS · `severidad`: P2 · `categoría`: contract
- `title`: `fill_rounded_rect`, `draw_rounded_border` y `draw_gradient` no
  pintan nada cuando `área > 16_777_216`, sin fallback cuadrado
- `evidence`: tres `return` tempranos idénticos; `MAX_ROUNDED_GROUP_PIXELS`
  reutilizado como cap de área (documentado como limitación). `cargo test
  -p paint` no cubre el umbral.
- `impact`: en páginas ≥16.7 Mpx los fondos redondeados/gradientes
  desaparecen (píxeles transparentes) en vez de degradar al rectángulo.
- `expected`: degradar a relleno cuadrado (lo que el propio grupo redondeado
  ya hace al caer al path plano, `paint_rounded_group` línea 2789).
- `fix`: fallback a `fill_rect`/gradiente cuadrado pasado el cap.
- `repro`: código + suite
- `dedup`: #1179 (deferred; vivo por diseño)

### w2-lp-07 — alpha_scale plano inalcanzable + doble escala en texto
- `archivo:línea`: `crates/paint/src/painter.rs:1922`, `1939-1952`
- `lente`: contrato vs documentación · `severidad`: P2 · `categoría`: bug
- `title`: el path plano de opacidad fraccional es inalcanzable en
  producción y, de alcanzarse, `paint_text_run` aplica `scaled()` dos veces
  al color del texto
- `evidence`: `paint_text_run` hace `let color = scaled(color, alpha_scale)`
  (1922) y luego emite `Text { color: scaled(color, alpha_scale) }` (1945) —
  alfa². Inalcanzabilidad: `phase()` (478-510) clasifica todo box con
  `opacity<1` como contexto (Zero/Negative/Positive) y los pases
  `paint_block_backgrounds`/`paint_inline_content` filtran `phase != InFlow`
  (1071-1074, 1380-1382); los contextos se pintan con `OpacityGroup`
  (líneas 914-923). Los `alpha_scale` que sí fluyen (`paint_stacking_context`
  líneas 838-848) son siempre `1.0`.
- `impact`: hoy ninguno visible; si el path se vuelve alcanzable, texto
  doblemente transparentado e inconsistente con sus `text-shadow`
  (éstas usan escala simple, línea 1937).
- `expected`: o eliminar el path plano o hacerlo coherente (una sola escala).
- `fix`: quitar el `scaled` de la línea 1945 (usar `color` ya escalado).
- `repro`: código + `cargo test -p paint` (253 passed)
- `dedup`: #1196 / #1211 (deferred; evidencia nueva: doble aplicación
  exacta + prueba de inalcanzabilidad)

### w2-lp-08 — el guard de profundidad de stacking contexts descarta contenido
- `archivo:línea`: `crates/paint/src/painter.rs:789-810`
- `lente`: contrato vs documentación · `severidad`: P2 · `categoría`: contract
- `title`: pasado `MAX_STACKING_CONTEXT_DEPTH` (256) los contextos anidados
  no se «aplanan»: sus fondos/bordes/texto se pierden
- `evidence`: la rama del guard sólo llama a `paint_block_backgrounds` /
  `paint_floats` / `paint_inline_content`, y los tres filtran
  `phase != InFlow`; los boxes de los contextos anidados (Negative/Zero/
  Positive) no se visitan. El comentario promete «paint the remaining
  subtree as flat in-flow content», pero el contenido de contextos se
  descarta, no se aplana.
- `impact`: subárboles enteros invisibles en documentos hostiles (>256
  niveles de contexto).
- `expected`: o aplanar de verdad (pintar fondos/texto sin grupos) o
  documentar la pérdida.
- `fix`: en el guard, recorrer también los buckets del contexto.
- `repro`: código + suite
- `dedup`: #1188 (deferred; sigue vivo; el comentario es inexacto)

### w2-lp-09 — el cap de floats anidados descarta floats en pintura
- `archivo:línea`: `crates/paint/src/painter.rs:1149-1151`,
  `233-236`
- `lente`: contrato vs documentación · `severidad`: P3 · `categoría`: contract
- `title`: pasado `MAX_FLOAT_DEPTH` (64) los floats anidados dejan de
  pintarse (layout los conserva)
- `evidence`: `paint_float` no desciende a `paint_floats` cuando
  `depth >= MAX_FLOAT_DEPTH`; layout sí los coloca (cap distinto,
  `MAX_FLOATS_PER_CONTEXT`, sólo los degrada a in-flow). Divergencia
  layout↔paint acotada a documentos hostiles.
- `impact`: floats perdidos en pintura a partir de 64 niveles de anidación.
- `expected`: documentar la divergencia o aplanar.
- `fix`: documentar en el doc de `MAX_FLOAT_DEPTH`.
- `repro`: código + suite
- `dedup`: #1189 (deferred; el contador ya es propio, pero la pérdida sigue)

### w2-lp-10 — el escalado de imágenes interpola alfa straight
- `archivo:línea`: `crates/paint/src/rasterizer.rs:2082-2108`
- `lente`: aritmética · `severidad`: P3 · `categoría`: bug
- `title`: `bilinear_sample` interpola los 4 canales (incluido A) por
  separado en straight-alpha
- `evidence`: bucle `for channel in 0..4` con lerp lineal por canal
  (líneas 2101-2107); el doc de `DisplayItem::Image` promete «straight-alpha
  source-over», pero la interpolación straight oscurece bordes
  semitransparentes al escalar.
- `impact`: halos en `<img>` escaladas con alfa parcial.
- `expected`: muestreo premultiplicado (o documentar la aproximación).
- `fix`: premultiplicar antes del lerp y deshacer después.
- `repro`: código + suite
- `dedup`: #1193 (deferred; sigue vivo)

### w2-lp-11 — budget global de background-clip:text por pase
- `archivo:línea`: `crates/paint/src/painter.rs:260`,
  `355`, `619-624`, `1603-1614`
- `lente`: estado global / límites · `severidad`: P3 · `categoría`: perf
- `title`: `TEXT_MASK_COLLECTORS` es un `AtomicUsize` global reseteado por
  pase; el cap (`MAX_TEXT_MASK_COLLECTORS=32`) silencia máscaras pasándolo
- `evidence`: `static TEXT_MASK_COLLECTORS` + `store(0, Relaxed)` por pase;
  pasado el cap `collect_text_mask_runs` no se ejecuta y el fondo se pinta
  sin máscara (fallback documentado en el comentario, pero el estado global
  hace el comportamiento dependiente del orden de pases concurrentes si dos
  hilos comparten el proceso — hoy los pases son secuenciales).
- `impact`: «gradient text» que deja de enmascarar a partir de 32 cajas.
- `expected`: contador por pase (local) en vez de global.
- `fix`: mover el contador a `PaintResources` (por pase).
- `repro`: código + suite
- `dedup`: #1187 (deferred; sigue vivo)

### w2-lp-12 — auto-márgenes con underflow negativo dan (0,0)
- `archivo:línea`: `crates/layout/src/block.rs:416-421`
- `lente`: contrato vs especificación · `severidad`: P3 · `categoría`: bug
- `title`: `width` fija mayor que el containing block con ambos márgenes
  `auto` produce márgenes 0 en vez de repartir el underflow negativo
- `evidence`: repro `/tmp/opencode/w2lp/src/bin/r6.rs` (2 ejecuciones
  idénticas): `R6 margins: left=0 right=0 x=0` para `width:900px` en
  viewport 800. En código: rama `(true, true)` con `underflow < 0` ⇒
  `(0.0, 0.0)`.
- `impact`: caja descentrada que desborda sólo por la derecha; los
  navegadores reparten `-50/-50` (desborda por ambos lados).
- `expected`: CSS 2.2 §10.3.3 — ambos `auto` ⇒ valores iguales (mitad del
  underflow, aunque sea negativo).
- `fix`: `(underflow / 2.0, underflow / 2.0)` sin el `if underflow >= 0`.
- `repro`: `/tmp/opencode/w2lp/src/bin/r6.rs`
- `dedup`: #107 (deferred/closed; evidencia nueva de que sigue vivo en HEAD)

## Verificados corregidos en HEAD (dedup con evidencia de fix)

- **w2-lp-13 — #1181 mix-blend-mode + opacity**: CORREGIDO. Repro R3
  (2 ejecuciones): `Effect Multiply n=1` anidado dentro de
  `Opacity 0.5 n=1` — `paint_stacking_context` envuelve primero el grupo de
  efecto (painter.rs:889-912) y después el de opacidad (914-923).
- **w2-lp-14 — #1194 face_index no cacheaba el fallo**: CORREGIDO.
  `TextCache::face_index` (rasterizer.rs:1229-1256) cachea `u32::MAX` cuando
  se alcanza `MAX_FACES` o la resolución falla (líneas 1237-1243,
  1251-1254); no re-pregunta a la font DB.
- **w2-lp-15 — #1415 paint budget no cobrado a sombras**: CORREGIDO.
  `PaintBudget::reserve_shadow` carga `SHADOW_SCRATCH_COPIES=4` por sombra
  (rasterizer.rs:1925-1935); `draw_box_shadow` (2668) y `paint_text_shadow`
  (2862) rechazan la capa pasado el cap; tests `shadow_budget_tests` pasan.
- **w2-lp-16 — #1417 float placement O(F²)**: MITIGADO. Cap
  `MAX_FLOATS_PER_CONTEXT=4096` (block.rs:204) + exclusiones en una pasada
  (594-614, 695-719). Repro R4: 5000 floats en ~180-275 ms (2 ejecuciones:
  180.37 ms / 274.56 ms); suite `float_budget` (4 passed).
- **w2-lp-17 — #1182 centro de elipse asimétrica**: CORREGIDO.
  `point_in_rounded_rect` usa el centro propio de cada esquina
  (rasterizer.rs:2277-2296); test `asymmetric_corner_uses_its_own_centre`
  pasa.
- **w2-lp-18 — #1190 root opacity ignorada**: CORREGIDO. Sin guard `is_root`
  en el wrap de opacidad; la raíz compone `OpacityGroup` igual que el resto.
- **w2-lp-19 — #1180 desync push/pop en clip cap**: CORREGIDO. Sentinel
  `None` (rasterizer.rs:912-917, hit_test.rs:101-105).
- **w2-lp-20 — #1186 blend_pixel sin clip**: CORREGIDO. `blend_pixel`
  comprueba `is_clipped` (rasterizer.rs:190-196).

## Comandos ejecutados (todos en `/tmp/opencode/megi-verify`, HEAD 3c06db8)

- `cargo test -p layout` ⇒ **287 passed, 0 failed**
- `cargo test -p paint` ⇒ **253 passed, 0 failed**
- Repros (binarios en `/tmp/opencode/w2lp`, deps por path contra el
  worktree limpio; cada uno ejecutado 2 veces con salida idéntica):
  - `cargo run --bin w2lp` ⇒ R1 `red … width: 120.0, height: 70.0 … true`;
    R2 `Some(Rect { x: -75.0, y: -75.0, width: 200.0, height: 200.0 }),
    any PushClip: false … true`; R3 `effect_groups: 1 (Multiply),
    opacity_groups: 1 … true`; R4 `5000 floats … ~180-275 ms`
  - `cargo run --bin r5` ⇒ `border-area(12,30)=Color { r: 0, … } … true`
  - `cargo run --bin r6` ⇒ `margins: left=0 right=0 x=0 … true`
  - (primera tanda de `cargo test` falló sólo por tmpfs lleno —
    `No space left on device` — no por el código; re-ejecutado con
    `CARGO_TARGET_DIR` en `/home`)
- `diff -r` layout/paint/css/dom/image-src entre árbol principal y worktree:
  idénticos (sólo difieren artefactos untracked en `crates/image`:
  `examples/`, `__pycache__/`).
- `gh issue list --search` para `margin auto underflow` ⇒ #107;
  `object-fit none unclipped` ⇒ #1185.

## Concern

1. **Deuda diferida concentrada en paint**: 11 de los 12 hallazgos vivos son
   issues deferred conocidos (#107, #1179, #1183-#1185, #1187-#1189,
   #1191-#1193, #1196/#1211). El trabajo de esta ola debería priorizar
   w2-lp-01/02/04 (píxeles observables con repro) y w2-lp-07 (trampa latente:
   el día que `alpha_scale<1` sea alcanzable, el texto se duplica en alfa).
2. **#1469 (`calc()` con viewport hard-coded 1280×720) vive en `crates/css`**
   (`css/src/calc.rs:29-30`, default de `CalcContext`), fuera de este scope
   (F-caja-css). Layout nunca aporta viewport: `calc::resolve` sólo pasa el
   reference del eje. Coordinar con ese carril: `resolve_document_styles`
   sí puede fijar viewport, pero `length()`/`length_optional()` de layout no
   lo propagan a `vw/vh`.
3. **Infra**: `/tmp` (tmpfs 7.7 G, 100 % lleno por worktrees y targets de
   otros auditores) impide compilar en el worktree; hubo que redirigir
   `CARGO_TARGET_DIR` a `/home`. Riesgo de interferencia entre auditores por
   espacio.
4. **Margen del informe**: ningún hallazgo depende de los WIP del usuario
   (`crates/js`, `crates/render`); scope layout+paint byte-idéntico al HEAD.
