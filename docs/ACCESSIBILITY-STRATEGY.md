# ADR-002 — Backend de accesibilidad de plataforma (F7-A11Y)

- **Estado**: **ACEPTADO (ADR-002, 2026-11-20) — IMPLEMENTADO**. Backend `accesskit` 0.25 en `crates/browser` (feature `a11y`), con adaptadores y enrutado de foco/click; F7-A11Y queda `partial` sólo por el bus AT real (AT-SPI/UIA) en CI y los widgets nativos.
- **Contexto**: `crates/js` expone un árbol de accesibilidad determinista (`A11yNode`, `AccessibilityTree`, `window.__megiAccessibilityTree()`) con roles, nombres accesibles, estados y jerarquía. El puente a la API de accesibilidad del sistema operativo (AT-SPI en Linux, UI Automation en Windows, NSAccessibility en macOS) ya está implementado mediante `accesskit`; F7-A11Y sigue `partial` únicamente por la verificación en CI con un bus AT real y los widgets nativos.

## Decisión

Autorizar **`accesskit` 0.25.x** (y sus adaptadores de plataforma `accesskit_unix`, `accesskit_windows`, `accesskit_macos`) como dependencias externas de bajo nivel **exclusivamente en `crates/browser`**, tras la matriz §3.1 de `docs/ARCHITECTURE.md`.

- `accesskit` es una biblioteca de infraestructura de accesibilidad multiplataforma, **Rust puro**, sin FFI escrita a mano en los crates de Megi; expone una API segura (`Tree`, `Node`, `Role`, `Action`, `TreeUpdate`) que respeta `#![forbid(unsafe_code)]` en nuestro código.
- La responsabilidad de Megi sigue siendo propia: construir el árbol semántico desde el DOM (ya hecho en `crates/js`), mapear roles/estados/nombres a AccessKit, decidir qué nodos se publican y gestionar el ciclo de vida del adaptedor por pestaña.
- La dependencia se añade **detrás de una feature `a11y`** (no incluida en el build mínimo `--no-default-features`), de modo que el Motor JS-free y los builds embebidos no arrastren el adaptador de plataforma.
- El adaptador de plataforma se activa **sólo en la ventana interactiva** (`crates/browser`), nunca en `crates/render` (que permanece con cero dependencias externas nuevas) ni en el pipeline headless determinista (los goldens PNG no cambian).

## Alcance y límites

1. Roles y estados: mapeo de `A11yNode` a `accesskit::Node` (rol, nombre, descripción, estados, hijos, foco). El contenido dinámico (cambios del DOM) se publica mediante `TreeUpdate` incrementales.
2. Acciones: activar (`click`/`press`), foco y desplazamiento al nodo, sobre los nodos interactivos.
3. Plataformas: Linux (AT-SPI vía `accesskit_unix`), Windows (UIA vía `accesskit_windows`) y macOS (NSAccessibility vía `accesskit_macos`), con degradación controlada a "sin backend" en cabezas sin servidor de accesibilidad (CI, headless).
4. **Fuera de alcance en ADR-002**: widgets nativos del SO, edición de texto accesible completa, y aislamiento por pestaña (F7-ISOLATION, que requiere su propia ADR de IPC/procesos).

## Alternativas consideradas

- **`atspi` directo (cliente D-Bus)**: es una biblioteca de *cliente*, no de *servidor*; no permite publicar un árbol propio sin reimplementar interfaces. Descartado.
- **FFI a ATK/UIA manual**: incompatible con `#![forbid(unsafe_code)]` y con la política de dependencias de bajo nivel. Descartado.
- **Sólo el snapshot JS**: útil para pruebas, pero no expone nada a tecnologías asistivas. Descartado como estado final; hoy se complementa con el backend AccessKit.

## Consecuencias

- `crates/browser/Cargo.toml` gana una feature opcional `a11y` con `accesskit` + el adaptador de la plataforma correspondiente (feature-gate por `cfg(target_os)`).
- `Cargo.lock` incorpora las dependencias (licencias MIT/Apache-2.0, compatibles con el proyecto).
- CI: los tests compilan y validan el *mapeado* y el ciclo de `TreeUpdate` sin necesidad de un bus AT-SPI real; la integración con el SO es opt-in en tiempo de ejecución.
- F7-A11Y podrá pasar de `partial` a `done` cuando el backend esté cableado y exista evidencia verificable (tests del adaptador + CI verde), sin cambiar las métricas deterministas del pipeline.
