# Mesa C — reunión de verificación (scope render, `w2-render-*`)

- Fecha: 2026-11-18. Chair: Mesa C (scope render).
- Commit verificado: `3c06db8` (worktree limpio `/tmp/opencode/megi-verify`; HEAD confirmado `git rev-parse` por Mesa A).
- Fuentes: informe `docs/audit/findings/wave2/f2-render.md` (13 hallazgos), `docs/audit/findings/wave2/verify-render-A.md` (reproductor independiente, binario `render` construido en `3c06db8` salvo w2-render-11), `docs/audit/findings/wave2/verify-render-B.md` (fiscal dedup/severidad contra FINDINGS/GAPS/`gh issue list`, HEAD código).
- Env Mesa A: `TMPDIR=/home/jp/.cache/megi-tmp CARGO_TARGET_DIR=/home/jp/.cache/f2-target`, repros en crate auxiliar `/home/jp/.cache/f2-repro` (sin modificar) + sondas propias `/home/jp/.cache/w2renderA/`.
- Regla include de esta ola: A=VERIFIED (+estático) y B=SOLO NUEVO, excluyendo (a) P1 DoS vía advisory privado (SECURITY.md), (b) B=YA REPORTADO → comment-existing, (c) A≠VERIFIED pleno (CONVERGENTE/NEEDS INFO/PARCIAL con oráculo roto), (d) código que no existe en `3c06db8`.
- No se editó código ni ledger. No se publicó nada.
- Formato según `meeting-net.md`.

## Tabla final

