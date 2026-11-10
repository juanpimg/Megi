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
Fase 1: Tokenizador & Árbol HTML5   [██████████] 100% (COMPLETADA - 100% Declarativo)
Fase 2: Motor CSS                    [██████████] 100% (COMPLETADA)
Fase 3: Motor Layout (BFC & IFC)     [██████████] 100% (COMPLETADA)
Fase 4: Paint, PNG & Tipografía      [██████████] 100% (CERRADA salvo background-image/avanzado de tablas)
Fase 5: Red, Render & Imágenes       [███████░░░]  70% (render URL→PNG + imágenes E2E + TLS/caché/data:)
Fase 6: Integración JS & Bindings    [░░░░░░░░░░]   0% (PLANIFICADA; DOM API lista)
Fase 7: Ventana OS & Shell           [██░░░░░░░░]  20% (EN CURSO: ventana winit + headless)

> La Fase 4 está funcionalmente cerrada para el objetivo «HTML real → PNG»: imágenes `<img>`
> E2E, tablas/`inline-block`/reemplazados, PNG comprimido, DOM generacional y painter iterativo.
> Quedan `background-image`, conflictos de `border-collapse`, `rowspan`/`fixed`/`inline-table`
> y block-in-inline.
> La Fase 5 está al ~70 %: pipeline de navegador (`render`), imágenes E2E, TLS/caché/`data:` y
> `@import`/media queries en CSS (el `render` aún no consume `@import`); faltan CORS/cookies/
> HTTP2/compresión de red.
> La Fase 6 (JS) no ha empezado, pero el DOM ya expone `innerHTML`/`outerHTML`/`document.write`
> y búsquedas por id/tag/clase. La Fase 7 arranca con `crates/shell` (ventana winit+softbuffer
> y screenshots headless). **546 tests** y gate de 4 suites PASS; rendimiento en `docs/PERFORMANCE.md`.
> Detalle en `docs/PHASE4-SUPPORT.md`.
```

| Fase / Bloque | Barra | Estado | Cobertura Verificada | Hito Clave Alcanzado |
|---|:---:|:---:|---|---|
| **Fase 0: Tooling, Arena DOM & Closed-Loop Harness** | `[██████████] 100%` | **COMPLETO** | 28 tests en `crates/dom` | Arena contigua indexada (`NodeId(NonZeroU32)` de 4 bytes) sin `Rc<RefCell<Node>>`, scripts CLI de inspección WHATWG en vivo. |
| **Fase 1A: Tokenizador HTML5 (84 Estados & 2.231 Entidades)** | `[██████████] 100%` | **COMPLETO** | 7.050 / 7.050 estados en el gate (allowlist vacía) | 13 suites estándar de `html5lib-tests/tokenizer` al 100%; `ParseOptions` anti-DoS y deduplicación de atributos en tiempo lineal. |
| **Fase 1B: Árbol DOM, Serializador §13.3 & DOM de navegador** | `[██████████] 100%` | **COMPLETO** | 3.898/3.904 árbol + 230/230 serializer + **53 tests html** | 21 modos de inserción, AAA, *foster parenting*, `innerHTML`; **serializador público WHATWG HTML §13.3** (`serialize_html`/`serialize_fragment`/`serialize_outer_html` + opciones) con las 5 suites `html5lib-tests/serializer` en el gate. |
| **Fase 2: Motor CSS (Sintaxis L3, Selectores L4, Cascada L4, Hoja UA §15)** | `[██████████] 100%` | **COMPLETO** | 51/51 unit + 30/30 WPT | Selectores L4 funcionales (`:is/:where/:not/:has`) y estructurales, especificidad, expansión de shorthands antes de la cascada, `font` shorthand, `@media` básico y índice de reglas (2,1× en resolve-styles 10k nodos); bases de Fase 4 (bordes por lado, `border-style: none`, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`, `line_height_factor`). |
| **Fase 3+4: Motor de Layout (BFC/IFC, posicionamiento, floats, tablas)** | `[██████████] 100%` | **COMPLETO** | 121 tests | Box Tree, cajas anónimas *inherit-only*, colapso de márgenes, ICB, `box-sizing`/min-max/%, sobre-restringido; **IFC**: oportunidades de salto reales, soft hyphen de ancho cero, `line-height` como factor, `vertical-align: top/bottom` (§10.8.1) y `text-align: justify`. |
| **Fase 4: Display Lists, Pipeline HTML → PNG & Rasterización 2D** | `[██████████] 100%` | *COMPLETA* | 84 tests en `crates/paint` | Pipeline `HTML → CSS → layout → display list → PNG` con texto real, **orden completo del Apéndice E** (stacking contexts/`z-index`), `OpacityGroup` (grupo offscreen), `PushClip`/`PopClip` desde `overflow`, `text-decoration`, `visibility`, marcadores de lista, bordes por lado, fondo bajo el *border box*, antialiasing exacto y culling/cachés; codificador PNG propio y golden determinista (PNG `md5 34152025…`). PNG comprimido propio (**44,8×**, md5 `9e957177…`), painter iterativo, **imágenes `<img>`** (ImageStore/bilineal) y DOM generacional. |
| **Fase 4: Motor tipográfico (`crates/fonts`)** | `[██████████] 100%` | **COMPLETO (integrado)** | 13 + 1 doctest | `FontDatabase` (fontdb), métricas OpenType (ttf-parser), shaping LTR con `rustybuzz` (kern/ligaduras), rasterización de glifos (`ab_glyph`) y fallback; **integrado en layout y paint** (texto real en el pipeline). Bidi/scripts complejos/fuentes variables quedan fuera de Fase 4. |
| **Fase 5: Red, Render Pipeline & Imágenes** | `[███████░░░] 70%` | *EN CURSO* | 85 (net) + 28 (render) + 46 (image) | URL WHATWG (subset http/https/file + join), HTTP/1.1 `GET`, **TLS 1.2/1.3 con `rustls` 0.23 (ring) + `webpki-roots`** (SNI/validación, feature `tls` por defecto), **caché HTTP RFC 9111 subset** (max-age/Expires/ETag/Last-Modified/304/no-store/Vary), **`data:` URLs** con Base64 propio, `file://`, MIME sniffing y `ResourceLoader` con caché; `#![forbid(unsafe_code)]`. **`crates/render`**: pipeline URL→PNG con CSS externo, CLI y **`<img>` E2E**; **`crates/image`**: decoder PNG/inflate propio; CSS con `@import`/media queries. Pendientes: integrar `@import` en render, CORS/cookies/HTTP2/compresión. |
| **Fase 6: Integración JS & DOM Bindings** | `[░░░░░░░░░░] 0%` | *Planificada* | — | Enlaces e integración de ejecución de scripts (`document.write`, DOM bindings). |
| **Fase 7: Ventana OS & Shell** | `[██░░░░░░░░] 20%` | *EN CURSO* | 20 tests en `crates/shell` | Ventana `winit` 0.30 + `softbuffer` 0.4 (X11/Wayland dlopen, sin GPU), presentación `0RGB`, resize con debounce y **modo headless `--screenshot`** (640×400 `md5 7324e851…`) + `tools/megi.sh`. Pendientes: scroll/hit-testing/navegación/zoom/HiDPI/worker. |

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

