#!/usr/bin/env python3
from __future__ import annotations

import sys
import tomllib
from pathlib import Path


def main() -> int:
    if len(sys.argv) not in {3, 4}:
        print(
            "Usage: read_manifest_value.py <Cargo.toml> <dotted.key.path> [default]",
            file=sys.stderr,
        )
        return 2

    manifest_path = Path(sys.argv[1])
    key_path = sys.argv[2]
    default = sys.argv[3] if len(sys.argv) == 4 else ""

    data = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    value = lookup_value(data, key_path.split("."))
    if value is None:
        print(default)
        return 0

    if isinstance(value, bool):
        print("true" if value else "false")
    else:
        print(str(value))
    return 0


def lookup_value(node: object, parts: list[str]) -> object | None:
    current = node
    for part in parts:
        if not isinstance(current, dict) or part not in current:
            return None
        current = current[part]
    return current


if __name__ == "__main__":
    raise SystemExit(main())
