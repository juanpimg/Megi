# Arquitectura Maestra del Motor de Navegación Web Megi (`docs/ARCHITECTURE.md`)

**Megi** es un motor de navegación web moderno, open source e independiente desarrollado desde cero en **Rust** (Edition 2024), diseñado sin bifurcar ni envolver motores preexistentes (Blink, WebKit, Gecko, ni los crates de parsing/layout de Servo o Blitz). Todo el diseño e implementación de Megi se rige estrictamente por las especificaciones formales vivas de **WHATWG** (HTML Living Standard, DOM Living Standard, Encoding, URL, Fetch) y del **W3C** (CSS Syntax Module Level 3, Selectors Level 4, CSS Box Model / Cascade), verificado continuamente en bucle cerrado contra **`html5lib-tests`** y **Web Platform Tests (WPT)**. La **Fase 4** añade el pipeline de renderizado `HTML → CSS → layout → display list → PNG` (`crates/paint`) y el motor tipográfico real (`crates/fonts`), documentado en [`PHASE4-SUPPORT.md`](PHASE4-SUPPORT.md).

---

## 1. Modelo de Crates y Flujo Acíclico de Dependencias (DAG)

El repositorio se estructura como un **Cargo Workspace modular** (`resolver = "2"`, `edition = "2024"`) donde cada subsistema del motor reside en un crate aislado dentro de `crates/`. Se prohíben terminantemente las dependencias circulares: el grafo de dependencias entre crates forma un **Grafo Acíclico Dirigido (DAG)** estricto en el que **`crates/dom`** actúa como el núcleo de datos libre de dependencias externas.

### 1.1 Diagrama del Grafo de Dependencias del Workspace

Fases: `dom` (F0), `html` (F1), `css` (F2), `layout` (F3), `paint`/`fonts` (F4), `net` (F5), `js` (F6), `shell` (F7).

```text
crates/dom  [F0 — 0 dependencias externas]
├──► crates/html         [F1]  WHATWG HTML §13.2–§13.5: InputStream, tokenizer de 84 estados,
│                              tree builder de 21 modos, serializer y ParseOptions anti-DoS.
├──► crates/css          [F2]  CSS Syntax L3, Selectors L4, cascada y ComputedStyle,
│                              hoja User-Agent WHATWG HTML §15.
├──► crates/test_harness [F0–F1] runner en bucle cerrado de html5lib-tests +
│                              fixtures WPT de CSS Syntax vendorizadas + gate `--summary`.
└──► crates/layout       [F3]  Box Tree, BFC (CSS 2.2 §8–§10), IFC y colapso de márgenes.
     └──► crates/paint   [F4]  Display List (CSS 2.2 §9.9/Apéndice E), rasterizador 2D
                              software, canvas y propagación raíz/`body` (§14.2) y
                              codificador PNG propio.

crates/fonts  [F4]  Motor tipográfico real: fontdb + ttf-parser + rustybuzz + ab_glyph.
                    Sin dependencias de otros crates internos; `layout` y `paint` la
                    consumirán en la integración de WP-4.4 fase 2 (hoy aún no integrada).

crates/net    [F5]  rustls / webpki-roots / mio o tokio / httparse — planificado.
crates/js     [F6]  Motor JavaScript y DOM bindings — planificado.
crates/shell  [F7]  winit / softbuffer (ventana, eventos y bucle principal) — planificado.
```

El grafo es estrictamente acíclico. `crates/dom` es el núcleo de datos sin dependencias externas; los crates de parsing/layout (`dom`, `html`, `css`, `layout`) se implementan sólo con `std`. Las únicas dependencias externas de bajo nivel autorizadas están en la matriz de §3.1 (`serde` en el arnés de test, `serde_json`, y las librerías tipográficas de `crates/fonts`).

### 1.2 Responsabilidades y Contratos por Crate

