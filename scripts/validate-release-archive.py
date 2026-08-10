#!/usr/bin/env python3
"""Extract and exercise the exact archive that a StructTrace user downloads."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import subprocess
import tarfile
import tempfile
import zipfile


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(command: list[str], cwd: Path) -> dict[str, object]:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    return {
        "command": command,
        "exit_code": completed.returncode,
        "stdout": completed.stdout[-4096:],
        "stderr": completed.stderr[-4096:],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--sbom", type=Path, required=True)
    args = parser.parse_args()

    archive = args.archive.resolve()
    with tempfile.TemporaryDirectory(prefix="structtrace-package-") as temporary:
        root = Path(temporary)
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive) as bundle:
                bundle.extractall(root)
            binary = root / "structtrace.exe"
        else:
            with tarfile.open(archive, "r:gz") as bundle:
                bundle.extractall(root, filter="data")
            binary = root / "structtrace"
        if not binary.is_file():
            raise SystemExit(f"packaged executable is missing: {binary}")
        if os.name != "nt":
            binary.chmod(0o755)

        project = root / "smoke-project"
        project.mkdir()
        commands = [
            [str(binary), "--version"],
            [str(binary), "doctor"],
            [str(binary), "--project-root", str(project), "demo", "invoice"],
            [str(binary), "--project-root", str(project), "replay", "latest-demo"],
        ]
        results = [run(command, root) for command in commands]
        platform_checks: list[dict[str, object]] = []
        if platform.system() == "Darwin":
            platform_checks.extend(
                [
                    run(["codesign", "--verify", "--deep", "--strict", str(binary)], root),
                    run(["spctl", "--assess", "--type", "execute", "--verbose=2", str(binary)], root),
                ]
            )
        elif os.name == "nt":
            platform_checks.append(
                run(
                    [
                        "powershell",
                        "-NoProfile",
                        "-Command",
                        f"Get-AuthenticodeSignature '{binary}' | ConvertTo-Json -Compress",
                    ],
                    root,
                )
            )
        evidence = {
            "schema_version": 1,
            "target": args.target,
            "source_commit": args.source_commit,
            "cargo_lock_sha256": sha256(Path("Cargo.lock")),
            "archive": archive.name,
            "archive_sha256": sha256(archive),
            "executable_sha256": sha256(binary),
            "sbom_sha256": sha256(args.sbom),
            "commands": results,
            "platform_security_checks": platform_checks,
            "passed": all(result["exit_code"] == 0 for result in results),
        }
        args.evidence.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        return 0 if evidence["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
