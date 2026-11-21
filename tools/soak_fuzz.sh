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
#   tools/soak_fuzz.sh --shard-index 0 --shard-count 4 --slice-seconds 2400
#                                       # deeper campaign: one of 4 parallel shards
#
# Notes:
#   * `--shard-index`/`--shard-count` split TARGETS by round-robin so several CI jobs
#     can fuzz different targets in parallel, giving each target a much longer slice
#     inside the same wall-clock budget (X-FUZZ-LONG).
#   * Invalid shard values (SHARD_COUNT=0, SHARD_INDEX=-1) are rejected before the
#     modulo arithmetic that selects the subset.
#   * `cargo +nightly fuzz` is required (sanitizer instrumentation).
#   * Boa-backed targets (`js_eval`, `js_module`) run with `-detect_leaks=0`:
#     Boa keeps a process-lifetime allocation alive, so leak detection reports a
#     constant false positive (same reason as the nightly workflow).
#   * A campaign that finds anything exits non-zero and leaves the input under
#     `$OUT/crashes/<target>/`, which is what the triage steps in docs/FUZZING.md
#     expect.
set -euo pipefail

HOURS=24
# 27 targets × 600 s = 16 200 s fits a full rotation inside the 5 h (18 000 s)
# scheduled workflow budget (#1833). At 900 s the last target never ran.
SLICE_SECONDS=600
SHARD_INDEX=0
SHARD_COUNT=1
OUT="target/soak"
TARGETS=(
  html_parse
  css_parse
  selectors
  net_url
  http_head
  compression
  svg_parse
  image_decode
  hpack_decode
  webp
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
  dom_mutate
  font_decode
  js_eval
  js_module
  layout_resolve
  net_surface
)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --hours) HOURS="$2"; shift 2 ;;
    --slice-seconds) SLICE_SECONDS="$2"; shift 2 ;;
    --shard-index) SHARD_INDEX="$2"; shift 2 ;;
    --shard-count) SHARD_COUNT="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --targets) read -r -a TARGETS <<< "$2"; shift 2 ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# Reject non-numeric/negative budgets before they reach shell arithmetic (#1397).
[[ $HOURS =~ ^[0-9]+$ ]] || { echo "--hours must be a non-negative integer (got: $HOURS)" >&2; exit 2; }
[[ $SLICE_SECONDS =~ ^[0-9]+$ ]] || { echo "--slice-seconds must be a non-negative integer (got: $SLICE_SECONDS)" >&2; exit 2; }
[[ $SLICE_SECONDS -gt 0 ]] || { echo "--slice-seconds must be greater than zero" >&2; exit 2; }
[[ $SHARD_COUNT =~ ^[0-9]+$ ]] || { echo "--shard-count must be a positive integer (got: $SHARD_COUNT)" >&2; exit 2; }
[[ $SHARD_COUNT -gt 0 ]] || { echo "--shard-count must be greater than zero (got: $SHARD_COUNT)" >&2; exit 2; }
[[ $SHARD_INDEX =~ ^[0-9]+$ ]] || { echo "--shard-index must be a non-negative integer (got: $SHARD_INDEX)" >&2; exit 2; }
[[ $SHARD_INDEX -lt $SHARD_COUNT ]] || { echo "--shard-index must be less than --shard-count (got: $SHARD_INDEX)" >&2; exit 2; }

# Round-robin partition: shard i owns every target whose original index i % SHARD_COUNT
# equals i. This is deterministic, covers every target exactly once and needs no state.
if [[ $SHARD_COUNT -gt 1 ]]; then
  SHARD_TARGETS=()
  for (( i = 0; i < ${#TARGETS[@]}; i++ )); do
    if (( i % SHARD_COUNT == SHARD_INDEX )); then
      SHARD_TARGETS+=("${TARGETS[i]}")
    fi
  done
  TARGETS=("${SHARD_TARGETS[@]}")
fi

command -v cargo >/dev/null || { echo "cargo not found" >&2; exit 2; }
if ! cargo +nightly fuzz --version >/dev/null 2>&1; then
  echo "cargo-fuzz (nightly) not installed; cargo install cargo-fuzz --version 0.13.2 --locked" >&2
  exit 2
fi

mkdir -p "$OUT/crashes" "$OUT/logs" fuzz/corpus
deadline=$(( $(date +%s) + HOURS * 3600 ))
rotations=0
failures=0

echo "soak campaign: ${HOURS}h, ${SLICE_SECONDS}s per target, ${#TARGETS[@]} targets, shard ${SHARD_INDEX}/${SHARD_COUNT}, out=$OUT"

while [[ $(date +%s) -lt $deadline ]]; do
  for target in "${TARGETS[@]}"; do
    now=$(date +%s)
    remaining=$(( deadline - now ))
    [[ $remaining -le 0 ]] && break
    # Never overshoot the advertised deadline: the final slice is capped at
    # the remaining time (#1397).
    slice=$(( remaining < SLICE_SECONDS ? remaining : SLICE_SECONDS ))

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
