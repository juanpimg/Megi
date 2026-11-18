# F2-core — auditoría línea por línea (`crates/core`, `crates/html`, `crates/css`)

- Prefijo: `w2-core`. Alcance: 33 archivos, 38 580 líneas (lista `find … | sort` verificada).
- Base verificada: worktree limpio `/tmp/opencode/megi-verify`, HEAD `3c06db8`, sin WIP.
  Alcance intacto entre `3c06db8` y HEAD actual (diff vacío en `crates/core|html|css`).
  WIP del principal sólo en `crates/js/**`, `crates/render/**`, `crates/net/**`
  (fuera de alcance) → **ningún hallazgo depende de WIP** (`excluir: WIP del usuario` — no aplica).
- `cargo test -p megi-core -p html -p css` (desde verify, `CARGO_TARGET_DIR`/`TMPDIR` en disco
  porque `/tmp` tmpfs estaba lleno): **529 passed, 0 failed** (css lib 164, dom lib 54,
  html lib 36 + 30 suites de integración/doc).
- Dedup: `docs/audit/FINDINGS.md` (228), `docs/GAPS.md`, `docs/audit/findings/f2/00-index.md`,
  índice local de issues (1000 issues; abiertas del scope: ninguna) y `gh issue list`.
  Todo lo publicado abajo es `dedup: new`; las relacionadas se citan como `rel:` (no duplicadas).

## Alcance (33 archivos leídos línea por línea)

`crates/css/src/`: animation.rs (561), animation_runtime.rs (712), calc.rs (442),
cascade.rs (1963), color.rs (694), component_value.rs (41), container.rs (420),
custom.rs (582), flex.rs (393), grid.rs (884), lib.rs (2131), media.rs (634),
parser.rs (861), properties.rs (6006), query.rs (370), rule.rs (332), selector.rs (2145),
shorthand.rs (966), state.rs (242), token.rs (125), tokenizer.rs (570),
transform.rs (271), user_agent.rs (578).
`crates/core/src/`: lib.rs (4011).
`crates/html/src/`: browser.rs (398), entities.rs (2234, tabla de datos WHATWG §13.5),
fast_hash.rs (146), input_stream.rs (225), lib.rs (97), scripting.rs (246),
serializer.rs (688), tokenizer.rs (3576), tree_builder.rs (5036).

## Cobertura

