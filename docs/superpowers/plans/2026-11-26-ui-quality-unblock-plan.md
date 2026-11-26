# UI Quality Unblock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convertir la chrome de Megi de formas dibujadas a mano en un sistema de UI coherente (iconos reales, estados, foco, tipografía legible, favicons SVG y verificación contra la ventana real), sin Rune ni dependencias de UI/layout/paint.

**Architecture:** La chrome sigue siendo pintado puro sobre `Pixmap` dentro de `browser`, pero pasa a tener un sistema de diseño: tokens semánticos, un atlas de iconos SVG rasterizados con el crate `svg` propio (expuesto por `webview`), y un modelo de foco/hover compartido por pintado e hit-testing. Un gate Python compara capturas reales contra la referencia versionada. Ningún bloque comparte archivos con otro; el integrador fusiona y regenera artefactos.

**Tech Stack:** Rust puro (sin dependencias nuevas), `megi_svg::render_svg`, `megi_image::RgbaImage`, `megi_webview::render` como fachada, `Pixmap`, Pillow para el gate Python, `spectacle` para captura real de Wayland.

**Spec:** `docs/UI-TARGET.md` + `docs/images/ui-target.jpg` (md5 `e145dca139302d79764a35d64ad20916`). Leer ambos antes de tocar código.

## Global Constraints

Copiadas literalmente de `AGENTS.md`; todo bloque las hereda:

