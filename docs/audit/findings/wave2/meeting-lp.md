# Mesa C — chair · F2 layout+paint (w2-lp-01…20)

- Fecha: 2026-11-18. Chair: Mesa C (scope layout/paint).
- Entradas: `docs/audit/findings/wave2/verify-lp-B.md` (fiscal dedup/severidad).
- Faltantes: informe original `docs/audit/findings/wave2/f2-layout-paint.md` **no existe**
  en el árbol y `docs/audit/findings/wave2/verify-lp-A.md` **no existe**
  (sólo existe `verify-js-A.md` de otro scope). No hay veredictos de Mesa A
  disponibles para este scope.
- HEAD de referencia para la evidencia: `3c06db8` (baseline wave 2).
  HEAD actual del árbol: `e87f2068`. No se editó código ni ledger.
- Regla aplicada (verifier-template): `include` exige A = VERIFIED/PARTIAL y
  B ≠ DUPLICADO; empate o duda → `exclude`. Sin mesa A, **cero `include`
  como issue nuevo**. Los 12 vivos van a `comment-existing` (comentar
  evidencia fresca en issues existentes, nunca abrir nuevos — coincide con
  la concern #1 de Mesa B y con R5 del plan: un deferred no se rearchiva).

## Tabla final

| id | veredicto consensuado | sev. | área | etiquetas | include/exclude | motivo |
|---|---|---|---|---|---|---|
| w2-lp-01 | YA REPORTADO #1183 | P2 | paint | bug, P2 | comment-existing | B: título idéntico, fix accionable. Sin A → no nuevo issue; comentar evidencia HEAD `3c06db8`. |
| w2-lp-02 | YA REPORTADO #1185 | P2 (upgrade P3→P2 propuesto) | paint | bug | comment-existing | B: sólo `none` sobredimensionado roto; upgrade defendible, decide chair → se acepta P2 en el comentario, sin mover etiqueta sin consenso. |
| w2-lp-03 | YA REPORTADO #1184 | P2 con reserva | paint | bug | comment-existing | B: sin repro de píxeles (sólo lectura); P2 sólo si hay efecto visible. El comentario lo marca como reserva probatoria. |
| w2-lp-04 | YA REPORTADO #1192 | P2 (upgrade P3→P2 propuesto) | paint | bug | comment-existing | B: repro de píxeles en HEAD, fix accionable. Upgrade defendible; se propone en el comentario. |
| w2-lp-05 | YA REPORTADO #1191 | P3 | paint | bug | comment-existing | Título concreto, P3 correcta. |
| w2-lp-06 | YA REPORTADO #1179 | P2 (rebaja P1→P2 propuesta) | paint | bug | comment-existing | B apoya la rebaja del informe (skip silencioso, sin crash/hang/DoS). Se propone en el comentario. |
| w2-lp-07 | YA REPORTADO #1196 | P3 (rebaja P1→P3 propuesta) | paint | bug | comment-existing | Path inalcanzable, sin efecto visible hoy. Comentar sólo en #1196; #1211 es dup interno (mismo título) → enlazar/cerrar como dup, no abrir nada. |
| w2-lp-08 | YA REPORTADO #1188 | P3 (P2 defendible) | paint | bug | comment-existing | Sólo docs hostiles (>256 niveles). Chair decide P3 conservador (tracker P3 razonable); se deja constancia de que P2 es defendible. |
| w2-lp-09 | YA REPORTADO #1189 | P3 | paint | bug | comment-existing | Hostile-only. El comentario refleja el mecanismo nuevo (contador propio, la pérdida persiste). |
| w2-lp-10 | YA REPORTADO #1193 | P3 | paint | bug | comment-existing | Igual reserva que w2-lp-03: sin repro de píxeles. P3 correcta. |
| w2-lp-11 | YA REPORTADO #1187 | P3 | paint | bug | comment-existing | Fix de una línea accionable. P3 correcta. |
| w2-lp-12 | YA REPORTADO #107 | P3 | layout | bug | comment-existing | Repro en HEAD, fix de una línea, spec CSS 2.2 §10.3.3 válida. P3 correcta. |
| w2-lp-13 | DUPLICADO #1181 | — | paint | — | exclude | B: informe confirma corrección. **Reserva**: el ledger aún lista #1181 como deferred → resincronizar ledger antes de darlo por cerrado. No reabrir. |
| w2-lp-14 | DUPLICADO #1194 | — | paint | — | exclude | B: confirma cacheo de `u32::MAX`. **Reserva**: ledger aún dice deferred → resinc. No reabrir. |
| w2-lp-15 | DUPLICADO #1415 | — | paint | — | exclude | B: confirma `reserve_shadow`. No reabrir. |
| w2-lp-16 | DUPLICADO #1417 | — | paint | — | exclude | B: confirma cap + tiempos. No reabrir. |
| w2-lp-17 | DUPLICADO #1182 | — | paint | — | exclude | B: test citado pasa. **Reserva**: ledger aún dice deferred → resinc. No reabrir. |
| w2-lp-18 | DUPLICADO #1190 | — | paint | — | exclude | B: sin guard `is_root`. Ledger dice fixed. No reabrir. |
| w2-lp-19 | DUPLICADO #1180 | — | paint | — | exclude | B: sentinel `None`. Ledger dice fixed. No reabrir. |
| w2-lp-20 | DUPLICADO #1186 | — | paint | — | exclude | B: `is_clipped` en `blend_pixel`. Ledger dice fixed. No reabrir. |

Conteos: 20 hallazgos · 0 include-nuevo · 12 comment-existing · 8 exclude ·
0 security · 0 genuinamente nuevos (B: cero novedad; A ausente, sin contradicción).

## comment-existing (texto literal a publicar por issue cerrado)

> Nota del chair: cada comentario indica evidencia nueva en HEAD `3c06db8`.
> Los veredictos de Mesa A para este scope no existen (`verify-lp-A.md`
> ausente); la afirmación "sigue presente en HEAD" proviene del re-audit
> wave 2 + fiscal B. Si Mesa A discrepa al aparecer, estos comentarios
> deberán corregirse antes de publicarse.

### #1183 (w2-lp-01)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 — background-clip:
padding-box / content-box is still ignored for the background colour. Fix
direction unchanged (clip_rect + PushClip at the cited lines). Keeping P2.
Full fiscal review: docs/audit/findings/wave2/verify-lp-B.md (w2-lp-01).
```

### #1185 (w2-lp-02)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 — object-fit: none
still paints outside the content box, unclipped. Scope note: only oversized
`none` is broken (cover already clips; contain/scale-down do not overflow).
Proposing P3→P2 (visible invasion of neighbouring layout). Full fiscal
review: docs/audit/findings/wave2/verify-lp-B.md (w2-lp-02).
```

### #1184 (w2-lp-03)

```text
Wave-2 re-audit (layout/paint): reviewed on HEAD 3c06db8 — blur_rgba still
blurs straight-alpha channels (dark fringes). Evidentiary reserve: no pixel
repro, code reading only, so P2 holds only if a visible effect is confirmed;
otherwise treat as P3. Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-03).
```

### #1192 (w2-lp-04)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 with a pixel
repro — inset box-shadow is still clipped to the border box instead of the
padding box. Fix direction unchanged (deflate by border_width). Proposing
P3→P2 (shadow painted where it must not be). Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-04).
```

### #1191 (w2-lp-05)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 —
dashed/dotted borders still get solid mitred corner squares. P3 stands;
fix-or-document (display_list.rs) direction unchanged. Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-05).
```

