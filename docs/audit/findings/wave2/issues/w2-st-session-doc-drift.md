[shell] fallback ~/.megi/session.json documentado es inalcanzable

## Resumen
El doc de `crates/shell/src/session.rs:3-5` promete fallback `~/.local/share/… and then ~/.megi/…`, pero `default_session_path` (`session.rs:522-553`) con `HOME` definido (caso normal) retorna siempre `~/.local/share/…`; `None` solo sin `XDG` ni `HOME`. Ninguna rama `~/.megi` existe en el código.

## Impacto
P3 — docs/contrato falso menor. Fix: implementar fallback o corregir docs (una de las dos).

## Pasos para reproducir
1. Leer `session.rs:1-6` (doc) vs `default_session_path` (`522-553`).
2. Con `HOME` definido, confirmar que el retorno es siempre `~/.local/share/…` y que no hay rama `~/.megi`.

## Esperado vs observado
Esperado: o bien el fallback `~/.megi/session.json` existe, o bien el doc no lo promete.
Observado: doc promete `~/.megi/…`; código sin rama `~/.megi`.

## Evidencia literal
```
doc session.rs:1-6: falling back to ~/.local/share/… and then ~/.megi/…
código default_session_path 522-553: con HOME definido retorna siempre ~/.local/share/…; None sólo sin XDG ni HOME; ninguna rama ~/.megi
```

Commit: 3c06db8

## Dedup
Dedup: new. Mesa A: VERIFIED. Mesa B: NUEVO, P3 ok.
