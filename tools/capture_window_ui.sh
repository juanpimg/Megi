#!/usr/bin/env bash
# Generate deterministic 1000x700 window screenshots for the UI parity gate.
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${MEGI_BIN:-$root/target/release/megi}"
out="${1:-$root/tests/fixtures/chrome/ui/captures}"

if [[ ! -x "$bin" ]]; then
  printf 'missing release binary: %s (build with cargo build --release -p megi-browser)\n' "$bin" >&2
  exit 1
fi
mkdir -p "$out"

for site in hn rust wikipedia; do
  src="$root/tests/fixtures/real-sites/$site.html"
  nice -n 19 ionice -c3 "$bin" "$src" \
    --screenshot "$out/$site.png" --width 1000 --height 700 --no-session
  printf '%s: %s\n' "$site" "$out/$site.png"
done
