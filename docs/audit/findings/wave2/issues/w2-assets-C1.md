[svg] <text font-size="5000"> se salta en silencio (sin error ni aviso)

## Resumen
`crates/svg/src/raster.rs:1016-1018` hace `if !size.is_finite() || size <= 0.0 || size > 4096.0 { return Ok(()); }`: un `<text font-size="5000">` se salta en silencio (pinta 0 píxeles, `Ok`), sin `SvgError::Limit` ni aviso. El truncado silencioso es error silencioso de corrección.

## Impacto
P3 — bug: contenido válido desaparece sin diagnóstico; el llamante no puede distinguir "texto vacío" de "límite excedido". Considerar `SvgError::Limit` o documentar el tope.

## Pasos para reproducir
1. Sonda Mesa A `concerns` (in-process `megi_svg::render_svg` sobre `<text font-size="5000">`).
2. Contar píxeles pintados.

## Esperado vs observado
Esperado: error tipado (`SvgError::Limit`) o tope documentado y visible.
Observado: `painted=0` con `Ok(())`.

## Evidencia literal
```
C1 font-size=5000: painted=0
```
Código (HEAD `d82118c`, idéntico a `3c06db8`; `git log 3c06db8..HEAD` vacío en el archivo):
```rust
let size = spec.font_size * scale;
if !size.is_finite() || size <= 0.0 || size > 4096.0 {
    return Ok(());
}
```
(`raster.rs:1016-1018`).

Commit: 3c06db8

## Fix sugerido
Retornar `SvgError::Limit` (o equivalente tipado) cuando `size > 4096`, o documentar el tope en el contrato público si el silencio es intencional.

## Dedup
Dedup: new (era concern C1 needs-repro del informe F2; Mesa A lo ejecutó y propone elevarlo a hallazgo P3; código sigue en HEAD). Mesa A: VERIFIED (ejecutado). Mesa B: n/a (posterior a su fiscalización; sólo lectura del concern).
