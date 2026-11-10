#!/usr/bin/env bash
# Convenience wrapper for the Megi shell (Phase 7).
#
# Usage:
#   tools/megi.sh <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
#
# Window mode (needs an X11/Wayland session):
#   - mouse wheel / touchpad: vertical scroll
#   - arrows: one line; PageUp/PageDown: one page; Home/End: top/bottom
#   - r: re-render; q or Esc: quit; left click: print the current URL
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
