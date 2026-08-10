#!/usr/bin/env python3
"""Reproducible Linux benchmark for the documented 10,000-case v1 envelope."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time
from datetime import datetime, timezone


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def timed(command: list[str], cwd: Path) -> dict[str, object]:
    started = time.monotonic()
    timing_path = cwd / f"timing-{time.monotonic_ns()}.txt"
    measured_command = command
    if Path("/usr/bin/time").is_file():
        measured_command = [
            "/usr/bin/time",
            "-f",
            "%e %M",
            "-o",
            str(timing_path),
            *command,
        ]
    completed = subprocess.run(
        measured_command, cwd=cwd, text=True, capture_output=True, check=False
    )
    result: dict[str, object] = {
        "command": command,
        "exit_code": completed.returncode,
        "wall_seconds": round(time.monotonic() - started, 3),
        "stdout_tail": completed.stdout[-2048:],
        "stderr_tail": completed.stderr[-2048:],
    }
    if timing_path.is_file():
        wall, peak = timing_path.read_text(encoding="utf-8").strip().split()
        result["measured_wall_seconds"] = float(wall)
        result["peak_rss_kib"] = int(peak)
        timing_path.unlink()
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/release/structtrace"))
    parser.add_argument("--cases", type=int, default=10_000)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"release binary not found: {binary}")

    with tempfile.TemporaryDirectory(prefix="structtrace-scale-") as temporary:
        root = Path(temporary)
        data = root / "data.jsonl"
        baseline = root / "baseline.jsonl"
        candidate = root / "candidate.jsonl"
        with data.open("w", encoding="utf-8") as dataset, baseline.open(
            "w", encoding="utf-8"
        ) as left, candidate.open("w", encoding="utf-8") as right:
            for index in range(args.cases):
                case_id = f"case-{index:06d}"
                label = "accepted" if index % 2 == 0 else "rejected"
                dataset.write(
                    json.dumps(
                        {
                            "id": case_id,
                            "input": {"text": f"bounded document {index}"},
                            "expected": {"label": label},
                        },
                        separators=(",", ":"),
                    )
                    + "\n"
                )
                row = {
                    "case_id": case_id,
                    "status": "ok",
                    "parsed_output": {"label": label},
                }
                encoded = json.dumps(row, separators=(",", ":")) + "\n"
                left.write(encoded)
                right.write(encoded)
        (root / "schema.json").write_text(
            '{"type":"object","required":["label"],"properties":{"label":{"enum":["accepted","rejected"]}},"additionalProperties":false}\n',
            encoding="utf-8",
        )
        (root / "structtrace.yaml").write_text(
            f"""version: 3
project: {{name: recorded-scale-{args.cases}}}
limits:
  max_cases: {args.cases}
dataset: {{path: data.jsonl}}
schema: {{path: schema.json}}
variants:
  baseline: {{kind: recorded, path: baseline.jsonl}}
  candidate: {{kind: recorded, path: candidate.jsonl}}
evaluators:
  - {{id: label, kind: json_pointer_exact, pointer: /label, expected_pointer: /label}}
outcomes: {{correct: {{all_of: [label]}}}}
analysis: {{primary_outcome: correct, bootstrap: {{samples: 1000, confidence: 0.95, seed: 17}}}}
gate: {{mode: advisory}}
report: {{include_raw_outputs: false, default_case_filter: all}}
""",
            encoding="utf-8",
        )
        commands = [
            timed([str(binary), "--project-root", str(root), "run"], root),
            timed([str(binary), "--project-root", str(root), "replay", "latest"], root),
        ]
        run_dirs = list((root / ".structtrace" / "runs").iterdir())
        run_dir = run_dirs[0]
        report = {
            "schema_version": 1,
            "generated_at": datetime.now(timezone.utc).isoformat(),
            "cases": args.cases,
            "platform": subprocess.check_output(["uname", "-srm"], text=True).strip(),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "source_commit": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip(),
            "source_worktree_clean": not subprocess.check_output(
                ["git", "status", "--porcelain"], text=True
            ).strip(),
            "cargo_lock_sha256": digest(Path("Cargo.lock")),
            "binary_sha256": digest(binary),
            "source_bytes": {
                "dataset": data.stat().st_size,
                "baseline": baseline.stat().st_size,
                "candidate": candidate.stat().st_size,
            },
            "run_artifact_bytes": sum(
                path.stat().st_size for path in run_dir.rglob("*") if path.is_file()
            ),
            "commands": commands,
            "passed": all(command["exit_code"] == 0 for command in commands),
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
