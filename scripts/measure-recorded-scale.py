#!/usr/bin/env python3
"""Reproducible Linux benchmark for the documented 10,000-case v1 envelope."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import select
import tempfile
import time
from urllib.parse import urlencode, urljoin
from urllib.request import urlopen
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


def timed_http(url: str, expected_total: int | None = None) -> dict[str, object]:
    started = time.monotonic()
    try:
        with urlopen(url, timeout=30) as response:  # noqa: S310 - loopback capability URL
            body = response.read()
            payload = json.loads(body)
            total = payload.get("total") if isinstance(payload, dict) else None
            valid = response.status == 200 and (
                expected_total is None or total == expected_total
            )
            return {
                "command": ["HTTP GET", url],
                "exit_code": 0 if valid else 1,
                "wall_seconds": round(time.monotonic() - started, 3),
                "status": response.status,
                "response_bytes": len(body),
                "matched_cases": total,
            }
    except Exception as error:  # benchmark receipt must retain bounded diagnostics
        return {
            "command": ["HTTP GET", url],
            "exit_code": 1,
            "wall_seconds": round(time.monotonic() - started, 3),
            "error": f"{type(error).__name__}: {error}",
        }


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
        commands: list[dict[str, object]] = [
            timed([str(binary), "--project-root", str(root), "run"], root),
            timed([str(binary), "--project-root", str(root), "replay", "latest"], root),
        ]
        run_dirs = list((root / ".structtrace" / "runs").iterdir())
        run_dir = run_dirs[0]
        server = subprocess.Popen(
            [str(binary), "--project-root", str(root), "open", "--no-browser"],
            cwd=root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            ready, _, _ = select.select([server.stdout], [], [], 30)
            if not ready or server.stdout is None:
                raise RuntimeError("local server did not publish its capability URL")
            line = server.stdout.readline().strip()
            if not line.startswith("StructTrace Local: "):
                raise RuntimeError(f"unexpected local-server greeting: {line}")
            base_url = line.removeprefix("StructTrace Local: ")
            commands.append(timed_http(urljoin(base_url, "api/v1/runs")))
            query = urlencode(
                {"offset": 0, "limit": 50, "filter": "all", "search": "case-009999"}
            )
            search_url = urljoin(
                base_url, f"api/v1/runs/{run_dir.name}/cases?{query}"
            )
            commands.append(timed_http(search_url, expected_total=1))
            commands.append(timed_http(search_url, expected_total=1))
        except Exception as error:
            commands.append(
                {
                    "command": ["local UI case-search benchmark"],
                    "exit_code": 1,
                    "error": f"{type(error).__name__}: {error}",
                }
            )
        finally:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=10)
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
