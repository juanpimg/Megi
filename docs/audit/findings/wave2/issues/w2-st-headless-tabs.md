[shell] --tabs se ignora en headless --screenshot (solo se renderiza el input posicional)

## Resumen
La rama `Screenshot` de `crates/shell/src/main.rs:75-113` no referencia `args.tabs`/`view.tabs` (construido en líneas 61-66 y abandonado). Los flags `--tabs` se aceptan sin efecto ni aviso; `lib.rs:108` ("headless renders the active (first) tab") sugiere lo contrario.

## Impacto
P2 — contrato roto: flags aceptados sin efecto ni aviso. Espejo headless de #1169.

## Pasos para reproducir
1. `cargo build -p shell` (worktree limpio HEAD `3c06db8`).
2. `megi "data:text/html,<h1>AAA</h1>" --tabs B --screenshot t1.png`
3. `megi "data:text/html,<h1>AAA</h1>" --tabs CCC-different --screenshot t2.png`
4. `md5sum t1.png t2.png` → idénticos.

## Esperado vs observado
Esperado: renderizar el primer tab / avisar `--tabs` sin efecto en headless (o `Usage`).
Observado: ambos renders `wrote … (800x600, 14344 bytes)`, md5 idénticos; `--tabs` muerto en la rama.

## Evidencia literal
```
wrote t1.png (800x600, 14344 bytes…)
wrote t2.png (800x600, 14344 bytes…)
29c7df93e9775d51d46db8c170d1ad32  t1.png
29c7df93e9775d51d46db8c170d1ad32  t2.png
```
Código: rama `Screenshot` (`main.rs:75-113`) sin referencia a `args.tabs`/`view.tabs`.

Commit: 3c06db8

## Dedup
Dedup: new (espejo headless de #1169; no hay issue previo headless). Mesa A: VERIFIED. Mesa B: NUEVO, P2 ok.