- «`#![forbid(unsafe_code)]` en los 17 crates de biblioteca (18 en el workspace; `megi-bench` es binario). Cero `unsafe`.»
- «Cero `Rc<`, `Rc<RefCell<`, `Weak<` en código nuevo. Usa `NodeId` + arena, `Arc<T>` solo para payload inmutable `Send + Sync`, o `mpsc` para mutación.»
- «Cero lifetimes propios ('a) en tipos públicos que crucen crates.»
- «Cero dependencias que implementen parseo HTML/CSS, layout o pintado (html5ever, cssparser, selectores, stylo, taffy, blitz…). Consulta §3.1.»
- «Cero dependencias circulares entre crates. Un edge nuevo requiere PR de arquitectura + actualización del DAG.»
- «Archivo `.rs` nuevo: **≤ 400 líneas** (objetivo) / **500 líneas** (tope duro).»
- «1. Un test mínimo que compruebe comportamiento observable (no un mock). 2. `cargo test -p <crate> <nombre>` y confirmar que FALLA por la razón esperada. 3. Implementar el arreglo más pequeño; refactorizar solo en verde. 4. Citar la norma. 5. Cambios visuales: golden PNG byte-idéntico salvo decisión explícita con md5 antiguo y nuevo en el commit.»
- «No edites artefactos generados: `PROGRESS.md`, `docs/status/*`, `docs/audit/FINDINGS.md`, `docs/work-packages/BOARD.md`.»
- «Antes de reportar verde, ejecuta `deslop-please --allow .deslop-please.allow`… Cero hallazgos nuevos.»
- «Nunca digas "verde" sin salida pegada.»
- Carga: horquilla `huacnlee/cargo`, **no fijar `CARGO_TARGET_DIR`**, `nice`/`ionice`, `CARGO_BUILD_JOBS=2`, un solo gate completo por integración.

## Review Focus

Entradas y fallos que los tests de los bloques deben cubrir explícitamente:

1. **Escala HiDPI 1,25**: con `scale = 1.25` los controles de la barra deben seguir alineados en fila y el blit de la página debe caer exactamente tras el sidebar — test en Task 2 (`hiDpi_controls_share_one_row`).
2. **Ventana estrecha (800 CSS px)**: los tiles del sidebar se reducen en columnas, nunca se recortan ni solapan la página — test en Task 2 (`narrow_sidebar_never_clips`).
3. **SVG malformed o gigante**: un icono/favicon inválido de >64 KiB debe degradar al color determinista, nunca panicar ni bloquear el render — tests en Task 1 (`oversized_icon_is_rejected`) y Task 3 (`oversized_svg_favicon_is_rejected`).
4. **Alineación hit/pintado**: después de unificar la barra, un clic en cada glifo debe activar ese control — test en Task 2 (`hit_rects_match_painted_controls`).
5. **Pestañas `file://` sin título**: el sidebar debe mostrar el nombre del archivo, no la URL completa, sin cambiar el comportamiento `http(s)` — test en Task 4.

## Estructura de archivos y propiedad

| Bloque | Dueño exclusivo de archivos | No toca |
|---|---|---|
| Task 0 (integrador) | `docs/work-packages/queue/WP-USER-ui-*.toml` | código |
| Task 1 iconos | `crates/browser/src/chrome/icons.rs` (nuevo), `crates/browser/src/chrome/tokens.rs`, `crates/browser/tests/chrome_icons.rs` | `shell.rs`, `chrome.rs` |
| Task 2 shell | `crates/browser/src/chrome/shell.rs`, `chrome.rs`, `navigation.rs`, `tests/mod.rs`, `crates/browser/src/window/input.rs`, `window/presentation.rs`, `tests/fixtures/chrome/compact.png` | `tokens.rs`, `icons.rs` |
| Task 3 favicon | `crates/render/src/lib.rs`, `crates/browser/src/favicon.rs`, `crates/browser/tests/favicon_svg.rs` | chrome |
| Task 4 títulos | `crates/browser/src/tabs.rs`, `crates/browser/tests/tab_titles.rs` | chrome |
| Task 5 gate | `tools/check_window_ui.py`, `tools/test_check_window_ui.py`, `tools/capture_window_ui.sh`, `tools/capture_real_window.sh`, `tools/check_no_compile_gates.py`, `docs/UI-TARGET.md`, `tests/fixtures/chrome/ui/*.png` | Rust |

Regla de fusión: cada bloque trabaja en su propio worktree desde `origin/main`, hace commit DCO atómico con su ID de WP, y el integrador rebasea + regenera artefactos con herramientas. Un bloque que necesite un archivo de otro usa **sólo la interfaz declarada abajo**; si la interfaz no basta, abre issue y espera, no edita el archivo ajeno.

---

### Task 0: Registrar los work packages

**Files:**
- Create: `docs/work-packages/queue/WP-USER-ui-icons.toml`, `WP-USER-ui-shell.toml`, `WP-USER-ui-favicon.toml`, `WP-USER-ui-titles.toml`, `WP-USER-ui-gate.toml`

**Interfaces:**
- Produces: WPs reclamables con `python3 tools/wp_plan.py --claim WP-USER-ui-<clave>`.

Campos comunes: `source = "user"`, `spec = "docs/UI-TARGET.md"`, `metric = "user_issues"`, `milestone = "M9"`, `impact = 8`, `frequency = 8`, `profile` = `heavy` para Tasks 1–4 y `light` para Task 5. `verify_cmd` = el comando de verificación de cada tarea. `expected`/`actual`/`acceptance` = los valores de la tabla de cada tarea.

- [ ] **Step 1: Escribir los cinco TOML con el esquema de `docs/work-packages/SCHEMA.md`**
- [ ] **Step 2: Validar**

Run: `python3 -c "import tomllib,glob; [tomllib.load(open(p,'rb')) for p in glob.glob('docs/work-packages/queue/WP-USER-ui-*.toml')]"`
Expected: EXIT 0, cinco archivos.
- [ ] **Step 3: Verificar que el tablero los ve**

Run: `python3 tools/wp_plan.py --board | grep -c 'WP-USER-ui-'`
Expected: `5`.
- [ ] **Step 4: Commit**

`git add docs/work-packages/queue/WP-USER-ui-*.toml && git commit -s -m 'chore(wp): queue UI quality unblock packages'`

---

### Task 1: WP-USER-ui-icons — tokens semánticos y atlas de iconos SVG

**Files:**
- Create: `crates/browser/src/chrome/icons.rs`, `crates/browser/tests/chrome_icons.rs`
- Modify: `crates/browser/src/chrome/tokens.rs` (añadir estados; NO cambiar `layout_shell`, `sidebar_columns`, `sidebar_history_top` ni `ShellView`)

**Interfaces:**
- Consumes: `megi_webview::render::decode_image_with_svg(bytes: &[u8], size: u32) -> Option<megi_image::RgbaImage>` (Task 3; la firma está fijada aquí para trabajar en paralelo).
- Produces:
  - `pub enum Icon { Back, Forward, Reload, ToggleSidebar, Gear, Share, Plus, Menu, Search, Globe, Folder, People, Help, User }` con `pub const ALL: &'static [Icon]` (los 14 variantes, en ese orden).
  - `pub struct IconAtlas { /* privado */ }`
  - `impl IconAtlas { pub fn new() -> Self; pub fn paint(&self, pixmap: &mut Pixmap, icon: Icon, rect: Rect, color: Color, scale: f32) -> usize; }`
  - `pub struct StatePalette { pub hover: Color, pub focus: Color, pub pressed: Color, pub disabled: Color }`
  - `pub const STATE: StatePalette` derivado de `Palette::LIGHT`? **No**: de `Palette::DEFAULT` (la chrome es oscura; `docs/UI-TARGET.md`).

- [ ] **Step 1: Test RED**

```rust
// crates/browser/tests/chrome_icons.rs
#[test]
fn icon_atlas_rasterizes_every_shell_icon() {
    let atlas = megi_browser::chrome::IconAtlas::new();
    for icon in megi_browser::chrome::Icon::ALL {
        let mut pixmap = megi_webview::paint::Pixmap::new(20, 20);
        let painted = atlas.paint(&mut pixmap, icon, megi_webview::layout::Rect::new(0.0,0.0,20.0,20.0),
                                  megi_webview::paint::Color::new(255,255,255,255), 1.0);
        assert!(painted > 0, "icon {icon:?} produced no pixels");
    }
}
#[test]
fn oversized_icon_is_rejected() {
    assert!(megi_webview::render::decode_image_with_svg(&vec![b'x'; 1_000_000], 16).is_none());
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p megi-browser --test chrome_icons`
Expected: EXIT 101, `cannot find type IconAtlas` / `decode_image_with_svg`.

- [ ] **Step 3: Implementar**

`IconAtlas::new()` rasteriza cada SVG embebido (constantes `&str` propias, sin assets externos) a 20×20 con `decode_image_with_svg` y guarda el `RgbaImage` en un `Vec<(Icon, Pixmap)>`. `paint()` aplica source-over con `color` sobre la imagen ya rasterizada y devuelve el número de píxeles escritos. SVG de 64 KiB máximo, sin red de por medio. `StatePalette` se deriva de `SHELL` con `color-mix` equivalente en enteros (p. ej. `hover = mezcla 8 % de ACCENT sobre text`). Citar CSS UI States: `:hover/:focus-visible` y WCAG 2.2 §1.4.3 para el contraste del estado focus.

- [ ] **Step 4: GREEN**

Run: `cargo test -p megi-browser --test chrome_icons`
Expected: EXIT 0, dos tests.

- [ ] **Step 5: Contraste**

```rust
#[test] fn state_palette_focus_contrast_is_at_least_4_5() { /* luminancia de STATE.focus sobre SHELL.top */ }
```
Run: `cargo test -p megi-browser --test chrome_icons state_palette_focus_contrast_is_at_least_4_5`
Expected: EXIT 0.

- [ ] **Step 6: Commit**

`git commit -s -m 'feat(chrome): WP-USER-ui-icons add semantic states and SVG icon atlas'`

---

### Task 2: WP-USER-ui-shell — barra, foco y estados como un solo componente

**Files:**
- Modify: `crates/browser/src/chrome/shell.rs`, `crates/browser/src/chrome.rs`, `crates/browser/src/chrome/navigation.rs`, `crates/browser/src/chrome/tests/mod.rs`, `crates/browser/src/window/input.rs`, `crates/browser/src/window/presentation.rs`, `tests/fixtures/chrome/compact.png`

**Interfaces:**
- Consumes: `Icon`, `IconAtlas`, `STATE` (Task 1); `sidebar_columns`, `sidebar_history_top`, `ShellView`, `layout_shell` (estables en `tokens.rs`).
- Produces:
  - `ShellView` añade `pub hover: Option<ShellHit>` y `pub focus: Option<ShellHit>` (ambos `Copy`).
  - `pub enum ShellHit` añade `Nav(crate::chrome::NavigationButton)`.
  - `pub fn shell_focus_order(view: &ShellView<'_>) -> Vec<ShellHit>` en `tokens.rs`? **No**: en `shell.rs`, para no tocar `tokens.rs`.

- [ ] **Step 1: Tests RED** (en `crates/browser/src/chrome/tests/mod.rs`)

```rust
#[test] fn hiDpi_controls_share_one_row() {
    let v = shell_view(&["A".into()], "https://example.com", CHROME_HEIGHT);
    let l = layout_shell(1000.0, 700.0, 1.25, &v);
    let cy = |r: megi_webview::layout::Rect| r.y + r.height * 0.5;
    assert!((cy(l.dots[0]) - cy(l.nav.back)).abs() <= 6.0);
    assert!((cy(l.toggle) - cy(l.actions[0])).abs() <= 6.0);
}
#[test] fn narrow_sidebar_never_clips() {
    let v = shell_view(&["A".into()], "https://example.com", CHROME_HEIGHT);
    let l = layout_shell(800.0, 600.0, 1.0, &v);
    let cols = super::tokens::sidebar_columns(l.sidebar.width - super::RAIL_CSS - 16.0, 52.0);
    assert!(cols <= 3 && l.page.x >= l.sidebar.x + l.sidebar.width - 1.0);
}
#[test] fn hit_rects_match_painted_controls() { /* para cada ShellHit, shell_hit en el centro devuelve ese hit */ }
#[test] fn tab_cycles_focus_over_top_bar_controls() { /* shell_focus_order empieza en ToggleSidebar y termina en Menu */ }
```

- [ ] **Step 2: Run** `cargo test -p megi-browser --lib shell_`
Expected: EXIT 101 por `ShellHit::Nav` / `shell_focus_order` ausentes.

- [ ] **Step 3: Implementar**

Sustituir glifos dibujados a mano en `paint_shell` por `IconAtlas::paint`; aplicar `STATE.hover/focus/pressed` según `view.hover`/`view.focus`; `shell_hit` devuelve `Nav(...)` usando `navigation_buttons_in_top_bar`; `shell_focus_order` devuelve la secuencia fija `ToggleSidebar, Nav(Back), Nav(Forward), Nav(Reload), Activate(0)…, NewTab`. `input.rs` usa `shell_hit` para puntero y Tab/Shift+Tab sobre esa secuencia; `presentation.rs` rellena `hover`/`focus` desde el estado de la ventana.

- [ ] **Step 4: GREEN** `cargo test -p megi-browser --lib shell_`
Expected: EXIT 0.

- [ ] **Step 5: Re-sellar golden**

Renderizar con el binario real:
`./target/release/megi 'data:text/html,<title>Compact</title><h1>Workspace</h1>' --screenshot tests/fixtures/chrome/compact.png --width 320 --height 240 --chrome --chrome-compact`
Registrar `md5 antiguo -> md5 nuevo` en el mensaje del commit. Verificar `cargo test -p megi-browser --test shell_tests chrome_compact_band`.

- [ ] **Step 6: Captura real**

`DISPLAY=:0 spectacle --background --output /tmp/opencode/ui-task2.png`, leer la imagen y comprobar que los controles están en una fila y el sidebar no se recorta.

- [ ] **Step 7: Commit**

`git commit -s -m 'feat(chrome): WP-USER-ui-shell unify states, icons and focus'`

---

### Task 3: WP-USER-ui-favicon — decoder SVG expuesto por `webview`

**Files:**
- Modify: `crates/render/src/lib.rs`, `crates/browser/src/favicon.rs`
- Create: `crates/browser/tests/favicon_svg.rs`

**Interfaces:**
- Produces: `megi_render::decode_image_with_svg(bytes: &[u8], size: u32) -> Option<megi_image::RgbaImage>`, re-exportado automáticamente como `megi_webview::render::decode_image_with_svg` (`crates/webview/src/lib.rs:254` ya re-exporta `megi_render`). Sin edge nuevo en el DAG.

- [ ] **Step 1: Tests RED**

```rust
#[test] fn svg_favicon_decodes_to_rgba() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#ff0000"/></svg>"#;
    let f = megi_browser::favicon::Favicon::from_bytes(svg, "https://es.wikipedia.org");
    assert!(f.pixels.is_some(), "SVG favicon must not fall back to a dot");
    assert!(f.pixels.unwrap().iter().any(|&b| b != 0));
}
#[test] fn non_svg_favicon_still_decodes() { /* PNG mínimo conocido -> pixels Some */ }
#[test] fn oversized_svg_favicon_is_rejected() { assert!(megi_webview::render::decode_image_with_svg(&vec![b'x';1_000_000],16).is_none()); }
```

- [ ] **Step 2: Run** `cargo test -p megi-browser --test favicon_svg`
Expected: EXIT 101, `pixels` es `None` para el SVG.

- [ ] **Step 3: Implementar**

En `render`: detectar SVG por sniffing (`<?xml`/`<svg` en los primeros 256 bytes, tolerando BOM/espacios) y llamar `megi_svg::render_svg_with_limits(bytes, &SvgOptions{width:Some(size),height:Some(size)}, &megi_svg::Limits::default())`, convirtiendo a `RgbaImage`; si no es SVG, `megi_image::decode(bytes).ok()`. En `favicon::decode_icon`, sustituir `megi_webview::image::decode(bytes)` por `megi_webview::render::decode_image_with_svg(bytes, ICON_SIZE)`. Sin nuevos crates.

- [ ] **Step 4: GREEN** `cargo test -p megi-browser --test favicon_svg`
Expected: EXIT 0, tres tests.

- [ ] **Step 5: Commit**

`git commit -s -m 'feat(render): WP-USER-ui-favicon rasterize SVG favicons through webview'`

---

### Task 4: WP-USER-ui-titles — títulos legibles para pestañas `file:`

**Files:**
- Modify: `crates/browser/src/tabs.rs`
- Create: `crates/browser/tests/tab_titles.rs`

**Interfaces:**
- Consumes: `Tab::title`, `Tab::navigation.current()` (existentes).
- Produces: `Tab::display_title() -> String` con comportamiento nuevo sólo para esquema `file`.

- [ ] **Step 1: Tests RED**

```rust
#[test] fn file_tab_without_title_shows_file_name_not_url() {
    let tab = /* Tab con current = file:///home/jp/doc/wiki.html, title = None */;
    assert_eq!(tab.display_title(), "wiki.html");
}
#[test] fn http_tab_without_title_keeps_url() {
    let tab = /* current = https://example.com/a, title = None */;
    assert_eq!(tab.display_title(), "https://example.com/a");
}
```

- [ ] **Step 2: Run** `cargo test -p megi-browser --test tab_titles`
Expected: EXIT 101, `left: "file:///home/jp/doc/wiki.html"`.

- [ ] **Step 3: Implementar**

En `title_fallback`, si `url.scheme()` es `File`, usar el último segmento del path (`percent_decode`) con `strip_suffix` de extensión opcional? **No**: conservar la extensión, el test exige `wiki.html`. Para `Http/Https` y `Data`, dejar el comportamiento actual. Citar WHATWG URL §4.3 (`path`).

- [ ] **Step 4: GREEN** `cargo test -p megi-browser --test tab_titles`
Expected: EXIT 0.

- [ ] **Step 5: Commit**

`git commit -s -m 'fix(tabs): WP-USER-ui-titles show file name for titled file tabs'`

---

### Task 5: WP-USER-ui-gate — verificación visual real en el gate

**Files:**
- Create: `tools/check_window_ui.py`, `tools/test_check_window_ui.py`, `tools/capture_window_ui.sh`, `tools/capture_real_window.sh`, `tests/fixtures/chrome/ui/{hn,rust,wikipedia}.png`
- Modify: `tools/check_no_compile_gates.py`, `docs/UI-TARGET.md`

**Interfaces:**
- Consumes: PNGs generados por `tools/capture_window_ui.sh` (CLI `megi --screenshot --width 1000 --height 700`, sin chrome para paridad con Chromium) y referencias Chromium ya documentadas en `docs/VISUAL-PARITY.md`.
- Produces: `python3 tools/check_window_ui.py --check` (EXIT 0 si MAE ≤ umbrales), y `python3 tools/check_window_ui.py --compare MEGI.png CHROME.png --max-mae 18` para uso manual.

- [ ] **Step 1: Tests RED**

`tools/test_check_window_ui.py` con imágenes sintéticas: `test_compare_rejects_mae_above_threshold`, `test_compare_accepts_identical_images`, `test_missing_capture_fails_with_clear_message`.

- [ ] **Step 2: Run** `python3 -m unittest tools.test_check_window_ui`
Expected: EXIT 101, `ModuleNotFoundError`.

- [ ] **Step 3: Implementar**

`check_window_ui.py` usa Pillow: `mae(a,b) = mean(|ΔR|+|ΔG|+|ΔB|)/3` sobre mismo tamaño; `--compare` imprime `MAE=…` y EXIT 0/1; `--check` lee las tres referencias de `tests/fixtures/chrome/ui/` y compara con capturas opcionales pasadas por `--captures DIR`, con umbrales HN 18, rust 14, wikipedia 22 (valores derivados de las mediciones actuales 15,48 / 10,62 / 19,22; margen +3). `capture_window_ui.sh` genera las capturas con el binario release y `nice/ionice`. `capture_real_window.sh` usa `spectacle --background --output …` y no falla si el compositor no soporta captura: imprime el motivo.

- [ ] **Step 4: GREEN**

`python3 -m unittest tools.test_check_window_ui && python3 tools/check_no_compile_gates.py`
Expected: EXIT 0, gate `window_ui` incluido en la lista.

- [ ] **Step 5: Capturas de referencia**

Ejecutar `capture_window_ui.sh`, verificar MAE reales contra las referencias Chromium y registrarlos en `docs/UI-TARGET.md` (sección «Verificación automática»). No inventar cifras.

- [ ] **Step 6: Commit**

`git commit -s -m 'test(ui): WP-USER-ui-gate add visual parity gate for window shell'`

---

## Integración final (integrador, no bloque paralelo)

1. Rebasear cada WP sobre `origin/main` en orden de llegada; conflictos de golden se resuelven conservando el md5 regenerado por el dueño del golden (Task 2).
2. Regenerar artefactos **sólo con herramientas**: `audit_coverage.py --generate` + `--mark` (leer diffs antes), `update_progress.py --write`, `update_coverage.py --write`.
3. Un único gate §6+§8 por integración, con salida pegada. `deslop-please --allow .deslop-please.allow` en 0, allowlist intacta.
4. Captura real de la ventana con `spectacle` y capturas 1000×700 contra Chromium; pegar md5 y MAE en el informe.
5. `wp_plan.py --done WP-USER-ui-<clave> --metric "…"` por cada bloque cerrado.

## Criterio de éxito

- Los 5 WPs cerrados con métrica declarada.
- `cargo test --workspace` verde, `cargo clippy --workspace --all-targets -- -D warnings` verde, `check_no_compile_gates` con el gate `window_ui` incluido.
- Captura real leída por un humano: una sola fila de controles, sidebar sin recortes, favicon de Wikipedia rasterizado, títulos `file:` legibles.
- MAE frente a Chromium no peor que la línea base: HN 15,48 / rust 10,62 / Wikipedia 19,22.
