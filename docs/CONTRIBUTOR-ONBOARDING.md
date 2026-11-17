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

> **Actualización (#1514)**: los ejemplos que aparecían en esta tabla (#467, #468,
> #469, #470, #471, #472, #473, #474, #483, #485, #439, #442, #443, #970) ya
> están resueltos o cerrados. Esta sección no debe tratar trabajo ya entregado
> como candidatura abierta; usa el inventario vivo del repositorio.

| Búsqueda | Cómo usarla | Verificación antes de empezar |
|---|---|---|
| Etiqueta `good first issue` | `gh issue list --repo juanpimg/Megi --label "good first issue" --state open` | Confirma que el issue sigue abierto y tiene criterio de aceptación claro |
| Etiqueta `help wanted` | `gh issue list --repo juanpimg/Megi --label "help wanted" --state open` | Acota el alcance a 1–2 archivos y revisa dependencias |
| Inventario por área | Filtra issues abiertas por `docs`, `tools`, `tests`, `fuzz`, `ci` u otra área relevante | Verifica el comando local sugerido por el issue (p. ej. `python3 tools/check_docs_links.py`, `cd fuzz && cargo +nightly fuzz build`, `cargo test -p <crate>`) |

> Nota: revisa el inventario actual antes de aplicar etiquetas o proponer una
> candidatura, porque las issues cambian en cada ronda. Los criterios anteriores
> son más duraderos que cualquier lista estática.

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
