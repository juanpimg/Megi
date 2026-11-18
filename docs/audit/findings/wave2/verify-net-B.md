# Mesa B — Fiscalía de dedup y severidad F2-net (`crates/net`)

- Fiscal: Mesa B (criterio de juez; no reproduce, juzga).
- Fuentes de dedup consultadas: `docs/audit/FINDINGS.md` (líneas 79–99 confirman
  #1252, #1260, #1262, #1286, #1288, #1290, #1293, #1294, #1297, #1298 con
  títulos coincidentes), `grep` de `websocket|Sec-WebSocket|wss://` en
  `FINDINGS.md` (sin entradas WebSocket: sólo falsos positivos de `mask` en
  svg), informe `f2-net.md` (declaraciones de dedup del auditor).
- No se consultó `gh issue list` (sin red en esta sesión); los 10 dedups se
  aceptan contra el ledger local, que los lista como Deferred/Fixed con el
  mismo título.
- Escala aplicada: P0 = RCE/crash remoto; P1 = crash/hang/DoS/bypass;
  P2 = corrupción visible o contrato roto; P3 = cosmetic/docs.

## Veredictos

| id | veredicto | sev. auditor → fiscal | motivo |
|----|-----------|----------------------|--------|
| w2-net-001 | YA REPORTADO #1252 (comentar con evidencia nueva) | P1 → P1 | El título de #1252 cubre exactamente esta ruta (`Set-Cookie` antes del gate CORS). Evidencia nueva real: el test existente no cubre cookie-en-bloqueado y el jar sí se contamina. Título concreto, fix accionable, spec Fetch/CORS existe. `security`: hereda advisory, no issue público. |
| w2-net-002 | SOLO NUEVO | P1 → P1 | Sin entrada WS en el ledger. `contains(" 101")` + ausencia de `Upgrade`/`Connection` es bypass del gate de handshake (confusión de protocolo). Título concreto, fix barato y accionable, spec RFC 6455 §4.1 existe. `security`. |
| w2-net-003 | SOLO NUEVO | P1 → P1 | Sin entrada WS en el ledger. Bundle de 5 sub-claims verificados por inspección (sin `Origin`, sin mixed-content/CSP/cookies/proxy). El más grave (`ws://` plaintext desde contexto seguro, elude proxy) sostiene P1. Nota a Mesa C: partir en issues atómicos al crear. `security`. |
| w2-net-004 | SOLO NUEVO | P2 → P2 | Nuevo en ledger. Contrato roto (RFC 6455 §5.3 exige impredictibilidad) → P2 por rúbrica. El texto de impacto (envenenar cachés) está sobredimensionado — requiere MITM que ya puede hacer cosas peores — pero la severidad se sostiene como contrato roto; Mesa C debe tonificar el impacto. Título y fix accionables. |
| w2-net-005 | SOLO NUEVO | P2 → P2 | Nuevo en ledger. Hang de 2 s por defecto + error diagnóstico erróneo; un peer malicioso retiene el hilo (DoS blando) → P2. Fix accionable (distinguir header parcial de frame completo inválido). |
| w2-net-006 | SOLO NUEVO | P3 → P3 | Nuevo en ledger. Aceptar RSV sin extensión negociada es violación de contrato de bajo impacto (el peer debe enviarlos; interop). P3 correcta. Fix de 1 línea. |
| w2-net-007 | SOLO NUEVO | P2 → P2 | Nuevo en ledger. Corrupción silenciosa a U+FFFD en vez de fallo 1007 → corrupción visible / contrato roto → P2. Título concreto, spec RFC 6455 §5.6 existe. |
| w2-net-008 | SOLO NUEVO | P3 → P3 | Nuevo en ledger. Enviar close reservado es malformación de bajo impacto (el peer aborta la sesión como mucho). Validador ya existe y sólo hay que cablearlo. P3 correcta. |
| w2-net-009 | SOLO NUEVO | P3 → P3 | Nuevo en ledger. Close de 1 byte aceptado; impacto mínimo (truncación silenciosa). P3 correcta. Fix accionable. |
| w2-net-010 | SOLO NUEVO | P3 → P3 | Nuevo en ledger. Acoplamiento write=read sin campo propio; el "fix" admite resolución sólo-docs. P3 correcta. Título concreto, sin spec externa (es contrato-vs-documentación propia). |
| w2-net-011 | SOLO NUEVO | P2 → P2 | Nuevo en ledger. Reintento Basic silenciosamente omitido (401 pese a tener credenciales) → contrato roto con efecto funcional visible → P2. Fix (normalizar ambos lados) accionable. |
| w2-net-012 | SOLO NUEVO | P3 → P3 | Nuevo en ledger. Nonce predecible tiempo+pid+contador; explotabilidad remota no demostrada (observador local) → P3. Nota del auditor aceptada: si el equipo lo considera explotable, tratar como advisory. |
| w2-net-013 | SOLO NUEVO | P3 → P3 | Nuevo en ledger. Variante muerta + GUID duplicado; deuda/higiene, falsa sensación de seguridad. P3 correcta. |
| w2-net-014 | SOLO NUEVO | P3 → P3 | Nuevo en ledger. Bundle de 3 sub-claims (sin reensamblado, sin eco de Close). Fragmentos entregados como mensajes completos es contrato roto, pero sin corrupción demostrada en el flujo actual → P3 sostenida por el límite bajo del impacto. Nota a Mesa C: partir al crear issues. |
| w2-net-d01 | YA REPORTADO #1252 (comentar con evidencia nueva) | — (hereda #1252) | El título de #1252 incluye literalmente "PUSH_PROMISE URLs are unvalidated". Evidencia nueva: localización precisa `:1442-1462` y coexistencia con 001 (jar + ocupación de `pushed_streams`). `security` por herencia. |
| w2-net-d02 | YA REPORTADO #1262 | — | Título idéntico ("streaming … before the CORS decision"). Evidencia nueva: líneas y doble ruta (normal + pooled). |
| w2-net-d03 | YA REPORTADO #1260 | — | Título idéntico (h2c raw + reenvío). Evidencia nueva: filtro débil enumerado vs validador `http.rs:1005`. |
| w2-net-d04 | YA REPORTADO #1286 | — | Título idéntico (closed stream → connection error). Evidencia nueva: línea `:1193-1198`. |
| w2-net-d05 | YA REPORTADO #1288 | — | Título idéntico (304 pela policy). Evidencia nueva: líneas `:884-891` + mecanismo (`updated_entry`). |
| w2-net-d06 | YA REPORTADO #1290 | — | Título idéntico (sin deadline acumulada). Evidencia nueva: `grep deadline` vacío + alcance al loop WS. |
| w2-net-d07 | YA REPORTADO #1293 | — | Título idéntico (pooled sin validación outbound ni 407). Evidencia nueva: ausencia confirmada en rango pooled + contraste con `:1729-1780`. |
| w2-net-d08 | YA REPORTADO #1294 | — | Título idéntico (`on_data` hunde el batch). Evidencia nueva: líneas `:782-795`. |
| w2-net-d09 | YA REPORTADO #1297 | — | Título idéntico (docs "sólo http GETs" vs https al pool). Evidencia nueva: `:431-432` vs `:515-520`. P3/docs por naturaleza. |
| w2-net-d10 | YA REPORTADO #1298 | — | Título idéntico (docs h2 rancias). Evidencia nueva: `lib.rs:27-32` vs multiplex/push/flow-control reales. P3/docs por naturaleza. |

## Resumen para Mesa C

- SOLO NUEVO: 13 (002–014 salvo 001). DUPLICADO puro (descartar como issue
  nuevo): 0 — los 11 restantes son YA REPORTADO con evidencia nueva, no
  duplicados descartables (001, d01–d10 van como comentarios a sus issues).
- REBAJAR A P3: 0. REQUIERE MÁS EVIDENCIA: 0.
- Severidades confirmadas: P1 → 001, 002, 003; P2 → 004, 005, 007, 011;
  P3 → 006, 008, 009, 010, 012, 013, 014.
- `security` (no publicar; advisory privado): 001, 002, 003, d01 (hereda #1252).
- Notas de calidad: tonificar el impacto de 004 (caché-poisoning sobredimensionado);
  partir 003 y 014 en issues atómicos; 010 admite resolución sólo-docs.
