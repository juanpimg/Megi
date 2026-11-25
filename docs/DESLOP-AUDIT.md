# Auditoría arquitectónica con `deslop-please`

`deslop-please` es un linter de arquitectura (deep modules, narrow interfaces)
que lee el índice SCIP de `rust-analyzer`. Comprueba seis reglas:
`module-cycle`, `tangle`, `field-leak`, `shallow-module`, `wide-directory` y
`long-file`. Megi additionally enforces file size (≤500 líneas) and the crate
DAG with `tools/check_arch.py`.

## Cómo ejecutarlo

```bash
# Instalación (o cargo binstall deslop-please)
cargo install deslop-please

# Requiere el componente rust-analyzer del toolchain pinnado:
rustup component add rust-analyzer --toolchain "$(cat rust-toolchain.toml | ...)"

# Auditoría: la primera pasada indexa el workspace (puede tardar minutos).
deslop-please --allow .deslop-please.allow
```

La salida es `0` si no hay hallazgos nuevos y `1` si los hay. Otras
configuraciones (`--target`, `--all-features`, `--no-default-features`) tienen
índices separados: audita cada configuración que importe. En entornos con poca
memoria usa `CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1` para evitar que
`rust-analyzer scip` sea abortado por OOM.

## Baseline de deuda (2026-11-25)

El primer escaneo completo sobre `origin/main` produjo **219 hallazgos**:

| Regla | Hallazgos |
|---|---:|
| `field-leak` | 119 |
| `module-cycle` | 53 |
| `tangle` | 17 |
| `shallow-module` | 15 |
| `wide-directory` | 15 |
| `long-file` | 0 |

`.deslop-please.allow` congela esa deuda. **No significa que esté resuelta**:
cada entrada es trabajo pendiente. Un hallazgo nuevo no incluido en el archivo
falla el gate; una entrada que deja de aparecer se reporta como *now fixed*.

## Política de corrección

- `field-leak`: un campo que otro módulo escribe, presta o indexa debe pasar a
  ser responsabilidad del tipo. Añadir un getter no arregla el problema.
- `shallow-module`: contraer la interfaz pública del módulo; esconder
  funciones detrás de forwarders uno a uno tampoco.
- `module-cycle` / `tangle`: decidir la capa de abstracción correcta y mover
  la responsabilidad, no romper el ciclo con un `use` distinto.
- `wide-directory`: en `crates/` el tamaño es consecuencia directa del DAG de
  17 crates aprobado; no se "corrige" fusionando crates. El resto se reparte
  en submódulos sólo cuando no violen el límite de 500 líneas por archivo.

Ninguna corrección debe alterar goldens PNG sin decisión explícita con md5
antiguo y nuevo, ni tocar artefactos generados (`PROGRESS.md`,
`docs/status/*`, `docs/audit/FINDINGS.md`, `docs/work-packages/BOARD.md`).
Los cambios que crucen crates requieren PR de arquitectura y actualización
del DAG (`AGENTS.md` §3).