## FASE 1B: Construcción del Árbol DOM (`TreeBuilder`) — 21 Modos de Inserción

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

**Estado ~100 %** para el objetivo «HTML real → PNG». Implementados: pipeline, tipografía real, bordes por lado, posicionamiento/floats/overflow, stacking/`opacity`, tablas CSS 2.1, `inline-block`, cajas reemplazadas, **imágenes `<img>` E2E**, PNG comprimido propio, DOM generacional y painter iterativo. Pendientes menores: `background-image`, conflictos de `border-collapse`, `rowspan`/`table-layout: fixed`/`inline-table` y block-in-inline.

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

## FASE 5: Red, Render Pipeline & Imágenes (EN CURSO, ~70%)

| WP | Objetivo | Estado |
|---|---|---|
| `WP-5.0`–`WP-5.4` | URL/HTTP1.1/TLS/caché/`data:`/file/MIME/loader (`net`) | **HECHO** (85 tests) |
| `WP-5.5` | Decoder PNG + inflate propio (`image`) | **HECHO** (46 tests) |
| `WP-5.6` | Pipeline de navegador URL→PNG con CSS externo (`render`) | **HECHO** (28 tests) |
| `WP-5.7` | **Imágenes `<img>` E2E** (fetch/decode/tamaño intrínseco/pintado/caps/placeholder) | **HECHO** (fixture 15.625 B `md5 1de31699…`) |
| `WP-5.8` | `@import` + media queries en CSS / integración en `render` | **PARCIAL**: parser/evaluador CSS hechos (51 tests); el `render` aún no consume `@import` |
| `WP-5.9` | CORS/`Referrer-Policy`, cookies, gzip/br, HTTP/2, streaming | **PENDIENTE** |

