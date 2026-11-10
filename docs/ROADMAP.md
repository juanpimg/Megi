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
Fase 4: Paint, PNG & Tipografía      [█████████░]  90% (CASI COMPLETA: render real determinista)
Fase 5: Red & Fetch Pipeline         [█░░░░░░░░░]  10% (EN CURSO: bootstrap URL/HTTP/loader)
Fase 6: Integración JS & Bindings    [░░░░░░░░░░]   0% (PLANIFICADA)
Fase 7: Ventana OS & Shell           [░░░░░░░░░░]   0% (PLANIFICADA)

> La Fase 4 está casi cerrada: el pipeline `HTML → CSS → layout → display list → PNG`
> renderiza texto real (fontes integradas), bordes por lado, antialiasing y es determinista
> byte a byte. Quedan `position`/stacking (`WP-4.6`), floats (`WP-4.7`), tablas (`WP-4.8`)
> y cerrar `inline-block`/clip/robustez/matriz (`WP-4.9`/4.10/4.12/4.13).
> La Fase 5 arranca con el crate `net` (URL/HTTP1.1/file/MIME/loader, 57 tests, sin TLS).
> Detalle HECHO/PARCIAL/PENDIENTE por paquete: secciones «FASE 4» y «FASE 5» de este
> documento y `docs/PHASE4-SUPPORT.md`; rendimiento en `docs/PERFORMANCE.md`.
```

| Fase / Bloque | Barra | Estado | Cobertura Verificada | Hito Clave Alcanzado |
|---|:---:|:---:|---|---|
| **Fase 0: Tooling, Arena DOM & Closed-Loop Harness** | `[██████████] 100%` | **COMPLETO** | 9/9 tests unitarios en `crates/dom` | Arena contigua indexada (`NodeId(NonZeroU32)` de 4 bytes) sin `Rc<RefCell<Node>>`, scripts CLI de inspección WHATWG en vivo. |
| **Fase 1A: Tokenizador HTML5 (84 Estados & 2.231 Entidades)** | `[██████████] 100%` | **COMPLETO** | 7.050 / 7.050 estados en el gate (allowlist vacía) | 13 suites estándar de `html5lib-tests/tokenizer` al 100%; `ParseOptions` anti-DoS y deduplicación de atributos en tiempo lineal. |
| **Fase 1B: Construcción del Árbol DOM (21 Modos) & Serializador §13.3** | `[██████████] 99.9%` | **COMPLETO (Declarativo)** | 3.898 / 3.904 árbol + **230/230 serializador** | 21 modos de inserción, AAA, *foster parenting*, `innerHTML`; **serializador público WHATWG HTML §13.3** (`serialize_html`/`serialize_fragment`/`serialize_outer_html` + opciones) con las 5 suites `html5lib-tests/serializer` en el gate. |
| **Fase 2: Motor CSS (Sintaxis L3, Selectores L4, Cascada L4, Hoja UA §15)** | `[██████████] 100%` | **COMPLETO** | 41/41 unit + 30/30 WPT CSS Syntax vendorizado | Selectores L4 funcionales (`:is/:where/:not/:has`) y estructurales, especificidad, expansión de shorthands antes de la cascada, `font` shorthand, `@media` básico y índice de reglas (2,1× en resolve-styles 10k nodos); bases de Fase 4 (bordes por lado, `border-style: none`, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`, `line_height_factor`). |
| **Fase 3: Motor de Layout (BFC e IFC)** | `[██████████] 100%` | **COMPLETO (núcleo)** | 80 tests en `crates/layout` | Box Tree, cajas anónimas *inherit-only*, colapso de márgenes, ICB, `box-sizing`/min-max/%, sobre-restringido; **IFC**: oportunidades de salto reales, soft hyphen de ancho cero, `line-height` como factor, `vertical-align: top/bottom` (§10.8.1) y `text-align: justify`. |
| **Fase 4: Display Lists, Pipeline HTML → PNG & Rasterización 2D** | `[█████████░] 90%` | *CASI COMPLETA* | 52 tests en `crates/paint` | Pipeline `HTML → CSS → layout → display list → PNG` con texto real (fonts integradas), **bordes por lado** (`solid`/`dashed`/`dotted`/`double`/`none|hidden`, mitre) y fondo bajo el *border box*,  antialiasing exacto, culling + cachés (raster 1598,8→0,9 ms), codificador PNG propio sin dependencias, canvas propagado (§14.2) y golden determinista. Pendientes: `position`/stacking/`opacity`, floats, tablas, `inline-block`, clip y compresión PNG. |
| **Fase 4: Motor tipográfico (`crates/fonts`)** | `[██████████] 100%` | **COMPLETO (integrado)** | 13 + 1 doctest | `FontDatabase` (fontdb), métricas OpenType (ttf-parser), shaping LTR con `rustybuzz` (kern/ligaduras), rasterización de glifos (`ab_glyph`) y fallback; **integrado en layout y paint** (texto real en el pipeline). Bidi/scripts complejos/fuentes variables quedan fuera de Fase 4. |
| **Fase 5: Red & Fetch Pipeline** | `[█░░░░░░░░░] 10%` | *EN CURSO* | 57 tests en `crates/net` | Bootstrap std-only: URL WHATWG (subset http/https/file + join), HTTP/1.1 `GET` (timeouts, Content-Length, chunked, redirects 301/302/303/307/308, caps), `file://`, MIME sniffing WHATWG (subset) y `ResourceLoader` con caché; `#![forbid(unsafe_code)]`. Pendiente: TLS (`rustls`), caché HTTP, Fetch/CORS y streaming. |
| **Fase 6: Integración JS & DOM Bindings** | `[░░░░░░░░░░] 0%` | *Planificada* | — | Enlaces e integración de ejecución de scripts (`document.write`, DOM bindings). |
| **Fase 7: Ventana OS & Shell** | `[░░░░░░░░░░] 0%` | *Planificada* | — | Creación de ventana de SO, gestión de eventos y bucle principal de renderizado. |

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

