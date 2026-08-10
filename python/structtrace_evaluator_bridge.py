"""Versioned bridge for StructTrace Python evaluator callables."""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import hashlib
import importlib
import inspect
import json
import os
import sys
import traceback
from typing import Any, Callable

PROTOCOL = "structtrace.evaluator"
PROTOCOL_VERSION = 2
PROTOCOL_STDOUT = sys.stdout
RESERVED = {"protocol", "protocol_version", "case_id", "evaluator_id"}
sys.path.insert(0, os.getcwd())


class PersistentAsyncRunner:
    def __init__(self) -> None:
        self.loop = asyncio.new_event_loop()

    def resolve(self, value: Any) -> Any:
        if inspect.isawaitable(value):
            return self.loop.run_until_complete(value)
        return value

    def close(self) -> None:
        self.loop.run_until_complete(self.loop.shutdown_asyncgens())
        self.loop.close()


def load_callable(reference: str) -> Callable[[dict[str, Any]], Any]:
    if ":" not in reference:
        raise ValueError("callable must use module:attribute syntax")
    module_name, attribute = reference.split(":", 1)
    target = getattr(importlib.import_module(module_name), attribute)
    if not callable(target):
        raise TypeError(f"{reference} is not callable")
    return target


def fingerprint(error: BaseException) -> str:
    material = f"{type(error).__module__}.{type(error).__qualname__}".encode()
    return hashlib.sha256(material).hexdigest()[:16]


def failure(evaluator_id: str, case_id: str, error: BaseException) -> dict[str, Any]:
    return {
        "protocol": PROTOCOL,
        "protocol_version": PROTOCOL_VERSION,
        "evaluator_id": evaluator_id,
        "case_id": case_id,
        "status": "error",
        "message": type(error).__name__,
        "details": {"error_fingerprint": fingerprint(error)},
    }


def emit(response: dict[str, Any], evaluator_id: str, case_id: str) -> None:
    try:
        encoded = json.dumps(response, ensure_ascii=False)
    except (TypeError, ValueError) as error:
        encoded = json.dumps(failure(evaluator_id, case_id, error))
    print(encoded, file=PROTOCOL_STDOUT, flush=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--callable", required=True)
    parser.add_argument("--sensitive-tracebacks", action="store_true")
    args = parser.parse_args()
    startup_error: BaseException | None = None
    target: Callable[[dict[str, Any]], Any] | None = None
    try:
        with contextlib.redirect_stdout(sys.stderr):
            target = load_callable(args.callable)
    except Exception as error:  # noqa: BLE001
        startup_error = error
        if args.sensitive_tracebacks:
            traceback.print_exc(file=sys.stderr)

    runner = PersistentAsyncRunner()
    try:
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
                emit(failure(evaluator_id, case_id, error), evaluator_id, case_id)
                continue
            if startup_error is not None:
                emit(failure(evaluator_id, case_id, startup_error), evaluator_id, case_id)
                continue
            try:
                with contextlib.redirect_stdout(sys.stderr):
                    result = runner.resolve(target(request))  # type: ignore[misc]
                if isinstance(result, bool):
                    result = {
                        "status": "passed" if result else "failed",
                        "score": 1 if result else 0,
                        "message": "evaluator returned true" if result else "evaluator returned false",
                    }
                if not isinstance(result, dict):
                    raise TypeError("evaluator must return bool or dict")
                overridden = RESERVED.intersection(result)
                if overridden:
                    raise ValueError("evaluator result contains reserved protocol fields")
                response = {
                    **result,
                    "protocol": PROTOCOL,
                    "protocol_version": PROTOCOL_VERSION,
                    "evaluator_id": evaluator_id,
                    "case_id": case_id,
                }
            except Exception as error:  # noqa: BLE001
                if args.sensitive_tracebacks:
                    traceback.print_exc(file=sys.stderr)
                response = failure(evaluator_id, case_id, error)
            emit(response, evaluator_id, case_id)
    finally:
        runner.close()


if __name__ == "__main__":
    main()
