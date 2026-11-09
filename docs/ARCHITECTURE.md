# Arquitectura Maestra del Motor de Navegación Web Megi (`docs/ARCHITECTURE.md`)

**Megi** es un motor de navegación web moderno, open source e independiente desarrollado desde cero en **Rust** (Edition 2024), diseñado sin bifurcar ni envolver motores preexistentes (Blink, WebKit, Gecko, ni los crates de parsing/layout de Servo o Blitz). Todo el diseño e implementación de Megi se rige estrictamente por las especificaciones formales vivas de **WHATWG** (HTML Living Standard, DOM Living Standard, Encoding, URL, Fetch) y del **W3C** (CSS Syntax Module Level 3, Selectors Level 4, CSS Box Model / Cascade), verificado continuamente en bucle cerrado contra **`html5lib-tests`** y **Web Platform Tests (WPT)**.

---

## 1. Modelo de Crates y Flujo Acíclico de Dependencias (DAG)

El repositorio se estructura como un **Cargo Workspace modular** (`resolver = "2"`, `edition = "2024"`) donde cada subsistema del motor reside en un crate aislado dentro de `crates/`. Se prohíben terminantemente las dependencias circulares: el grafo de dependencias entre crates forma un **Grafo Acíclico Dirigido (DAG)** estricto en el que **`crates/dom`** actúa como el núcleo de datos libre de dependencias externas.

### 1.1 Diagrama del Grafo de Dependencias del Workspace

```text
                        ┌───────────────────────────────────────┐
                        │              crates/dom               │
                        │  (Arena DOM, NodeId(NonZeroU32),      │
                        │   NodeKind, QuirksMode, Namespace,    │
                        │   Mutaciones O(1) intrusivas)         │
                        │   [0 dependencias externas]           │
                        └───────────────────▲───────────────────┘
                                            │
                  ┌─────────────────────────┴─────────────────────────┐
                  │                                                   │
      ┌───────────┴───────────┐                           ┌───────────┴───────────┐
      │      crates/html      │                           │      crates/css       │
      │ (WHATWG HTML §13.2:   │                           │ (W3C CSS Syntax L3,   │
      │  InputStream, 84-State│                           │  Selectors L4,        │
      │  Tokenizer, 21-Mode   │                           │  Cascade & Computed   │
      │  TreeBuilder,         │                           │  Style Structures)    │
      │  Serializer)          │                           │  [Depende solo de dom]│
      │ [Depende solo de dom] │                           └───────────▲───────────┘
      └───────────▲───────────┘                                       │
                  │                                                   │
                  ├─────────────────────────┬─────────────────────────┤
                  │                         │                         │
      ┌───────────┴───────────┐ ┌───────────┴───────────┐ ┌───────────┴───────────┐
      │  crates/test_harness  │ │  crates/layout (F2+)  │ │   crates/net (F2+)    │
      │ (Bucle cerrado WPT &  │ │ (Block/Inline/Table   │ │ (Transporte HTTP/TLS, │
      │  html5lib-tests JSON  │ │  Formatting Contexts, │ │  Fetch Pipeline)      │
      │  y .dat runner)       │ │  Text Shaping)        │ └───────────▲───────────┘
      └───────────────────────┘ └───────────▲───────────┘             │
                                            │                         │
                                ┌───────────┴───────────┐             │
                                │  crates/paint (F3+)   │             │
                                │ (Display List &       │             │
                                │  Rasterización 2D)    │             │
                                └───────────▲───────────┘             │
                                            │                         │
                                            └────────────┬────────────┘
                                                         │
                                             ┌───────────┴───────────┐
                                             │  crates/shell (F3+)   │
                                             │ (Ventana OS, Eventos, │
                                             │  Orquestación)        │
                                             └───────────────────────┘
```

### 1.2 Responsabilidades y Contratos por Crate

