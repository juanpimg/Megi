# Borrador de advisory privado — w2-render-01 (NO publicar como issue público)

- Nombre propuesto: `Per-level iframe cap allows render-count amplification (~0.3 KiB → hundreds of renders; W=16 projects ~70k renders, hang/OOM)`.
- Severidad propuesta: P1 (disponibilidad). Sin corrupción de memoria, sin escape de sandbox ni ejecución de código fuera del render.
- Commit afectado: `3c06db8` (`crates/render/src/frames.rs:39-48`, `lib.rs:1223`, `frames.rs:113`).
- Alcance: `crates/render` — `collect_frames` / `render_url_inner` / `MAX_FRAMES_PER_PAGE` / `MAX_FRAME_DEPTH`.

## Impacto (resumen, sin PoC público)
El cap `MAX_FRAMES_PER_PAGE` (=16) se aplica por nivel de `collect_frames` (vector `out` local por llamada), no como presupuesto global por página; la recursión vía `render_url_inner(..., depth+1)` solo topa en `MAX_FRAME_DEPTH` (=4). Una página adversa (local o remota) con iframes anidados autorreferenciales multiplica el nº de renders (1+W+W²+…): con W=4 medido 341 renders desde ~0.3 KiB (≈31.5 s en el binario debug del auditor; ×32 frente al control plano); la proyección aritmética con W=16 es 1+16+256+4096+65536 = 69905 renders (horas/OOM). El depth-cap evita la recursión infinita (termina) pero no la amplificación. Si el shell renderiza contenido no confiable, es DoS remoto no autenticado contra disponibilidad; el OOM agota memoria del proceso host.

## PoC resumido (solo para el borrador privado, no difundir)
Página mínima con un puñado de `<iframe src=self>` anidados (orden de 0.3 KiB) + medición `time render …` comparada contra control plano con iframes vacíos. Detalles completos (fixtures `nest4.html`/`flat4.html`, anchos W) solo en el canal privado tras el triage.

## Evidencia verificada (Mesa A, sin reproducir los 31.5 s)
`EXP01 width=2 elapsed_ms=413` · `width=3 elapsed_ms=1473` (31→121 renders, ×3.6) · `EXP01-FLAT width=2 elapsed_ms=35` (×12 / ×42 vs plano). Código: cap local a cada llamada `collect_frames`, recursión solo limitada por `MAX_FRAME_DEPTH`. Proyección W=16 = 69905 renders: aritmética confirmada del código.

## Fix sugerido
Contador global de frames por página compartido a través de `render_url_inner`/`collect_frames` (análogo a `MAX_TOTAL_IMPORTS`).

## Referencias de proceso
- `SECURITY.md:25` lista "hang, excessive CPU/memory" como comportamiento a reportar en privado; no exige corrupción de memoria.
- Decisión Mesa B: vía privada primero (borrador de advisory sin PoC público); si el triage lo considera endurecimiento ordinario, degradar a issue público de endurecimiento una vez mergeado el fix. Ningún otro hallazgo de esta mesa requiere aviso `security`.

## Dedup
Dedup: new (sin colisión en ledger, GAPS ni abiertas; iframe en GAPS es #1532 "blank", distinto). Mesa A: VERIFIED, P1 proporcionada. Mesa B: SOLO NUEVO, P1 mantener, vía advisory privado.
