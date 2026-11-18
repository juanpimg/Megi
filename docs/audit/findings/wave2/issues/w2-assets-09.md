[image] WebP ALPH con filtro vertical/gradient se rechaza (imagen entera falla)

## Resumen
`apply_alpha_plane` (`crates/image/src/webp.rs:1025-1029`) sólo implementa métodos 0 (none) y 1 (horizontal); 2 (vertical) y 3 (gradient), válidos según el WebP Container Spec ("ALPH chunk", byte 0, métodos 0–3), devuelven `UnsupportedFeature` y la imagen entera falla.

## Impacto
P2 — bug de corrección: WebP extended válidos (codificadores emiten 2/3) no abren.

## Pasos para reproducir
1. `docs/audit/findings/wave2/repro-w2-assets/alph-f1.webp` (control OK), `alph-f2.webp`, `alph-f3.webp`.
2. `megi_image::decode` sobre cada uno (harness Mesa A `alph`; bytes idénticos salvo el byte 0 del chunk ALPH).

## Esperado vs observado
Esperado: decode OK 16x16 en los tres (WebP Container Spec, métodos 0–3).
Observado: `filter=0: OK 16x16`, `filter=1: OK 16x16`, `filter=2: ERR unsupported feature: WebP ALPH filtering method`, `filter=3: ERR …`.

## Evidencia literal
```
ALPH chunk at file offset 30
filter=0: OK 16x16
filter=1: OK 16x16
filter=2: ERR UnsupportedFeature { feature: "WebP ALPH filtering method" }
filter=3: ERR UnsupportedFeature { feature: "WebP ALPH filtering method" }
```
Código: `webp.rs:1025-1029` (`apply_alpha_plane`).

Commit: 3c06db8

## Fix sugerido
Implementar filtros vertical y gradient (espejos del horizontal por columnas / predictor), o decodificarlos a plano sin filtrar equivalente.

## Dedup
Dedup: new (sin colisión en ledger para ALPH 2/3). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P2 mantener.
