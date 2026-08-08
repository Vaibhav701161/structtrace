"""Versioned JSONL bridge for StructTrace Python callables."""

from __future__ import annotations

import argparse
import importlib
import inspect
import json
import os
import sys
import traceback
from typing import Any, Callable

PROTOCOL = "structtrace.variant"
PROTOCOL_VERSION = 1

# The bridge lives under `.structtrace/runtime`; user modules resolve from the
# configured project working directory rather than the bridge directory.
sys.path.insert(0, os.getcwd())


def load_callable(reference: str) -> Callable[[dict[str, Any]], Any]:
    if ":" not in reference:
        raise ValueError("callable must use module:attribute syntax")
    module_name, attribute = reference.split(":", 1)
    target = getattr(importlib.import_module(module_name), attribute)
    if not callable(target):
        raise TypeError(f"{reference} is not callable")
    return target


def success(case_id: str, value: Any) -> dict[str, Any]:
    if isinstance(value, dict) and value.get("protocol") == PROTOCOL:
        return value
    envelope: dict[str, Any] = {
        "protocol": PROTOCOL,
        "protocol_version": PROTOCOL_VERSION,
        "case_id": case_id,
        "status": "ok",
    }
    if isinstance(value, str):
        envelope["raw_output"] = value
    else:
        envelope["output"] = value
    return envelope


def failure(case_id: str, error: BaseException) -> dict[str, Any]:
    traceback.print_exc(file=sys.stderr)
    return {
        "protocol": PROTOCOL,
        "protocol_version": PROTOCOL_VERSION,
        "case_id": case_id,
        "status": "error",
        "error": {
            "kind": "python_exception",
            "message": f"{type(error).__name__}: {error}",
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--callable", required=True)
    args = parser.parse_args()
    target = load_callable(args.callable)
    for line in sys.stdin:
        request = json.loads(line)
        case_id = str(request.get("case_id", ""))
        case = {
            "id": case_id,
            "input": request.get("input"),
            "expected": request.get("expected"),
            "metadata": request.get("metadata"),
        }
        try:
            value = target(case)
            if inspect.isawaitable(value):
                raise TypeError("async Python callables are not supported by this bridge")
            response = success(case_id, value)
        except Exception as error:  # noqa: BLE001 - converted into protocol evidence
            response = failure(case_id, error)
        print(json.dumps(response, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
