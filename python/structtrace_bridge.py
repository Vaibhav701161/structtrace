"""Versioned JSONL bridge for StructTrace Python callables."""

from __future__ import annotations

import argparse
import asyncio
import importlib
import inspect
import json
import os
import sys
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


def failure(case_id: str, error: BaseException, kind: str = "python_exception") -> dict[str, Any]:
    return {
        "protocol": PROTOCOL,
        "protocol_version": PROTOCOL_VERSION,
        "case_id": case_id,
        "status": "error",
        "error": {
            "kind": kind,
            "message": f"{type(error).__name__}",
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--callable", required=True)
    args = parser.parse_args()
    target = load_callable(args.callable)
    for line in sys.stdin:
        case_id = ""
        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                raise ValueError("request must be a JSON object")
            case_id = str(request.get("case_id", ""))
        except (json.JSONDecodeError, ValueError) as error:
            print(json.dumps(failure(case_id, error, "protocol_error")), flush=True)
            continue
        case = {
            "input": request.get("input"),
            "metadata": request.get("metadata"),
        }
        try:
            value = target(case)
            if inspect.isawaitable(value):
                value = asyncio.run(value)
            response = success(case_id, value)
        except Exception as error:  # noqa: BLE001 - converted into protocol evidence
            response = failure(case_id, error)
        try:
            encoded = json.dumps(response, ensure_ascii=False)
        except (TypeError, ValueError) as error:
            encoded = json.dumps(failure(case_id, error, "non_serializable_output"))
        print(encoded, flush=True)


if __name__ == "__main__":
    main()
