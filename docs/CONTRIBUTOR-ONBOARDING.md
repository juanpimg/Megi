# Onboarding de colaboradores (propuesta para #970)

> **Estado**: propuesta para revisión del mantenedor. Las acciones que cambian
> metadatos de GitHub (etiquetas, Projects, permisos de workflows) requieren
> permisos de maintainer; este documento deja todo listo para aplicarlas.

Este documento complementa [`CONTRIBUTING.md`](../CONTRIBUTING.md) con una lista
de trabajo concreta para quien quiera hacer su primer PR en Megi, y la configuración
propuesta del tablero de colaboración.

## 1. Issues candidatos a `good first issue`

Criterios usados: alcance acotado a 1–2 archivos, criterio de aceptación explícito,
sin dependencias de permisos especiales y verificable con un comando local.

| # | Issue | Por qué es abordable | Verificación |
|---|---|---|---|
| [#472](https://github.com/juanpimg/Megi/issues/472) | `ARCHITECTURE §3.3` describe 4 superficies de fuzz | Solo documentación; apunta a `FUZZING.md` como fuente única | `python3 tools/check_docs_links.py` |
| [#474](https://github.com/juanpimg/Megi/issues/474) | Matrices de dependencias y recuentos obsoletos | Solo documentación, filas de una tabla | `python3 tools/check_docs_links.py` |
| [#485](https://github.com/juanpimg/Megi/issues/485) | PRs de docs lanzan toda la matriz Rust | Un workflow, sin tocar código del motor | `actionlint .github/workflows/ci.yml` |
| [#471](https://github.com/juanpimg/Megi/issues/471) | ~~Targets `webp_vp8`/`webp_lossy` duplicados~~ — **HECHO**: fusionados en el target único `webp` | `ls fuzz/fuzz_targets/webp.rs` |
| [#473](https://github.com/juanpimg/Megi/issues/473) | Faltan diccionarios/libFuzzer y entradas estructuradas | Workspace de fuzz aparte | `cd fuzz && cargo +nightly fuzz build` |
| [#469](https://github.com/juanpimg/Megi/issues/469) | ~~`html_parse` no cubre `ParseOptions` con scripting~~ — **HECHO**: el harness ejercita `parse_document_with_options` con scripting activado/desactivado según el input | `ls fuzz/fuzz_targets/html_parse.rs` |
| [#468](https://github.com/juanpimg/Megi/issues/468) | ~~`net` sin target para cookies/caché/proxy~~ — **HECHO**: `net_surface` cubre `parse_set_cookie`, `parse_cache_policy` y `proxy_authorization` | `ls fuzz/fuzz_targets/net_surface.rs` |
| [#467](https://github.com/juanpimg/Megi/issues/467) | ~~Sin targets directos de layout/paint/fonts~~ — **HECHO**: existen `layout_resolve`, `layout_tree`, `paint_display_list` y `fonts_shape` (los 4 en la matriz ASan, 300 s) | `ls fuzz/fuzz_targets/layout_*.rs fuzz/fuzz_targets/paint_display_list.rs fuzz/fuzz_targets/fonts_shape.rs` |
| [#470](https://github.com/juanpimg/Megi/issues/470) | Harness fidelity: checks inalcanzables en fuzz | Corrección localizada en `fuzz/` | `cd fuzz && cargo +nightly fuzz build` |
| [#483](https://github.com/juanpimg/Megi/issues/483) | Fuzz de la interacción DOM×JS | Añade camino con DOM al target `js_eval` | `cd fuzz && cargo +nightly fuzz build` |
| [#443](https://github.com/juanpimg/Megi/issues/443) | Stacking contexts anidados cuadráticos | `S` si se limita a la partición en una pasada | `cargo test -p paint` + bench |
| [#442](https://github.com/juanpimg/Megi/issues/442) | `apply_group_bbox` reescanea descendientes | `S`: bbox incremental por pila | `cargo test -p svg` |
| [#439](https://github.com/juanpimg/Megi/issues/439) | Patrones anidados crecen exponencialmente | `M`: presupuesto global de tiles | `cargo test -p svg` |
| [#970](https://github.com/juanpimg/Megi/issues/970) | ~~Etiquetas y project board~~ — **HECHO (etiquetas)**: `labeler.yml` e `issue-title-labeler.yml` etiquetan por área; el tablero vive en GitHub | `ls .github/labeler.yml .github/workflows/issue-title-labeler.yml` |

> Nota: la lista debe revisarse antes de aplicar etiquetas, porque el inventario
> de issues cambia cada ronda. El criterio anterior es más duradero que la lista.

## 2. Propuesta de tablero (GitHub Projects)

Columnas sugeridas, con automatización nativa de Projects (`Draft → Todo → In
Progress → In Review → Done`), más dos campos:

- **Fase**: `F0`…`F7`, `tooling`, `docs`, `ci`.
- **Tamaño**: `XS` (≤1 archivo), `S` (1–2 días), `M` (1 semana), `L` (épica).

Reglas de automatización:

1. Issue abierto → `Todo` (o `Draft` si le falta criterio de aceptación).
2. Issue asignado → `In Progress`.
3. PR abierto con `Closes #N` → mueve el issue a `In Review`.
4. PR mergeado → `Done`; issue cerrado sin merge → vuelve a `Todo`.

## 3. Automatización del ciclo de vida

Receta mínima, sin dependencias de terceros, para `.github/workflows/community.yml`
(requiere `pull-requests: write` e `issues: write`, a aprobar por el mantenedor):

```yaml
name: Community
on:
  pull_request_target:
    types: [opened]
  issues:
    types: [opened]
permissions:
  pull-requests: write
  issues: write
jobs:
  welcome:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/github-script@v7
        with:
          script: |
            const isPR = !!context.payload.pull_request;
            const author = isPR ? context.payload.pull_request.user.login
                                : context.payload.issue.user.login;
            if (author === 'juanpimg') return;
            const body = isPR
              ? `¡Gracias por el PR, @${author}! Revisa CONTRIBUTING.md: DCO, commits atómicos y \`./tools/dev.sh all\` en verde.`
              : `¡Gracias por el reporte, @${author}! Revisa CONTRIBUTING.md y adjunta repro + versión de Rust.`;
            await github.rest.issues.createComment({
              ...context.repo,
              issue_number: isPR ? context.payload.pull_request.number
                                 : context.payload.issue.number,
              body,
            });
```

`pull_request_target` ejecuta el workflow en el contexto de la base; el script no
hace checkout ni ejecuta código del PR, así que es seguro para colaboradores
externos.

## 4. Reconocimiento a colaboradores

- Añadir una sección `Contributors` en el README generada por un action de
  confianza, o mantenerla a mano en `docs/CONTRIBUTORS.md`.
- Cerrar cada PR con una mención explícita al colaborador en el `CHANGELOG.md`
  (el proyecto ya usa `cliff.toml`, así que basta con conservar los coautores).
- Rotular los issues del board con el autor del PR que los resolvió.
