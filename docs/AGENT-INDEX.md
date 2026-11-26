# Índice operativo para agentes de IA

> **Empieza aquí.** Punto único de entrada para cualquier agente que trabaje en Megi. Si este índice y un documento histórico difieren, manda este índice y el roadmap aprobado (PR #2290).

## Orden de lectura (obligatorio)

1. `AGENTS.md` — reglas técnicas y estratégicas (única fuente de reglas).
2. `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` — estrategia, barra de producción §2, hitos M0–M12.
3. `docs/superpowers/plans/2026-11-24-megi-ambition-master-plan.md` — índice de planes, dependencias y barras de cierre.
4. El plan del hito que toques (tabla abajo).
5. El WP asignado en `docs/work-packages/BOARD.md` y su esquema en `docs/work-packages/SCHEMA.md`.
6. Si tocas la UI del shell (chrome, rail, window): `docs/UI-TARGET.md` y su imagen
   `docs/images/ui-target.jpg` son la referencia visual obligatoria (UI oscura
   inspirada en Zen Browser).

## Estado real verificado (2026-11-26)

> No es una meta: es lo medible hoy. Leer antes de elegir trabajo.

- **6/13** hitos cerrados · **47/64** criterios verificados (**73 %**) · **3.697** tests · **18** crates (`PROGRESS.md:7`).
- Gate html5lib PASS (7.050 + 3.904 + 230) y CSS 30/30; **WPT 596/596 en 18 fixtures/6 dirs** (~0,03 % del árbol de referencia, 103 fixtures `skip`) — `docs/status/wpt.md`.
- **Test262: 13 casos locales**, no el árbol tc39 — `docs/status/test262.json`.
- Sitios reales (live): **2 PASS / 1 DEGRADED / 1 ERROR** — `docs/status/real-sites.json`.
- Cobertura 76/76 **autorreferencial** — `docs/status/coverage.json`.
- Paridad visual vs Chrome: 0,92 % example, 4,88 % Wikipedia, 10,78 % rust, 17,39 % HN — `docs/VISUAL-PARITY.md`.
- 34 targets de fuzz y 176 semillas, **sin ejecuciones publicadas**; sandbox `enforced` sólo en Linux.
- 49 gates Python en verde (`python3 tools/check_no_compile_gates.py`); `update_coverage` depende de un sello de conformidad fresco.

**Plan activo de auditoría línea por línea**: `docs/superpowers/plans/2026-11-26-line-by-line-audit-plan.md`.
Parte del estado: escaneo por área completado en 5 dominios (core/html, CSS/layout/paint, net/js/render, browser/shell, herramientas); lectura línea por línea y conversión a WPs pendientes.

## Hitos M0–M12

| Hito | Nombre | Plan(es) | Cierre |
|---|---|---|---|
| M0 | Verdad y cimiento | wpt-harness, security-isolation, release-product | WPT `P/T` publicado; 0 fugas T1/T2; release verify+firma |
| M1 | Interactividad | engine-depth, release-product | 10 fixtures E2E; sitios sin console_errors |
| M2 | Profundidad declarativa | wpt-harness, engine-depth | allowlist reducida; fixtures md5 estables |
| M3 | Frontera de seguridad | security-isolation | 0 fugas cross-site; 31+ targets fuzz; sandbox |
| M4 | Componentes y persistencia | engine-depth, web-platform-extension | 100+ subtests shadow-dom; offline demo |
| M5 | Plataforma nativa | security-isolation, native-integration, release-product | sandbox 3 SO; a11y auditada |
| M6 | Gráficos y media | graphics-media | Canvas 2D, WebGL, media E2E |
| M7 | Herramientas de desarrollador | devtools-platform | suites DevTools/WebDriver |
| M8 | Motor ofensivo | js-evolution, web-platform-extension | JIT/motor por datos; HTTP/3 |
| M9 | Producción | production-readiness | barra §2; 100/100 sitios; auditoría |
| M10 | Plataforma extendida | extended-platform | WebGPU/WebXR/WebRTC, extensiones, sync, mobile |
| M11 | Integración nativa | native-integration | matriz `enforced` por SO |
| M12 | Media/legal | media-licensing | DRM decidido con ADR público |

## Planes ejecutables (15)

Todos en `docs/superpowers/plans/`:

- `2026-11-24-megi-ambition-master-plan.md` — índice maestro
- `2026-11-24-wpt-harness-plan.md`
- `2026-11-24-security-isolation-plan.md`
- `2026-11-24-engine-depth-plan.md`
- `2026-11-24-release-product-plan.md`
- `2026-11-24-js-evolution-plan.md`
- `2026-11-24-graphics-media-plan.md`
- `2026-11-24-web-platform-extension-plan.md`
- `2026-11-24-devtools-platform-plan.md`
- `2026-11-24-work-package-engine-plan.md`
- `2026-11-26-ui-quality-unblock-plan.md` — calidad de la UI del shell sin Rune: 5 WPs paralelos, iconos SVG, estados/foco, favicons y gate visual
- `2026-11-24-production-readiness-plan.md`
- `2026-11-24-extended-platform-plan.md`
- `2026-11-24-native-integration-plan.md`
- `2026-11-24-media-licensing-plan.md`

## Motor de work packages

**Esquema**: `docs/work-packages/SCHEMA.md` (validador `tools/wp_schema.py`). Campos obligatorios: `id` (`WP-<FUENTE>-<clave>`), `source`, `file`, `expected`, `actual`, `spec`, `acceptance`, `verify_cmd`, `metric`, `milestone`. Prioridad = `impact × frequency`.

**Siete generadores** (`tools/wp_generate.py`): `wpt`, `js262`, `site`, `fuzz`, `audit`, `depth`, `user`. Hoy producen cero WPs cuando no hay artefactos (nunca inventan trabajo); los parsers concretos los implementa el plan `work-package-engine`.

```bash
# Ver la cola y el tablero
find docs/work-packages/queue -maxdepth 1 -name '*.toml' | sort
cat docs/work-packages/BOARD.md

# Ver qué generaría cada fuente, sin escribir nada
python3 tools/wp_generate.py --all --dry-run

# Generar WPs de una fuente concreta
python3 tools/wp_generate.py wpt          # o: js262 | site | fuzz | audit | depth | user

# Seleccionar trabajo (máx. 10 en curso) y regenerar el tablero
python3 tools/wp_plan.py --board
```

**Regla**: máximo 10 WPs `in_progress`; el humano revisa y fusiona, los agentes nunca. Cada WP cerrado mueve una métrica; si no la mueve, se rechaza.

## Documentos que NO son fuente de estado

| No mirar para decidir | Mirar en su lugar |
|---|---|
| `docs/GAPS.md`, `docs/ROADMAP.md` (resúmenes históricos) | spec v2 + master plan + `PROGRESS.md` + código |
| `PROGRESS.md`, `docs/status/*` (generados) | fuentes: `docs/status/progress.toml`, `coverage.toml` |
| `docs/audit/FINDINGS.md` (registro/generado) | `docs/audit/README.md` y hallazgos abiertos |

Si encuentras una cifra o fecha vieja en un doc histórico, **no la corrijas a ciegas**: verifica el código y abre un `WP-AUDIT`.

## Verificación antes de reportar éxito

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets
cargo check --workspace --no-default-features
cargo test --workspace
cargo test -p megi-webview --no-default-features
cargo run --release -p megi-test-harness -- --summary
python3 tools/check_arch.py
python3 tools/check_docs_links.py
python3 tools/update_progress.py --check
python3 tools/update_coverage.py --check
python3 tools/audit_coverage.py --check --strict
python3 -m unittest tools.test_wp_schema tools.test_wp_generate tools.test_wp_plan
```
