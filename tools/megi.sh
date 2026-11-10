#!/usr/bin/env bash
# Convenience wrapper for the Megi shell binary.
#
# Usage:
#   tools/megi.sh <url|file> [--width W] [--height H] [--screenshot PATH] [--title T]
#
# Without --screenshot it opens a real winit window (needs X11/Wayland).
# With --screenshot it renders headlessly and writes a PNG (CI-friendly).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

exec cargo run -q -p shell -- "$@"
