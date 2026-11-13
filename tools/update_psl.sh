#!/usr/bin/env bash
# Refreshes the vendored Public Suffix List (MPL-2.0) used by the `net` crate.
#
# Usage: tools/update_psl.sh [snapshot-date]
# Requires curl. Embeds the snapshot date in the header and runs the PSL tests.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${repo_root}/crates/net/data/public_suffix_list.dat"
snapshot="${1:-$(date +%Y-%m-%d)}"
if [[ ! "${snapshot}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "error: snapshot must be YYYY-MM-DD (got '${snapshot}')" >&2
    exit 1
fi
url="https://publicsuffix.org/list/public_suffix_list.dat"
tmp="$(mktemp)"
trap 'rm -f "${tmp}"' EXIT

echo "downloading ${url}"
curl -fsSL --proto =https --proto-redir =https --max-time 120 "${url}" -o "${tmp}"
lines=$(wc -l < "${tmp}")
if [ "${lines}" -lt 10000 ]; then
    echo "error: downloaded list looks truncated (${lines} lines)" >&2
    exit 1
fi
# Validate content markers before touching the vendored file (#608).
if ! grep -q "BEGIN ICANN DOMAINS" "${tmp}"; then
    echo "error: downloaded payload is missing the 'BEGIN ICANN DOMAINS' marker" >&2
    exit 1
fi
if ! grep -q "END ICANN DOMAINS" "${tmp}"; then
    echo "error: downloaded payload is missing the 'END ICANN DOMAINS' marker" >&2
    exit 1
fi

# Build the new vendored file in a temp path and install it atomically, keeping a
# backup of the previous version so a failure never leaves a corrupt PSL in place.
out="$(mktemp)"
trap 'rm -f "${tmp}" "${out}"' EXIT
{
    printf '// Megi vendored snapshot: %s (publicsuffix.org/list/public_suffix_list.dat)\n' "${snapshot}"
    cat "${tmp}"
} > "${out}"
if [ "$(wc -l < "${out}")" -lt 10000 ]; then
    echo "error: assembled PSL looks truncated" >&2
    exit 1
fi
if [ -f "${target}" ]; then
    cp "${target}" "${target}.bak"
fi
mv -f "${out}" "${target}"
echo "wrote ${target} (${lines} lines, snapshot ${snapshot})"

cd "${repo_root}"
cargo test -p net psl
