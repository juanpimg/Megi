#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [ "$#" -gt 1 ]; then
    echo "Usage: $0 {check|test|lint|doc|report|all}" >&2
    exit 1
fi

cmd="${1:-all}"

case "$cmd" in
    check)
        cargo check --workspace --all-targets
        ;;
    test)
        cargo test --workspace
        ;;
    lint)
        cargo fmt --all -- --check
        cargo clippy --workspace --all-targets -- -D warnings
        python3 tools/check_docs_links.py
        python3 tools/update_progress.py --check --no-gate --no-count
        ;;
    doc)
        cargo doc --workspace --no-deps
        ;;
    report)
        cargo run -p test_harness -- --summary
        ;;
    all)
        echo "==> [1/5] Checking formatting..."
        cargo fmt --all -- --check
        echo "==> [2/5] Running clippy across workspace..."
        cargo clippy --workspace --all-targets -- -D warnings
        python3 tools/check_docs_links.py
        python3 tools/update_progress.py --check --no-gate --no-count
        echo "==> [3/5] Running closed-loop workspace tests..."
        cargo test --workspace
        echo "==> [4/5] Building workspace documentation..."
        cargo doc --workspace --no-deps
        echo "==> [5/5] Running conformance gate (test_harness --summary)..."
        cargo run -p test_harness -- --summary
        echo "==> All checks passed!"
        ;;
    -h|--help)
        echo "Usage: $0 {check|test|lint|doc|report|all}"
        exit 0
        ;;
    *)
        echo "Usage: $0 {check|test|lint|doc|report|all}" >&2
        exit 1
        ;;
esac
