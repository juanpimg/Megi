# Documentación de Tooling, Skills y Automatización (FASE 2)

Este documento registra la investigación, evaluación, instalación y configuración de herramientas CLI, scripts de extracción de especificaciones y skills locales realizadas durante la **FASE 2** para optimizar el desarrollo autónomo guiado por especificaciones (WHATWG / W3C) y el auto-testeo continuo (`html5lib-tests` y Web Platform Tests) en **Megi**.

---

## 1. Auditoría del Entorno y Ecosistema Evaluado

Antes de instalar o construir herramientas, se auditó el entorno de ejecución local y la disponibilidad de herramientas del ecosistema Rust, Python y MCP:

| Categoría | Herramienta Evaluada | Estado en Entorno | Decisión Técnica y Justificación |
|---|---|---|---|
| **Compilador y Gestor Rust** | `rustc 1.99.1`, `cargo 1.99.1`, `clippy`, `rustfmt` | Preinstalado (`rustup 1.29.3`) | Base oficial para el Cargo Workspace (`edition = "2024"`, `resolver = "2"`). Configurado mediante `.cargo/config.toml`. |
| **Inspección de Código / LSP** | `rust-analyzer`, `ripgrep 13.0.0` (`rg`), `fdfind` | Preinstalado | Utilizado para diagnóstico estático, verificación de tipos y búsqueda rápida en el monorepo. |
| **Test Runners Externos** | `cargo-nextest`, `cargo-insta` | No preinstalado | Se descartó compilar `cargo-nextest` desde código fuente (alto coste de CPU en máquina de 2 vCPUs) a favor de `cargo test --workspace` nativo combinando un binario y librería interna `test_harness` con reportes detallados. |
| **Extracción de Specs (WHATWG/W3C)** | Servidores MCP externos vs. `python3 3.11.2` + `lxml 4.9.2` | `python3` + `lxml` preinstalados | En lugar de depender de scraping manual o lectura web no estructurada, se construyó `tools/whatwg_spec.py` sobre `lxml.html` con caché local en `.cache/specs/` para extraer pasos normativos exactos (`<ol>`, `<dl class="switch">`, códigos de error y `entities.json`) en milisegundos. |
| **Integración de Suites Oficiales** | `wptrunner` completo vs. Sparse Checkout + `tools/test_inspector.py` | `git 2.39.5` + `jq 1.6` + `python3` | `wptrunner` completo está diseñado para navegadores con WebDriver/automatización de ventana completa. Para la Fase 0 y Fase 1 (Parser HTML y construcción DOM), se integraron los repositorios oficiales `html5lib/html5lib-tests` y `web-platform-tests/wpt` (`html/syntax/parsing`) con `tools/test_inspector.py` y el crate interno `crates/test_harness`. |

---

## 2. Herramientas y Skills Añadidas al Proyecto

### 2.1 `tools/whatwg_spec.py` — Extractor y Caché de Especificaciones Oficiales (WHATWG / W3C)

Script CLI ejecutable en Python 3 (`lxml.html` + `urllib.request`) que descarga, almacena en caché local (`.cache/specs/`, ignorado por Git) y consulta de forma estructurada las especificaciones formales:
- **WHATWG HTML Living Standard**:
  - `parsing.html` (`https://html.spec.whatwg.org/multipage/parsing.html` — §13.2 Parsing HTML documents, §13.3 Serializing HTML fragments, §13.4 Parsing HTML fragments).
  - `syntax.html` (`https://html.spec.whatwg.org/multipage/syntax.html` — §13.1 Writing HTML documents).
  - `dom.html` (`https://html.spec.whatwg.org/multipage/dom.html` — §3 Semantics, structure, and APIs of HTML documents).
  - `entities.json` (`https://html.spec.whatwg.org/entities.json` — Tabla oficial de las 2.231 referencias de caracteres con nombre de §13.5).
- **WHATWG DOM Living Standard**: `https://dom.spec.whatwg.org/`.
- **W3C CSS Syntax Module Level 3**: `https://drafts.csswg.org/css-syntax-3/`.

**Subcomandos disponibles**:
```bash
# 1. Descargar y actualizar la caché local en .cache/specs/
./tools/whatwg_spec.py fetch [--all] [--force]

# 2. Listar el índice completo de secciones, números y anclas (#id) de parsing.html
./tools/whatwg_spec.py toc [--filter 13.2.5]

# 3. Extraer los pasos normativos exactos de un estado del tokenizador o modo de inserción
./tools/whatwg_spec.py section 13.2.5.1
./tools/whatwg_spec.py section "#tag-open-state"
./tools/whatwg_spec.py section "#adoption-agency-algorithm"

# 4. Listar los 52 códigos normativos de error de parseo de §13.2.2
./tools/whatwg_spec.py errors

# 5. Consultar la tabla oficial entities.json
./tools/whatwg_spec.py entities --query amp
```

