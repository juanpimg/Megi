[net] encode_frame siempre pone MASK aunque el frame diga masked=false

## Resumen
`encode_frame` (`websocket.rs:284-299`) pone el bit MASK (`0x80 | len`) siempre,
ignorando `frame.masked`. No hay forma de serializar un frame de servidor (sin
máscara); el propio test lo admite construyendo el vector a mano (`:509-513`).

## Impacto
P3 — asimetría encode/decode: el codificador no sirve para tests de servidor ni para
re-emitir frames recibidos; trampa para futuros usos.

## Pasos para reproducir
1. `encode_frame(&Frame { fin:true, opcode:Text, masked:false, payload:b"Hi" }, [0;4])`
   → segundo byte `0x82` (MASK puesto) en vez de `0x02`.

## Esperado vs observado
Esperado: respetar `frame.masked` (sin MASK ni clave si `false`).
Observado: MASK siempre; comentario en `:284` lo declara pero el flag miente.

## Evidencia literal
Código: `websocket.rs:290-299` (tres ramas, todas `0x80 | ...`); test `:509-513`
("encode_frame always masks; build the unmasked server form manually").

Commit: 3c06db8

## Workaround
Construir frames de servidor manualmente como hace el test.

## Dedup
Dedup: new (rel. #1534 abierta). Mesa A: VERIFIED (código). Mesa B: SOLO NUEVO, P3 mantener.
