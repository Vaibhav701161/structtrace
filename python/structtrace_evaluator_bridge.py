"""Versioned bridge for StructTrace Python evaluator callables."""

from __future__ import annotations

import argparse
import asyncio
import importlib
import inspect
import json
import os
import sys
from typing import Any, Callable

PROTOCOL = "structtrace.evaluator"
PROTOCOL_VERSION = 1
sys.path.insert(0, os.getcwd())


def load_callable(reference: str) -> Callable[[dict[str, Any]], Any]:
    module_name, attribute = reference.split(":", 1)
    target = getattr(importlib.import_module(module_name), attribute)
    if not callable(target):
        raise TypeError(f"{reference} is not callable")
    return target


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--callable", required=True)
    args = parser.parse_args()
    target = load_callable(args.callable)
    for line in sys.stdin:
        if not line.strip():
            continue
        evaluator_id = ""
        case_id = ""
        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                raise ValueError("request must be a JSON object")
            evaluator_id = str(request.get("evaluator_id", ""))
            case_id = str(request.get("case_id", ""))
        except (json.JSONDecodeError, ValueError) as error:
            response = {
                "protocol": PROTOCOL,
                "protocol_version": PROTOCOL_VERSION,
                "evaluator_id": evaluator_id,
                "case_id": case_id,
                "status": "error",
                "message": type(error).__name__,
            }
            print(json.dumps(response), flush=True)
            continue
        try:
            result = target(request)
            if inspect.isawaitable(result):
                result = asyncio.run(result)
            if isinstance(result, bool):
                result = {"status": "passed" if result else "failed", "score": 1 if result else 0}
            if not isinstance(result, dict):
                raise TypeError("evaluator must return bool or dict")
            response = {
                "protocol": PROTOCOL,
                "protocol_version": PROTOCOL_VERSION,
                "evaluator_id": evaluator_id,
                "case_id": case_id,
                **result,
            }
        except Exception as error:  # noqa: BLE001 - converted into protocol evidence
            response = {
                "protocol": PROTOCOL,
                "protocol_version": PROTOCOL_VERSION,
                "evaluator_id": evaluator_id,
                "case_id": case_id,
                "status": "error",
                "message": type(error).__name__,
            }
        try:
            encoded = json.dumps(response, ensure_ascii=False)
        except (TypeError, ValueError) as error:
            encoded = json.dumps({
                "protocol": PROTOCOL,
                "protocol_version": PROTOCOL_VERSION,
                "evaluator_id": evaluator_id,
                "case_id": case_id,
                "status": "error",
                "message": type(error).__name__,
            })
        print(encoded, flush=True)


if __name__ == "__main__":
    main()
