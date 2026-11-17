# Diseño: estado del proyecto honesto (estilo AnyPS5)

## Intención

`PROGRESS.md` muestra un 99 % global que es el promedio de porcentajes **escritos a mano** en
`docs/status/progress.toml` (F1–F6 = 100, F7 = 99). Las listas `pending` no reducen ese número:
una fase puede decir 100 % con trabajo pendiente documentado. El resultado es una gráfica que no
describe si Megi es un navegador usable.

AnyPS5 resuelve esto con un conteo verificable: `funciones implementadas / funciones conocidas`.
El denominador son las funciones declaradas en el código; cuando descubren nuevas funciones,
el denominador crece y el porcentaje **baja**, lo cual se presenta como una señal de salud
(estamos midiendo mejor), no como una regresión.

## Objetivos

1. Sustituir el "99 %" por una cobertura `implementadas / conocidas` con denominador explícito.
2. Que cada capacidad declarada tenga **evidencia verificable por una máquina**, no una estimación.
3. Que la página muestre explícitamente **qué falta**, agrupado por área y enlazado a issues.
4. Añadir una prueba real de render en webs reales con capturas y métricas, para ver la verdad.
5. Integrar todo en los artefactos generados existentes y en el gate de frescura de CI.

## No objetivos

- No sustituir el gate de conformidad de 4 suites (es una medida distinta y sigue vigente).
- No afirmar conformidad WPT/spec a partir de este conteo: es cobertura de capacidades declaradas.
- No descargar webs en CI (red no determinista): el informe real se genera con un comando local.

## Reglas de conteo (el corazón del diseño)

- El **denominador** es el número de entradas `[[capability]]` en `docs/status/coverage.toml`.
  Cada entrada declara: `id`, `area`, `name`, `source` (spec/ROADMAP/issue) y `state`.
- `state` ∈ `done` | `partial` | `missing`. Solo `done` cuenta como implementada.
  `partial` se muestra por separado y **cuenta como no implementada** (postura conservadora).
- `done` exige `evidence` verificable. Si la evidencia no se puede verificar, el generador
  **falla** (no degrada silenciosamente a "implementada"). Esto impide marcar a ojo.
- Formatos de evidencia:
  - `gate:<suite>` — la suite existe en el bloque `[gate]` de `progress.toml` y el gate está PASS.
  - `test:<ruta>::<fn>` — el fichero existe y contiene `fn <fn>`; es un test real en el repo.
  - `fixture:<ruta>` — el fichero (golden/fixture) existe.
  - `none` — obligatorio para `partial`/`missing`; prohibido para `done`.
- Añadir una capacidad al inventario aumenta el denominador y puede bajar el porcentaje.
  La propia página lo documenta para que no se lea como regresión.

## Decisiones de implementación

- Manifiesto: `docs/status/coverage.toml` (editado a mano, como `progress.toml`).
- Generador: `tools/update_coverage.py` (Python 3.11 stdlib, mismo estilo que `update_progress.py`),
  con `--write` y `--check`; cachea el HEAD en los artefactos para el check de frescura.
- Artefactos generados: `docs/status/coverage.svg`, `docs/status/coverage.html`,
  `docs/status/coverage.json`, `docs/status/badge-coverage.svg`.
- Documentación de reglas: `docs/status/COVERAGE.md` (equivalente a AnyPS5 `docs/dev/PROGRESS.md`).
- Integración: `update_progress.py` lee `coverage.json` y usa su porcentaje como titular;
  `PROGRESS.md` y `progress.svg` dejan de presentar el promedio de fases como "overall completion".
  README apunta al nuevo badge y acota el claim "HTML5 100 %" a las 4 suites del gate.
- CI: el job "Docs freshness" ejecuta también `update_coverage.py --check`.

## Prueba real de render

- `docs/status/real_sites.toml`: lista versionada de URLs reales con umbral mínimo de
  `display_items`, nota y issue asociado.
- `tools/real_site_check.py`: ejecuta `render --json` para cada sitio, guarda PNG en
  `docs/status/real-sites/`, clasifica `PASS` / `DEGRADADO` / `PÁGINA EN BLANCO`, y genera
  `docs/status/real-sites.html` con capturas, métricas y enlaces a issues.
- Modo por defecto usa instantáneas HTML versionadas (determinista, sin red); `--live` usa URLs.

## Criterio de aceptación

- `python3 tools/update_coverage.py --check` pasa tras regenerar.
- `python3 tools/update_progress.py --check --no-gate` sigue pasando.
- Los tests unitarios nuevos pasan y cubren: evidencia no verificable, denominador creciente,
  parciales, salida SVG/HTML y clasificación de render.
- `PROGRESS.md` ya no dice 99 % como titular; dice la cobertura real con su denominator.
