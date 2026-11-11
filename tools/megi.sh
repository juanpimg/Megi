#!/usr/bin/env bash
# Convenience wrapper for the Megi shell (Phase 7).
#
# Usage:
#   tools/megi.sh <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
#
# Window mode (needs an X11/Wayland session):
#   - left click on a link: navigate (resolved against the page URL, history push)
#   - Alt+Left / Backspace / b: back; Alt+Right / n: forward; r: reload
#   - Ctrl+= / Ctrl++ (or =/+): zoom in; Ctrl+- (or -): zoom out; Ctrl+0 (or 0): reset
#   - Ctrl+L: address bar with full editing (Home/End, Shift-selection, Ctrl+A/C/X/V internal
#     clipboard, Ctrl+Backspace, history autocomplete with Tab; URL selected on open)
#   - tab strip: click/wheel cycles tabs, +N opens a real overflow menu, loading dot per tab;
#     address bar (Enter navigates via Url::join, Esc cancels); tab strip clickable
#     (activate/close/+), select controls open an option dropdown; --chrome for screenshots
#   - Ctrl+F: find in page (yellow highlights, Enter next, Shift+Enter previous, Esc close);
#     --find QUERY does the same for headless --screenshot output
#   - F5 / g / r reload, Alt+Home first history entry, Ctrl+D prints URL + title
#   - Ctrl+S writes the render-produced PNG bytes directly (atomic, no re-encode);
#     headless --save-png shares --screenshot bytes
#   - session: tabs/history/zoom/scroll persist in $XDG_DATA_HOME/megi/session.json
#     (--no-session, --session-file PATH); Ctrl+S saves PNG, Ctrl+Shift+S saves serialized
#     HTML (doctype) to --download-dir or ~/Downloads; headless: --save-png/--save-html PATH
#   - history: Ctrl+H opens the last-10 menu (arrows/Enter/Esc); chrome back/forward/reload
#     buttons (disabled per history); about:history/megi://history renders the internal
#     history page (headless --chrome deterministic); recent picks persist in session.json
#     unless --no-session
#   - links: hover shows the URL in a bottom status bar; Ctrl+click / middle click opens the
#     link in a new tab; right click opens a context menu (open/copy link, save image,
#     reload, save page PNG/HTML; arrows/Enter/Esc); --chrome --chrome-menu for headless
#   - file:// directories render a generated listing (name/type/size, breadcrumb, safe links);
#     load failures (DNS/HTTP/TLS/file) render a deterministic error page with a Reintentar
#     link (press r to reload); tabs show the page favicon (16x16 nearest decode) or a
#     deterministic per-origin dot fallback
#   - tabs: Ctrl+T new, Ctrl+W close (last tab -> New Tab), Ctrl+Tab / Ctrl+Shift+Tab
#     cycle, Alt+1..9 activate; --tabs URL repeatable (headless renders the first/active tab)
#   - Ctrl+L: print the current URL; title shows page — zoom% — scroll% (+warnings)
#   - form controls: click an input/textarea to focus and type (Backspace, Tab); click
#     checkbox/radio to toggle; buttons flash pressed; Enter never submits (no backend)
#   - --zoom F (0.5–3.0): initial page zoom; --refresh-ms [MS]: auto-refresh pages whose
#     scripts leave timers pending (default 250 ms; omitted/0 = disabled)
#   - mouse wheel / touchpad: vertical scroll
#   - arrows: one line; PageUp/PageDown: one page; Home/End: top/bottom
#   - q or Esc: quit; right/middle click: ignored
#   - window title: document <title> + short URL + scroll percentage
#   - HiDPI: the CSS viewport is physical/scale and the frame is upscaled
#   - renders run on a worker thread with generation-based cancellation
#
# Headless mode (CI-friendly, no display server):
#   tools/megi.sh tests/fixtures/render/basic.html \
#       --screenshot /tmp/s.png --width 640 --height 400
#   python3 tools/png_check.py /tmp/s.png
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Forward every argument to the `megi` binary. MEGI_RELEASE=1 builds/uses the
# release profile (slower first build, much faster window renders).
if [[ "${MEGI_RELEASE:-0}" == "1" ]]; then
    exec cargo run --release -q -p shell -- "$@"
else
    exec cargo run -q -p shell -- "$@"
fi
