#!/usr/bin/env bash
# Generate the three 1000x700 Megi captures read by tools/check_window_ui.py.
# Uses the release binary in headless --screenshot mode: no chrome flag, no
# display server, no Chromium. Fails clearly when the binary or fixture is
# missing; never writes a stand-in PNG.
#
# Usage: tools/capture_window_ui.sh [output-directory]
#        MEGI_BIN=/path/to/megi tools/capture_window_ui.sh [output-directory]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

outdir="${1:-target/ui-captures}"
mkdir -p "$outdir"

bin="${MEGI_BIN:-$REPO_ROOT/target/release/megi}"
if [[ ! -x "$bin" ]]; then
    echo "capture_window_ui: missing release binary $bin" >&2
    echo "capture_window_ui: build it first (cargo build --release -p megi); no capture was fabricated" >&2
    exit 1
fi

runner=()
if command -v nice >/dev/null 2>&1; then
    runner+=(nice -n 10)
fi
if command -v ionice >/dev/null 2>&1; then
    runner+=(ionice -c 3)
fi

# Live URLs, not file:// snapshots: the real pages use root-absolute /static/...
# CSS that a file: document cannot resolve (measured MAE 42.8 for rust), which
# would turn this gate into a false regression signal.
declare -A sites=(
    [hn]="https://news.ycombinator.com/"
    [rust]="https://www.rust-lang.org/"
    [wikipedia]="https://es.wikipedia.org/wiki/Wikipedia:Ignora_las_normas"
)
for site in hn rust wikipedia; do
    echo "==> $site: $bin ${sites[$site]} --screenshot $outdir/$site.png (1000x700, no --chrome)"
    "${runner[@]}" "$bin" "${sites[$site]}" \
        --screenshot "$outdir/$site.png" --width 1000 --height 700
done

python3 - "$outdir" <<'PY'
import hashlib
import sys
from pathlib import Path

from PIL import Image

outdir = Path(sys.argv[1])
for site in ("hn", "rust", "wikipedia"):
    path = outdir / f"{site}.png"
    with Image.open(path) as image:
        if image.format != "PNG" or image.size != (1000, 700):
            raise SystemExit(
                f"capture_window_ui: {path}: expected a 1000x700 PNG, "
                f"got {image.format} {image.size}"
            )
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    print(f"{path}: 1000x700 sha256={digest}")
PY
