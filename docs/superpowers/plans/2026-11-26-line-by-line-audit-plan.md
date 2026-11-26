# Plan de auditoría línea por línea y cierre de brechas hacia un navegador 100 % funcional

> Estado: **plan activo**. Dueño: agentes de IA con revisión humana.
> Barra de referencia: spec v2 §2 (`docs/superpowers/specs/2026-11-24-megi-ambition-design.md:23-41`).
> Reglas: `AGENTS.md` (única fuente de reglas). No editar artefactos generados
> (`PROGRESS.md`, `docs/status/*`, `docs/audit/FINDINGS.md`, `docs/work-packages/BOARD.md`).

## 1. Objetivo

Auditar **todo el código del motor** (270.986 líneas Rust en `crates/`, 273.621
`.rs` contabilizadas por `PROGRESS.md`) línea por línea, convertir cada hallazgo
en un work package verificable y cerrar las brechas que separan a Megi de un
navegador capaz de renderizar cualquier página de Internet con paridad frente a
Chrome/Ladybird.

**Criterio de honestidad**: ninguna afirmación de "100 %" es válida mientras la
barra de producción de spec §2 esté sin cumplir. El propio spec lo dice en `:43`:
«el estado actual está lejos de esta barra».

## 2. Estado real verificado (2026-11-26)

| Métrica | Valor real | Evidencia |
|---|---|---|
| Hitos cerrados | **6/13** (M0, M1, M2, M3, M7, M12) | `PROGRESS.md:7-25` |
| Criterios verificados | **47/64 (73 %)** | `PROGRESS.md:7` |
| Tests | **3.697** | `PROGRESS.md:7` |
| Gate html5lib | PASS 7.050/7.050 + 3.904/3.904 + 230/230 + CSS 30/30 | `PROGRESS.md:201-203` |
| WPT adoptado | **596/596 en 18 fixtures, 6 dirs**; 103 fixtures `skip` del manifiesto; ≈0,03 % del árbol de referencia (~2,18 M) | `docs/status/wpt.md:5-17`; `tests/wpt/MANIFEST.json`; spec §1/:11 |
| Test262 | **13 casos locales**, no el árbol tc39 | `docs/status/test262.json:5-16` |
| Sitios reales (live) | **2 PASS / 1 DEGRADED / 1 ERROR** (example timeout, rust 9,2 s + 3 warnings) | `docs/status/real-sites.json`; `docs/status/real-sites.html` |
| Cobertura declarada | 76/76 **autorreferencial** | `docs/status/coverage.json`; `PROGRESS.md:11` |
| Paridad visual vs Chrome | example 0,92 %; Wikipedia 4,88 %; rust 10,78 %; HN 17,39 % de píxeles distintos con `--system-fonts` | `docs/VISUAL-PARITY.md:43-53` |
| Fuzzing | 34 targets, 176 seeds; **sin ejecuciones publicadas** (barra: 1 M/target) | `fuzz/Cargo.toml`; `.github/workflows/fuzz-nightly.yml`; spec §2/:31 |
| Sandbox | `enforced` solo en Linux; macOS/Windows `unsupported` | `crates/browser/src/ipc/sandbox.rs:28-33`; `PROGRESS.md:103` |
| Gates sin compilar | 49 gates Python en verde, pero `update_coverage` depende de un sello de conformidad fresco | `tools/check_no_compile_gates.py` |

**Conclusión**: hoy Megi es un pipeline de render determinista con JS embebido y
un shell funcional, **no** un motor equivalente a Ladybird. Las métricas en verde
son internamente consistentes pero sus denominadores son locales y pequeños.

## 3. Metodología de auditoría línea por línea

1. **Unidad**: un archivo `.rs` (o un módulo ≤500 líneas). Para cada uno:
   - leer el archivo completo;
   - registrar `leído — sin hallazgo` o un hallazgo con `archivo:línea`,
     norma (WHATWG/W3C/ECMA/RFC), impacto y comando de reproducción;
   - usar subagentes en paralelo por área, sin editar mientras auditamos;
   - priorizar por `impacto × frecuencia`, no por orden alfabético.
