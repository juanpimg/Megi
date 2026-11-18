# Verificación Mesa A — browser (`w2-browser`, informe F5)

Verificador independiente. No se editó código de producto ni ledger.
Solo se crea este fichero.

- Worktree de verificación: `/tmp/opencode/megi-verify`, HEAD
  `3c06db8c186e4f7a19c7816aed4955a0b68f91ef` (`git status`: solo 2
  untracked ajenos `tests/fixtures/render/f2-exp10.html`,
  `f2-exp12.html`, resto limpio).
- `CARGO_TARGET_DIR=/home/jp/.cache/megi-verify-target` (build release
  propia de esta mesa; el target original del auditor ya no se reutiliza).
- AVISO DE PROCEDENCIA: entre la lectura inicial y la ejecución, los
  repros originales (`docs/audit/repros/browser/*`, untracked en el árbol
  principal) y el propio `f5-break-browser.md` desaparecieron del árbol
  (otra sesión los eliminó/movió; `find` no los localiza en todo el repo).
  Los 4 repros usados aquí son reconstrucciones byte-fieles del contenido
  leído íntegro por esta mesa antes del borrado, guardadas en
  `/tmp/opencode/browser-repro/` (fuera del repo). El veredicto se basa en
  esas copias + el texto del informe (conservado en contexto).
- Comando base (×3 cada repro, `timeout 60s`, CWD `/tmp/opencode/megi-verify`):
  `cargo run -q --release -p megi-browser -- <repro> --screenshot /tmp/v-<id>-<i>.png --save-html /tmp/v-<id>-<i>.html --no-session`

| id | veredicto | comando | salida literal (recortada) | nota |
|----|-----------|---------|----------------------------|------|
| w2-browser-01 | VERIFIED | `… -- gbr-mutate.html --screenshot … --save-html … --no-session` (×3, `exit=0`) | `<p id="r">{"x":8,"y":8,"width":100,"height":50,…}\|{"x":8,"y":8,"width":100,"height":50,…}` (r1=r2, idéntico ×3) | Stale confirmado: tras `el.style.width="300px"` el 2.º rect sigue en `width:100` (esperado 300). Respaldo estático: `crates/render/src/scripts.rs:1040-1043` admite proveedor = snapshot pre-script. |
| w2-browser-02 | VERIFIED | `… -- title-roundtrip.html …` (×3, `exit=0`) | `<p id="out">CHANGED123` pero `<title>orig` (idéntico ×3) | Desync confirmado: el getter devuelve el valor seteado pero el DOM conserva `<title>orig`. Respaldo estático: setter solo escribe `host.title` (`runtime.rs:3785-3793`); ventana lee el elemento `<title>` (`shell/src/title.rs`); `host.title` sin más lectores. |
| w2-browser-03 | VERIFIED | `… -- worker-tamper.html …` (×3, `exit=0`) | `<p id="done">worker-err:TypeError: JavaScript error: worker script file:///tmp/opencode/browser-repro/data:text/javascript,postMessage(1): file not found: /tmp/opencode/browser-repro/data:text/javascript,postMessage(1)` (idéntico ×3) | Filtración de ruta absoluta confirmada (misma forma que el informe; el path visible es el de mi copia en `/tmp` porque el repro original ya no existe). Mecanismo: `load_worker_source` (`scripts.rs:754-775`) incrusta la URL `file://` resuelta + `FileNotFound(path)` con path absoluto (`net/src/error.rs:189`), y `worker_constructor` (`runtime.rs:8022`) lo vuelca al script vía `js_error`. |
| w2-browser-04 | EXCLUDE | `… -- iframe-contentdoc.html …` (×3, `exit=0`) | Sin `<p id=out>` en `--save-html`; log: `warning: script inline-script-0 failed: JavaScript error: TypeError: cannot convert 'null' or 'undefined' to object (unknown at :3:49)` | No aplica a HEAD: `grep contentDocument crates/js/src/runtime.rs` → 0 coincidencias; `contentDocument` es `undefined`, el script muere antes del `qSA`. Es WIP sin commitear del usuario, tal como el propio informe marca (`excluir: WIP del usuario`). Confirmado como EXCLUDE. |

## Concerns

1. Los repros y el informe F5 fueron borrados del árbol principal a mitad de
   esta verificación por otra sesión (ya existen `verify-browser-B.md` y
   `meeting-browser.md`). Riesgo de que la Mesa C decida sobre ficheros que ya
   no existen; esta mesa deja constancia de la reconstrucción.
2. `w2-browser-03`: con `file://`, cualquier página local que instancie un
   Worker con URL inexistente recibe el layout absoluto de disco en el mensaje
   de error. Menor (P3, no remoto), pero el saneamiento (`basename`/URL
   original) es trivial.
