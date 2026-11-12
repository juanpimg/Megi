#!/usr/bin/env bash
# Render the versioned render-pipeline fixtures to PNG, validate them and print md5s.
#
# Usage: tools/render_fixture.sh [output-directory]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

cleanup_outdir=0
if [ -n "${1:-}" ]; then
    outdir="$1"
    mkdir -p "$outdir"
else
    outdir="$(mktemp -d)"
    cleanup_outdir=1
fi
trap 'if [ "$cleanup_outdir" -eq 1 ]; then rm -rf "$outdir"; fi' EXIT

digest_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1"
    else
        python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest(), sys.argv[1])' "$1"
    fi
}

check_json() {
    python3 -c 'import json,sys; d=json.load(sys.stdin); assert isinstance(d, dict) and d.get("png"), "invalid --json output"'
}

echo "==> basic fixture (640x400)"
cargo run -q -p render -- tests/fixtures/render/basic.html "$outdir/megi-basic.png" \
    --width 640 --height 400 --json | check_json
python3 tools/png_check.py "$outdir/megi-basic.png"
digest_file "$outdir/megi-basic.png"

echo "==> backgrounds fixture (800x600, background-image + list-style-image)"
cargo run -q -p render -- tests/fixtures/render/backgrounds.html "$outdir/megi-backgrounds.png" \
    --json | check_json
python3 tools/png_check.py "$outdir/megi-backgrounds.png"
digest_file "$outdir/megi-backgrounds.png"

echo "==> scripting fixture (800x600, inline JS mutates text/colour)"
cargo run -q -p render -- tests/fixtures/render/js_text.html "$outdir/megi-js-text.png" --json | check_json
python3 tools/png_check.py "$outdir/megi-js-text.png"
digest_file "$outdir/megi-js-text.png"
cargo run -q -p render -- tests/fixtures/render/js_text.html \
    "$outdir/megi-js-text-disabled.png" --no-scripting --json | check_json
digest_file "$outdir/megi-js-text-disabled.png"
if cmp -s "$outdir/megi-js-text.png" "$outdir/megi-js-text-disabled.png"; then
    echo "ERROR: scripting did not change the PNG" >&2
    exit 1
else
    echo "expected difference: scripting changes the rendered PNG"
fi

echo "==> SVG image fixture (800x600, rasterized SVG <img>)"
cargo run -q -p render -- tests/fixtures/render/svg_img.html "$outdir/megi-svg-img.png" --json | check_json
python3 tools/png_check.py "$outdir/megi-svg-img.png"
digest_file "$outdir/megi-svg-img.png"

echo "==> Inline SVG fixture (800x600, <svg> replaced by synthetic <img> data URL)"
cargo run -q -p render -- tests/fixtures/render/svg_inline_simple.html "$outdir/megi-svg-inline.png" --json | check_json
python3 tools/png_check.py "$outdir/megi-svg-inline.png"
digest_file "$outdir/megi-svg-inline.png"

echo "==> showcase fixture (1024x768, @import + media + images + backgrounds + table)"
cargo run -q -p render -- tests/fixtures/render/showcase/index.html \
    "$outdir/megi-showcase.png" --width 1024 --height 768 --json | check_json
python3 tools/png_check.py "$outdir/megi-showcase.png"
digest_file "$outdir/megi-showcase.png"

echo "==> golden comparison"
if cmp -s "$outdir/megi-showcase.png" tests/fixtures/render/showcase/golden.png; then
    echo "showcase PNG matches the versioned golden byte-for-byte"
else
    echo "ERROR: showcase PNG does not match tests/fixtures/render/showcase/golden.png" >&2
    exit 1
fi