2. **Evidencia**: toda conclusión lleva salida de comando o cita de código.
   Si un doc y el código discrepan, manda el código (`AGENTS.md` §1.3).
3. **Conversión a trabajo**: cada hallazgo se escribe como WP con los campos de
   `docs/work-packages/SCHEMA.md` y se registra en la cola con las herramientas
   (`tools/wp_generate.py`, `tools/wp_plan.py`). Máx. 10 `in_progress`.
4. **TDD**: test observable que falla → fix mínimo → gates → commit atómico DCO.
5. **Sin compilar mientras el disco esté justo**: primero agotar gates Python,
   revisión estática y datos; compilar sólo cuando el WP lo exija y haya espacio.
6. **Estado**: regenerar `PROGRESS.md`/status con herramientas, nunca a mano;
   el ledger `docs/audit/coverage.tsv` debe quedar en 0 pendientes.

### Plantilla de ficha

```text
MÓDULO: crates/<crate>/<archivo>.rs  (N líneas)
ESTADO: leído — sin hallazgo | hallazgo #n
HALLAZGO: <archivo:línea> — <qué falta>
NORMA: <WHATWG §x / W3C §y / ECMA §z / RFC n>
IMPACTO: <sitio real o API afectada>
VERIFICACIÓN: <comando + salida>
WP: WP-<FUERTE>-<clave>   ESFUERZO: S|M|L   HITO: M#
```

## 4. Inventario y avance de la auditoría

| Área | Líneas | Estado | Plan |
|---|---:|---|---|
| `core` + `html` | 20.776 | Escaneo de área completo; línea por línea pendiente | F1 |
| `css` + `layout` + `paint` + `raster` + `fonts` + `image` + `svg` | 100.900 | Escaneo de área completo | F2 |
| `net` + `js` + `render` | 95.848 | Escaneo de área completo | F3 |
| `browser` + `webview` + `display` + `test_harness` + `sandbox` + `bench` | 51.056 | Escaneo de área completo | F4 |
| Herramientas, WPT, fuzz y métricas | — | Escaneo de área completo | F5 |

> El escaneo de área no sustituye la lectura línea por línea; sólo prioriza dónde empezar.

## 5. Hallazgos priorizados (top 25)

Los IDs `A#` son hallazgos de auditoría; se convertirán en WPs al ejecutar cada fase.

### F1 — DOM, HTML y parser

| ID | Hallazgo | Norma | E |
|---|---|---|---|
| A1 | `document.write` sólo hace append al `<body>`; sin insertion point ni re-tokenización (`html/src/browser.rs:169-210`, `js/.../document/parse.rs:89-94`) | HTML §13.2.6.4.7 | L |
| A2 | `ScriptExecutor` omite scripts externos parser-blocking (`html/src/scripting.rs:21-23`, `tree_builder/insertion.rs:126-133`) | HTML §4.12.1 | L |
| A3 | Script metadata no llega al hook (`type`, `nomodule`, `src`, `defer`): `html/src/scripting.rs:50-57` | HTML §4.12.1 | M |
| A4 | Shadow DOM ausente en la arena (`core/src/node.rs:265-282`); tabla lateral en `js` | DOM §4.9 | L |
| A5 | Topes duros truncan atributos/texto (256 attrs, 64 KiB/valor, 16 MiB texto) | anti-DoS propio | M |
| A6 | API DOM incompleta: `importNode`, `adoptNode`, `Range`, `TreeWalker`, `NamedNodeMap` | DOM §4.4 | M |
| A7 | `innerHTML` fuerza `scripting_enabled=false` (`html/src/browser.rs:68,138`) | HTML §13.4 | S |
| A8 | `set_outer_html` rechaza `head`/`body` (`html/src/browser.rs:116-126`) | DOM §4.4 | S |
| A9 | Fragment mutations clonan el documento completo (`tree_builder/document.rs:192-220`) | HTML §13.4 | M |
| A10 | Parser sólo `&str` completo; sin stream ni sniffing (`tree_builder/document.rs:14-44`) | HTML §13.2.3.2 | L |

