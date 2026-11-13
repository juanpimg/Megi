#!/usr/bin/env bash
# Compiles the Megi benchmark crate in release mode, runs it and saves the raw
# output under target/ (gitignored). Only `cargo` and a POSIX shell are needed:
# hyperfine and criterion are deliberately not required.
#
# Usage:
#   ./tools/bench.sh                       # full suite
#   ./tools/bench.sh --scale 0.25          # smaller smoke run
#   ./tools/bench.sh --json                # machine-readable output
#   ./tools/bench.sh --filter html         # one group
#   ./tools/bench.sh --filter pipeline     # E2E rows (incl. gradients/grid/clipping)
#   ./tools/bench.sh --filter image        # image decode (PNG/JPEG/GIF 800x600 + WebP VP8L)
#   ./tools/bench.sh --save-png-isolated  # isolated shell --screenshot vs --save-png
#   BENCH_FEATURES=fonts ./tools/bench.sh  # optional font benchmarks
#
# All arguments are forwarded verbatim to the `bench` binary.

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OUTPUT="target/bench-latest.txt"
METADATA="target/bench-latest-meta.txt"

usage() {
    sed -n '9,18p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

case "${1:-}" in
    -h | --help)
        usage
        exit 0
        ;;
esac

mkdir -p target

# Isolated save-png measurement (R166): the shell only enters headless mode when
# `--screenshot` is present, so the pure `--save-png` cost is measured as
# `--screenshot` (one PNG write) versus `--screenshot --save-png` (the second
# write reuses the same PNG bytes). Component costs live in the `shell/save-png-*`
# bench rows (`cargo run --release -p bench -- --filter shell`).
if [[ "${1:-}" == "--save-png-isolated" ]]; then
    shift
    fixture="${1:-tests/fixtures/render/showcase/index.html}"
    mkdir -p target/bench-save-png
    echo "==> Building release shell"
    cargo build --release -p shell
    binary=target/release/megi
    screenshot=target/bench-save-png/screenshot.png
    saved=target/bench-save-png/saved.png
    measure() {
        local label="$1"
        shift
        local values=()
        for _ in $(seq 1 7); do
            local start end
            start=$(python3 -c 'import time; print(int(time.time()*1000))')
            if ! "$@" >/dev/null 2>&1; then
                echo "error: benchmark command failed: $*" >&2
                exit 1
            fi
            end=$(python3 -c 'import time; print(int(time.time()*1000))')
            values+=($((end - start)))
        done
        local median
        median=$(printf '%s\n' "${values[@]}" | sort -n | sed -n '4p')
        printf '%-46s %s ms\n' "${label}" "${median}"
    }
    measure "--screenshot only" "${binary}" "${fixture}" --screenshot "${screenshot}" --width 1024 --height 768
    measure "--screenshot + --save-png" "${binary}" "${fixture}" --screenshot "${screenshot}" --save-png "${saved}" --width 1024 --height 768
    echo
    echo "Note: '--save-png' alone opens a window today; the second write reuses the"
    echo "already-encoded PNG. Isolated encode/write rows: --filter shell."
    exit 0
fi

features_args=()
if [[ -n "${BENCH_FEATURES:-}" ]]; then
    # Word-split the env var so multi-word values work (e.g. BENCH_FEATURES="fonts extra").
    read -ra _bench_features <<< "${BENCH_FEATURES}"
    features_args=(--features "${_bench_features[@]}")
fi

echo "==> Building release benchmark binary (cargo build --release -p bench ${features_args[*]:-})"
cargo build --release -p bench "${features_args[@]}"

echo "==> Running target/release/bench $*"
set +e
target/release/bench "$@" | tee "${OUTPUT}"
status=${PIPESTATUS[0]}
set -e

{
    echo "# Megi benchmark run metadata"
    python3 -c 'import datetime; print(datetime.datetime.now().astimezone().isoformat())' 
    echo "commit: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "dirty:  $(git status --porcelain | wc -l) modified/untracked paths"
    echo "rustc:  $(rustc --version)"
    echo "args:   $*"
} >"${METADATA}"

echo
echo "Raw table : ${OUTPUT}"
echo "Metadata  : ${METADATA}"
printf 'Reproduce : cargo run --release -p bench --'
printf ' %q' "$@"
echo

exit "${status}"
