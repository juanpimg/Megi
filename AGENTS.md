# AGENTS.md — Reglas de arquitectura de Megi

Eres un agente que trabaja en un motor web 100 % Rust, desde cero, con
vocación de ser embebible. Estas reglas son obligatorias. Si no puedes
cumplirlas, para y pregunta. Mantén este archivo por debajo de 200 líneas.

## 1. Orientación antes de tocar nada
1. Lee `README.md`, `docs/ARCHITECTURE.md` (DAG y §3.1) y este archivo.
2. `git status --short`; NO pises cambios ajenos sin commitear. Trabaja en worktree.
3. Verifica el código, no solo la documentación: `docs/GAPS.md` es histórico.
   Si un doc y el código discrepian, el código + sus tests son la verdad; abre issue para el doc.
4. No edites artefactos generados: `PROGRESS.md`, `docs/status/*`, `docs/audit/FINDINGS.md`.

## 2. Vetos absolutos
- `#![forbid(unsafe_code)]` en los 14 crates. Cero `unsafe`.
- Cero `Rc<`, `Rc<RefCell<`, `Weak<` en código nuevo. Usa `NodeId` + arena,
  `Arc<T>` solo para payload inmutable `Send + Sync`, o `mpsc` para mutación.
- Cero lifetimes propios ('a) en tipos públicos que crucen crates. Préstamos
  cortos dentro de un módulo están bien.
- Cero dependencias que implementen parseo HTML/CSS, layout o pintado
  (html5ever, cssparser, selectores, stylo, taffy, blitz…). Consulta §3.1.
- Cero dependencias circulares entre crates. Un edge nuevo requiere PR de
  arquitectura + actualización del DAG.

## 3. DAG: quién puede importar a quién
    core → html, css, net, js, image, fonts
    css + fonts → layout → paint → raster → render → webview → browser
    image + fonts → svg → render
    display (core + fonts) es el contrato de paint y raster.
Prohibiciones clave:
- `net` NUNCA importa `js` (PAC usa el trait `PacEvaluator`).
- `js` NUNCA importa `net`, `layout`, `paint` ni `render` (usa providers).
- `layout` NUNCA importa `paint`, `display` ni `render`.
- `paint` NUNCA importa `raster` ni `render`; `raster` NUNCA importa `layout`.
- `webview` NUNCA importa winit/softbuffer/UI; `browser` no importa crates de motor
  directamente (solo `webview`).
- `test_harness`/`bench` son herramientas: nadie del motor depende de ellos.

## 4. Tamaño y forma de archivos
- Archivo `.rs` nuevo: **≤ 400 líneas** (objetivo) / **500 líneas** (tope duro).
- Modificar un archivo > 500 líneas: divide primero la sección que tocas en un
  submódulo, salvo excepción documentada con
  `// ARCH-EXCEPTION(#issue): motivo; plan de reducción en <issue>`.
- Crate: objetivo ≤ 12.000 líneas de `src/`; > 20.000 requiere plan de escisión.
- Un módulo público expone uno o pocos tipos relacionados; `lib.rs` solo
  re-exporta (sin `impl` de más de 20 líneas).
- Contratos entre crates: enums/structs owned, `Copy` cuando sea posible, sin
  `Rc`/`RefCell`/`&'a`. Traits se definen en el consumidor.

## 5. TDD — ley de hierro
1. Un test mínimo que compruebe comportamiento observable (no un mock).
2. `cargo test -p <crate> <nombre>` y confirmar que FALLA por la razón esperada.
3. Implementar el arreglo más pequeño; refactorizar solo en verde.
4. Citar la norma: `// WHATWG HTML §13.2.6.4.7` / `// RFC 6455 §5.2`.
5. Cambios visuales: golden PNG byte-idéntico salvo decisión explícita con
   md5 antiguo y nuevo en el commit.

## 6. Verificación antes de reportar éxito (pega la salida)
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo check --workspace --all-targets
    cargo check --workspace --no-default-features
    cargo test --workspace
    cargo test -p megi-webview --no-default-features
    cargo run --release -p megi-test-harness -- --summary   # debe decir PASS
    python3 tools/check_arch.py                             # DAG + tamaños
    python3 tools/check_docs_links.py
    python3 tools/update_progress.py --check
    python3 tools/update_coverage.py --check
    python3 tools/audit_coverage.py --check --strict
Cero advertencias. Reporta cada comando con su código de salida. Nunca digas
"verde" sin salida pegada.

## 7. Dependencias, métricas y entrega
- Diseño de specs vivas (WHATWG/W3C/RFC); prohíbido inventar algoritmos.
- Nueva dependencia externa: solo transporte/cripto/ventana/tipografía de bajo
  nivel, previa lectura de `docs/ARCHITECTURE.md` §3.1 y aprobación.
- Si cambia el inventario de archivos: `tools/audit_coverage.py --generate` y
  marcar filas como covered SOLO tras leerlas. Ledger en 0 pendientes.
- Commits atómicos, Conventional Commits en inglés, DCO (`git commit -s`).
- Entrega: (a) qué cambió y por qué, (b) comandos + códigos de salida,
  (c) salida del gate, (d) limitaciones conocidas por nombre.