### F2 — CSS, layout, paint y formatos

| ID | Hallazgo | Norma | E |
|---|---|---|---|
| A11 | `transform` sólo ajusta geometría; no hay `DisplayItem::Transform` (`layout/.../positioning.rs:183-227`) | CSS Transforms 1 §3 | M |
| A12 | Sin scroll containers, scrollbars ni sticky real (`css/.../text.rs:209-216`, `paint/background.rs:18-19`) | CSS Overflow 3 §3 | L |
| A13 | Shaping sólo LTR horizontal; bidi/CJK/vertical incompleto (`fonts/src/db/shape.rs:229-234`) | UAX #9, CSS Writing Modes 3 | L |
| A14 | Filtros: 10 funciones, blur box aproximado, sin SVG filters (`css/.../effect.rs:7-37`) | Filter Effects 1 §3 | M |
| A15 | Gradientes truncados (64 stops/256 capas); sin OKLCH/ICC completo | CSS Images 3 §3.4, Color 4 | M |
| A16 | Widgets de formulario sintéticos; `select` sin flecha nativa (`paint/src/painter/forms.rs:13-14`) | HTML §4.10.5 | M |
| A17 | `border-collapse` sin reparto de borde; tablas legacy divergentes (`layout/src/table.rs:41-54`) | CSS 2.1 §17.5.2 | M |
| A18 | `mask-image` sólo primer `url()` (`css/src/properties/mask.rs:1-6`) | CSS Masking 1 §6.1 | S-M |
| A19 | SVG filters/text incompletos (`svg/src/document/model.rs:220-260`) | SVG 2 §12/§10 | L |
| A20 | GIF animado quieto; JPEG CMYK y WebP ALPH comprimido rechazados | GIF89a, T.81, WebP | M-L |

### F3 — Red, JS y render

