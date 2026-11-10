# Render pipeline fixtures

Self-contained documents for the `crates/render` integration tests. No network, no
JavaScript, no images.

| File | Purpose |
|---|---|
| `basic.html` | `<base href="sub/">` + external `theme.css` (green canvas) + a `media="print"` sheet that must be ignored + an inline `<style>`. |
| `sub/theme.css` | Screen stylesheet applied through the `<base>` URL (green `html` background). |
| `sub/print-only.css` | Print-only sheet (`media="print"`); must never be applied. |

The tests assert that the green canvas comes from `sub/theme.css` (resolved through
`<base>`) and not from the print sheet, which proves both the `<base>` handling and the
media screen filter.