| id | A | B | sev | área | etiquetas | incl/excl | motivo / cuerpo |
|---|---|---|---|---|---|---|---|
| w2-render-01 | VERIFIED | SOLO NUEVO (vía advisory privado) | P1 | frames / DoS | security, advisory-privado | exclude (advisory) | Cap por nivel, no global; 341 renders desde ~0.3 KiB, proyección W=16 ≈70k. Borrador en `docs/audit/findings/wave2/advisories/w2-render-01.md`. Nunca issue público (SECURITY.md:25 hang/CPU/memoria en privado). |
| w2-render-02 | VERIFIED | SOLO NUEVO | P2 | build | public, build | include | `--no-default-features` roto (`ws_bridge.rs:8-9` + `lib.rs:215`). Cuerpo en `issues/w2-render-02.md`. |
| w2-render-03 | VERIFIED | YA REPORTADO #1299 | P2 | scripts budget | public, budget | exclude (comment-existing) | Presupuesto cobrado por adelantado, ≤10 scripts por defecto. Ver comentario #1299 abajo; no abrir issue. |
| w2-render-04 | VERIFIED | SOLO NUEVO | P2 | svg/fonts | public, determinismo | include | Ruta SVG directa usa system fonts (2 vs 1106 caras, digests distintos). Cuerpo en `issues/w2-render-04.md`. |
| w2-render-05 | CONVERGENTE | YA REPORTADO #1304 | P2 | scripts módulos | public, latente | exclude (comment-existing) | `contains("await")` existe pero converge en 3-4 sondas; riesgo latente. Ver comentario #1304 abajo; no abrir issue. |
| w2-render-06 | PARCIAL (mecanismo sí / cuantificación y oráculo no) | SOLO NUEVO (rel. #1534) | P2 | ws drenaje | — | exclude (oráculo roto) | `sleep 5ms×20` verificado en código pero oráculo `document.title`↔`<title>` roto (A: `DOC-TITLE:hello` / `TITLE-EL:init`) y cifra 2 s no reproducida (0.33–0.43 s). No publicar como issue nuevo en esta ola; posible hallazgo title↔elemento es ámbito js/host, fuera de esta mesa. |
| w2-render-07 | VERIFIED | YA REPORTADO #1305 | P3 | stats | public, stats | exclude (comment-existing) | `external_stylesheets` cuenta intentos. Ver comentario #1305 abajo. |
| w2-render-08 | VERIFIED (estático) | YA REPORTADO #1306 | P3 | error taxonomía | public, errores | exclude (comment-existing) | PNG vacío → `LayoutFailed` (dos sitios). Ver comentario #1306 abajo. |
| w2-render-09 | VERIFIED | YA REPORTADO #1220 | P3 | inline-svg | public, dom | exclude (comment-existing) | Fast-path salta reemplazo si `executed==0` tras mutar DOM. Ver comentario #1220 abajo. |
| w2-render-10 | VERIFIED | SOLO NUEVO | P3 | images budget | public, telemetría | include | `break` sin `failed++`. Cuerpo en `issues/w2-render-10.md`. |
| w2-render-11 | VERIFIED (fuera de 3c06db8) | SOLO NUEVO (vinc. #1544) | P3 | contentDocument | — | exclude (fuera de ola) | No existe en `3c06db8` (grep `contentDocument`/`frame_documents` vacío en `crates/`); verificado con binario HEAD `d82118c`. Válido para HEAD, no para el commit de esta ola → no entra aquí. |
| w2-render-12 | VERIFIED | YA REPORTADO #1301 | P3 | caps | public, caps | exclude (comment-existing) | Cap 1024 silencioso. Ver comentario #1301 abajo. |
| w2-render-13 | NEEDS INFO | REQUIERE MÁS EVIDENCIA | P3 plausible | ws bridge | — | exclude (sin repro) | Solo lectura (`from_utf8_lossy`, `is_open` ignora cierre remoto); sin sonda ejecutable vía CLI. Devolver para test unitario; no entra al backlog aún. |

Resultado: 3 include (cuerpos en `docs/audit/findings/wave2/issues/w2-render-{02,04,10}.md`), 1 advisory privado (`docs/audit/findings/wave2/advisories/w2-render-01.md`), 6 comment-existing, 4 excludes técnicos (05 también cuenta como comment-existing; 06, 11, 13 sin issue en esta ola).

## Avisos privados (1, sin issue público)

- w2-render-01 (P1): cap de iframes por nivel permite amplificación de renders (DoS). Borrador en `docs/audit/findings/wave2/advisories/w2-render-01.md` (impacto + PoC resumido + nombre propuesto, sin pasos explotables detallados). No publicar fixtures `nest4.html`/`flat4.html` en abierto antes del fix.

## Incluidos como issue público (3)

w2-render-02, w2-render-04, w2-render-10. Cuerpos en `docs/audit/findings/wave2/issues/<id>.md` (primera línea `[render] ...`, resumen, impacto, pasos, esperado/observado, evidencia literal, `Commit: 3c06db8`, `Dedup:`).

## comment-existing (pegar en issues existentes, no abrir nuevos)

### #1299 ← w2-render-03
```
Evidencia nueva wave2 (HEAD 3c06db8, f2-repro exp03 + lectura scripts.rs:1284-1300):
EXP03 executed=10 failed=0 limited=true
EXP03 warn: total JavaScript loop-iteration budget exhausted; skipping remaining scripts
Causa: remaining_loops -= grant sin reembolso (total 1_000_000 / grant 100_000 = 10 scripts; el 11º recibe grant == 0 → break; max_scripts=64 inalcanzable por defecto). Página con 12 scripts triviales → solo 10 ejecutados. Mesa A: VERIFIED.
```

### #1304 ← w2-render-05
```
Evidencia nueva wave2 (HEAD 3c06db8, f2-repro exp05 + CLI --json, scripts.rs:1310):
EXP05 await_comment=false executed=1 tasks=1 virt_ms=50 limited=false r="done" · idéntica con await_comment=true
PLAIN … failed: SyntaxError … at line 4 vs AWAIT … at line 2 (mismo mensaje, solo nº de línea desplazado por la línea de comentario)
Matiz Mesa A (CONVERGENTE): la rama if script.text.contains("await") existe y se toma, pero converge en comportamiento en las 3 sondas (timer 50 ms + sintaxis). El diagnóstico a nivel de código sigue válido (riesgo latente + maquinaria async innecesaria). Mesa B: YA REPORTADO, P2 mantener.
```

### #1305 ← w2-render-07
```
Evidencia nueva wave2 (HEAD 3c06db8, render exp07.html con <link> a missing-xyz.css):
"external_stylesheets":1 · warnings:["skipping stylesheet file:///home/jp/.cache/w2renderA/missing-xyz.css: file not found: …"]
Causa: summary.external_stylesheets += 1 (lib.rs:1981, informe :1995) antes del load_stylesheet_text que puede fallar. Fix: mover += 1 tras carga exitosa. Mesa A: VERIFIED.
```

### #1306 ← w2-render-08
```
Evidencia nueva wave2 (HEAD 3c06db8, lectura literal + grep):
1308: return Err(RenderError::LayoutFailed); tras if png.is_empty() (ruta HTML) · mismo patrón en :1526 (ruta SVG directa) · LayoutFailed => write!(f, "layout produced no root box")
Sin variante PngEncode; la más cercana (Output(io::Error)) es solo escritura de fichero CLI. Sin repro dinámico (forzar to_png() vacío no es observable vía CLI). Nota: el auditor solo reclama la mitad "variante errónea" (la de variantes muertas puede estar ya resuelta — confirmar ambas mitades al tocar el issue). Mesa A: VERIFIED (estático). Mesa B: YA REPORTADO.
```

### #1220 ← w2-render-09
```
Evidencia nueva wave2 (HEAD 3c06db8, f2-repro exp09 + lectura lib.rs:1121 / :1134-1135):
EXP09 executed=0 failed=1 inline_svgs=0 dom_mutated=1 svg_nodes_left=1
Causa: guarda scripts_executed == 0 && !source_mentions_svg no exige failed == 0; el script insertó <svg> vía '<s'+'vg…' (fuente sin literal <svg, verificado grep -c '<svg' = 0) y lanzó después. Control con literal <svg en fuente: inline_svgs=1 (el mecanismo funciona; solo el fast-path lo salta). Fix: exigir también failed == 0. Mesa A: VERIFIED.
```

### #1301 ← w2-render-12
```
Evidencia nueva wave2 (HEAD 3c06db8, f2-repro exp12 + lectura images.rs:519-521):
EXP12 loaded=1024 failed=0 warnings=0 (con 1100 <img>, límites elevados para aislar el cap)
Causa: if sources.len() >= 1024 { break; } sin warning, frente al resto de caps que avisan. Fix: warning como el resto de caps. Mesa A: VERIFIED.
```

## Descartes / excludes técnicos

- w2-render-05 (CONVERGENTE): excluido como issue nuevo por convergencia conductual en sondas; se deja comment-existing en #1304 con el matiz. Sin cambio de severidad (P2).
- w2-render-06 (PARCIAL, oráculo roto): excluido como issue nuevo en esta ola. Mecanismo (`sleep 5 ms × 20`, `scripts.rs:1704-1714`) confirmado en código; overhead medido por Mesa A ~0.33–0.43 s (no 2 s del informe, plausiblemente ambiental por tmpfs lleno) y oráculo del repro refutado (`document.title = e.data` NO sincroniza el elemento `<title>`: `DOC-TITLE:hello` pero `TITLE-EL:init`; la entrega a 95 ms SÍ funciona). El corte de inclusión existe (mensaje a 2000 ms perdido) pero su posición la decide el reloj real. La desincronización `document.title`↔elemento `<title>` es en sí un posible bug de contrato de ámbito js/host (fuera de esta mesa; Mesa A Concern C1) y merece hallazgo propio en la mesa correspondiente, no aquí. No se publica issue w2-render-06 en scope render en esta ola.
- w2-render-11: excluido de esta ola. En `3c06db8` la feature no existe (`grep -rn "contentDocument\|frame_documents" crates/` vacío; solo existe en árbol principal vía e87f206/runtime.rs + scripts.rs). Verificado por Mesa A con binario HEAD `d82118c` (SRC `RESULT:NULL` vs SRCDOC `RESULT:from-srcdoc`). Válido para HEAD, no para el commit base → fuera del alcance de esta ola.
- w2-render-13 (NEEDS INFO): excluido del backlog hasta repro unitario (`is_open` tras `Close`, roundtrip binario no-UTF-8). Lectura verosímil (`ws_bridge.rs:53-57`, `:97-99`) pero sin sonda ejecutable vía CLI, según el propio informe. Relacionado con #1534 OPEN como contexto.