| ID | Hallazgo | Norma | E |
|---|---|---|---|
| A21 | ESM por defecto es preprocesador línea a línea; path nativo sólo con `await`/`import(` (`js/.../modules.rs:49-165`) | ECMA-262 §16.2 | M |
| A22 | `DOMParser.parseFromString` devuelve `{}` (`prelude_b.js:594-597`) ⇒ bbc/clarin TypeError (#2245) | DOM Parsing §6 | M |
| A23 | `IntersectionObserver`/`ResizeObserver` son stubs (`prelude_b.js:478-506`) | IO §3, RO §2 | L |
| A24 | `getBoundingClientRect` usa rects pre-script congelados (`render/.../script.rs:239-259`) | CSSOM View §4 | L |
| A25 | `performance.mark/measure/getEntries*` no-ops (`js/.../builders.rs:63-77`) | User Timing 3 | S |
| A26 | CORS incompleto: Expose-Headers, Vary: Origin, Max-Age, `null`, credentials (`net/src/cors.rs:20-33`) | Fetch §4.2/§4.8 | M |
| A27 | `blob:` y `URL.createObjectURL` no reales (`net/src/url.rs:21-23`) | File API §8 | M |
| A28 | Presupuestos y reloj virtual saltan scripts (#1881/#2246) | HTML §8.1.7 | M |
| A29 | Sin IDNA/UTS #46 (`net/src/psl.rs:16-18`) | URL §3.2.4 | M |
| A30 | Scripts se ejecutan post-parse; sin parser-blocking real (`render/src/scripts.rs:49-55`) | HTML §13.2.6 | L |

### F4 — Shell y capacidades nativas

| ID | Hallazgo | Barra §2 | E |
|---|---|---|---|
| A31 | Sandbox enforced sólo Linux; sin aislamiento real por pestaña | Seguridad, Estabilidad | L |
| A32 | Formularios sin submit/validación/select real/IME completo | Interactividad | L |
| A33 | UI sin atlas SVG, hover/focus ni gate contra `UI-TARGET.md` | M9/auditoría | M |
| A34 | `Ctrl+D` sólo imprime texto; descargas sin I/O; sin impresión/portapapeles real | Interactividad | M-L |
| A35 | ARIA parcial (~25 roles), sin nombre accesible completo ni AT verificado | Interactividad/M5 | L |
| A36 | `SessionLock` no cableado; se pierde estado | Interactividad | M |
| A37 | Render en vuelo no abortable; un estado para todas las pestañas | Estabilidad | L |
| A38 | Find-in-page sin regex/Unicode real | Interactividad | M |
| A39 | DevTools un target; BiDi sin eventos de red/consola | Herramientas | L |

### F5 — Medición y conformidad

| ID | Hallazgo | Barra §2 | E |
|---|---|---|---|
| A40 | 103 fixtures WPT `skip`; allowlist=0 por exclusión, no por fix | Conformidad | L |
| A41 | Test262 con 13 casos locales, no tc39 | Conformidad | L |
| A42 | Sin 1 M ejecuciones/target publicadas | Estabilidad | L |
| A43 | Real-sites: 2/4 PASS; `PROGRESS.md` cita artefacto viejo 4/4 | Sitios reales | M |
| A44 | Coverage 76/76 autorreferencial | Conformidad | M |
| A45 | WP engine con 44 WPs `depth` ya cerrados; faltan generadores WPT/Test262 | Proceso | M |

## 6. Secuencia de ejecución

| Fase | Entrada | Trabajo | Salida verificable |
|---|---|---|---|
| F0 | Este plan + auditorías de área | Inventario línea por línea por módulo | Fichas `leído — sin hallazgo`/hallazgo; cola WPs |
| F1 | A1–A10 | Parser↔JS↔DOM reales | `document.write` conforme; scripts parser-blocking; shadow DOM en arena |
| F2 | A11–A20 | Paridad visual | scroll/sticky, transforms pintados, bidi/CJK, widgets, tablas |
| F3 | A21–A30 | Web moderna | ESM nativo, observers, geometría fresca, CORS/IDNA/blob |
| F4 | A31–A39 | Navegador de escritorio | sandbox 3 SO, formularios, descargas/impresión, a11y, UI verificada |
| F5 | A40–A45 | Conformidad honesta | WPT epochs crecientes, Test262 real, fuzz publicado, 100 sitios |
| F6 | Barra §2 completa | Producción | release verificada, auditoría externa, bus factor >1 |

**Regla de dependencia**: F1–F4 pueden avanzar en paralelo si no comparten
archivos (AGENTS.md §0). F5 agrega barras y no puede cerrarse antes. F6 agrega todo.

## 7. Cómo actualizar el estado (para agentes)

1. Ejecutar gates Python: `python3 tools/check_no_compile_gates.py`.
2. Si cambia el inventario de archivos: `python3 tools/audit_coverage.py --generate`
   y marcar cada fila nueva como `covered` **después de leerla**.
3. Si cambian métricas: `python3 tools/update_progress.py --write` (o
   `--no-count --no-gate` si no se puede compilar) y `python3 tools/update_coverage.py --write`.
4. Regenerar el tablero con `python3 tools/wp_plan.py --board`; nunca editar `BOARD.md`.
5. Publicar evidencia: comando + salida + código de salida en el commit/issue.
6. Si una auditoría contradice un doc histórico, abrir issue para el doc; el código manda.

## 8. Definición de "100 % funcional" (defendible)

Sólo se podrá afirmar cuando, simultáneamente:

- WPT con allowlist 0 **sobre el árbol adoptado**, con denominador creciendo hacia el completo;
- Test262 del subconjunto declarado con allowlist 0 y denominador real upstream;
- 100/100 sitios del corpus PASS, sin warnings degradantes, <2,5 s;
- 0 fugas cross-site, fuzz 1 M/target, sandbox enforced en Linux/macOS/Windows;
- paridad visual con diferencias residuales documentadas por nombre;
- auditoría externa publicada y bus factor >1.

Hoy **no se cumple**. La frase correcta es: «596/596 subtests adoptados pasan;
cobertura WPT ≈0,03 % del árbol de referencia; 2/4 sitios PASS en vivo; no somos
funcionalmente equivalentes a Ladybird/Chrome».
