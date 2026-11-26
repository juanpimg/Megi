#!/usr/bin/env bash
# Best-effort capture of the real composited window using spectacle.
# A compositor that refuses background capture is NOT an error: print the
# reason and exit 0, because no real image and no measurement is claimed.
#
# Usage: tools/capture_real_window.sh [output.png]
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$REPO_ROOT/target/ui-captures/real-window.png}"
mkdir -p "$(dirname "$out")"

if ! command -v spectacle >/dev/null 2>&1; then
    echo "capture_real_window: spectacle is not installed; real window capture skipped" >&2
    exit 0
fi

if reason="$(spectacle --background --output "$out" 2>&1)"; then
    echo "capture_real_window: wrote $out"
    if [[ -f "$out" ]] && command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$out"
    fi
else
    echo "capture_real_window: spectacle failed (compositor or window may not support background capture): $reason" >&2
    echo "capture_real_window: real window capture skipped; no PNG or measurement claimed" >&2
fi
exit 0
