#!/usr/bin/env bash
# Render the offline render-pipeline fixture to PNG and validate the output.
#
# Usage: tools/render_fixture.sh [output.png]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

output="${1:-/tmp/megi-render-fixture.png}"
input="tests/fixtures/render/basic.html"

cargo run -q -p render -- "$input" "$output" --width 640 --height 400 --json
python3 tools/png_check.py "$output"
md5sum "$output"
