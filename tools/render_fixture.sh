#!/usr/bin/env bash
# Render the versioned render-pipeline fixtures to PNG, validate them and print md5s.
#
# Usage: tools/render_fixture.sh [output-directory]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

outdir="${1:-/tmp}"
mkdir -p "$outdir"

echo "==> basic fixture (640x400)"
cargo run -q -p render -- tests/fixtures/render/basic.html "$outdir/megi-basic.png" \
    --width 640 --height 400 --json
python3 tools/png_check.py "$outdir/megi-basic.png"
md5sum "$outdir/megi-basic.png"

echo "==> backgrounds fixture (800x600, background-image + list-style-image)"
cargo run -q -p render -- tests/fixtures/render/backgrounds.html "$outdir/megi-backgrounds.png" \
    --json
python3 tools/png_check.py "$outdir/megi-backgrounds.png"
md5sum "$outdir/megi-backgrounds.png"

echo "==> showcase fixture (1024x768, @import + media + images + backgrounds + table)"
cargo run -q -p render -- tests/fixtures/render/showcase/index.html \
    "$outdir/megi-showcase.png" --width 1024 --height 768 --json
python3 tools/png_check.py "$outdir/megi-showcase.png"
md5sum "$outdir/megi-showcase.png"

echo "==> golden comparison"
cmp "$outdir/megi-showcase.png" tests/fixtures/render/showcase/golden.png \
    && echo "showcase PNG matches the versioned golden byte-for-byte"