## FASE 4: Display Lists, Pipeline HTML → PNG y Tipografía Real (CASI COMPLETA, ~90%)

**Objetivo de la fase**: renderizar una página HTML real (autocontenida, sin red ni JavaScript) a una imagen PNG, completando el modelo de pintado CSS 2.2 (§9.9/Apéndice E, §10, §14.2) y la tipografía. **Estado actual**: el pipeline completo funciona con **texto real**, **bordes por lado**, **antialiasing** y determinismo byte a byte; lo que falta son los esquemas de posicionamiento y los formatos de layout restantes (floats, tablas, `inline-block`), el clicado desde `overflow` y cerrar la matriz de soporte.

- Resultado visual: [`../docs/images/phase4-basic.png`](../docs/images/phase4-basic.png) / `tests/fixtures/phase4/basic.html`.
- Matriz detallada por propiedad CSS: [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md).
- Rendimiento del pipeline y del rasterizador: [`PERFORMANCE.md`](PERFORMANCE.md).

| WP | Objetivo | Crate | Estado |
|---|---|---|---|
| `WP-4.0` | Cimientos CSS (bordes, props. de Fase 4, selectores, `@media`, `line-height` factor) | `css` | **HECHO** |
| `WP-4.1` | Pipeline `HTML→PNG`, codificador PNG sin dependencias, canvas §14.2, `<style>` | `paint` | **HECHO** |
| `WP-4.2` | BFC y modelo de cajas (anónimos, collapso, ICB, box-sizing/min-max/%/sobre-restringido, `<br>`) | `layout` | **HECHO** |
| `WP-4.3` | Pintado de caja: fondos en *border box*, bordes por lado con 4 colores/estilos y esquinas mitre | `paint` | **HECHO** |
| `WP-4.4` | Tipografía real integrada: métricas/shaping/glyph runs/baseline en layout y paint | `fonts`, `layout`, `paint` | **HECHO** |
| `WP-4.5` | IFC conforme: saltos UAX#14, soft hyphen, `line-height` factor, `vertical-align` §10.8.1, justify | `layout` | **HECHO** |
| `WP-4.6` | `position: relative/absolute/fixed`, `z-index`, stacking contexts y `opacity` por grupos | `layout`, `paint` | **PENDIENTE** |
| `WP-4.7` | Floats y `clear` (§9.5) y su paso de pintado | `layout`, `paint` | **PENDIENTE** |
| `WP-4.8` | Tablas CSS 2.1 (layout automático, `colspan`/`rowspan`, `border-collapse`) | `layout`, `paint` | **PENDIENTE** |
| `WP-4.9` | `inline-block`, marcadores `list-item`, `hr`, `text-decoration` y placeholders de reemplazados | `css`, `layout`, `paint` | **PARCIAL** |
| `WP-4.10` | `overflow` y clipping de descendientes (`PushClip`/`PopClip` desde el painter) | `css`, `layout`, `paint` | **PARCIAL** |
| `WP-4.11` | Calidad de raster: AA exacto, cachés/culling, (premultiplicado/compresión PNG pendientes) | `paint` | **HECHO** (AA/perf) |
| `WP-4.12` | Robustez/presupuestos: límites anti-DoS, API con errores explícitos, profundidad layout/paint | `html`, `dom`, `layout`, `paint` | **PARCIAL** |
| `WP-4.13` | Golden images/fixtures, matriz de soporte y documentación en verdad | `paint`, `docs` | **PARCIAL** |