| Crate | Ruta | Dependencias Internas | Dependencias Externas | Responsabilidad Arquitectónica |
|---|---|---|---|---|
| **`dom`** | `crates/dom` | Ninguna | Ninguna (`std` puro) | Propiedad única de la arena contigua de nodos (`Document::nodes: Vec<Node>`), identificadores `NodeId(NonZeroU32)`, tipos de nodo, espacios de nombres y `QuirksMode`; mutaciones en tiempo constante `O(1)` con validación de ciclos (`append_child`/`insert_before` devuelven `bool`), `try_node` y `clone_node` correcto para `<template>`. |
| **`html`** | `crates/html` | `dom` | Ninguna (`std` puro) | WHATWG HTML §13.2–§13.5: preprocesador de flujo, 2.231 entidades, tokenizador de 84 estados, constructor de árbol de 21 modos, *Adoption Agency Algorithm*, *foster parenting*, `innerHTML` y serializador **iterativo**. Endurecido contra entrada hostil: `ParseOptions` con límites de profundidad (4.096), nodos (1 M), atributos (4.096) e input (16 MiB), reportados en `LimitViolations`. |
| **`css`** | `crates/css` | `dom` | Ninguna (`std` puro) | Tokenización y parseo CSS Syntax L3, Selectores L4, cascada (origen/importancia/especificidad/orden) y `ComputedStyle` con herencia y hoja User-Agent (WHATWG HTML §15). Incluye las bases de Fase 4: bordes por lado/shorthands, `border-style: none` ⇒ ancho 0, `float`/`clear`/`overflow`/`visibility`/`text-decoration`/`list-style-type` y correcciones de selectores (pseudo-clases desconocidas, SVG/MathML, comas en atributos). |
| **`layout`** | `crates/layout` | `dom`, `css` | Ninguna (`std` puro) | Box Tree, cajas anónimas *inherit-only* (CSS 2.2 §9.2.1.1), BFC con **colapso de márgenes** (§8.3.1), bloque contenedor inicial con altura del viewport, `box-sizing`/`min-max`/alturas porcentuales, sobre-restringido e IFC con `<br>` forzado. Pendiente de Fase 4: posicionados, floats, tablas e `inline-block`. |
| **`fonts`** | `crates/fonts` | Ninguna | `fontdb`, `ttf-parser`, `rustybuzz`, `ab_glyph` (§3.1) | Motor tipográfico real (Fase 4): descubrimiento y fallback de fuentes, `FontMetrics` desde tablas OpenType (`hhea`/`OS/2`), `measure_text`, shaping LTR con kern/ligaduras y rasterización de glifos a máscaras alfa de 8 bits. Expone un contrato documentado para `layout` (baseline/line boxes) y `paint` (máscaras); la integración en ambos es WP-4.4 fase 2. |
| **`paint`** | `crates/paint` | `dom`, `css`, `layout` (`html` sólo como *dev-dependency* del ejemplo/tests) | Ninguna (§3.1) | Display lists desacopladas y rasterizador 2D software (Pixmap RGBA + blending), orden de pintado CSS 2.2, canvas con propagación del fondo raíz/`body` (§14.2), `RenderConfig`, extracción de `<style>` y **codificador PNG propio sin dependencias** (§3.2). Es el crate que materializa el objetivo «HTML real → PNG» de Fase 4. |
| **`test_harness`** | `crates/test_harness` | `dom`, `html`, `css` | `serde`, `serde_json` | Runner en bucle cerrado: 14 suites JSON de tokenizador (6.824 casos), 62 suites `.dat`/`.json` de construcción de árbol (1.973 casos) y las 3 fixtures WPT de CSS Syntax **vendorizadas** (30 comprobaciones offline). `--summary` implementa el gate de conformidad con allowlist documentada (exit 0 con fallos conocidos; `--strict` exit 1). |
| **`net`** | `crates/net` | `dom`, `html` (planificado) | `rustls`, `webpki-roots`, `mio`/`tokio`, `httparse`/`ureq` (planificado) | Fase 5: transporte HTTP/TLS y pipeline de recursos según WHATWG Fetch. |
| **`js`** | `crates/js` | `dom` (planificado) | Motor JS embebido (a decidir) | Fase 6: ejecución de scripts y DOM bindings. |
| **`shell`** | `crates/shell` | `dom`, `html`, `css`, `layout`, `paint` (planificado) | `winit`, `softbuffer` (planificado) | Fase 7: ventana del sistema operativo, eventos y bucle principal de renderizado. |

---

## 2. Estrategia de Gestión de Memoria para el Árbol DOM

