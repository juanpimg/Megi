[render] presupuesto total de imágenes: break sin failed++, resto abandonado

## Resumen
En `crates/render/src/images.rs:694-701`, al agotar el presupuesto total de bytes de imagen el bucle hace `break` sin `failed++` y sin contar las imágenes omitidas, frente al patrón `continue`+`failed++` de `backgrounds.rs:272-324`. Las imágenes no intentadas desaparecen de la telemetría.

## Impacto
P3 — telemetría: el contador `failed` y los warnings por imagen quedan incompletos; no hay pérdida de render más allá de la ya presupuestada. Consistencia interna backgrounds vs images.

## Pasos para reproducir
1. Crate auxiliar `f2-repro exp10` (límites custom: 6 imgs, presupuesto ≈2.5 ficheros), o CLI con 12 PNGs de ~3.16 MB (38 MB > 32 MB por defecto).
2. Observar `loaded / failed / warnings` en la salida.

## Esperado vs observado
Esperado: cada imagen omitida por presupuesto cuenta como fallo individual (como backgrounds) o warning individual.
Observado (límites custom): `loaded=2 failed=0`, un solo warning `image byte budget exhausted ... (18048 bytes total, max 15040)`; 4 imágenes ni intentadas ni contadas. CLI: `loaded=10 failed=0`, un warning `... (34816226 bytes total, max 33554432)`; img10 e img11 caídas sin rastro individual.

## Evidencia literal
```
EXP10 file_bytes=6016 loaded=2 failed=0 downloaded=12032
EXP10 warn: image byte budget exhausted at … (18048 bytes total, max 15040)
```
Código: `images.rs:694-701` (`break` sordo) vs `backgrounds.rs:272-324` (`continue`+`failed++`).

Commit: 3c06db8

## Workaround
Inspeccionar el warning agregado de presupuesto en vez de `failed` por imagen.

## Dedup
Dedup: new (sin colisión exacta: #1300 bytes solo decodificados y #1303 bypass vía @import son aspectos distintos). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P3 mantener. Fix sugerido: alinear con backgrounds (contar cada omitida).
