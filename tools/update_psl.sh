#!/usr/bin/env bash
# Refreshes the vendored Public Suffix List (MPL-2.0) used by the `net` crate.
#
# Usage: tools/update_psl.sh [snapshot-date]
# Requires curl. Embeds the snapshot date in the header and runs the PSL tests.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${repo_root}/crates/net/data/public_suffix_list.dat"
snapshot="${1:-$(date +%Y-%m-%d)}"
url="https://publicsuffix.org/list/public_suffix_list.dat"
tmp="$(mktemp)"
trap 'rm -f "${tmp}"' EXIT

echo "downloading ${url}"
curl -fsSL --max-time 120 "${url}" -o "${tmp}"
lines=$(wc -l < "${tmp}")
if [ "${lines}" -lt 10000 ]; then
    echo "error: downloaded list looks truncated (${lines} lines)" >&2
    exit 1
fi

{
    printf '// Megi vendored snapshot: %s (publicsuffix.org/list/public_suffix_list.dat)\n' "${snapshot}"
    cat "${tmp}"
} > "${target}"
echo "wrote ${target} (${lines} lines, snapshot ${snapshot})"

cd "${repo_root}"
cargo test -p net psl