Uno de los mayores desafíos al construir un motor web en Rust es conciliar la naturaleza altamente mutante y cíclica del árbol DOM (punteros padre/hijos/hermanos, reparentado masivo durante el *Adoption Agency Algorithm*, *foster parenting* en tablas mal anidadas, fragmentos inertes en `<template>` y futuros enlaces con el motor JavaScript) con el modelo de propiedad y préstamo (*ownership & borrowing*) de Rust.

Durante la **FASE 3.3**, se investigaron en detalle las arquitecturas reales de cuatro proyectos de referencia (**`html5ever` (`markup5ever_rcdom`)**, **Servo**, **Ladybird** y **Blitz**).

### 2.1 Análisis Comparativo de Arquitecturas DOM en Motores Independientes

| Dimensión Técnica | 1. Conteo de Referencias (`Rc<RefCell<Node>>` + `Weak<Node>` en `markup5ever_rcdom`) | 2. Recolector de Basura Trazador Externo (`#[dom_struct]`, `DomRoot<T>`, `LibGC` en Servo y Ladybird) | 3. Arena Contigua Indexada con Enlaces Intrusivos (`Document` + `NodeId(NonZeroU32)` en Megi / `SlotMap` en Blitz) |
|---|---|---|---|
| **Representación del Handle** | `Rc<Node>` (puntero de 8 bytes; clonar muta contadores en heap). | `DomRoot<T>` / `GC::Ref<T>` (requiere registro RAII en pila de raíces del GC). | `NodeId(NonZeroU32)` (`Copy`, 4 bytes exactos; `Option<NodeId>` ocupa 4 bytes gracias a optimización de nicho). |
| **Localidad de Caché y Layout en Memoria** | **Deficiente**: Cada `Node` es una asignación independiente en el heap más un `RefCell<Vec<Handle>>` secundario para sus hijos. Recorrer el DOM produce fallos constantes de caché L1/L2 (*pointer chasing*). | **Moderada**: Asignadores por bloques dentro del heap de SpiderMonkey (Servo) o `LibGC` (Ladybird), pero con sobrecoste de cabeceras de GC por nodo. | **Óptima**: Todos los nodos residen contiguamente en `Document::nodes: Vec<Node>`. Los recorridos en preorden y de hermanos leen líneas de caché contiguas. |
| **Riesgo de Pánicos en Tiempo de Ejecución (`BorrowMutError`)** | **Alto**: Algoritmos como el *Adoption Agency Algorithm* (§13.2.6.4.7) mantienen referencias simultáneas a `formatting_element`, `furthest_block`, `common_ancestor` y sus padres. Mantener un `children.borrow()` abierto mientras se invoca `remove_from_parent` produce pánicos `BorrowMutError` en tiempo de ejecución. | **Medio**: Servo requiere `DomRefCell<T>` para campos mutables internos y un plugin personalizado del compilador (`crown`) para evitar punteros `Dom<T>` no enraizados en la pila. | **Nulo por Construcción**: `NodeId` es un valor entero `Copy` sin tiempo de vida (`'a`) atado a `&Document`. La pila de elementos abiertos (`Vec<NodeId>`) y la lista de formato activo coexisten sin conflictos mientras `&mut Document` ejecuta mutaciones atómicas. |
| **Complejidad de Inserción Antes de Hermano (*Foster Parenting*)** | **`O(grado)` lineal**: En `rcdom/lib.rs`, `get_parent_and_index` realiza una búsqueda lineal `O(N)` con `Rc::ptr_eq` sobre el `Vec<Handle>` del padre y desplaza los elementos posteriores, abriendo vectores de denegación de servicio (`O(N²)`) en tablas con miles de hijos. | **`O(1)`** en Ladybird (`TreeNode.h` usa lista doblemente enlazada intrusiva: `m_first_child`, `m_last_child`, `m_next_sibling`, `m_previous_sibling`). | **`O(1)` estricto**: Cada `Node` almacena 5 enlaces `Option<NodeId>` (20 bytes en total), permitiendo `append_child`, `insert_before` y `detach` en tiempo constante `O(1)`. |
| **Fugas por Ciclos y Desbordamiento de Pila en `Drop`** | **Frágil**: Cualquier ciclo fuerte accidental fuga memoria permanentemente, y destruir un árbol profundo (>10.000 niveles) desborda la pila de llamadas salvo que se implemente un `Drop` iterativo manual. | **Resuelto por GC**, a costa de pausas de recolección y trazado transversal C++/Rust. | **Garantizado por Construcción**: Al destruir `Document`, se libera el único `Vec<Node>` de forma lineal sin recursión ni posibilidad de fugas por ciclos entre nodos. |
| **Acoplamiento con Futuros Wrappers JS (Fase 6)** | Imposible de escalar a ciclos cruzados DOM ↔ JS sin reescribir el DOM. | Fuertemente acoplado desde el día 1 en Servo (`JS-Servos-only-GC.md`); desacoplado recientemente en Ladybird (PR #9973, agosto 2026). | **Desacoplado (Inspirado en Ladybird PR #9973)**: Los nodos DOM no cargan estado de objetos JS hasta que un script los toca; una tabla lateral asociará `NodeId <-> JSWrapper` por mundo de ejecución. |

### 2.2 Diseño Detallado de la Arena DOM de Megi (`crates/dom`)

En `crates/dom/src/lib.rs`, cada documento es dueño absoluto de su arena de nodos:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(NonZeroU32); // 4 bytes; Option<NodeId> == 4 bytes

pub struct Node {
    pub id: NodeId,                  // 4 bytes
    pub parent: Option<NodeId>,      // 4 bytes
    pub first_child: Option<NodeId>, // 4 bytes
    pub last_child: Option<NodeId>,  // 4 bytes
    pub prev_sibling: Option<NodeId>,// 4 bytes
    pub next_sibling: Option<NodeId>,// 4 bytes (Total enlaces árbol: 20 bytes)
    pub kind: NodeKind,              // Payload discriminado
}
```

**Invariantes clave de la arena**:
1. **Índice 1-based con optimización de nicho**: `NodeId` envuelve `NonZeroU32` almacenando `vec_index + 1`. Esto garantiza en tiempo de compilación que `size_of::<NodeId>() == 4` y `size_of::<Option<NodeId>>() == 4` (verificado por el test unitario `test_node_id_niche_optimization_is_4_bytes`).
2. **Nodos `<template>` en la misma Arena**: Cuando `Document::create_element` crea un `<template>` en el espacio de nombres HTML, aloja automáticamente un nodo `NodeKind::DocumentFragment` en la misma arena `Document` y guarda su `NodeId` en `ElementData::template_contents`. Así, todo el contenido inerte de las plantillas comparte la misma arena contigua y las mismas primitivas `O(1)` sin requerir asignadores externos.
3. **Coalescencia automática de nodos `Text` en `O(1)`**: Tanto `Document::append_text(parent, text)` como `Document::insert_text_before(reference_sibling, text)` inspeccionan en `O(1)` el nodo inmediatamente anterior (`parent.last_child` o `reference_sibling.prev_sibling`). Si ya es un `NodeKind::Text`, concatenan el texto in-place cumpliendo exactamente con WHATWG HTML §13.2.6.1 (*insert a character*).

---

## 3. Política de Dependencias Externas de Bajo Nivel Autorizadas

En cumplimiento estricto de la directiva operativa del proyecto:
- **Queda terminantemente prohibido** delegar la lógica nuclear del navegador (tokenización HTML, construcción del árbol DOM, tokenización/cascada CSS, cálculo de layout de cajas o construcción del árbol de renderizado) a librerías de motores existentes como `html5ever`, `markup5ever`, `lol_html`, `scraper`, `cssparser`, `selectors`, `stylo`, `taffy` o `blitz`.
- Solo se autorizan **dependencias externas de bajo nivel** para transporte de red/criptografía, acceso a ventanas/buffers gráficos del sistema operativo, lectura de tablas tipográficas OpenType y serialización de fixtures de test. La **codificación de imagen** (PNG) y el resto de la lógica de pintado se implementan desde cero con `std` (ver §3.2).

### 3.1 Matriz de Dependencias Autorizadas por Subsistema

| Capa / Subsistema | Crates Autorizados | Alcance Estricto Permitido | Responsabilidad que Megi Implementa Desde Cero |
|---|---|---|---|
| **0. Núcleo DOM, HTML y CSS (`dom`, `html`, `css`)** | **Ninguno** (`std` puro en Fase 0–3) | Estructuras de datos estándar de Rust. | Arena DOM, Preprocesador de flujo, Tokenizador HTML de 84 estados, Constructor de Árbol de 21 modos, Tokenizador/Parser CSS y Selectores. |
| **1. Arnés de Pruebas (`test_harness`)** | `serde`, `serde_json` | Deserialización de fixtures JSON (`tokenizer/*.test` y `tree-construction-json/*.json`). | Lógica de desescapado `doubleEscaped` con soporte de sustitutos UTF-16 (`Vec<u32>`), parser de máquina de estados `.dat` (`parseDat`), coalescencia de tokens, comparador diferencial y gate de conformidad con allowlist. |
| **2. Red y Transporte (`crates/net` — Fase 5)** | `rustls`, `webpki-roots`, `mio` / `tokio`, `httparse` / `ureq` | Handshake TLS 1.3, sockets TCP/UDP no bloqueantes y framing de transporte HTTP de bajo nivel. | Pipeline de **WHATWG Fetch Standard** (CORS, redirecciones, cabeceras `Content-Type`, sniffing MIME, políticas de referrer y cola de carga de recursos del documento). |
| **3. Ventanas y Rasterización 2D (`crates/paint`, `crates/shell` — Fase 4/Fase 7)** | `winit`, `softbuffer`, `tiny-skia` (CPU) o `wgpu` (GPU) | Creación de ventana multiplataforma (X11/Wayland/macOS/Windows), presentación de framebuffer de píxeles y primitivas geométricas 2D de bajo nivel (relleno de rectángulos/paths, composición alfa). | **CSS Box Model**, **Layout** (BFC/IFC), construcción de **Display List**, orden de pintado CSS 2.2, canvas con propagación de fondo raíz/`body`, árbol de apilamiento (*stacking contexts*) y, en Fase 7, *hit-testing*. `crates/paint` **no usa hoy ninguna de estas dependencias**: el rasterizador y el codificador PNG son propios (§3.2). |
| **4. Tipografía y Shaping de Texto (`crates/fonts` — Fase 4)** | `fontdb`, `ttf-parser` / `skrifa`, `rustybuzz`, `ab_glyph` | Enumeración de fuentes del sistema, lectura de tablas binarias TrueType/OpenType (`cmap`, `hmtx`, `glyf`, `GPOS`, `GSUB`), moldeado de glifos (*glyph shaping*) y rasterización de contornos de glifos individuales. | Política de selección y fallback de fuentes de Megi, contrato de métricas/baseline para layout, segmentación de líneas, cálculo de `line-height`/`vertical-align`, posicionamiento de cajas en línea y consumo de las máscaras de glifo en paint (integración WP-4.4 fase 2). |
| **5. Codificación de Imagen (`crates/paint/src/png.rs` — Fase 4)** | **Ninguno** | Serialización binaria de imágenes generadas por el propio motor. | Codificador **PNG propio**: firma, `IHDR`, `IDAT` (zlib RFC 1950 + DEFLATE RFC 1951 con bloques *stored*), CRC-32 IEEE y `IEND`; validación con `tools/png_check.py`. Ver decisión D3 en §3.2. |

### 3.2 Política de Codificación PNG (decisión D3, Fase 4)

La auditoría de Fase 4 detectó que la salida de imagen era únicamente PPM. Se evaluaron tres opciones para producir PNG:

1. **Añadir el crate `png`/`flate2`** (fuera de la matriz de §3.1) — habría requerido una enmienda de política y una justificación de licencia/impacto; no se eligió.
2. **Diferir PNG a la Fase 5** — inaceptable: el objetivo de Fase 4 es «HTML real → PNG» y los golden tests necesitan una salida estándar.
3. **Implementar el codificador desde cero con `std`** (elegida): `crates/paint/src/png.rs` genera PNG válidos usando DEFLATE con bloques *stored*, CRC-32 IEEE y Adler-32. Es determinista (requisito de los golden tests), no introduce dependencias y mantiene el DAG del núcleo intacto.

**Trade-off aceptado**: al no comprimir, un PNG de 800×600 RGBA ocupa ~1,92 MB (frente a unos pocos KB con *deflate* real). Es aceptable para la fase de pruebas y golden images; la compresión se planifica en `WP-4.11` (bien implementando Huffman/LZ77 propio, bien solicitando explícitamente una enmienda de política para `flate2`/`png` si el tamaño pasa a ser un requisito). El validador reproducible es `python3 tools/png_check.py <file.png>` (sólo biblioteca estándar de Python) y se ejecuta en los tests/documentación de Fase 4.
