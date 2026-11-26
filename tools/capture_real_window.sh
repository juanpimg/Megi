#!/usr/bin/env bash
# Capture the real interactive window; a compositor without support is non-fatal.
set -euo pipefail

out="${1:-/tmp/opencode/ui-real-window.png}"
log="/var/tmp/capture-real-window.log"

if ! command -v spectacle >/dev/null 2>&1; then
  printf 'spectacle not found: compositor capture unavailable, no image at %s\n' "$out"
  exit 0
fi
mkdir -p -- "$(dirname -- "$out")"

rc=0
timeout 30s spectacle --background --output "$out" >"$log" 2>&1 || rc=$?
if [[ "$rc" -ne 0 || ! -s "$out" ]]; then
  printf 'spectacle did not capture the window (exit %s, file %s): compositor may not support background capture; see %s\n' \
    "$rc" "$out" "$log"
  exit 0
fi
printf 'captured real window: %s\n' "$out"