### `WP-4.0` — Cimientos CSS (estado: **HECHO**)
- **Objetivo**: `ComputedStyle` con toda la información que layout/paint necesitan y semántica normativa de bordes, shorthands y selectores.
- **Spec**: CSS 2.2 §8.5 (`box.html#border-style-properties`, `#border-width-properties`, `#border-color-properties`, `#border-shorthand-properties`), §9.5/§11.1.1/§11.2/§16.3.1; Selectors L4 (`https://www.w3.org/TR/selectors-4/`); CSS Sizing 3; CSS Lists 3; CSS Conditional 3 `@media`.
- **Hecho**: bordes por lado/shorthands y `border-style: none|hidden` ⇒ ancho 0; `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`; selectores L4 funcionales y estructurales, especificidad correcta, expansión de shorthands antes de la cascada, `font` shorthand, `@media` básico (`resolve_document_styles_with_media`) e índice de reglas (resolve-styles 10k nodos 63,6 → 30,3 ms, 2,1×); `line_height_factor` aditivo.
- **Criterio**: `cargo test -p css` (41/41) + `cargo run -p test_harness -- --summary` (CSS 30/30).

### `WP-4.1` — Pipeline HTML → PNG (estado: **HECHO**)
- **Objetivo**: driver reproducible `HTML → CSS → layout → display list → Pixmap → PNG`, canvas blanco, propagación raíz/`body` (§14.2) y extracción de `<style>`.
- **Spec**: CSS 2.2 §14.2 (`colors.html#background`), CSS Backgrounds 3 §2.11 (`#root-background`, `#body-background`), CSS 2.2 Apéndice E paso 1; PNG 3, RFC 1950/1951.
- **Hecho**: codificador PNG propio sin dependencias (DEFLATE *stored*, CRC-32/Adler-32), `RenderConfig { background, transparent }`, canvas opaco (0 % de transparencia), `collect_style_text`/`resolve_embedded_styles`, ejemplo `html_to_png`, golden determinista. **Limitación**: PNG sin compresión (~1,92 MB a 800×600), seguimiento en `WP-4.11`.
- **Criterio**: `cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png` + `python3 tools/png_check.py /tmp/basic.png` + `cargo test -p paint --test end_to_end_html --test png_format`.

