# Borrador de advisory privado — w2-st-1173-file-icon (NO publicar como issue público)

- Nombre propuesto: `favicon fetch lee ficheros locales indicados por páginas http(s) sin chequeo de esquema`.
- Severidad propuesta: P2 (confidencialidad). Sin RCE; oráculo de existencia/validez + exfiltración de píxeles 16×16 vía icono.
- Commit afectado: `3c06db8` (`crates/shell/src/favicon.rs:50-129`).
- Alcance: `crates/shell` — `icon_url` / `fetch` rama `File` / `for_page`.

## Impacto (resumen, sin PoC público)
`for_page` no filtra por esquema de la página: una página remota puede referenciar un icono `file:` y el shell lee el fichero local para pintarlo en la banda chrome (tab + address). Efecto observado: el render difiere según el fichero exista/sea imagen válida (oráculo), y los píxeles del icono se exfiltran al screenshot. Dedup #1173 (evidencia nueva en HEAD).

## Evidencia verificada (Mesas A/B, sin pasos explotables)
Lectura: `icon_url` acepta cualquier `href` resuelto; rama `File` lee disco sin mirar la página; `for_page` no filtra. Mesa A replicó el oráculo presente/ausente en headless con `--chrome` (md5 difieren). No se publican fixtures, rutas locales ni diffs de píxeles en este borrador.

## Fix sugerido
En `for_page`/`icon_url`, exigir mismo esquema (o rechazar `file:` cuando la página es `http(s)`/`data:`); resto → fallback. Test: página http con icono file nunca toca disco.

## Referencias de proceso
- `SECURITY.md`: reportar por canal privado (aviso GitHub o email al maintainer). Nunca issue público.
- Mesa A: VERIFIED + ADVISORY. Mesa B: YA REPORTADO (#1173) + security + advisory.

## Dedup
Dedup: #1173 (evidencia nueva: renders + diff + control). Mesa A: VERIFIED + ADVISORY. Mesa B: YA REPORTADO, P2 mantener.