- `css/animation.rs` — leído — sin hallazgo (clamps MAX_TIME_MS/MAX_ITERATIONS; `function_args` con guarda de char-boundary).
- `css/animation_runtime.rs` — leído — sin hallazgo (bisección acotada 32 iters; `MAX_KEYFRAME_STEPS`; discretos a 50 %).
- `css/calc.rs` — leído — hallazgo `w2-core-03`; resto sin hallazgo (signos iterativos #1128, paréntesis cap 64).
- `css/cascade.rs` — leído — sin hallazgo (índice id/class/tag como superfiltro + tests de equivalencia; `revert-layer`; fingerprint sólo pista).
- `css/color.rs` — leído — hallazgo `w2-core-11`; resto sin hallazgo (depth guard no-panic-safe #1474, finitos filtrados).
- `css/component_value.rs`, `css/token.rs` — leídos — sin hallazgo.
- `css/container.rs` — leído — sin hallazgo (predicción single-pass documentada; `BaseStyles` memoizado).
- `css/custom.rs` — leído — sin hallazgo (budget bytes+work, depth 32, `MAX_CUSTOM_PROPERTIES`, escapes en scan).
- `css/flex.rs` — leído — sin hallazgo.
- `css/grid.rs` — leído — hallazgo `w2-core-12`; resto sin hallazgo (`MAX_TRACKS`/`MAX_REPEAT`).
- `css/lib.rs` — leído (código + 60 tests) — sin hallazgo.
- `css/media.rs` — leído — sin hallazgo (`MAX_MEDIA_NESTING=32`; no-modelados → false documentado).
- `css/parser.rs` — leído — sin hallazgo (`MAX_COMPONENT_NESTING=256`, `MAX_RULE_NESTING=32` #1412).
- `css/properties.rs` — leído — hallazgos `w2-core-04/05/06/10a/10b`; resto sin hallazgo.
- `css/query.rs` — leído — sin hallazgo (`:scope` invalida; variantes `try_*`).
- `css/rule.rs` — leído — sin hallazgo (escapes round-trip #1446).
- `css/selector.rs` — leído — hallazgos `w2-core-08/09`; resto sin hallazgo (budgets compartidos #1413, nesting 32 #376, partes/bytes cap #1414, `AnPlusB` i64 #674).
- `css/shorthand.rs` — leído — sin hallazgo (`var()` diferido; inválido descarta todo).
- `css/state.rs` — leído — sin hallazgo.
- `css/tokenizer.rs` — leído — sin hallazgo (preproceso §3.3, escapes, `MAX_TOKEN_BYTES`).
- `css/transform.rs` — leído — hallazgos `w2-core-01/02`.
- `css/user_agent.rs` — leído — hallazgo `w2-core-07`.
- `dom/lib.rs` — leído — sin hallazgo (generaciones+seed, free-list defensiva #1467, walks acotados, `validate_invariants`).
- `html/browser.rs` — leído — hallazgo `w2-core-14`; resto sin hallazgo (rollback #646/#1121, `document_write` acotado #1125).
- `html/entities.rs` — leído (tabla 2231 entradas) — sin hallazgo.
- `html/fast_hash.rs` — leído — sin hallazgo (seed `| 1` #1122).
- `html/input_stream.rs` — leído — sin hallazgo (CRLF/cols UTF-16).
- `html/lib.rs` — leído — sin hallazgo.
- `html/scripting.rs` — leído — sin hallazgo (sólo inline, errores no-fatales, fuel #639).
- `html/serializer.rs` — leído — sin hallazgo (iterativo, `MAX_SERIALIZED_BYTES`, stale-skipping #32).
- `html/tokenizer.rs` — leído — hallazgo `w2-core-13`; resto sin hallazgo (84 estados, caps #560/#636/#638, errores cap 1024).
- `html/tree_builder.rs` — leído — sin hallazgo (AAA iterativo con budget #412, foster/table, fragmento con contexto cap #1444/#558, `catch_unwind` #1454).

## Hallazgos

### w2-core-01 — `Transform::parse` ignora unidades: `%`/`rad`/sufijos basura se aceptan como px/grados
- archivo:línea: `crates/css/src/transform.rs:216-236` (`parse_args` recorta al prefijo numérico y descarta la unidad)
- lente: contrato-vs-spec | severidad: P2 | categoría: bug
- title: `translate(10%)` computa 10 px, `rotate(1.57rad)` computa 1.57° y `translate(10qpx)` es válido
- evidence: sonda contra HEAD limpio (2 ejecuciones, idéntico resultado):
  `translate(10%) → Some(e=10.0)`, `rotate(1.5707963rad) → a≈0.9996 (1.57°, no 90°)`,
  `translate(10qpx) → Some(e=10.0)`, `translate(1px,2px,3px)` ignora el 3º, `rotate(90deg,1)` ignora el 2º
- impact: posicionamiento/rotación visiblemente erróneos con CSS válido (`rad`, `%`); basura aceptada en silencio
- expected: CSS Transforms 1 §6/§13: ángulos tipados (`rad`→`×180/π`), `%` referido a la caja de referencia (o inválido en este subset), aridad estricta por función
- fix: validar unidad por función (`translate*`: px o número→px, resto inválido; `rotate`: deg/grad/rad/turn→grados; `scale*`: unitless) y aridad exacta
- repro: sonda `/tmp/probe` (T2/T3/T5/T6/T8) + `Transform::parse` en `crates/css/src/transform.rs:42`
- dedup: new (rel: #1539 cerrada — es el feature, no el defecto de unidades)

### w2-core-02 — `Transform::parse` distingue mayúsculas en nombres de función
- archivo:línea: `crates/css/src/transform.rs:51` (`match name` con literales minúsculas; `split_functions` no pliega, `:170`)
- lente: contrato-vs-spec | severidad: P3 | categoría: bug
- title: `TRANSLATE(10px)` / `Rotate(90deg)` se rechazan (valor inválido)
- evidence: `Transform::parse("TRANSLATE(10px)") → None` (2 ejecuciones, HEAD limpio); CSS exige comparación ASCII case-insensitive en nombres de función; sólo `"none"` usa `eq_ignore_ascii_case` (`:44`)
- impact: declaraciones válidas caen a `none`
- expected: CSS Syntax §4.3.4 / Transforms: function names ASCII case-insensitive
- fix: `eq_ignore_ascii_case` o plegar en `split_functions`
- repro: sonda T1
- dedup: new (rel: #1127 índice por tag, #1452 nombres de grid — distintas)

### w2-core-03 — `calc()` elude el tope `|x| > 1e6` de `Length::parse`
- archivo:línea: `crates/css/src/properties.rs:386-424` vs `crates/css/src/calc.rs:260-326` (`number()` sin tope; `to_length` `:46-47` sólo filtra finitud)
- lente: aritmética/límites | severidad: P3 | categoría: bug
- title: `width: 99999999px` es inválido pero `width: calc(99999999px)` computa `Px(1e8)` (y `calc(99999999em)` → `Px(1.6e9)`)
- evidence: `Length::parse("99999999px") → None` vs `Length::parse("calc(99999999px)") → Some(Px(1e8))` (2 ejecuciones, HEAD limpio). Valores resultantes finitos → P3 (sin no-finitos ni crash)
- impact: validación inconsistente; valores gigantes llegan a layout/paint por una sola vía
- expected: el tope anti-DoS debe aplicarse a todo camino que produzca `Length` (o documentar la excepción)
- fix: clamp/tope en `CalcValue::to_length` o en `evaluate_length`/`evaluate_mixed` igual al de `Length::parse`
- repro: sonda L1/L2/L5/L7
- dedup: new (rel: #1128 recursión de signos, #1469 viewport — distintas)

### w2-core-04 — `opacity: NaN` se almacena como NaN (el clamp no filtra no-finitos)
- archivo:línea: `crates/css/src/properties.rs:3200-3203` (`.parse::<f32>().ok()` sin `is_finite`; `NaN.clamp(0,1)` devuelve NaN)
- lente: aritmética/contrato | severidad: P2 | categoría: bug
- title: `opacity: NaN` computa `NaN` y llega a paint/mezclas alfa
- evidence: `compute("opacity: NaN;").opacity → NaN`; `opacity: 1e999 → 1.0` (inf colapsa por accidente) (2 ejecuciones, HEAD limpio)
- impact: NaN en alfa propaga a blending/paint (píxeles indefinidos aguas abajo)
- expected: valores no-finitos son inválidos en CSS → declaración ignorada (1.0 inicial)
- fix: `.filter(|v| v.is_finite())` antes del clamp (igual que `parse_number_or_percent`, `parse_angle_degrees`)
- repro: sonda O1/O2
- dedup: new (rel: #1153 capa layout cerrada, #1445 slice en `Color::parse` — distintas)

### w2-core-05 — `line-height` acepta infinito, NaN y negativos
- archivo:línea: `crates/css/src/properties.rs:3407-3430` (`parse_line_height_factor` filtra `>= 0` pero no finitud, `:3408-3414`; `parse_line_height` acepta cualquier `f32`, `:3416-3430`)
- lente: aritmética/contrato | severidad: P2 | categoría: bug
- title: `line-height: 1e999 → Some(inf)`, `NaN → Some(NaN)`, `-5 → Some(-80.0)`
- evidence: sonda H1/H2/H3 (2 ejecuciones, HEAD limpio)
- impact: inf/NaN/negativo en altura de línea → matemática de layout con no-finitos
- expected: CSS 2.2 §10.8.1: `<number>` no-negativo y `<length>` no-negativa; no-finitos inválidos
- fix: `is_finite` + `>= 0` en ambas vías
- repro: sonda H1/H2/H3
- dedup: new

### w2-core-06 — `background-repeat: repeat, bogus` no invalida la declaración: cicla `[repeat, repeat]`
- archivo:línea: `crates/css/src/properties.rs:4315-4335` (`cycle_property_list` usa `filter_map`: descarta componentes inválidos y cicla el resto)
- lente: contrato-vs-spec | severidad: P3 | categoría: bug
- title: con 2 capas, `repeat, bogus` computa `[repeat, repeat]` en vez de la inicial; `bogus` solo sí cae a inicial (inconsistente)
- evidence: sonda B1 `[Repeat, Repeat]` vs B2 `Repeat` inicial (2 ejecuciones, HEAD limpio); el propio docstring promete invalidez total si falta la declaración pero no si un componente falla — inconsistencia interna
- impact: declaración inválida parcialmente aplicada; desalineación del ciclo multicapa
- expected: CSS Backgrounds 3 §3.1: un componente inválido invalida la declaración completa
- fix: si algún componente no parsea → lista inválida → `default` (no `filter_map` parcial)
- repro: sonda B1/B2
- dedup: new

### w2-core-07 — `user_agent.rs` documenta `:checked`/`:disabled`/`:focus` como `Never`, pero el motor los implementa
- archivo:línea: `crates/css/src/user_agent.rs:8-9` vs `crates/css/src/selector.rs:415-426` (`Focus`/`Checked`/`Disabled` en `:359-370` con `state.rs`)
- lente: contrato-vs-documentación | severidad: P3 | categoría: docs
- title: comentario UA obsoleto contradice los `PseudoClass` reales
- evidence: lectura + sonda Q1 (`div:is(.a)` casa; `Focus`/`Checked`/`Disabled` existen y casan vía `state.rs`)
- impact: confusión de mantenedor/consumidor sobre qué pseudoclases casan
- expected: docs y código de acuerdo
- fix: reescribir el bloque de comentario UA (matizar que la hoja UA usa selectores de atributo, no que el motor no los implemente)
- repro: lectura de ambos archivos (sin fixture)
- dedup: new

### w2-core-08 — `parse_selectors` documenta `:is()/:where()/:not()/:has()` como invalidantes, pero están soportados
- archivo:línea: `crates/css/src/selector.rs:1219-1220` vs `1690-1693` (`"is"|"where"|"not"|"has"` parsean y casan)
- lente: contrato-vs-documentación | severidad: P3 | categoría: docs
- title: docstring pendiente obsoleto («currently invalidate») contradice la implementación y sus tests
- evidence: `parse_selectors("div:is(.a)").matches → true` (sonda Q1, 2 ejecuciones); tests `test_is_where_not_matching_and_specificity` lo confirman
- impact: lectores creen que `:is()` invalida listas; el comportamiento real es correcto
- expected: actualizar docstring (conservar la parte F13 de pseudos desconocidas)
- fix: una línea de doc
- repro: sonda Q1
- dedup: new (rel: patrón #1407 — distinta)

### w2-core-09 — brazo duplicado `first-line | first-letter` (el segundo inalcanzable, sin warning)
- archivo:línea: `crates/css/src/selector.rs:1725-1727` (dos brazos idénticos patrón+guarda)
- lente: código muerto | severidad: P3 | categoría: cleanup
- title: duplicación exacta; el segundo brazo nunca ejecuta y rustc no avisa con guardas
- evidence: compilación limpia (`cargo build -p css` sin warnings tras `cargo clean -p css`) + programa mínimo que imprime `1` (rama 1) — el brazo 2 es código muerto verificado
- impact: nulo en runtime; ruido de mantenimiento
- expected: un solo brazo
- fix: eliminar el duplicado
- repro: `/tmp/dup.rs` + lectura
- dedup: new

### w2-core-10a — `font-weight: 0` aceptado (fuera del rango CSS 1–1000)
- archivo:línea: `crates/css/src/properties.rs:3397-3405` (`other.parse().unwrap_or(400)` sin validación de rango)
- lente: contrato-vs-spec | severidad: P3 | categoría: bug
- title: `font-weight: 0 → 0`; desbordes (`99999999999999999999`) caen a 400 por accidente
- evidence: sonda W1/W2 (2 ejecuciones, HEAD limpio)
- impact: peso 0 indefinido por spec; comportamiento casual según la entrada
- expected: CSS Fonts 4 §2.2: `<number>` en 1–1000; fuera de rango → inválido (400)
- fix: `.filter(|w| (1..=1000).contains(w))` antes de devolver
- repro: sonda W1/W2
- dedup: new (escisión de w2-core-10; corrección Mesa B)

### w2-core-10b — `VerticalAlign::parse` sin tope de magnitud (solo esta mitad; `is_finite` SÍ se filtra)
- archivo:línea: `crates/css/src/properties.rs:989-1018` (ambas ramas filtran `is_finite`, `:1004-1006` y `:1009-1011`; falta el tope 1e6 de `Length::parse`)
- lente: aritmética/contrato | severidad: P3 | categoría: bug
- title: `vertical-align: 999999999px → Length(1e9)` mientras `Length::parse` topa en `|x| > 1e6`
- evidence: sonda V1 `Some(Length(1000000000.0))` (2 ejecuciones, HEAD limpio). Corrección explícita: el claim anterior «sin `is_finite`» era falso y se retira; negativos SÍ son válidos aquí (CSS 2.1 §10.8.1 permite bajar la caja)
- impact: inconsistencia de validación entre dos parseadores de longitudes
- expected: mismo tope 1e6 que `Length::parse` (o documentar la excepción)
- fix: rechazar `|val| > 1e6` en ambas ramas
- repro: sonda V1
- dedup: new (escisión de w2-core-10; corrección Mesa B)

### w2-core-11 — `color-mix()` rechaza espacios `lab`/`lch` (y `xyz-*) válidos en CSS Color 5
- archivo:línea: `crates/css/src/color.rs:586-644` (`match space`: sólo `srgb`/`srgb-linear`/`oklab`/`oklch`; `_ => None`)
- lente: contrato-vs-spec | severidad: P3 | categoría: bug
- title: `color-mix(in lab, red 50%, blue)` y `oklch`→? no: `oklch` sí; `lab`, `lch`, `xyz-*` devuelven `None`
- evidence: sonda M1 `color-mix(in lab, red 50%, blue) → None` vs M2 `in srgb → Some(...)` (2 ejecuciones, HEAD limpio); el módulo promete «CSS Color 4/5» (`color.rs:1-8`)
- impact: declaraciones válidas con interpolación lab/lch caen a valor inicial
- expected: CSS Color 5 §3.2: espacios de interpolación incluyen `lab`, `lch`, `oklab`, `oklch`, `srgb`, `srgb-linear`, `xyz*`
- fix: implementar mezcla en `lab`/`lch` (vía XYZ↔Lab existente: `lab_to_xyz_d50` + inversa) o documentar la limitación en el módulo y en `ComputedStyle`
- repro: sonda M1/M2
- dedup: new

### w2-core-12 — brazo muerto `strip_suffix("fr ")` en `parse_minmax` (máximo ya recortado)
- archivo:línea: `crates/css/src/grid.rs:623-627` (`.strip_suffix("fr").or_else(strip_suffix("FR")).or_else(strip_suffix("fr "))`; `max` viene de `max.trim()`, `:615`)
- lente: código muerto | severidad: P3 | categoría: cleanup
- title: el tercer `or_else` es inalcanzable: tras `trim()` ningún valor termina en espacio
- evidence: lectura estática (2 revisiones del fichero); `minmax(10px, 20fr )` casa por la primera rama, nunca por la tercera
- impact: nulo en runtime; confusión de mantenimiento
- expected: eliminar el brazo o recortar dentro de la rama si se pretendía tolerar espacios internos
- fix: borrar `.or_else(|| max.strip_suffix("fr "))`
- repro: lectura + `parse_template("minmax(100px, 150px)")` (test existente `round19_tests` lo cubre por la vía viva)
- dedup: new

### w2-core-13 — `line_starts` amplifica memoria ~12 B por byte de entrada (16 MiB → ~200 MB transitorios)
- archivo:línea: `crates/html/src/tokenizer.rs:772-778` (`line_starts: Vec<usize>` una entrada por línea) + `input_stream.rs:112-201` (`out: Vec<u32>` 4 B/cp)
- lente: límites/DoS | severidad: P3 | categoría: perf
- title: 2 MiB de `\n` → +24.6 MB RSS medidos (4 B/cp + 8 B/línea ≈ 12.4×); el cap de entrada 16 MiB permite ~200 MB
- evidence: sonda `mem` en HEAD limpio: `nodes=4 rss_before=5200KB rss_after=29812KB delta=24612KB`, `violations` vacío (el input pasa todos los límites); 2 ejecuciones
- impact: amplificación de memoria con input permitido; sin crash pero presión transitoria alta
- expected: o tope documentado o tabla compacta (u32/u64 de 32 bits, o line_starts perezoso); como mínimo documentar el coste
- fix: `Vec<u32>` para line_starts (16 MiB → 64 MB máx.) o cómputo bajo demanda; documentar en `ParseOptions`
- repro: sonda `/tmp/probe mem` (input `"\n".repeat(2MiB)`)
- dedup: new (rel: #1124 cerrada — era el cap de posiciones, que se revirtió; citar, no duplica)

### w2-core-14 — `parse_fragment_into` devuelve violaciones vacías (=éxito) si el parser entra en pánico
- archivo:línea: `crates/html/src/browser.rs:465-474` (`Err(_) => LimitViolations::default()`); llamantes `set_inner_html:77-82`, `set_outer_html:146-151`
- lente: contrato/robustez | severidad: P3 | categoría: bug
- title: el path de pánico es indistinguible del éxito: el llamante destruye el contenido original creyendo que el fragmento parseó
- evidence: código — con `doc` intacto pero sin hijos nuevos, `new_root_children` devuelve `[]`, `violations.is_empty()` es true, y `set_inner_html` libera los hijos viejos e inserta nada (pérdida silenciosa). Sin trigger reproducible: el parser está diseñado para no entrar en pánico → `needs-repro`, no publicar como issue hasta reproducir
- impact: latente — hoy sin trigger conocido; si un pánico fuese alcanzable, pérdida silenciosa de subárbol en `innerHTML`/`outerHTML`
- expected: el path de fallo debe señalar fallo (flag dedicado o `input_truncated=true`) para que el llamante aborte sin tocar el árbol
- fix: devolver violación que fuerce el aborto (`written_bytes_dropped` no; mejor nuevo flag o `Err`), o `debug_assert` + documentar invariante
- repro: needs-repro (sin fixture que dispare el pánico; `catch_unwind` de #1454)
- dedup: new (rel: #1454 aislamiento, #1468 worker, #1121 rollback — citar, no duplica)

## Latent (security — advisory, no publicar como issue)

- `parse_fragment_into` (`browser.rs:437-447`) + llamantes: si el parser entrara en pánico,
  el `Err(_)` devuelve éxito aparente y `set_inner_html`/`set_outer_html` destruyen el
  contenido original. No hay trigger conocido desde entrada (parser iterativo + budgets +
  `catch_unwind`), luego no es explotable hoy: se reporta como advisory para aviso privado
  según `SECURITY.md` si la Mesa lo eleva. Sin evidencia de alcanzabilidad → `needs-repro`.

## Comandos ejecutados (todos desde `/tmp/opencode/megi-verify`, HEAD `3c06db8`)

- `cargo test -p megi-core -p html -p css` → 529 passed, 0 failed (con `CARGO_TARGET_DIR`/`TMPDIR`
  en disco por tmpfs lleno; doctests incluidos en el total final).
- `cargo build -p css` tras `cargo clean -p css` → 0 warnings (evidencia w2-core-09).
- Sonda `/tmp/probe` (deps re-apuntadas al worktree limpio), 2 ejecuciones idénticas:
  T1–T10, L1–L7, O1/O2, H1–H3, W1, B1/B2, Q1/Q2, V1, M1/M2, F1, C1/C2, S1 + `mem`
  (2 MiB `\n` → ΔRSS 24 612 KB, `nodes=4`, sin violaciones).
- `rustc /tmp/dup.rs` → sin warning, imprime `1` (evidencia w2-core-09).
- `git diff 3c06db8 HEAD -- crates/core crates/html crates/css` → vacío (alcance intacto).

## Concern

1. Informe anterior truncado por límite de salida — reescrito completo aquí (15 hallazgos + latent).
2. w2-core-10 corregido y dividido (10a `font-weight`, 10b `VerticalAlign` sin tope; retirado el claim falso sobre `is_finite`; negativos de `vertical-align` son válidos por spec).
3. Severidades proporcionales Mesa B: ningún P0/P1 en el scope (sin crash/hang/DoS/bypass reproducibles); P2 = 01, 04, 05.
4. Dedup limpio con relacionadas citadas (01→#1539, 02→#1127/#1452, 03→#1128/#1469, 04→#1153, 08→#1407, 13→#1124, 14→#1454/#1468/#1121).
5. `w2-core-14` + latent quedan `needs-repro`: no crear issues públicas hasta reproducir un pánico del parser.
