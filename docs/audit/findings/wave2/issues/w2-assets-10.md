[image] doc de JPEG progresivo promete rechazar refines entrelazados, pero los acepta

## Resumen
El docstring (`crates/image/src/jpeg.rs:8-11`) dice que "colour refinement scans (`Ah > 0` con más de un componente) retornan `UnsupportedFeature`", pero `parse_sos` (`jpeg.rs:881-953`, chequeo `948-950`: sólo rechaza `Ns>1 && ss!=0`) acepta un scan DC-refine entrelazado (`Ss==0, Ah>0, Ns>1`), que se decodifica por `decode_dc_refine` sin error.

## Impacto
P3 — docs: discrepancia doc/código; el comportamiento real (decodificar) es el razonable, pero el contrato escrito miente. Dirección del fix: corregir el doc, no el comportamiento.

## Pasos para reproducir
1. Parsear SOS de `crates/image/tests/fixtures/jpeg/progressive.jpg`: contiene `Ns=3 Ss=0 Se=0 Ah=1 Al=0`.
2. `cargo test -p megi-image progressive` (tests `jpeg_scaled` y `scaled` en verde: `decode`/`decode_scaled` tienen éxito).

## Esperado vs observado
Esperado (según doc): `UnsupportedFeature` para `Ah>0` multi-componente.
Observado: decode OK.

## Evidencia literal
```
SOS scans: [(3, 0, 0, 0, 1), …, (3, 0, 0, 1, 0), (1, 1, 63, 1, 0), …]
decode OK 12x8
scaled OK 6x4
```
Refine entrelazado `(Ns=3, Ss=0, Se=0, Ah=1, Al=0)` presente y decodificado.

Commit: 3c06db8

## Fix sugerido
Corregir el docstring (líneas 8–11 y 26): "sólo se rechazan AC entrelazados (`Ss>0`)".

## Dedup
Dedup: new (sin filas en ledger sobre doc JPEG progresivo). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P3 ok.
