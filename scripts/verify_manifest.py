#!/usr/bin/env python3
"""Verify every workbook in a manifest exists under a directory with a
matching sha256. Usage: verify_manifest.py <manifest.json> <dir>"""
import hashlib
import json
import pathlib
import sys


def main() -> int:
    manifest_path, subset_dir = sys.argv[1], pathlib.Path(sys.argv[2])
    entries = json.load(open(manifest_path))["workbooks"]
    bad = 0
    for e in entries:
        p = subset_dir / e["file"]
        if not p.is_file():
            print(f"missing: {p}", file=sys.stderr)
            bad += 1
            continue
        digest = hashlib.sha256(p.read_bytes()).hexdigest()
        if digest != e["sha256"]:
            print(f"hash mismatch: {p}", file=sys.stderr)
            bad += 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
