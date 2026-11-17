# Cobertura de capacidades: reglas de conteo

Inspirado en [AnyPS5](https://github.com/boykopovar/AnyPS5): no medimos "cuánto hemos hecho"
según una estimación, sino **cuántas capacidades conocidas están implementadas con evidencia
verificable**.

[![coverage](badge-coverage.svg)](coverage.html) · [Gráfica](coverage.svg) ·
[Datos crudos](coverage.json) · [Inventario](coverage.toml)

## La regla

- El **denominador** es el número de entradas `[[capability]]` en `coverage.toml`.
  Esas entradas son las capacidades *conocidas*, declaradas a mano por quien hace el trabajo.
- El **numerador** son las capacidades con `state = "done"` **y** evidencia verificable.
- `state = "partial"` cuenta como **no implementada** y se lista por separado. Es una postura
  conservadora: "más o menos hecho" no es "hecho".
- `state = "missing"` cuenta como no implementada y se lista con su issue, si tiene.

### Evidencia verificable

`done` sin evidencia verificable es un **error del generador**, no un 100 % silencioso.

| Evidencia | Cómo se verifica |
|---|---|
| `gate:<suite>` | La suite existe en `[gate]` de `progress.toml`, el gate está `PASS` y sus dos números coinciden (`7050 / 7050`). |
| `test:<ruta>::<fn>` | El fichero existe y contiene `fn <fn>`. Es un test real en el árbol de fuentes. |
| `fixture:<ruta>` | Existe el golden/fixture. |
| `none` | Solo para `partial` o `missing`; prohibido para `done`. |

La verificación de `test:` no ejecuta el test (eso ya lo hace CI), pero impide que un test
renombrado o borrado siga contando como evidencia. Si el test existe pero está roto, CI lo detecta.

## ¿Por qué el porcentaje puede bajar?

Porque el denominador crece. Si descubrimos que Hacker News sale en blanco y añadimos la
capacidad `F3-TABLE-CENTER` como `missing`, pasamos de 54/69 a 54/70: el porcentaje baja aunque
no se haya eliminado ninguna línea de código. Eso es **una señal de que el inventario es más
honesto**, no una regresión. AnyPS5 tuvo exactamente este comportamiento al descubrir nuevas
funciones exportadas.

Por la misma razón, este número **no es comparable entre meses sin leer el inventario**: siempre
se muestra junto a `implementadas/conocidas` y junto a la lista de pendientes.

## Qué NO mide

- No mide conformidad WPT general (solo las 4 suites del gate, que son otra métrica).
- No mide que una web real se vea bien: para eso está el
  [informe de render en webs reales](real-sites.html).
- No mide rendimiento: para eso están `docs/PERFORMANCE.md` y `crates/bench`.

## Añadir o resolver una capacidad

1. Edita `coverage.toml`: añade `[[capability]]` con `id`, `area`, `name`, `source`, `state` y
   `evidence`. Si es `missing`, añade `issue = N` cuando exista un issue.
2. Cuando la capacidad se implemente, cambia `state` a `done` y pon evidencia real
   (`test:…`, `gate:…` o `fixture:…`). Añade el test que la demuestre: el generador no aceptará
   evidence apuntando a un `fn` inexistente.
3. Regenera: `python3 tools/update_coverage.py --write`.
4. Commitea el manifiesto y todos los artefactos generados. CI verifica la frescura con
   `python3 tools/update_coverage.py --check`.