| Crate | Ruta | Dependencias Internas | Dependencias Externas | Responsabilidad Arquitectónica |
|---|---|---|---|---|
| **`dom`** | `crates/dom` | Ninguna | Ninguna (`std` puro) | Propiedad única de la arena contigua de nodos (`Document::nodes: Vec<Node>`), identificadores `NodeId(NonZeroU32)`, tipos de nodo (`Document`, `DocumentFragment`, `Doctype`, `Element`, `Text`, `Comment`, `ProcessingInstruction`), espacios de nombres (`Namespace`), `QuirksMode` y primitivas de mutación del árbol en tiempo constante `O(1)`. |
| **`html`** | `crates/html` | `dom` | Ninguna (`std` puro) | Implementación fiel de WHATWG HTML §13.2–§13.5: Preprocesador del flujo de entrada (`input_stream.rs`, §13.2.3.5), Tabla completa de 2.231 referencias de caracteres con nombre (`entities.rs`, §13.5), Tokenizador de 84 estados y 52 códigos de error (`tokenizer.rs`, §13.2.5 y §13.2.2), Constructor de árboles de 21 modos de inserción (`tree_builder.rs`, §13.2.6 y §13.4) y Serializador canónico de pruebas (`serializer.rs`). |
| **`css`** | `crates/css` | `dom` | Ninguna (`std` puro) | Tokenización y parseo según W3C CSS Syntax Module Level 3, evaluación de selectores sobre `dom::Document` (Selectors Level 4), orden de precedencia de la cascada (origen, importancia, especificidad y orden de aparición) y valores computados (`ComputedStyle`) con soporte para herencia y hoja de estilo por defecto del User-Agent (WHATWG HTML §15). |
| **`layout`** | `crates/layout` | `dom`, `css` | Ninguna (`std` puro) | Construcción del Box Tree (árbol de cajas de renderizado), supresión de elementos `display: none`, envoltura automática de hijos en línea en bloques anónimos (`AnonymousBlock`, CSS 2.2 §9.2.1.1), y algoritmo del contexto de formato de bloque (Block Formatting Context, CSS 2.2 §10) para el cálculo de anchos, márgenes automáticos centrados, apilamiento vertical y altura automática basada en cajas de margen. |
| **`test_harness`** | `crates/test_harness` | `dom`, `html`, `css` | `serde`, `serde_json` | Ejecutor autónomo en bucle cerrado (closed-loop runner) que parsea los 14 archivos JSON de `tests/html5lib-tests/tokenizer/*.test` (6.824 casos), los 62 archivos `.dat` / `.json` de `tests/html5lib-tests/tree-construction/` (1.973 casos), y las suites oficiales de WPT en `tests/wpt/css/css-syntax` (30 tests de sintaxis, preprocesamiento Unicode y fórmulas $An+B$), verificando automáticamente las salidas mediante `cargo test`. |

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
| **Acoplamiento con Futuros Wrappers JS (Fase 3+)** | Imposible de escalar a ciclos cruzados DOM ↔ JS sin reescribir el DOM. | Fuertemente acoplado desde el día 1 en Servo (`JS-Servos-only-GC.md`); desacoplado recientemente en Ladybird (PR #9973, agosto 2026). | **Desacoplado (Inspirado en Ladybird PR #9973)**: Los nodos DOM no cargan estado de objetos JS hasta que un script los toca; una tabla lateral asociará `NodeId <-> JSWrapper` por mundo de ejecución. |

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
- Solo se autorizan **dependencias externas de bajo nivel** para transporte de red/criptografía, acceso a ventanas/buffers gráficos del sistema operativo, lectura de tablas tipográficas OpenType y serialización de fixtures de test.

### 3.1 Matriz de Dependencias Autorizadas por Subsistema

| Capa / Subsistema | Crates Autorizados | Alcance Estricto Permitido | Responsabilidad que Megi Implementa Desde Cero |
|---|---|---|---|
| **0. Núcleo DOM, HTML y CSS (`dom`, `html`, `css`)** | **Ninguno** (`std` puro en Fase 0–1) | Estructuras de datos estándar de Rust. | Arena DOM, Preprocesador de flujo, Tokenizador HTML de 84 estados, Constructor de Árbol de 21 modos, Tokenizador/Parser CSS y Selectores. |
| **1. Arnés de Pruebas (`test_harness`)** | `serde`, `serde_json` | Deserialización de fixtures JSON (`tokenizer/*.test` y `tree-construction-json/*.json`). | Lógica de desescapado `doubleEscaped` con soporte de sustitutos UTF-16 (`Vec<u32>`), parser de máquina de estados `.dat` (`parseDat`), coalescencia de tokens y comparador diferencial. |
| **2. Red y Transporte (`crates/net` — Fase 2+)** | `rustls`, `webpki-roots`, `mio` / `tokio`, `httparse` / `ureq` | Handshake TLS 1.3, sockets TCP/UDP no bloqueantes y framing de transporte HTTP de bajo nivel. | Pipeline de **WHATWG Fetch Standard** (CORS, redirecciones, cabeceras `Content-Type`, sniffing MIME, políticas de referrer y cola de carga de recursos del documento). |
| **3. Ventanas y Rasterización 2D Básica (`crates/paint`, `crates/shell` — Fase 3+)** | `winit`, `softbuffer`, `tiny-skia` (CPU) o `wgpu` (GPU) | Creación de ventana multiplataforma (X11/Wayland/macOS/Windows), presentación de framebuffer de píxeles y primitivas geométricas 2D de bajo nivel (relleno de rectángulos/paths, composición alfa). | **CSS Box Model**, cálculo de **Layout** (Block Formatting Context, Inline Formatting Context, Flexbox, Tablas), construcción de **Display List**, árbol de apilamiento (*stacking contexts*) y *hit-testing*. |
| **4. Tipografía y Shaping de Texto (`crates/layout` — Fase 2+)** | `fontdb`, `ttf-parser` / `skrifa`, `rustybuzz`, `ab_glyph` | Enumeración de fuentes del sistema, lectura de tablas binarias TrueType/OpenType (`cmap`, `hmtx`, `glyf`, `GPOS`, `GSUB`), moldeado de glifos (*glyph shaping*) y rasterización de contornos de glifos individuales. | **CSS Inline Layout & Line Breaking**: segmentación de líneas, cálculo de `line-height` y `vertical-align`, posicionamiento de cajas en línea (*inline boxes*) y medición de fragmentos de texto dentro del árbol de layout. |
