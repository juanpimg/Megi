# Verificación Mesa A — scope core (`f2-core.md`)

- Mesa: A (reproductor independiente). Fecha: 2026-11-18.
- Informe verificado: `docs/audit/findings/wave2/f2-core.md` (untracked, sin commit).
- Base de ejecución: worktree limpio `/tmp/opencode/megi-verify`, HEAD `3c06db8`
  (sólo `?? fuzz/corpus/...` untracked; `git status` sin WIP en `crates/`).
  Env: `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target`,
  `TMPDIR=/home/jp/.cache/megi-tmp`. Cada sonda se ejecutó 2 veces (`diff` → idénticas).
- Sonda: `/home/jp/.cache/megi-probe-A` (deps → crates del worktree verify) + sonda
  del auditor `/tmp/probe` (modo `mem`) + `/tmp/dup.rs` + `cargo build -p megi-css`.
- ⚠️ El informe está **truncado a mitad de `w2-core-10`** (termina en el literal
  `...[truncated 3880 chars]`): los cuerpos de `w2-core-11/12/13/14` y
  `latent-security` **no existen** en el fichero. Esos 4 (+latent) se verificaron
  contra su línea de cobertura + reconstrucción del repro; se marca [T] = cuerpo truncado.
- ⚠️ Conteo: el brief pide 12 hallazgos pero el informe referencia `w2-core-01…14`
  (14). Se verifican los 14 referenciados + `latent-security`.

## Tabla de veredictos