### #1179 (w2-lp-06)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 — rounded
rects/borders/gradients are still skipped when the unclipped area exceeds
16.7 Mpx. No crash/hang/DoS, only silent missing paint, so proposing
P1→P2; fix direction unchanged (fallback to fill_rect). Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-06).
```

### #1196 (w2-lp-07)

```text
Wave-2 re-audit (layout/paint): reviewed on HEAD 3c06db8 — the alpha_scale
flat-opacity path is still unreachable (no visible effect today; latent
trap only). Proposing P1→P3. Note: #1211 has the identical title and looks
like an internal duplicate of this issue — suggest linking/closing it as
dup rather than tracking separately. Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-07).
```

### #1188 (w2-lp-08)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 — the
stacking-depth guard still drops nested contexts instead of flattening
them (hostile-only, >256 levels). Chair keeps P3 (tracker's P3 reasonable);
P2 is defensible (invisible subtrees) but not adopted without consensus.
Full fiscal review: docs/audit/findings/wave2/verify-lp-B.md (w2-lp-08).
```

### #1189 (w2-lp-09)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 — float nesting
is still limited by a stacking-context depth counter (hostile-only). Update
vs the original report: the counter is now per-context, but the loss
persists; the remaining action is to document the divergence. P3 stands.
Full fiscal review: docs/audit/findings/wave2/verify-lp-B.md (w2-lp-09).
```

### #1193 (w2-lp-10)

```text
Wave-2 re-audit (layout/paint): reviewed on HEAD 3c06db8 — image scaling
still interpolates straight alpha, contradicting its doc comment.
Evidentiary reserve: no pixel repro, code reading only. P3 stands; fix
direction unchanged. Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-10).
```

### #1187 (w2-lp-11)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 — the
background-clip:text collector budget is still process-global and reset by
every pass. One-line fix direction unchanged (per-pass counter in
PaintResources). P3 stands. Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-11).
```

### #107 (w2-lp-12)

```text
Wave-2 re-audit (layout/paint): re-verified on HEAD 3c06db8 — negative
underflow with both margins auto still snaps to 0 instead of splitting
equally (CSS 2.2 §10.3.3 citation confirmed valid). One-line fix direction
unchanged. P3 stands. Full fiscal review:
docs/audit/findings/wave2/verify-lp-B.md (w2-lp-12).
```

## security

Ninguno. Los 20 hallazgos son layout/paint visibles o latentes sin vector
explotable; Mesa B no marca ninguno como `security`. Sin aviso privado.

## Descartes (exclude con motivo)

- w2-lp-13…20 → exclude per Mesa B DUPLICADO (ya corregidos, no reabrir).
- w2-lp-13 (#1181), w2-lp-14 (#1194), w2-lp-17 (#1182): **discrepancia
  ledger** — `docs/audit/FINDINGS.md` aún los lista como deferred
  (líneas 31, 32, 42) mientras B los da por fixed según el informe.
  Acción requerida: resincronizar el ledger (`audit_recover_issues.py`)
  antes de darlos por cerrados; mientras tanto, no reabrir y no publicar.
- Cero hallazgos genuinamente nuevos: B dictamina "ningún SOLO NUEVO" y,
  sin Mesa A, no hay base para crear `issues/w2-lp-*.md`. No se creó
  ningún cuerpo de issue nuevo.
