#!/usr/bin/env python3
"""Check rust-toolchain.toml against Cargo.toml MSRV/edition (Python only).

Usage:
    python3 tools/check_toolchain_config.py [--root /path/to/repo]
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

VERSION = re.compile(r"^(\d+)\.(\d+)(?:\.(\d+))?$")


def version_tuple(value: str) -> tuple[int, int, int]:
    match = VERSION.match(str(value).strip())
    if not match:
        raise ValueError(f"invalid version {value!r}")
    major, minor, patch = match.groups()
    return (int(major), int(minor), int(patch or 0))


def validate(root: Path) -> list[str]:
    toolchain_path, cargo_path = root / "rust-toolchain.toml", root / "Cargo.toml"
    if not toolchain_path.is_file() or not cargo_path.is_file():
        return ["missing rust-toolchain.toml or Cargo.toml"]
    with toolchain_path.open("rb") as handle:
        toolchain = tomllib.load(handle).get("toolchain", {})
    with cargo_path.open("rb") as handle:
        workspace = tomllib.load(handle).get("workspace", {}).get("package", {})
    problems: list[str] = []
    try:
        channel = version_tuple(toolchain.get("channel", ""))
    except ValueError as error:
        return [f"rust-toolchain.toml: {error}"]
    try:
        msrv = version_tuple(workspace.get("rust-version", ""))
    except ValueError as error:
        return [f"Cargo.toml: {error}"]
    if channel < msrv:
        problems.append(
            f"toolchain channel {toolchain.get('channel')} is older than MSRV {workspace.get('rust-version')}"
        )
    components = toolchain.get("components", [])
    if not isinstance(components, list) or not components:
        problems.append("rust-toolchain.toml: components must be a non-empty list")
    else:
        for required in ("rustfmt", "clippy"):
            if required not in components:
                problems.append(f"rust-toolchain.toml: missing {required} component")
    if str(workspace.get("edition", "")) != "2024":
        problems.append(f"Cargo.toml: unexpected edition {workspace.get('edition')!r}")
    return problems


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args(argv)
    problems = validate(args.root.resolve())
    for problem in problems:
        print(f"TOOLCHAIN: {problem}", file=sys.stderr)
    if problems:
        return 1
    print("toolchain configuration consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
