#!/usr/bin/env python3
"""Fail when two *regular* fixture files share a SHA-256 (#1432).

Render fixtures that mirror an `image`/`fonts` fixture are stored as symlinks to the
canonical copy: every path keeps working, but the bytes exist only once. Symlinks are
skipped here so the check enforces one canonical copy per byte sequence without
flagging the aliases.
"""

from __future__ import annotations

import hashlib
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCAN_DIRS = (
    ROOT / "crates" / "image" / "tests" / "fixtures",
    ROOT / "crates" / "fonts" / "tests" / "fixtures",
    ROOT / "tests" / "fixtures",
)


def main() -> int:
    by_hash: dict[str, list[str]] = defaultdict(list)
    scanned = 0
    for directory in SCAN_DIRS:
        if not directory.is_dir():
            continue
        for path in sorted(directory.rglob("*")):
            if not path.is_file() or path.is_symlink():
                continue
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            by_hash[digest].append(str(path.relative_to(ROOT)))
            scanned += 1
    duplicates = {digest: paths for digest, paths in by_hash.items() if len(paths) > 1}
    if duplicates:
        for digest, paths in sorted(duplicates.items()):
            print(f"duplicate fixture bytes {digest[:12]}:")
            for path in paths:
                print(f"  {path}")
        print(f"ERROR: {len(duplicates)} duplicate fixture byte sequence(s)", file=sys.stderr)
        return 1
    print(f"fixture hashes ok: {scanned} regular files, no duplicates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