Criterio global: `cargo test -p net -p image -p render` + `cargo run -p render -- tests/fixtures/render/images.html /tmp/images.png` + `bash tools/render_fixture.sh`.

---

## FASE 6: Integración JS & DOM Bindings (PLANIFICADA — DECISIÓN PENDIENTE DEL DUEÑO)

**Estado**: **0 % de ejecución de JS**; `crates/js` no existe. El DOM ya expone la API de navegador necesaria (`document_element/head/body`, `get_element_by_id`, `get_elements_by_tag_name/class_name`, `text_content`, `innerHTML`/`outerHTML`, `document_write`, factorías y mutaciones con `DomError`) y la arena tiene handles generacionales con free list, que es la base del diseño de bindings.

**No se implementa nada hasta que el dueño resuelva el ADR**: [`JAVASCRIPT-STRATEGY.md`](JAVASCRIPT-STRATEGY.md) (ADR-001). Resumen de la propuesta:

- **Motor recomendado**: **Boa** `boa_engine` **0.21.x** (pure Rust, MSRV Rust 1.88, licencia Unlicense OR MIT), en un nuevo `crates/js`, con **feature `js` opcional y apagada por defecto**; alternativa/plan de migración: **QuickJS vía `rquickjs`** (MIT, casi 100 % Test262, pero añade toolchain C y FFI).
- **Diseño de bindings**: tabla lateral `HashMap<NodeId, JsValue>` por realm/mundo, wrappers con comprobación de generación (handles stale/foráneos rechazados), `EventTarget` mínimo, `document.write`/`innerHTML` reutilizando la API del parser, y presupuesto anti-DoS (instrucciones/tiempo/memoria/pila, sin red/FS por defecto).
- **Qué NO se hace**: motor JS propio, delegar parseo HTML/CSS/layout/paint a JS, JIT, o exponer `fetch`/filesystem por defecto.

**Preguntas pendientes del dueño** (detalle en el ADR §8): (1) motor Boa vs QuickJS; (2) feature `js` apagada vs encendida por defecto; (3) alcance inicial mínimo (`document.write` + bindings, ≈14 días) vs eventos/timers/CSSOM/`querySelector` (≈25 días).

**Plan propuesto (si se aprueba)**: `WP-6.1` crate/feature/realm (3 d) → `WP-6.2` bindings DOM core (6 d) → `WP-6.3` `<script>` parser-inserted + `document.write` (5 d; criterio: `scripted_*` fuera de la allowlist y árbol **3.904/3.904**) → `WP-6.4` eventos/timers/event loop (5 d) → `WP-6.5` `querySelector`/CSSOM (6 d) → `WP-6.6` endurecimiento/fuzz (3 d) → `WP-6.7` shell/docs (3 d). Total ≈30 días-persona; camino mínimo ≈14 días.

## FASE 7: Ventana del Sistema & Shell (EN CURSO, ~20%)

| WP | Objetivo | Estado |
|---|---|---|
| `WP-7.1` | Ventana nativa `winit` 0.30 + presentación `softbuffer` 0.4 (X11/Wayland dlopen, sin GPU) | **HECHO** |
| `WP-7.2` | Modo headless `--screenshot` (sin `EventLoop`, CI-friendly) + CLI `megi` + `tools/megi.sh` | **HECHO** (640×400 `md5 7324e851…`) |
| `WP-7.3` | Navegación básica (URL bar/atajos/historial), re-render y manejo de errores en ventana | **PARCIAL**: render/errores/resize hechos; sin navegación/historial |
| `WP-7.4` | Scroll, inputs de ratón/teclado, foco y **hit-testing** (`NodeId`/`LayoutBox` ↔ `DisplayItem`) | **PENDIENTE** |
| `WP-7.5` | Render en worker con cancelación, zoom y **HiDPI/device-scale** | **PENDIENTE** |
| `WP-7.6` | Accesibilidad mínima, menús/atajos y pruebas de ventana reales en CI con Xvfb | **PENDIENTE** |

Limitaciones actuales: sin scroll/hit-testing/navegación/zoom/JS, sin GPU, render en el hilo de eventos, HiDPI scale 1. La ventana no se ha verificado visualmente en el host de desarrollo (sin servidor gráfico), pero compila y el modo headless está testeado.

### Fuera del alcance inicial de Fase 7

GPU/compositor, pestañas, extensiones, descargas, impresión, sandbox de procesos por sitio (se diseñará con la Fase 6) y accesibilidad completa.
