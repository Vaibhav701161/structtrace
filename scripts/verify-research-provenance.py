#!/usr/bin/env python3
"""Verify StructTrace's frozen research lineage against a checked-out lab revision."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("provenance", type=Path)
    parser.add_argument("research_root", type=Path)
    args = parser.parse_args()
    record = json.loads(args.provenance.read_text(encoding="utf-8"))
    actual_commit = subprocess.check_output(
        ["git", "-C", str(args.research_root), "rev-parse", "HEAD"], text=True
    ).strip()
    if actual_commit != record["source_commit"]:
        raise SystemExit(
            f"research commit mismatch: expected {record['source_commit']}, got {actual_commit}"
        )
    for artifact in record["artifacts"]:
        path = args.research_root / artifact["path"]
        if not path.is_file():
            raise SystemExit(f"research artifact is missing: {artifact['path']}")
        actual = sha256(path)
        if actual != artifact["sha256"]:
            raise SystemExit(
                f"research artifact digest mismatch for {artifact['path']}: "
                f"expected {artifact['sha256']}, got {actual}"
            )
    print(
        f"Verified {len(record['artifacts'])} accepted research artifacts at {actual_commit}."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
