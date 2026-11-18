# Verificación Mesa A — layout/paint (w2-lp-01…w2-lp-20)

Worktree: `/tmp/opencode/megi-verify` (HEAD `3c06db8`).
`diff -r` confirma `crates/layout` y `crates/paint` **byte-idénticos** al árbol
principal, por lo que los veredictos valen para el HEAD actual en este scope.
Env: `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`,
`TMPDIR=/home/jp/.cache/megi-tmp`. Suites: `cargo test -p paint` ⇒
**253 passed, 0 failed**; `cargo test -p layout` ⇒ **287 passed, 0 failed**.

| id | veredicto | comando | salida literal (recorte) | nota |
|----|-----------|---------|--------------------------|------|
| w2-lp-01 | VERIFIED | `cargo run --bin w2lp` (en `/tmp/opencode/w2lp`) | `R1 background-clip:padding-box red rects: [Rect { x: 0.0, y: 0.0, width: 120.0, height: 70.0 }]` / `R1 verdict: red covers border-box (120x70)? true` | El rojo cubre 120×70 = border box con `clip: padding-box`. Código: `paint_background_border` emite sobre `border_box` sin consultar `layer.clip`. |
| w2-lp-02 | VERIFIED | `cargo run --bin w2lp` | `R2 object-fit:none image rect: Some(Rect { x: -75.0, y: -75.0, width: 200.0, height: 200.0 }), any PushClip: false` / `overflows content box unclipped? true` | Sólo `Cover` recibe `PushClip`; la rama `else` (`None`/`Contain`/`ScaleDown`) emite sin clip. `none` sobredimensionado desborda. |
| w2-lp-03 | VERIFIED | lectura `rasterizer.rs:3784-3830` + `cargo test -p paint` (253 passed, 0 failed) | `planes[0][index] = f32::from(pixel.r); …` (RGB sin premultiplicar; alfa en plano aparte sólo si no uniforme); doc módulo: `Pixels are stored as straight (non-premultiplied) RGBA` | Sin fixture de píxeles en suite; P2 sin claim cuantitativo, como dice el informe. |
| w2-lp-04 | VERIFIED | `cargo run --bin r5` | `R5 border-area(12,30)=Color { r: 0, g: 0, b: 0, a: 255 } padding-area(30,30)=Color { r: 255, g: 255, b: 255, a: 255 }` / `shadow leaks into border ring? true` | Código: `alpha *= rounded_rect_coverage(rect, …)` con `rect` = border box, no padding box. |
| w2-lp-05 | VERIFIED | lectura `rasterizer.rs:556-589` + suite (sin test de esquinas con patrón) | `Mitred corners: the adjacent sides partition the corner square along the outer→inner diagonal.` — `draw_border` llama a `draw_corner` incondicionalmente; mezcla `color_a`/`color_b` sin patrón de dashes | Fidelidad visual P3, sin repro de píxeles. |
| w2-lp-06 | VERIFIED | lectura `rasterizer.rs:2494,2538,3410` + suite | `if area > 16_777_216 {` (×3, `return` temprano en `fill_rounded_rect`, `draw_rounded_border`, `draw_gradient`) | Vivo por diseño; sin test del umbral. |
| w2-lp-07 | VERIFIED | lectura `painter.rs:1922,1945` + filtros `1072,1381` + suite | `let color = scaled(color, alpha_scale)` … `color: scaled(color, alpha_scale)` (alfa²); `if phase(child, effects_present) != Phase::InFlow {` filtra contextos en los dos pases; `OpacityGroup` en 914-923 | Doble escala exacta + inalcanzabilidad confirmadas. Impacto hoy nulo (latente). |
| w2-lp-08 | VERIFIED | lectura `painter.rs:789-810` + filtros `1072,1381` | `// Depth guard: paint the remaining subtree as flat in-flow content` … sólo llama a `paint_block_backgrounds` / `paint_floats` / `paint_inline_content`, los tres con filtro `phase != InFlow` | El comentario promete aplanar; los boxes de contextos anidados se descartan. |
| w2-lp-09 | VERIFIED | lectura `painter.rs:1149-1151` + suite | `if depth < MAX_FLOAT_DEPTH { paint_floats(…) }` — pasado 64 no desciende | Layout conserva los floats (`MAX_FLOATS_PER_CONTEXT` sólo degrada); divergencia layout↔paint en documentos hostiles. P3. |
| w2-lp-10 | VERIFIED | lectura `rasterizer.rs:2082-2108` + suite | `for channel in 0..4 { let top = …; … out[channel] = … }` — lerp por canal en straight-alpha | Sin fixture de píxeles; P3. |
| w2-lp-11 | VERIFIED | lectura `painter.rs:260,355,624,1606-1607` + suite | `static TEXT_MASK_COLLECTORS: AtomicUsize` … `TEXT_MASK_COLLECTORS.store(0, Relaxed)` por pase … `if count < MAX_TEXT_MASK_COLLECTORS` (=32) | Fallback documentado; global reseteado por pase. Pases hoy secuenciales. P3. |
| w2-lp-12 | VERIFIED | `cargo run --bin r6` | `R6 margins: left=0 right=0 x=0` / `spec-deviation (0,0 instead of -50,-50)? true` | Código `block.rs:416-421`: rama `(true,true)` con `underflow < 0` ⇒ `(0.0, 0.0)`. Desvía CSS 2.2 §10.3.3. |
| w2-lp-13 | NOT VERIFIED | `cargo run --bin w2lp` | `R3 effect_groups: 1 (Multiply), opacity_groups: 1` / `blend preserved with opacity? true` | ya corregido, no publicar (efecto anidado dentro de opacidad, como describe el fix). |
| w2-lp-14 | NOT VERIFIED | lectura `rasterizer.rs:1229-1256` | `face_index` inserta/retorna `u32::MAX` ante cap o fallo (`lines 1237-1243, 1251-1254`) | ya corregido, no publicar. |
| w2-lp-15 | NOT VERIFIED | lectura `rasterizer.rs:1925-1935,2668,2862` + `cargo test -p paint` | `reserve_shadow` carga `SHADOW_SCRATCH_COPIES=4`; `draw_box_shadow` y `paint_text_shadow` rechazan pasado el cap | ya corregido, no publicar. |
| w2-lp-16 | NOT VERIFIED | `cargo run --bin w2lp` (R4) | `R4 5000 floats layout: 126.67832ms, root height: 600` (cap `MAX_FLOATS_PER_CONTEXT=4096`, block.rs:204) | ya corregido (mitigado), no publicar. |
| w2-lp-17 | NOT VERIFIED | `cargo test -p paint` (incl. `asymmetric_corner_uses_its_own_centre`, rasterizer.rs:4332) | suite 253 passed, 0 failed | ya corregido, no publicar. |
| w2-lp-18 | NOT VERIFIED | lectura `painter.rs:760-770,914-923,1535-1571` | `is_root` sólo se usa para propagación de fondo; el wrap `OpacityGroup` no tiene guard de raíz | ya corregido, no publicar. |
| w2-lp-19 | NOT VERIFIED | lectura `rasterizer.rs:912-917`, `hit_test.rs:101-105` | `// Push a sentinel so pop_clip stays balanced (#1180).` / `None => rect` | ya corregido, no publicar. |
| w2-lp-20 | NOT VERIFIED | lectura `rasterizer.rs:183-196` | `blend_pixel` comprueba `is_clipped` en ambas ramas | ya corregido, no publicar. |

## Concerns

1. Ningún hallazgo es `security`/explotable de forma remota evidente; el peor
   caso es DoS local por documentos hostiles (w2-lp-08/09: pérdida de subárboles
   con >256 niveles de contexto / >64 floats anidados; w2-lp-06: desaparición
   silenciosa de fondos ≥16.7 Mpx). Nada que publicar como aviso privado.
2. Prioridad sugerida si la ola actúa: w2-lp-01/02/04 (píxeles observables con
   repro binario) y w2-lp-07 (trampa latente: doble alfa el día que el path
   plano sea alcanzable). Coincide con el concern del auditor.
3. w2-lp-03/10 (alfa straight en blur y en `bilinear_sample`) comparten raíz
   (interpolación sin premultiplicar); convendría un solo issue o dos
   vinculados, decisión de Mesa B/C.
4. Infra: compilar en el worktree exige `CARGO_TARGET_DIR` fuera de `/tmp`
   (tmpfs lleno por otros auditores); con el env indicado todo reproduce a la
   primera. R4 midió 126 ms (más rápido que los 180-275 ms del informe;
   misma conclusión).
