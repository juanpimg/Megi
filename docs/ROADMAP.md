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
Fase 1: Tokenizador & Árbol HTML5   [██████████]  99.9% (COMPLETADA - 100% Declarativo)
Fase 2: Motor CSS                    [██████████] 100% (COMPLETADA)
Fase 3: Motor Layout (BFC & IFC)     [██████████] 100% (COMPLETADA)
Fase 4: Paint, PNG & Tipografía      [█████░░░░░]  50% (EN CURSO: pipeline HTML→PNG operativo)
Fase 5: Red & Fetch Pipeline         [░░░░░░░░░░]   0% (PLANIFICADA)
Fase 6: Integración JS & Bindings    [░░░░░░░░░░]   0% (PLANIFICADA)
Fase 7: Ventana OS & Shell           [░░░░░░░░░░]   0% (PLANIFICADA)

> El 50% de la Fase 4 corresponde al pipeline `HTML → CSS → layout → display list → PNG`
> ya operativo (WP-4.1), los cimientos CSS (WP-4.0), las correcciones de BFC/IFC (WP-4.2)
> y el motor tipográfico `crates/fonts` (WP-4.4 fase 1). El detalle HECHO/PARCIAL/PENDIENTE
> por paquete está en la sección «FASE 4» al final de este documento y en
> `docs/PHASE4-SUPPORT.md`.
```

| Fase / Bloque | Barra | Estado | Cobertura Verificada | Hito Clave Alcanzado |
|---|:---:|:---:|---|---|
| **Fase 0: Tooling, Arena DOM & Closed-Loop Harness** | `[██████████] 100%` | **COMPLETO** | 9/9 tests unitarios en `crates/dom` | Arena contigua indexada (`NodeId(NonZeroU32)` de 4 bytes) sin `Rc<RefCell<Node>>`, scripts CLI de inspección WHATWG en vivo. |
| **Fase 1A: Tokenizador HTML5 (84 Estados & 2.231 Entidades)** | `[██████████] 100%` | **COMPLETO** | 7.047 / 7.050 estados superados | 100% verde en las 13 suites estándar de `html5lib-tests/tokenizer`. |
| **Fase 1B: Construcción del Árbol DOM (21 Modos de Inserción)** | `[██████████] 99.9%` | **COMPLETO (Declarativo)** | 3.898 / 3.904 casos superados | 100% verde en 58 de 62 suites (100% del HTML5 declarativo superado; los 6 casos restantes requieren JavaScript dinámico `document.write`). |
| **Fase 2: Motor CSS (Sintaxis L3, Selectores L4, Cascada L4, Hoja UA §15)** | `[██████████] 100%` | **COMPLETO** | 23/23 unit tests + 30/30 comprobaciones WPT CSS Syntax vendorizadas | Tokenizador L3, parser AST, especificidad $(A,B,C)$, pseudoclases, cascada por origen e importancia (6 niveles), `ComputedStyle` y hoja User-Agent WHATWG HTML §15; bases de Fase 4: bordes por lado/shorthands, `border-style: none` ⇒ ancho 0, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type` y correcciones de selectores. |
| **Fase 3: Motor de Layout (Box Tree, Bloques Anónimos §9.2.1.1, BFC e IFC)** | `[██████████] 100%` | **COMPLETO (núcleo)** | 61 tests en `crates/layout` | Box Tree con supresión de `display: none`, cajas anónimas *inherit-only* (§9.2.1.1), BFC con **colapso de márgenes** (§8.3.1), altura del bloque contenedor inicial, `box-sizing`/`min-max`/porcentajes, sobre-restringido e IFC con `<br>` forzado. El layout de posicionados, floats, tablas e inline-block se planifica en Fase 4. |
| **Fase 4: Display Lists, Pipeline HTML → PNG & Rasterización 2D** | `[█████░░░░░] 55%` | *EN CURSO* | 33 tests en `crates/paint` | Display list desacoplada, rasterizador software con blending, **pipeline HTML → PNG de extremo a extremo** (codificador PNG propio sin dependencias), canvas blanco con propagación de fondo raíz/`body` (CSS 2.2 §14.2), `RenderConfig`, extracción de `<style>` y golden determinista 800×600. Pendientes: integración de fuentes reales, bordes por lado en el raster, stacking contexts/`opacity`/`position`, clip, antialiasing y compresión PNG. |
| **Fase 4: Motor tipográfico (`crates/fonts`)** | `[█████░░░░░] 50%` | *EN CURSO (fase 1)* | 13 tests + 1 doctest | `FontDatabase` con `fontdb`, métricas OpenType vía `ttf-parser`, shaping LTR con `rustybuzz` (kern/ligaduras), rasterización de glifos con `ab_glyph` y fallback de familias; fuente DejaVu determinista empaquetada con licencia. Aún no consumido por `layout`/`paint` (WP-4.4 fase 2). |
| **Fase 5: Red & Fetch Pipeline** | `[░░░░░░░░░░] 0%` | *Planificada* | — | Transporte HTTP/TLS y pipeline de recursos según WHATWG Fetch. |
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

