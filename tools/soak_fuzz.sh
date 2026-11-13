#!/usr/bin/env bash
# Long-running ("soak") fuzz campaign driver (#964).
#
# Rotates through the libFuzzer targets for a wall-clock budget, keeping each
# target's corpus on disk so successive campaigns build on each other, and
# collecting every crash/leak artifact under $OUT/crashes/<target>/.
#
# Usage:
#   tools/soak_fuzz.sh                        # 24 h over the default target set
#   tools/soak_fuzz.sh --hours 2              # shorter run for a smoke check
#   tools/soak_fuzz.sh --targets html_parse css_parse
#   tools/soak_fuzz.sh --slice-seconds 600    # time per target per rotation
#
# Notes:
#   * `cargo +nightly fuzz` is required (sanitizer instrumentation).
#   * Boa-backed targets (`js_eval`, `js_module`) run with `-detect_leaks=0`:
#     Boa keeps a process-lifetime allocation alive, so leak detection reports a
#     constant false positive (same reason as the nightly workflow).
#   * A campaign that finds anything exits non-zero and leaves the input under
#     `$OUT/crashes/<target>/`, which is what the triage steps in docs/FUZZING.md
#     expect.
set -euo pipefail

HOURS=24
SLICE_SECONDS=900
OUT="target/soak"
TARGETS=(
  html_parse
  css_parse
  selectors
  net_url
  svg_parse
  image_decode
  hpack_decode
  webp_lossy
  webp_vp8
  render_pipeline
  container_queries
  text_boundary
  body_stream
  h2_push
  exif_parse
  woff2_decode
  layout_tree
  paint_display_list
  dom_tree
  fonts_shape
)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --hours) HOURS="$2"; shift 2 ;;
    --slice-seconds) SLICE_SECONDS="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --targets) read -r -a TARGETS <<< "$2"; shift 2 ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

command -v cargo >/dev/null || { echo "cargo not found" >&2; exit 2; }
if ! cargo +nightly fuzz --version >/dev/null 2>&1; then
  echo "cargo-fuzz (nightly) not installed; cargo install cargo-fuzz --version 0.13.2 --locked" >&2
  exit 2
fi

mkdir -p "$OUT/crashes" "$OUT/logs" fuzz/corpus
deadline=$(( $(date +%s) + HOURS * 3600 ))
rotations=0
failures=0

echo "soak campaign: ${HOURS}h, ${SLICE_SECONDS}s per target, ${#TARGETS[@]} targets, out=$OUT"

while [[ $(date +%s) -lt $deadline ]]; do
  for target in "${TARGETS[@]}"; do
    now=$(date +%s)
    remaining=$(( deadline - now ))
    [[ $remaining -le 0 ]] && break
    slice=$(( remaining < SLICE_SECONDS ? remaining : SLICE_SECONDS ))
    [[ $slice -lt 30 ]] && slice=30

    log="$OUT/logs/${target}.log"
    mkdir -p "fuzz/corpus/$target"
    echo "── $(date -u +%FT%TZ) $target (${slice}s, rotation $rotations)" | tee -a "$log"

    extra=()
    case "$target" in
      js_eval|js_module) extra+=("-detect_leaks=0") ;;
    esac

    if ! cargo +nightly fuzz run "$target" "fuzz/corpus/$target" -- \
        -max_total_time="$slice" \
        -rss_limit_mb=4096 \
        -timeout=25 \
        -print_final_stats=1 \
        "${extra[@]}" >>"$log" 2>&1; then
      echo "!! $target exited non-zero; artifacts under fuzz/artifacts/$target" | tee -a "$log"
      mkdir -p "$OUT/crashes/$target"
      if [[ -d "fuzz/artifacts/$target" ]]; then
        cp -a "fuzz/artifacts/$target/." "$OUT/crashes/$target/" 2>/dev/null || true
      fi
      failures=$((failures + 1))
    fi
  done
  rotations=$((rotations + 1))
done

{
  echo
  echo "soak campaign finished: $rotations rotation(s), $failures failing target run(s)"
  if [[ $failures -gt 0 ]]; then
    echo "crashes: $(find "$OUT/crashes" -type f | wc -l) artifact(s) in $OUT/crashes"
  fi
} | tee -a "$OUT/logs/summary.log"

[[ $failures -eq 0 ]]
