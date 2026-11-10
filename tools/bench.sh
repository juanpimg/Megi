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
#   ./tools/bench.sh --filter image        # image decode (PNG/JPEG/GIF 800x600)
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

features_args=()
if [[ -n "${BENCH_FEATURES:-}" ]]; then
    features_args=(--features "${BENCH_FEATURES}")
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
    date --iso-8601=seconds
    echo "commit: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "dirty:  $(git status --porcelain | wc -l) modified/untracked paths"
    echo "rustc:  $(rustc --version)"
    echo "args:   $*"
} >"${METADATA}"

echo
echo "Raw table : ${OUTPUT}"
echo "Metadata  : ${METADATA}"
echo "Reproduce : cargo run --release -p bench -- $*"

exit "${status}"