## FASE 4: Display Lists, Pipeline HTML → PNG y Tipografía Real (EN CURSO)

**Objetivo de la fase**: renderizar una página HTML real (autocontenida, sin red ni JavaScript) a una imagen PNG, y para ello completar el modelo de pintado CSS 2.2 (§9.9/Apéndice E, §10, §14.2) y la tipografía. Estado global estimado: **~50%**. El pipeline `HTML → CSS → layout → display list → PNG` ya es ejecutable y está cubierto por tests + golden determinista; lo que falta es fidelidad (fuentes reales integradas, bordes por lado en el raster, posicionamiento/stacking, floats, tablas, `inline-block`, overflow y calidad de antialiasing).

Matriz detallada de soporte CSS 2.2 y decisiones de alcance: [`docs/PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md).

**Estado de los paquetes**:

| WP | Objetivo | Crate | Estado |
|---|---|---|---|
| `WP-4.0` | Cimientos CSS: bordes por lado, `border-style: none`, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type`, `line-height` | `css` | **PARCIAL** |
| `WP-4.1` | Pipeline `HTML→PNG`, codificador PNG sin dependencias, canvas y propagación raíz/`body`, extracción de `<style>` | `paint` | **HECHO** |
| `WP-4.2` | BFC y modelo de cajas: anónimos *inherit-only*, colapso de márgenes, ICB, `box-sizing`/min-max/%, sobre-restringido, `<br>` | `layout` | **PARCIAL** |
| `WP-4.3` | Pintado de caja: fondos en *border box* y bordes por lado/estilo/color en la display list | `paint` | **PARCIAL** |
| `WP-4.4` | Tipografía real: `crates/fonts` + integración en layout (métricas/shaping/glyph runs) | `fonts`, `layout`, `paint` | **PARCIAL** (fase 1 hecha) |
| `WP-4.5` | IFC conforme: oportunidades de salto UAX#14, soft hyphen, `vertical-align` §10.8.1 | `layout` | **PARCIAL** |
| `WP-4.6` | `position: relative/absolute/fixed`, `z-index`, stacking contexts y `opacity` por grupos | `layout`, `paint` | **PENDIENTE** |
| `WP-4.7` | Floats y `clear` (CSS 2.2 §9.5) y su paso de pintado | `layout`, `paint` | **PENDIENTE** |
| `WP-4.8` | Tablas CSS 2.1 (layout automático, `colspan`/`rowspan`, `border-collapse`) | `layout`, `paint` | **PENDIENTE** |
| `WP-4.9` | `inline-block`, marcadores `list-item`, `hr`, `text-decoration` y placeholders de reemplazados | `css`, `layout`, `paint` | **PENDIENTE** |
| `WP-4.10` | `overflow` y clipping de descendientes (`PushClip`/`PopClip`) | `css`, `layout`, `paint` | **PENDIENTE** |
| `WP-4.11` | Calidad de raster: cobertura por píxel/antialiasing, alfa premultiplicado, compresión PNG | `paint` | **PARCIAL** |
| `WP-4.12` | Robustez/rendimiento: límites anti-DoS, API con errores explícitos, CPU cuadrática hallada en auditoría | `html`, `dom`, `layout`, `paint` | **PARCIAL** |
| `WP-4.13` | Golden images, fixtures `real_page`/`torture`, matriz de soporte y documentación en verdad | `paint`, `docs` | **PARCIAL** |

### `WP-4.0` — Cimientos CSS de Fase 4 (estado: **PARCIAL**)
- **Duración estimada**: 3 días
- **Objetivo**: que `ComputedStyle` contenga la información que layout/paint necesitan y que el cálculo de bordes sea normativo.
- **Sección exacta de la Spec W3C**:
  - CSS 2.2 §8.5.3 *Border style* (`https://www.w3.org/TR/CSS22/box.html#border-style-properties`, `none` ⇒ ancho computado 0)
  - CSS 2.2 §8.5.1/§8.5.2/§8.5.4 *Border width/color/shorthand* (`#border-width-properties`, `#border-color-properties`, `#border-shorthand-properties`)
  - CSS 2.2 §9.5.1/§9.5.2 *float/clear* (`https://www.w3.org/TR/CSS22/visuren.html#floats`), §11.1.1 *overflow* (`#overflow`), §11.2 *visibility* (`#visibility`), §16.3.1 *text-decoration* (`https://www.w3.org/TR/CSS22/text.html#decoration`), CSS Lists 3 §5 *list-style-type*
- **Crate y módulos**: `crates/css/src/properties.rs`, `crates/css/src/selector.rs`, `crates/css/src/user_agent.rs`
- **Hecho en este sprint**: bordes por lado y shorthands (`border-top/right/bottom/left`, `*-color`/`*-style`), expansión `{1,4}` de `border-width/color/style`, `border-style: none|hidden` ⇒ ancho computado 0, campos `float`, `clear`, `overflow`, `visibility`, `text-decoration`, `list-style-type` en `ComputedStyle`, y correcciones de selectores (pseudo-clase desconocida invalida, SVG/MathML, comas en atributos).
- **Pendiente**: semántica de herencia de `line-height` con `<number>`/`normal` (hoy se congela en px; ver hallazgo del informe de auditoría), `vertical-align` porcentual.
- **Criterio de aceptación**: `cargo test -p css` (23/23) + `cargo run -p test_harness -- --summary` (30/30 CSS Syntax).

### `WP-4.1` — Pipeline HTML → PNG (estado: **HECHO**)
- **Duración estimada**: 3 días
- **Objetivo**: driver reproducible `HTML → CSS → layout → display list → Pixmap → PNG`, con canvas blanco, propagación de fondo raíz/`body` (CSS 2.2 §14.2) y extracción de `<style>`.
- **Sección exacta de la Spec**:
  - CSS 2.2 §14.2 *The background* (`https://www.w3.org/TR/CSS22/colors.html#background`) y CSS Backgrounds 3 §2.11 *root/body backgrounds* (`https://www.w3.org/TR/css-backgrounds-3/#root-background`, `#body-background`)
  - CSS 2.2 Apéndice E §E.2 paso 1 (fondo del elemento raíz sobre el canvas, `https://www.w3.org/TR/CSS22/zindex.html#painting-order`)
  - PNG 3 (`https://www.w3.org/TR/png-3/`), zlib RFC 1950, DEFLATE RFC 1951
- **Crate y módulos**: `crates/paint/src/png.rs` (nuevo), `crates/paint/src/style.rs` (nuevo), `crates/paint/src/painter.rs`, `crates/paint/src/rasterizer.rs` (`RenderConfig`), `crates/paint/examples/html_to_png.rs` (nuevo), `crates/paint/tests/end_to_end_html.rs` y `png_format.rs` (nuevos), `tests/fixtures/phase4/`
- **Hecho en este sprint**: codificador PNG propio sin dependencias (DEFLATE *stored*, CRC-32/Adler-32, tamaño ~1,92 MB para 800×600), `Pixmap::to_png`, `RenderConfig { background, transparent }`, propagación de fondo raíz/`body` con supresión del pintado duplicado, canvas opaco blanco (0 % de transparencia), extracción de `<style>` (`collect_style_text`/`resolve_embedded_styles`), ejemplo CLI y golden `basic.rgba`; 12 tests end-to-end + 8 de formato PNG.
- **Criterio de aceptación**:
  - `cargo run -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/basic.png`
  - `python3 tools/png_check.py /tmp/basic.png` → `OK`
  - `cargo test -p paint --test end_to_end_html --test png_format`
- **Limitación conocida**: PNG sin compresión (el tamaño es aceptable para tests pero se optimizará en WP-4.11); glifos 8×8 *stub* hasta integrar `crates/fonts`.

### `WP-4.2` — BFC y modelo de cajas (estado: **PARCIAL**)
- **Duración estimada**: 5 días
- **Objetivo**: cajas anónimas correctas, colapso de márgenes, bloque contenedor inicial, `box-sizing`/min-max/alturas porcentuales, sobre-restringido y `<br>`.
- **Sección exacta de la Spec**:
  - CSS 2.2 §9.2.1.1 *Anonymous block boxes* (`https://www.w3.org/TR/CSS22/visuren.html#anonymous-block-level`)
  - CSS 2.2 §8.3.1 *Collapsing margins* (`https://www.w3.org/TR/CSS22/box.html#collapsing-margins`)
  - CSS 2.2 §10.1 *Containing block*, §10.3.3 *block width* (`#blockwidth`), §10.4/§10.7 *min/max*, §10.6.3/§10.6.7 *auto heights*
  - CSS Sizing 3 §box-sizing (`https://www.w3.org/TR/css-sizing-3/#box-sizing`)
- **Crate y módulos**: `crates/layout/src/tree_builder.rs`, `block.rs`, `lib.rs`, `box_tree.rs`; `crates/layout/tests/bfc_conformance.rs` (nuevo); `crates/paint` adaptado a la nueva API
- **Hecho en este sprint**: estilos anónimos *inherit-only* (fin de márgenes/bordes/fondos fantasma, incluidos los huecos por whitespace entre bloques), `<br>` como salto forzado, colapso de márgenes padre-hijo/hermanos con márgenes negativos, ICB con `viewport.height`, alturas porcentuales, `box-sizing: border-box`, `min/max-width`/`min/max-height` y anchos sobre-restringidos; 18 tests nuevos de conformidad BFC.
- **Pendiente**: bloques de altura cero con márgenes superior+inferior (*self-collapsing margins*, CSS 2.2 §8.3.1: cuando el margen superior e inferior de una caja se colapsan entre sí), API de layout con errores explícitos (`Result`) en lugar de `Option`.
- **Criterio de aceptación**: `cargo test -p layout` (61 tests).

### `WP-4.3` — Pintado de caja: fondos y bordes (estado: **PARCIAL**)
- **Duración estimada**: 3 días
- **Objetivo**: display list y rasterizador capaces de pintar el modelo de caja completo: bordes por lado con estilo/color y fondo en el *border box*.
- **Sección exacta de la Spec**: CSS 2.2 §8.5 (`https://www.w3.org/TR/CSS22/box.html#border-properties`), CSS 2.2 §14.2 (`https://www.w3.org/TR/CSS22/colors.html#background`), CSS Backgrounds 3 §2.7 `background-clip` (`https://www.w3.org/TR/css-backgrounds-3/#background-clip`), CSS 2.2 Ap. E pasos 2/4 (`https://www.w3.org/TR/CSS22/zindex.html#painting-order`)
- **Crate y módulos**: `crates/paint/src/display_list.rs`, `painter.rs`, `rasterizer.rs`; `crates/paint/tests/box_model_paint.rs` (pendiente)
- **hecho en el sprint (parcial)**: el CSS ya entrega `border_color[4]`/`border_style[4]`; el painter pinta fondos en *border box* cuando aplica, omite cajas de formato (`LineBox`/`TextNode`/`AnonymousBlock`) y no pinta `border-style: none`; el avance del texto se toma de la anchura medida por layout.
- **Pendiente**: `DisplayItem::Border` con **cuatro** colores/estilos y esquinas a inglete, `Dashed/Dotted/Double/Groove/Ridge/Inset/Outset`, fondo con `background-clip` configurable; tests de píxeles por lado.
- **Criterio de aceptación**: `cargo test -p paint` + golden de una caja con cuatro bordes de colores distintos.

### `WP-4.4` — Tipografía real e integración (estado: **PARCIAL**, fase 1 hecha)
- **Duración estimada**: 7 días (3 de crate + 4 de integración)
- **Objetivo**: sustituir la métrica falsa (`DefaultTextMeasurer`) y los glifos 8×8 por fuentes reales, métricas OpenType, shaping y rasterización de glifos con la misma verdad en layout y paint.
- **Sección exacta de la Spec**: CSS Fonts 4 §5 *font matching* (`https://www.w3.org/TR/css-fonts-4/#font-matching-algorithm`), CSS 2.2 §15.3 (`https://www.w3.org/TR/CSS22/fonts.html#font-family-prop`) y §10.8.1 (`https://www.w3.org/TR/CSS22/visudet.html#leading`), OpenType `OS/2`/`hhea`; HTML §15.3 (fuentes UA)
- **Crate y módulos**: `crates/fonts/` (**nuevo crate**), `crates/layout/src/inline.rs`, `lib.rs`, `box_tree.rs`, `crates/paint/src/{display_list,painter,rasterizer}.rs`
- **Hecho en este sprint (fase 1)**: crate `fonts` con `FontDatabase` (`fontdb`, carga determinista de bytes), `FontMetrics` (`ttf-parser`), `measure_text`, `shape` con `rustybuzz` (LTR, kern/ligaduras, fallback por glifo) y `rasterize_glyph` con `ab_glyph`; fuente DejaVu Sans/Bold empaquetada con licencia; 13 tests + 1 doctest + ejemplo `report`.
- **Pendiente (fase 2)**: `TextMeasurer`/`FontSpec` con peso/estilo en layout, `ShapedRun`/glyph runs en `LayoutBox`, `line-height: normal` desde métricas reales, baseline almacenado en la line box, rasterizado de máscaras de glifo en paint y goldens con fuente real.
- **Criterio de aceptación**: `cargo test -p fonts` (13+1) + tests de integración layout↔paint con fuente determinista.

### `WP-4.5` — IFC conforme (estado: **PARCIAL**)
- **Duración estimada**: 3 días
- **Objetivo**: saltos de línea correctos según oportunidades UAX #14, soft hyphens invisibles, `<br>` en todos los `white-space`, y `vertical-align` §10.8.1 (incluido *aligned subtree* `top`/`bottom`).
- **Sección exacta de la Spec**: CSS 2.2 §9.4.2 *Inline formatting contexts* (`https://www.w3.org/TR/CSS22/visuren.html#inline-formatting`), §10.8/§10.8.1 (`https://www.w3.org/TR/CSS22/visudet.html#line-height`, `#leading`), CSS Text 3 §line breaking/white-space/hyphenation (`https://www.w3.org/TR/css-text-3/#line-breaking`, `#white-space-property`, `#hyphenation`), UAX #14 (`https://www.unicode.org/reports/tr14/`)
- **Crate y módulos**: `crates/layout/src/inline.rs`, `tree_builder.rs`; `crates/layout/tests/ifc_conformance.rs` (pendiente)
- **Hecho en este sprint**: `<br>` como `ForcedBreak` (también dentro de inline elements) y `&nbsp;` no colapsable.
- **Pendiente**: no romper palabras entre nodos inline adyacentes sin espacio (hallazgo F-09 del informe de auditoría), soft hyphen de ancho cero que solo se convierte en guion al romper, `vertical-align: top/bottom` conforme al *aligned subtree* y unificación de las dos pasadas de métricas, `line-height` con semántica de número por elemento, `text-align: justify`.
- **Criterio de aceptación**: `cargo test -p layout` con los tests de `ifc_conformance` + golden de párrafo con estilos inline.

### `WP-4.6` — Posicionamiento y contextos de apilamiento (estado: **PENDIENTE**)
- **Duración estimada**: 6 días
- **Objetivo**: `position: relative/absolute/fixed`, contenedores de posicionados, posición estática, `z-index`, árbol de *stacking contexts* y `opacity` como grupo compuesto.
- **Sección exacta de la Spec**: CSS 2.2 §9.3/§9.4.3/§9.6 *position schemes* (`https://www.w3.org/TR/CSS22/visuren.html#positioning-scheme`, `#relative-positioning`, `#absolute-positioning`), §9.9/§9.9.1 `z-index` (`#z-index`, `#layers`), §10.1 *containing block* (`https://www.w3.org/TR/CSS22/visudet.html#containing-block-details`), §10.3.7/§10.6.4 (tamaños de posicionados), Apéndice E (`https://www.w3.org/TR/CSS22/zindex.html#painting-order`), CSS Color 3 §transparency (`https://www.w3.org/TR/css-color-3/#transparency`) y Compositing 1 §groups (`https://www.w3.org/TR/compositing-1/#groups`)
- **Crate y módulos**: `crates/layout/src/positioned.rs` (nuevo), `block.rs`, `box_tree.rs`, `lib.rs`; `crates/paint/src/stacking.rs` (nuevo), `painter.rs`, `display_list.rs`, `rasterizer.rs`; tests `layout/tests/positioning.rs` y `paint/tests/stacking_order.rs`
- **Estado**: propiedades calculadas en CSS pero **no leídas** por layout/paint; hoy un `position: absolute` se pinta en el flujo normal. Es el bloqueador principal de fidelidad en páginas reales.
- **Criterio de aceptación**: `cargo test -p layout -p paint` con `test_absolute_removed_from_normal_flow`, `test_z_index_ordering` y `test_opacity_group_composited_once`; PNG de fixture con badge absoluto.

### `WP-4.7` — Floats y `clear` (estado: **PENDIENTE**)
- **Duración estimada**: 3 días
- **Objetivo**: colocación de flotantes (§9.5.1), acortado de line boxes alrededor de floats (§9.4.2), `clear` (§9.5.2), altura de BFC con flotantes (§10.6.7) y paso de pintado 5 del Apéndice E.
- **Sección exacta de la Spec**: CSS 2.2 §9.5 *Floats* (`https://www.w3.org/TR/CSS22/visuren.html#floats`, `#float-rules`, `#flow-control`)
- **Crate y módulos**: `crates/layout/src/float.rs` (nuevo), `block.rs`, `inline.rs`; `crates/paint/src/painter.rs` (orden de pintado)
- **Estado**: `float`/`clear` ya se parsean a `ComputedStyle` (WP-4.0), pero layout y paint no los aplican.
- **Criterio de aceptación**: `cargo test -p layout -p paint` con `test_line_box_shortened_beside_float`, `test_clear_below_float`, `test_float_painted_after_block_backgrounds_before_inline_text`.

### `WP-4.8` — Tablas CSS 2.1 (estado: **PENDIENTE**)
- **Duración estimada**: 5 días
- **Objetivo**: estructura de cajas de tabla, layout automático de columnas/filas, `colspan`/`rowspan`, `border-collapse` y pintado de tablas según el Apéndice E.
- **Sección exacta de la Spec**: CSS 2.2 §17 *Tables* (`https://www.w3.org/TR/CSS22/tables.html`), CSS Tables 3 §table-layout (`https://www.w3.org/TR/css-tables-3/#table-layout-algorithm`)
- **Crate y módulos**: `crates/layout/src/table.rs` (nuevo), `tree_builder.rs`, `box_tree.rs`; `crates/paint/src/painter.rs`; `crates/layout/tests/table_layout.rs`
- **Estado**: `display: table/table-row/table-cell` se mapea hoy a bloques apilados; no hay algoritmo de tablas.
- **Criterio de aceptación**: `cargo test -p layout -p paint` con tabla de 3×3 con bordes y celdas de distinto contenido; golden de fixture `real_page.html` cuando exista.

### `WP-4.9` — `inline-block`, listas, decoraciones y reemplazados (estado: **PENDIENTE**)
- **Duración estimada**: 4 días
- **Objetivo**: `display: inline-block` con *shrink-to-fit* y baseline propia; marcadores `list-item`; `hr` con estilo UA; `text-decoration`; placeholders de elementos reemplazados (`<img>` con tamaño intrínseco de atributos).
- **Sección exacta de la Spec**: CSS 2.2 §10.3.9 (`https://www.w3.org/TR/CSS22/visudet.html#inlineblock-width`), §10.8.1 *baseline de inline-block*, §10.3.2 *inline replaced width*, §12.5 *lists* (`https://www.w3.org/TR/CSS22/generate.html`), CSS Text Decoration 3 (`https://www.w3.org/TR/css-text-decor-3/#line-decoration`), CSS Lists 3, HTML §15.3 (UA)
- **Crate y módulos**: `crates/css/src/user_agent.rs`, `crates/layout/src/{inline,block,tree_builder,box_tree}.rs`, `crates/paint/src/{display_list,rasterizer,painter}.rs`
- **Estado**: `list-style-type`/`text-decoration` se parsean (WP-4.0) pero no se pintan; `inline-block` se mapea a inline normal; `<img>` genera una caja vacía invisible (las imágenes reales dependen de `crates/net`, Fase 5).
- **Criterio de aceptación**: `cargo test -p layout -p paint` con `test_inline_block_shrink_to_fit_width`, `test_list_item_disc_marker_outside`, `test_anchor_underline_rendered`.

### `WP-4.10` — Overflow y clipping (estado: **PENDIENTE**)
- **Duración estimada**: 2 días
- **Objetivo**: `overflow: visible|hidden|clip|scroll|auto` y clip de descendientes al *padding box*, emitiendo `PushClip`/`PopClip` correctamente (sin clipear fondo/borde propios).
- **Sección exacta de la Spec**: CSS 2.2 §11.1 *Overflow and clipping* (`https://www.w3.org/TR/CSS22/visufx.html#overflow`, `#overflow-clipping`), CSS Overflow 3
- **Crate y módulos**: `crates/paint/src/{painter,display_list,rasterizer}.rs`, `crates/layout/src/box_tree.rs`; `crates/paint/tests/clipping.rs`
- **Estado**: `overflow` ya se parsea (WP-4.0); la infraestructura de clip existe en el rasterizador pero el painter no emite clips.
- **Criterio de aceptación**: `cargo test -p paint` con `test_overflow_hidden_clips_descendant`, `test_overflow_hidden_does_not_clip_own_background`.

### `WP-4.11` — Calidad de raster y alfa (estado: **PARCIAL**)
- **Duración estimada**: 3 días
- **Objetivo**: cobertura por píxel/antialiasing de rectángulos y glifos, representación premultiplicada, sin sobreárea por redondeo y compresión PNG.
- **Sección exacta de la Spec**: Compositing 1 §simple alpha compositing y §groups (`https://www.w3.org/TR/compositing-1/#simplealphacompositing`, `#groupcompositing`); la política de muestreo/AA es decisión del UA (documentada en `docs/PHASE4-SUPPORT.md`)
- **Crate y módulos**: `crates/paint/src/rasterizer.rs`, `display_list.rs`; `crates/paint/tests/rasterizer_quality.rs`
- **Estado**: el codificador PNG (WP-4.1) y el blending correcto ya existen; `fill_rect` sigue redondeando hacia fuera (sin AA) y el PNG no comprime.
- **Criterio de aceptación**: `cargo test -p paint` con `test_fractional_rect_area_matches_coverage`, `test_half_pixel_edge_antialiased`, `test_premultiplied_roundtrip`; PNG comprimido (o decisión documentada).

### `WP-4.12` — Robustez y rendimiento (estado: **PARCIAL**)
- **Duración estimada**: 2 días
- **Objetivo**: que HTML no confiable no aborte el proceso ni consuma CPU desmedida; errores diagnosticables.
- **Sección exacta de la Spec**: no normativa (endurecimiento); referencia de límites de implementación de WHATWG HTML
- **Crate y módulos**: `crates/html/src/tree_builder.rs`, `crates/html/src/serializer.rs`, `crates/dom/src/lib.rs`, `crates/layout/src/*`, `crates/paint/src/*`
- **Hecho en este sprint**: parsear 100k/200k `<div>` anidados ya no aborta (límite de profundidad 4.096 reportado en `LimitViolations`; ~70 ms), 40k atributos bajan de ~1.276 ms a ~10 ms, serializador iterativo y DOM con `try_node`/`clone_node` de `<template>`; tests de robustez en `crates/html/tests/robustness.rs`.
- **Pendiente**: **CPU cuadrática del tree builder** (`has_element_in_scope`, `push_active_formatting`, `reconstruct_active_formatting_elements`) con entradas adversariales, presupuesto de tiempo de parseo/layout/paint y límite de profundidad en layout/paint.
- **Criterio de aceptación**: `cargo test -p html -p dom -p layout -p paint` con los tests de robustez y un presupuesto de tiempo documentado.

### `WP-4.13` — Golden images y documentación en verdad (estado: **PARCIAL**)
- **Duración estimada**: 2 días
- **Objetivo**: red de seguridad visual (golden deterministas) y documentación que refleje el estado real.
- **Sección exacta de la Spec**: no normativa; validación PNG según PNG 3 (`https://www.w3.org/TR/png-3/`)
- **Crate y ficheros**: `tests/fixtures/phase4/` (`basic.html`, `basic.rgba`, `README.md`), `crates/paint/tests/end_to_end_html.rs`, `tools/png_check.py`, `docs/PHASE4-SUPPORT.md`, `README.md`, `docs/*.md`
- **Hecho en este sprint**: golden determinista de `basic.html` comparado byte a byte, validador `tools/png_check.py`, README/ROADMAP/ARCHITECTURE/HANDOFF/CONTRIBUTING/TOOLING sincronizados y matriz de soporte de Fase 4 creada.
- **Pendiente**: fixtures `real_page.html` y `torture.html`, goldens cuando existan posicionamiento/tablas/fuentes reales, y comparación con tolerancia para antialiasing.
- **Criterio de aceptación**: `cargo test -p paint --test end_to_end_html` + `python3 tools/png_check.py`.

### Orden de ejecución y camino crítico de Fase 4

```text
WP-4.1 (PNG, HECHO) ─┐
WP-4.0 (cimientos CSS) ─────┼─► WP-4.2 (HECHO mayormente) ─► WP-4.4 fase 2 (PENDIENTE)
                     │                                  │
                     └─► WP-4.3 (PARCIAL) ──────────────┼─► WP-4.5 ─► WP-4.6 ─► WP-4.10 ─► WP-4.11 ─► WP-4.13
                                                        └─► WP-4.7 / WP-4.8 / WP-4.9 (paralelizables)
WP-4.12 (robustez) ∥ en cualquier momento
```

Prioridad recomendada para la próxima iteración: **integrar `crates/fonts` en layout/paint (WP-4.4 fase 2)** → **pintado de bordes por lado/fondos (WP-4.3)** → **IFC pendiente (WP-4.5)** → **posición/stacking (WP-4.6)**. El resto (floats, tablas, inline-block, overflow, AA) puede abordarse en paralelo según la matriz de soporte.

### Fuera del alcance de Fase 4

Red/`<img>` real, JavaScript, formularios interactivos, `flex`/`grid`, `@media`, `@font-face`, bidi y scripts complejos, scroll/hit-testing/eventos, animaciones, `border-radius`/`box-shadow`/`background-image`/gradientes y medios paginados. Ver la lista completa y su justificación en [`docs/PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md).