### `WP-4.2` — BFC y modelo de cajas (estado: **HECHO**)
- **Objetivo**: cajas anónimas correctas, colapso de márgenes, ICB, `box-sizing`/min-max/%, sobre-restringido y `<br>`.
- **Spec**: CSS 2.2 §9.2.1.1 (`visuren.html#anonymous-block-level`), §8.3.1 (`box.html#collapsing-margins`), §10.1/§10.3.3/§10.4/§10.6.3/§10.6.7/§10.7; CSS Sizing 3.
- **Hecho**: anónimos *inherit-only* (fin de márgenes/fondos fantasma y huecos por whitespace), `<br>` forzado, colapso de márgenes con negativos, ICB con `viewport.height`, alturas porcentuales, `box-sizing`, min/max y sobre-restringido; `tests/bfc_conformance.rs` (18).
- **Pendiente menor**: *self-collapsing margins* (bloques de altura cero con márgenes superior+inferior) — documentado en `HANDOFF.md` y `PHASE4-SUPPORT.md`.
- **Criterio**: `cargo test -p layout` (80).

### `WP-4.3` — Pintado de caja: fondos y bordes por lado (estado: **HECHO**)
- **Objetivo**: `DisplayItem::Border` con cuatro lados independientes (ancho/estilo/color), esquinas a inglete y fondo bajo el *border box*.
- **Spec**: CSS 2.2 §8.5, §14.2; CSS Backgrounds 3 §2.7 (`#background-clip`); CSS 2.2 Ap. E pasos 2/4.
- **Hecho**: `Border { rect, sides: [BorderSide; 4] }` con estilos `solid`/`dashed`/`dotted`/`double`/`none|hidden` (groove/ridge/inset/outset degradan a sólido), partición de esquinas por diagonal, fondo bajo el borde; verificable en la captura del README y en el golden.
- **Criterio**: `cargo test -p paint --test end_to_end_html --test raster_quality`.

### `WP-4.4` — Tipografía real integrada (estado: **HECHO**)
- **Objetivo**: métricas OpenType, shaping, baseline y máscaras de glifo con una única fuente de verdad entre layout y paint.
- **Spec**: CSS Fonts 4 §5 (`#font-matching-algorithm`), CSS 2.2 §15.3 y §10.8.1 (`visudet.html#leading`), OpenType `OS/2`/`hhea`; WHATWG HTML §15.3.
- **Hecho**: `crates/fonts` (`fontdb`+`ttf-parser`+`rustybuzz`+`ab_glyph`) integrado en `layout` (métricas/shaping/líneas) y `paint` (máscaras con antialiasing); `Text` en la display list lleva `font_weight`, `font_italic` y `baseline`; fuentes DejaVu deterministas.
- **Criterio**: `cargo test -p fonts -p layout -p paint` + render real en el PNG del fixture.