| id | veredicto | comando | salida literal (recortada) | nota |
|----|-----------|---------|----------------------------|------|
| w2-core-01 | VERIFIED | `cargo run --manifest-path /home/jp/.cache/megi-probe-A/Cargo.toml` (T2/T3/T5/T6/T8) | `T2 translate pct: Some(Transform { e: 10.0, .. })`; `T3 rotate rad: Some(Transform { a: 0.9996242, b: 0.027412131, .. })`; `T5 translate garbage unit: Some(… e: 10.0 …)`; `T6 translate 3 args: Some(… e: 1.0, f: 2.0 })`; `T8 rotate 2 args: Some(… 90° …)` | `%`→px, `rad`→grados, `qpx` aceptado, aridad ignorada. Causa en `transform.rs:216-236` (`parse_args` recorta al prefijo numérico). 2 ejecuciones idénticas. |
| w2-core-02 | VERIFIED | mismo (T1/T1b) | `T1 TRANSLATE upper: None`; `T1b translate lower: Some(Transform { e: 10.0, .. })` | `match name` (`transform.rs:51`) sin plegado case-insensitive; `split_functions` no pliega. 2 ejecuciones idénticas. |
| w2-core-03 | VERIFIED | mismo (L1/L2/L5/L7) | `L1 px huge: None`; `L2 calc huge: Some(Px(100000000.0))`; `L5 calc em huge: Some(Px(1600000000.0))`; `L7 calc pct huge: Some(Percent(100000000.0))` | `Length::parse` topa en 1e6 (`properties.rs:386-424`); `calc.rs:number()` (`calc.rs:260-326`) sólo filtra no-finitos, sin tope. 2 ejecuciones idénticas. |
| w2-core-04 | VERIFIED | mismo (O1/O2) | `O1 opacity 1e999: 1`; `O2 opacity NaN: NaN` | `.parse::<f32>().ok()` + `.clamp(0,1)` (`properties.rs:3200-3203`); `NaN.clamp` → NaN. 2 ejecuciones idénticas. |
| w2-core-05 | VERIFIED | mismo (H1/H2/H3) | `H1 line-height 1e999: Some(inf)`; `H2 line-height NaN: Some(NaN)`; `H3 line-height -5: Some(-80.0)` | `parse_line_height` acepta cualquier `f32` (`properties.rs:3418-3426`); el filtro `>= 0` de `parse_line_height_factor` no se alcanza para `f32` directo. 2 ejecuciones idénticas. |
| w2-core-06 | VERIFIED | mismo (B1/B1b/B2) | `B1 repeat layers: [Repeat, Repeat]`; `B1b norepeat+bogus 2 layers: [NoRepeat, NoRepeat]`; `B2 bogus solo: [Repeat]` | `cycle_property_list` usa `filter_map` (`properties.rs:4315-4335`). B1b es el discriminador limpio: `no-repeat, bogus` → `[NoRepeat, NoRepeat]` en vez de la inicial `[Repeat, Repeat]`. B2 cae a inicial, pero la inicial de `repeat` ES `Repeat`: indistinguible en la práctica. 2 ejecuciones idénticas. |
| w2-core-07 | VERIFIED | lectura + mismo (Q3/Q3b) | `Q3 :checked on checked input: true`; `Q3b :checked on plain div: false` | El parser mapea `focus/checked/disabled/…` a variantes reales (`selector.rs:1702-1711`) y `matches()` las evalúa contra estado (`selector.rs:415-426`, `is_focused/is_checked/is_disabled`), pero `user_agent.rs:8-9` documenta que se mapean a `PseudoClass::Never`. Contradicción doc-vs-código confirmada en HEAD. |
| w2-core-08 | VERIFIED | lectura + mismo (Q1/Q2) + test existente | `Q1 :is matches: true`; `Q2 unknown invalidates: 0`; test `test_is_where_not_matching_and_specificity` existe (`css/src/lib.rs:831`) | Docstring `selector.rs:1219-1220` («currently invalidate») contradicho por el parser (`selector.rs:1697-1700`, `is/where/not/has` reales) y sus tests. La parte F13 del docstring sigue siendo cierta (Q2: `div, p:bogus` → 0 selectores). |
| w2-core-09 | VERIFIED | `rustc -O /tmp/dup.rs -o /home/jp/.cache/dup && /home/jp/.cache/dup`; `cargo build -p megi-css` | `1`; `Finished dev profile …` (sin warnings) | Brazos idénticos `selector.rs:1725-1727` (mismo patrón + guarda): imprime `1` (rama 1), 2ª inalcanzable; `rustc` no avisa con guardas; build limpio sin warnings. |
| w2-core-10 | PARTIAL | mismo (W1/W0b/V1/V0b) | `W1 font-weight 0: 0`; `W0b font-weight 1001: 1001`; `V1 valign huge: Some(Length(1000000000.0))`; `V0b valign inf: None` | (a) `font-weight` sin rango confirmado (`properties.rs:3397-3405`; acepta 0 y 1001). (b) `VerticalAlign` **SÍ filtra no-finitos** (`properties.rs:989-1018`, `V0b → None`): la afirmación «sin `is_finite`» es falsa; sólo es cierta la ausencia de tope 1e6 (`V1 → 1e9`). Fallo real, descripción a medias. |
| w2-core-11 [T] | VERIFIED | mismo (M0/M1/M1b/M1c) | `M0 colormix srgb: Some(Color { r: 128, g: 0, b: 128, .. })`; `M1 colormix lab: None`; `M1b colormix lch: None`; `M1c colormix hsl: None` | `mix_colors` (`color.rs:585-644`) sólo acepta `srgb/srgb-linear/oklab/oklch`, `_ => None`. `lab/lch` (y también `hsl`) rechazados. Cuerpo del hallazgo truncado: severidad/expected/fix no verificables. |
| w2-core-12 [T] | VERIFIED | lectura estática (`grid.rs:615-631`) | `let max = max.trim(); … .strip_suffix("fr").or_else(\|\| max.strip_suffix("FR")).or_else(\|\| max.strip_suffix("fr "))` | `max` ya está `trim()`eado (línea 623) → el sufijo `"fr "` (con espacio) es inalcanzable: brazo muerto confirmado. Cuerpo del hallazgo truncado: severidad/fix no verificables. |
| w2-core-13 [T] | VERIFIED | `cargo run --manifest-path /tmp/probe/Cargo.toml -- mem` | `MEM nodes=4 rss_before_kb=5752 rss_after_kb=30788 delta_kb=25036`; `violations: LimitViolations { … all zero }` | 2 MiB de `\n` → ΔRSS ≈ 25 MB ≈ **12,2 B por byte de entrada**, cero violaciones: consistente con «`line_starts` ~12 B/B» (`Vec<usize>` por línea ≈ 16 MB + `out: Vec<u32>` ≈ 8 MB). Sustrato medido con el repro del auditor. Cuerpo truncado: no se puede juzgar si el informe lo presenta como bug o medición; `line_starts` se conserva en `Tokenizer` para lookups línea/columna (uso legítimo). |
| w2-core-14 [T] | VERIFIED | lectura estática (`tree_builder.rs:4437-4476`, `browser.rs:49-110`) | `Err(_) => { LimitViolations::default() }` tras `catch_unwind`; `set_inner_html` prosigue con violaciones vacías (libera hijos viejos, mueve 0 nuevos, `Ok`) | Un pánico dentro del fragment parser se traga vía `catch_unwind` (#1454) y devuelve violaciones **vacías**, indistinguibles de éxito; `set_inner_html` lo trata como vía OK. Mecanismo confirmado en código + test `tree_builder.rs:5003`. Sin trigger vivo de pánico en HEAD (el #1454 está fixeado): no ejercitado dinámicamente. Cuerpo truncado. |
| latent-security | NEEDS INFO | — | — | Sin contenido alguno en el informe (ni en cobertura ni en hallazgos). Nada que reproducir. |

## Concerns (para Mesa C)

1. **Informe truncado**: `f2-core.md` termina a mitad de `w2-core-10` con el literal
   `...[truncated 3880 chars]` y contenido posterior duplicado/mezclado (repite 04–09).
   Cuerpos de 11–14 y `latent-security` ausentes; su verificación es reconstruida,
   no contra evidencia del auditor.
2. **Conteo**: brief = 12 hallazgos; informe = referencias `w2-core-01…14` (14).
3. **Informe sin commit** (untracked): riesgo de pérdida; fijar versión antes de Mesa C.
4. **w2-core-06/B2**: «bogus solo cae a inicial» es inobservable (inicial = `Repeat`);
   el caso que demuestra el bug es B1b (`no-repeat, bogus` → `[NoRepeat, NoRepeat]`).
5. **w2-core-10**: la mitad de la afirmación sobre `VerticalAlign` es falsa
   (sí hay filtro `is_finite`); rebajar precisión del cuerpo si pasa a issue.
6. **w2-core-13/14**: verificados a nivel de mecanismo/medición, no de repro del
   auditor; si Mesa C los incluye, pedir al auditor el cuerpo completo o marcar
   `needs-repro` → `exclude`.
