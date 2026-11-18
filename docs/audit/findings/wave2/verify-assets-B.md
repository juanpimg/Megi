# Verificación Mesa B — scope F2-assets (`crates/fonts` / `crates/image` / `crates/svg`)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f2-assets.md` (11 hallazgos `w2-assets-01..11`).
No se edita código ni ledger. Fecha: 2026-11-18. HEAD auditado por el informe: `3c06db8`;
HEAD actual al fiscalizar: `d82118c`. Nota: no existe `verifier-template.md` en el repo
(búsqueda por glob sin resultados); se sigue el formato de los informes hermanos de Mesa B
(`verify-js-B.md`, `verify-browser-B.md`).
Método: lectura íntegra del informe + `FINDINGS.md` (ledger, 254 líneas) + estado vía
`gh issue view` + spot-checks de código en el árbol actual.

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md`: hits relevantes — #1270 (svg gradient linear scan, Deferred),
  #1273 (svg switch renders every child, P3), #1275 (gradient stop style/sort, otro defecto),
  #1276 (currentColor→black, P3), #1324 (miter bowtie, P1), #1329 (switch first-passing, P2),
  #1331 (currentColor hardcode + color no parseado, P2), #1129/#1130 (css named-colors/hsl,
  Fixed on main — otro crate), #1190 (paint root opacity ignored, Fixed),
  #1268 (svg root presentation attrs/opacity, Fixed), #1281 (svg stale docs
  textPath/SMIL/filtros, Fixed — temas distintos de w2-assets-11), #1298 (net stale docs,
  otro crate). Sin filas sobre colores con nombre en `svg`, `hsl()` en `svg`,
  `visibility="inherit"`, `first_number`/unidades, opacidad raíz porcentual, ALPH 2/3
  ni doc JPEG progresivo.
- `gh issue view` (estados): #1270, #1273, #1276, #1329, #1331, #1324, #1416
  (paint gradient linear scan, otro crate) — todos CLOSED. #1270/#1273/#1275/#1276/
  #1324/#1329/#1331 figuran como cerrados-diferidos pero vivos según el informe.
- Spot-checks de código (árbol actual; `git log 3c06db8..HEAD` vacío en los 5 archivos
  revisados → el código citado es el auditado): `document.rs:1067` rama
  `"g"|"a"|"switch"` como grupo (01 vivo); `raster.rs:1883-1920` intersección
  `v+t·n0` vs `v+u·n1` con comentario que la admite (02 vivo; derivación `t≡−1`
  re-chequeada por el fiscal vía Cramer: `t=(b×n1)/(n0×n1)` con `b=n1−n0` → `−1`
  idéntico); `color.rs:138-163` sólo 16 básicos + orange/transparent (03 vivo);
  `webp.rs:1025-1029` rechazo `ALPH filtering method` (09 vivo); `document.rs:2813`
  `first_number` sin unidades (06 vivo); `document.rs:419-422` opacidad raíz con
  `parse::<f32>` (07 vivo); `gradient.rs:225-242` scan lineal (08 vivo);
  `jpeg.rs:8-11` doc vs `parse_sos` (10 plausible por lectura).
- Verificado-fijo del informe (`currentColor` #1276/#1331): corroborado a nivel código
  (`color.rs` gestiona `currentcolor` contra la propiedad `color` heredada, con
  comentario que cita #1276). Correcto no publicarlo como hallazgo.

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | accionable | spec citada | nota fiscal |
|----|-----------|----------------|-----------------|------------|-------------|-------------|
| w2-assets-01 | YA REPORTADO (#1329; #1273 duplicado previo del mismo síntoma) | P2 (mantener) | sí | sí (evaluar tests por hijo, pintar primero válido) | sí (SVG 1.1 §5.8) | Evidencia nueva con repro en HEAD. Consecuencia procesal: comentar/reabrir #1329 (match exacto y P2 coincidente), no issue nuevo. #1273 (P3, mismo síntoma) es duplicado previo: citarlo al filear para que la Chair los vincule. |
| w2-assets-02 | YA REPORTADO (#1324) | P1 (mantener) | sí | sí (resolver `v+h·n0+s·d0 = v+h·n1+t·d1`; distancia para el límite) | sí (SVG 1.1 §11.4) | Evidencia nueva fuerte: píxeles (tip correcto (100,1.97) vacío) + derivación `t≡−1`, re-derivada y confirmada por el fiscal. P1 coherente con la del propio #1324. Comentar en #1324, no issue nuevo. |
| w2-assets-03 | SOLO NUEVO (relacionado #1129, otro crate) | P2 (mantener) | sí | sí (tabla 148 nombres; reutilizar la del crate css) | sí (CSS Color 3 / SVG 1.1 §4.2) | #1129 fixed es del crate `css`; `svg::named_color` sigue con 16+2. Fallo silencioso a transparente = contenido invisible. Distinción de crate correcta. |
| w2-assets-04 | SOLO NUEVO (relacionado #1130, otro crate) | P2 (mantener) | sí | sí (parser `hsl()`/`hsla()` como el del crate css) | sí (CSS Color 3 §4.2.4) | Sin colisión en ledger para `svg`. Mismo patrón que 03: forma funcional común → invisible. |
| w2-assets-05 | SOLO NUEVO | P3 (ok) | sí | sí (no tocar `visibility` con `inherit`) | sí (SVG 1.1 §5.3 / CSS2) | Sin colisión. `value == "visible"` convierte cualquier otro valor en oculto; fix one-liner. P3 correcta (caso borde explícito). |
| w2-assets-06 | SOLO NUEVO | P2 (mantener) | sí | sí (`parse_length` o desnudar `px` en `first_number`) | sí (SVG 1.1 §4, `px` unidad nula) | El P2 se sostiene por el efecto grave: `stdDeviation="2px"` → `0.0` → `feGaussianBlur` descartado en silencio, más allá del desplazamiento de texto. Sin colisión en ledger. |
| w2-assets-07 | SOLO NUEVO (distinto de #1190/#1268) | P3 (ok) | sí | sí (`parse_opacity_attribute` en raíz) | sí (SVG 1.1 §12) | #1190 (paint) y #1268 (svg) eran "opacidad raíz ignorada" y están Fixed; este defecto es distinto (decimal funciona, `%` no) e inconsistente con el resto de elementos. Al filear, referenciarlos para evitar confusión. |
| w2-assets-08 | YA REPORTADO (#1270; #1416 es otro crate, bien distinguido) | P2 (mantener; P3 admisible) | sí | sí (bisección con `partition_point`, precedente `raster.rs:951`) | n/a formal (perf; stops ordenados en `gradient.rs:365`) | Evidencia nueva con timings debug (g2 ~1.0-1.3s vs g256 ~1.8-2.6s). P2 coherente con la del propio #1270; P3 admisible si la política reserva P2 a corrección (es sólo perf, sin incorrección). Comentar en #1270, no issue nuevo. |
| w2-assets-09 | SOLO NUEVO | P2 (mantener) | sí | sí (implementar filtros 2/3 espejos del horizontal) | sí (WebP Container Spec, ALPH byte 0, métodos 0–3) | Sin colisión. Archivos válidos que fallan enteros (codificadores emiten 2/3) → P2 proporcionada. Harness con bytes idénticos salvo byte 0 es evidencia limpia. |
| w2-assets-10 | SOLO NUEVO | P3 (ok) | sí | sí (corregir docstring, no el comportamiento) | n/a (docs; el comportamiento real es el razonable) | Dirección del fix correcta (doc miente, código decodifica bien; fixture `progressive.jpg` con refine entrelazado + suite verde). P3 docs correcta. |
| w2-assets-11 | SOLO NUEVO (distinto de #1281) | P3 (ok) | sí (ómnibus de 3 docstrings) | sí (editar `lib.rs`, `png.rs:358`, `gif.rs:315`) | n/a (docs) | #1281 cubría otros temas stale (textPath/SMIL/filtros) ya Fixed. Ómnibus aceptable para docs de una línea; si se filea como issue, sugerir checklist de 3 ítems. |

## Concerns

1. **Plantilla ausente**: `verifier-template.md` (Mesa B) no existe en el repo; formato
   tomado de `verify-js-B.md`/`verify-browser-B.md`. La Chair debe confirmar conformidad.
2. **Dedup con doble antecedente (01)**: #1273 y #1329 describen el mismo síntoma de
   `<switch>`; el informe cita ambos. Recomendación: comentar en #1329 (título exacto,
   P2 coincidente) y vincular #1273, en vez de abrir issue nuevo.
3. **Vecindad de fileados (07, 11)**: 07 linda con #1190/#1268 (Fixed) y 11 con #1281
   (Fixed) sin solapar; al crear issues, referenciarlos explícitamente para que el
   triaje no los cierre como duplicados.
4. **Calibración P2-perf (08)**: único P2 no-correctness del scope junto a 02/09
   (éstos sí funcionales). Mantengo P2 por coherencia con #1270 y coste por píxel
   persistente en release, pero P3 es admisible bajo política estricta.
5. **Deriva de HEAD**: `git log 3c06db8..HEAD` sin cambios en los archivos citados
   (`document.rs`, `raster.rs`, `color.rs`, `gradient.rs`, `webp.rs`, `jpeg.rs`);
   los veredictos transfieren limpiamente al HEAD actual. Los 303 tests del informe
   no se re-ejecutaron (sólo spot-checks de código); re-correr suites si se exige
   verificación fresca al filear.
6. **Concerns C1–C6 del informe**: correctamente no publicados (needs-repro/notas,
   incluido C1 bloqueado por `/tmp` lleno). No requieren acción de la Mesa; C1
   (`font-size>4096` truncado silencioso) es el candidato más digno de un repro
   futuro.
