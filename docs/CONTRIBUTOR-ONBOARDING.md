# Onboarding de colaboradores

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
| [#1424](https://github.com/juanpimg/Megi/issues/1424) | Sin target de fuzz para gzip/deflate | Añade un target sobre `compression` (workspace de fuzz aparte) | `cd fuzz && cargo +nightly fuzz build` |
| [#1426](https://github.com/juanpimg/Megi/issues/1426) | Status-line/headers HTTP/1.1 inalcanzables desde un harness | Añade un target sobre el parser puro | `cd fuzz && cargo +nightly fuzz build` |
| [#1422](https://github.com/juanpimg/Megi/issues/1422) | `dom_mutate` no valida invariantes del DOM | Añade comprobaciones al harness existente | `cd fuzz && cargo +nightly fuzz build` |
| [#1429](https://github.com/juanpimg/Megi/issues/1429) | `container_queries` anuncia style queries no implementadas | Recorta el target/docs a size-only o implementa `style()` | `cd fuzz && cargo +nightly fuzz build` |
| [#1484](https://github.com/juanpimg/Megi/issues/1484) | `PHASE4-SUPPORT.md` marca capacidades terminadas como PENDIENTE | Solo documentación, filas de una tabla | `python3 tools/check_docs_links.py` |
| [#1414](https://github.com/juanpimg/Megi/issues/1414) | Selector parsing O(n²) (`Vec::remove(0)`) | Refactor localizado en `css/src/selector.rs` | `cargo test -p css` + bench |
| [#1416](https://github.com/juanpimg/Megi/issues/1416) | Color de gradientes: escaneo lineal por píxel | Precalculación de stops en `paint` | `cargo test -p paint` + bench |
| [#1431](https://github.com/juanpimg/Megi/issues/1431) | Fixtures no ejecutados y demos sin test | Añade tests sobre fixtures existentes | `cargo test --workspace` |
| [#1432](https://github.com/juanpimg/Megi/issues/1432) | Pares de fixtures byte-idénticos | Limpieza de fixtures duplicados | `cargo test --workspace` |

> Nota (actualizada 2026-11-17): la tabla anterior listaba #467/#468/#471/#970 (y otras) que ya
> están resueltos — los targets de fuzz y el project board existen en el árbol. Esta lista se
> verificó contra el estado **abierto** de cada issue y su ausencia en el código. El criterio
> anterior (tareas acotadas y verificables con un comando local) es más duradero que la lista.

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