### `WP-4.5` — IFC conforme (estado: **HECHO**)
- **Objetivo**: saltos de línea correctos (UAX #14), soft hyphen invisible, `line-height` numérico, `vertical-align` §10.8.1 y `justify`.
- **Spec**: CSS 2.2 §9.4.2, §10.8/§10.8.1; CSS Text 3 (`#line-breaking`, `#white-space-property`, `#hyphenation`); UAX #14.
- **Hecho**: los breaks solo ocurren en oportunidades reales (nodos inline adyacentes sin espacio ya no se separan), soft hyphen de ancho cero que solo materializa `-` al romper, `line-height` numérico heredado como factor, `vertical-align: top/bottom` con *aligned subtree* y cálculo unificado, `text-align: justify` en líneas soft-wrap; `tests/ifc_conformance2.rs` (14) y `fonts_integration.rs` (4).
- **Criterio**: `cargo test -p layout` (80).

### `WP-4.6` — Posicionamiento y contextos de apilamiento (estado: **PENDIENTE**)
- **Objetivo**: `position: relative/absolute/fixed`, contenedores de posicionados, posición estática, `z-index`, árbol de *stacking contexts* y `opacity` como grupo compuesto.
- **Spec**: CSS 2.2 §9.3/§9.4.3/§9.6 (`visuren.html#positioning-scheme`, `#relative-positioning`, `#absolute-positioning`), §9.9/§9.9.1 (`#z-index`, `#layers`), §10.1, §10.3.7, §10.6.4 y Apéndice E; CSS Color 3 §transparency; Compositing 1 §groups.
- **Estado**: propiedades calculadas en CSS pero **no leídas** por layout/paint; un `position: absolute` se pinta hoy en el flujo normal. Bloqueador principal de fidelidad de páginas reales.
- **Criterio**: `cargo test -p layout -p paint` con `test_absolute_removed_from_normal_flow`, `test_z_index_ordering`, `test_opacity_group_composited_once`.

### `WP-4.7` — Floats y `clear` (estado: **PENDIENTE**)
- **Spec**: CSS 2.2 §9.5 (`visuren.html#floats`, `#float-rules`, `#flow-control`).
- **Estado**: `float`/`clear` se parsean, pero layout y paint no los aplican (ni el acortado de line boxes ni el paso 5 del Apéndice E).
- **Criterio**: `cargo test -p layout -p paint` con `test_line_box_shortened_beside_float`, `test_clear_below_float`.

### `WP-4.8` — Tablas CSS 2.1 (estado: **PENDIENTE**)
- **Spec**: CSS 2.2 §17 (`tables.html`), CSS Tables 3 (`#table-layout-algorithm`).
- **Estado**: `display: table/table-row/table-cell` se apila como bloques; no hay algoritmo de columnas/filas, `colspan`/`rowspan` ni `border-collapse`.
- **Criterio**: `cargo test -p layout -p paint` con tabla 3×3 con bordes y celdas heterogéneas.

### `WP-4.9` — `inline-block`, listas, decoraciones y reemplazados (estado: **PARCIAL**)
- **Spec**: CSS 2.2 §10.3.9 (`#inlineblock-width`), §10.8.1, §10.3.2, §12.5; CSS Text Decoration 3 (`#line-decoration`); CSS Lists 3; WHATWG HTML §15.3.
- **Estado**: `inline-block` sigue mapeado a inline normal (sin shrink-to-fit ni baseline propia); los marcadores de lista y `text-decoration` se parsean pero no se pintan; `<img>` genera caja vacía hasta que la Fase 5 aporte red/decodificación (placeholder planificado).
- **Criterio**: `cargo test -p layout -p paint` con `test_inline_block_shrink_to_fit_width`, `test_list_item_disc_marker_outside`, `test_anchor_underline_rendered`.

### `WP-4.10` — Overflow y clipping (estado: **PARCIAL**)
- **Spec**: CSS 2.2 §11.1 (`visufx.html#overflow`, `#overflow-clipping`), CSS Overflow 3.
- **Estado**: el rasterizador ya soporta clips (`ClipStack`/`PushClip`/`PopClip` y respecta el clip en `fill_rect`/bordes/texto), pero el painter todavía no genera clips desde `overflow`; `overflow` se parsea.
- **Criterio**: `cargo test -p paint` con `test_overflow_hidden_clips_descendant`, `test_overflow_hidden_does_not_clip_own_background`.

### `WP-4.11` — Calidad de raster y rendimiento (estado: **HECHO** en AA/perf; PNG comprimido pendiente)
- **Spec**: Compositing 1 §simple alpha compositing (`#simplealphacompositing`); el muestreo/AA es decisión del UA.
- **Hecho**: antialiasing exacto por cobertura para rectángulos fraccionarios, bordes y glifos; alfa straight justificado; **culling** de items fuera del canvas y cachés por pasada (shaped runs, caras, máscaras de glifo). Baseline: raster 800×600 1598,8 → 0,9 ms; render-to-PNG 1628 → 7,7 ms; pipeline 1842 → 170,7 ms (10,8×).
- **Pendiente**: compresión PNG (o enmienda de política) y, opcionalmente, representación premultiplicada interna.
- **Criterio**: `cargo test -p paint --test raster_quality` + `./tools/bench.sh` comparado con [`PERFORMANCE.md`](PERFORMANCE.md).

### `WP-4.12` — Robustez y rendimiento (estado: **PARCIAL**)
- **Hecho**: límites anti-DoS `ParseOptions` (profundidad 4.096, 1 M nodos, 4.096 atributos, 16 MiB), `LimitViolations`, serializador iterativo, DOM con `try_node` y validación de ciclos; tree builder ya **lineal** (eliminada la CPU cuadrática; 40k atributos ~10 ms) y presupuestos medidos con `bench`.
- **Pendiente**: recursión de layout/paint en documentos muy profundos, API de layout con errores explícitos (`Result`), presupuestos de tiempo en layout/paint.
- **Criterio**: `cargo test -p html -p dom -p layout -p paint` + benchmarks estables.

### `WP-4.13` — Golden images, fixtures y documentación (estado: **PARCIAL**)
- **Hecho**: golden determinista de `basic.html` regenerado (`basic.rgba` `md5 4139fdd8…`, PNG del pipeline `md5 d97c67dc…`), fixture enriquecido con bordes por lado y texto real, validador `tools/png_check.py` y matriz de soporte/documentación sincronizadas (esta ronda).
- **Pendiente**: fixtures `real_page.html`/`torture.html`, goldens cuando existan posicionamiento/tablas, y comparación con tolerancia si se añade más AA.
- **Criterio**: `cargo test -p paint --test end_to_end_html` + `python3 tools/png_check.py`.

### Camino crítico restante de Fase 4

```text
WP-4.6 (position/stacking/opacity) ─► WP-4.7 (floats) ─┐
WP-4.8 (tablas) ───────────────────────────────────────┼─► WP-4.9 (inline-block/listas/decoraciones)
WP-4.10 (clips desde overflow) ────────────────────────┘        │
WP-4.12 (robustez/presupuestos) ∥                              ▼
                                                      WP-4.13 (fixtures real_page/torture + goldens)
```

Prioridad recomendada: **WP-4.6** (mayor impacto de fidelidad), seguido de **WP-4.8** (tablas) y **WP-4.10**, con **WP-4.7** y **WP-4.9** en paralelo; **WP-4.12/4.13** cierran la fase.

### Fuera del alcance de Fase 4

Red y `<img>` reales (Fase 5), JavaScript (Fase 6), formularios interactivos, `flex`/`grid`, `@font-face`, bidi/scripts complejos, scroll/hit-testing/eventos (Fase 7), animaciones, `border-radius`, `box-shadow`, `background-image`/gradientes y medios paginados. Ver [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md).

---

## FASE 5: Red & Fetch Pipeline (EN CURSO, ~10%)

**Objetivo de la fase**: implementar el pipeline de carga de recursos de WHATWG Fetch (URL, HTTP/TLS, caché, CORS/referrer, carga de subrecursos del documento) para que Megi pueda dejar de depender de HTML autocontenido y renderizar páginas reales con hojas de estilo y, más adelante, imágenes.

**Estado actual**: **bootstrap completado** con el crate `crates/net`, **std-only** (`#![forbid(unsafe_code)]`, 0 dependencias externas) y **57 tests offline** (cada test HTTP arranca su propio `TcpListener` en `127.0.0.1:0`; `file://` usa ficheros temporales).

### `WP-5.0` — URL WHATWG (subset) y `join` relativo (**HECHO**)
- **Spec**: WHATWG URL Standard (`https://url.spec.whatwg.org/`).
- **Hecho**: esquemas `http`, `https` y `file`; userinfo, hosts dominio/IPv4/IPv6 entre corchetes, puertos por defecto, normalización de path (`.`/`..`), query y fragment, percent-encoding de los *encode sets* de la spec y resolución relativa (`Url::join`).
- **Limitaciones**: subset (sin IDNA/hosts no ASCII, sin `data:`/`ftp:`/`blob:`, sin percent-encoded hosts).
- **Criterio**: `cargo test -p net --test url_tests` (12).

### `WP-5.1` — Cliente HTTP/1.1 `GET` sobre `std::net` (**HECHO**)
- **Spec**: RFC 9110/9112 (HTTP Semantics/1.1), lectura de `Content-Length` y `Transfer-Encoding: chunked`.
- **Hecho**: `TcpStream` bloqueante, timeouts de conexión/lectura, cuerpos `Content-Length` y *chunked*, redirecciones 301/302/303/307/308 con límite, caps de respuesta (16 MiB) y cabeceras (64 KiB).
- **Limitaciones**: sin TLS (https ⇒ `FetchError::TlsUnsupported`), sin HTTP/2, sin cookies, sin `Accept-Encoding`/descompresión.
- **Criterio**: `cargo test -p net --test http_tests` (15).

### `WP-5.2` — `file://`, MIME sniffing y `ResourceLoader` con caché (**HECHO**)
- **Spec**: WHATWG MIME Sniffing (`https://mimesniff.spec.whatwg.org/`, subset §5); WHATWG Fetch (modelo de `Response`/recursos).
- **Hecho**: `file://` con path percent-decoded, sniffing HTML/imágenes/SVG-XML/CSS/JSON y fallback binario/texto, `ResourceLoader` con caché en memoria por URL (`Resource { url, bytes, content_type }`), errores tipados (`FetchError`/`LoadError`/`UrlError`).
- **Criterio**: `cargo test -p net` (57 tests) + `cargo run -p test_harness -- --summary` (sin cambios, 4 suites PASS).

### `WP-5.3` — TLS con `rustls` (siguiente)
- **Objetivo**: handshake TLS 1.2/1.3, validación de certificados con raíces `webpki-roots`, `https://` funcional y límites de tiempo/tamaño.
- **Spec**: WHATWG Fetch §HTTP-network-or-cache fetch; RFC 8446 (TLS 1.3).
- **Crate**: `crates/net`; dependencias autorizadas por `ARCHITECTURE.md` §3.1 (`rustls`, `webpki-roots`).
- **Criterio**: tests offline con servidor TLS local o fixtures de handshake + `cargo test -p net`.

### `WP-5.4` — Política de caché HTTP y redirecciones/referrer (pendiente)
- **Objetivo**: caché HTTP (`Cache-Control`, `ETag`, `Last-Modified`), política de redirecciones con contador y `Referer`/`Origin`.
- **Spec**: WHATWG Fetch §HTTP fetch y RFC 9111 (HTTP Caching).
- **Criterio**: `cargo test -p net` con servidores de prueba que devuelvan 304/ETag y cadenas de redirects.

### `WP-5.5` — Fetch/CORS y carga de subrecursos del documento (pendiente)
- **Objetivo**: modelo de `Request`/`Response`/`fetch` con modos CORS (simple/preflight), `Referrer-Policy`, y extracción de subrecursos del DOM (`<link rel=stylesheet>`, `<style>` externos, `@import`).
- **Spec**: WHATWG Fetch (`https://fetch.spec.whatwg.org/`), WHATWG HTML §4.2.4 / §15 (recursos), CSSOM.
- **Criterio**: tests de integración dominio cruzado simulado con `TcpListener` y tests de carga de hojas de estilo externas en el pipeline de render.

### `WP-5.6` — Integración en el pipeline de render (pendiente)
- **Objetivo**: que `paint`/un futuro crate de orquestación cargue hojas externas y, más adelante, imágenes; `background-image`/`<img>` con decodificación de imágenes.
- **Spec**: WHATWG HTML §15 + CSS Images 3 + Fetch.
- **Criterio**: fixture HTML con `<link rel=stylesheet>` externo servido por HTTP local → PNG con estilos aplicados.

### `WP-5.7` — Streaming y compresión (pendiente, no bloqueante)
- **Objetivo**: streaming de cuerpos para respuestas grandes, `Content-Encoding` (gzip/br), HTTP/2 (evaluar `h2`/`hyper` bajo §3.1) y límites de memoria.
- **Criterio**: benchmarks de carga y tests de cuerpos grandes/chunked con compresión.

### Fuera del alcance de Fase 5 (inicial)

Cookies y almacenamiento, HTTP/2/3 obligatorio, proxies, WebSockets, IDNA completa, `data:`/`blob:`/`ftp:`, autenticación HTTP, descarga de fuentes web (`@font-face`) y streaming de vídeo. Se reevaluarán en fases posteriores según las necesidades del renderizador (imágenes) y del motor JS.