---

### 2.2 `tools/test_inspector.py` — Inspector y Conversor de Suites `html5lib-tests` y WPT

En junio de 2026 (commit `224991ec10db04f056a89eed8b0bd8695fd2950e`), el repositorio oficial `html5lib/html5lib-tests` migró sus tests de `tree-construction/*.dat` a `web-platform-tests/wpt/html/syntax/parsing/resources/*.dat`. Además, mientras que `tokenizer/*.test` utiliza archivos JSON, `tree-construction/*.dat` utiliza el formato basado en directivas de línea (`#data`, `#errors`, `#new-errors`, `#document-fragment`, `#script-on`, `#script-off`, `#document`).

`tools/test_inspector.py` resuelve esta dualidad:
1. Implementa en Python la máquina de estados exacta de `parseDat()` de WPT (`html/syntax/parsing/resources/test.js`), respetando nodos de texto multilínea con líneas en blanco internas (`\n\n`).
2. Permite inspeccionar resúmenes globales y casos individuales tanto de `tokenizer/*.test` como de `tree-construction/*.dat`.
3. Exporta los 62 archivos `.dat` de construcción de árboles (1.973 casos de prueba) a fixtures JSON estructurados en `tests/html5lib-tests/tree-construction-json/*.json`, permitiendo que `crates/test_harness` consuma tanto `.dat` nativos como `.json` preprocesados.

**Subcomandos disponibles**:
```bash
# Resumen estadístico de los 14 archivos tokenizer/*.test (6.824 tests) y 62 archivos tree-construction/*.dat (1.973 tests)
./tools/test_inspector.py summary

# Mostrar la entrada, estado inicial, errores y salida esperada de un test específico
./tools/test_inspector.py show test1.test 0
./tools/test_inspector.py show doctype01.dat 0

# Exportar los 62 archivos tree-construction/*.dat a tests/html5lib-tests/tree-construction-json/*.json
./tools/test_inspector.py export-json
```

---

### 2.3 `tools/dev.sh` y Alias en `.cargo/config.toml` — Automatización del Ciclo de Verificación Rust

Para acelerar la inspección de código, el formateo, el análisis estático (`clippy`), la generación de documentación (`rustdoc`) y la ejecución del bucle cerrado de tests:

- **Alias de Cargo (`.cargo/config.toml`)**:
  - `cargo test-all` → `cargo test --workspace`
  - `cargo test-harness` → `cargo test -p test_harness`
  - `cargo lint` → `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo doc-check` → `cargo doc --workspace --no-deps`
- **Script unificado (`tools/dev.sh`)**:
  - `./tools/dev.sh check`: Compilación rápida de todos los targets del workspace.
  - `./tools/dev.sh test`: Ejecución del bucle de tests unitarios e integración.
  - `./tools/dev.sh lint`: Verificación estricta con `rustfmt` y `clippy` (`-D warnings`).
  - `./tools/dev.sh doc`: Verificación de documentación de API (`rustdoc`).
  - `./tools/dev.sh report`: Ejecución del reporte completo de conformidad del `test_harness`.
  - `./tools/dev.sh all`: Ejecuta secuencialmente `fmt`, `clippy`, `test` y `doc`.

---

### 2.4 Skill Local para Agentes (`.agents/skills/whatwg-spec-driven-dev/SKILL.md`)

Se instaló la skill local `whatwg-spec-driven-dev` en `.agents/skills/whatwg-spec-driven-dev/SKILL.md` (directorio ignorado en `.gitignore` para mantener limpio el árbol Git de metadatos de agentes, mientras su documentación permanente reside en este archivo `docs/TOOLING.md`). Esta skill estandariza el protocolo operativo para cualquier agente que trabaje en la Fase 1 y posteriores:
1. Consultar primero la sección normativa con `./tools/whatwg_spec.py section <id>`.
2. Inspeccionar los casos de prueba objetivo con `./tools/test_inspector.py show <file> <idx>`.
3. Implementar la transición de estado o regla del árbol DOM en `crates/html` o `crates/dom`.
4. Verificar en bucle cerrado con `cargo test -p test_harness`.
