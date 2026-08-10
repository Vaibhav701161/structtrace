"""Versioned bridge for StructTrace Python evaluator callables."""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import dataclasses
import datetime
import decimal
import hashlib
import importlib
import inspect
import json
import math
import os
import pathlib
import sys
import traceback
import uuid
from collections.abc import Mapping
from enum import Enum
from typing import Any, Callable

PROTOCOL = "structtrace.evaluator"
PROTOCOL_VERSION = 3
PROTOCOL_STDOUT = sys.stdout
RESERVED = {"protocol", "protocol_version", "case_id", "evaluator_id"}
sys.path.insert(0, os.getcwd())


class NormalizationError(ValueError):
    def __init__(self, kind: str, message: str) -> None:
        super().__init__(message)
        self.kind = kind


class PersistentAsyncRunner:
    def __init__(self) -> None:
        self.loop = asyncio.new_event_loop()

    def resolve(self, value: Any) -> Any:
        if inspect.isawaitable(value):
            return self.loop.run_until_complete(value)
        return value

    def close(self) -> None:
        pending = asyncio.all_tasks(self.loop)
        for task in pending:
            task.cancel()
        if pending:
            self.loop.run_until_complete(asyncio.gather(*pending, return_exceptions=True))
        self.loop.run_until_complete(self.loop.shutdown_asyncgens())
        self.loop.run_until_complete(self.loop.shutdown_default_executor())
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


def normalize(value: Any) -> Any:
    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return normalize(dataclasses.asdict(value))
    if isinstance(value, Enum):
        return normalize(value.value)
    if isinstance(value, bytes):
        raise NormalizationError("bytes_require_wrapper", "bytes are not evaluator JSON")
    if isinstance(value, float) and not math.isfinite(value):
        raise NormalizationError("non_finite_number", "non-finite evaluator number")
    if isinstance(value, decimal.Decimal):
        if not value.is_finite():
            raise NormalizationError("non_finite_number", "non-finite evaluator Decimal")
        return str(value)
    if isinstance(value, uuid.UUID):
        return str(value)
    if isinstance(value, datetime.datetime):
        if value.tzinfo is None or value.utcoffset() is None:
            raise NormalizationError("naive_datetime", "datetime requires a timezone")
        return value.isoformat()
    if isinstance(value, datetime.date):
        return value.isoformat()
    if isinstance(value, pathlib.Path):
        return str(value)
    model_dump = getattr(value, "model_dump", None)
    if callable(model_dump):
        return normalize(model_dump(mode="json"))
    legacy_dict = getattr(value, "dict", None)
    if callable(legacy_dict) and not isinstance(value, Mapping):
        return normalize(legacy_dict())
    if hasattr(value, "__attrs_attrs__"):
        try:
            import attrs  # type: ignore[import-not-found]

            return normalize(attrs.asdict(value))
        except ImportError:
            pass
    if isinstance(value, Mapping):
        normalized: dict[str, Any] = {}
        for key, item in value.items():
            if not isinstance(key, str):
                raise NormalizationError("non_string_key", "mapping keys must be strings")
            if key in normalized:
                raise NormalizationError("key_collision", f"duplicate normalized key: {key}")
            normalized[key] = normalize(item)
        return normalized
    if isinstance(value, (list, tuple)):
        return [normalize(item) for item in value]
    if type(value).__module__.startswith("numpy") and hasattr(value, "item"):
        return normalize(value.item())
    return value


def reject_constant(value: str) -> Any:
    raise NormalizationError("non_finite_number", f"invalid JSON number {value}")


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise NormalizationError("duplicate_protocol_key", f"duplicate key: {key}")
        value[key] = item
    return value


def validate_request(request: dict[str, Any]) -> None:
    if request.get("protocol") != PROTOCOL:
        raise NormalizationError("protocol_error", "unexpected protocol")
    if request.get("protocol_version") != PROTOCOL_VERSION:
        raise NormalizationError("protocol_error", "unsupported protocol version")
    for field in ("case_id", "evaluator_id"):
        if not isinstance(request.get(field), str) or not request[field]:
            raise NormalizationError("protocol_error", f"{field} must be a non-empty string")


def valid_json_pointer(pointer: Any) -> bool:
    if not isinstance(pointer, str) or (pointer and not pointer.startswith("/")):
        return False
    index = 0
    while index < len(pointer):
        if pointer[index] == "~":
            if index + 1 >= len(pointer) or pointer[index + 1] not in "01":
                return False
            index += 1
        index += 1
    return True


def validate_evaluator_result(result: dict[str, Any]) -> None:
    status = result.get("status")
    if status not in {"passed", "failed", "error", "not_applicable"}:
        raise NormalizationError("invalid_evaluator_result", "unsupported evaluator status")
    score = result.get("score")
    if score is not None:
        if isinstance(score, bool) or not isinstance(score, (int, float)):
            raise NormalizationError("invalid_evaluator_result", "score must be numeric")
        if not math.isfinite(float(score)):
            raise NormalizationError("non_finite_number", "evaluator score must be finite")
        if not 0 <= score <= 1:
            raise NormalizationError("invalid_evaluator_result", "score must be between 0 and 1")
    fields = result.get("fields", [])
    if not isinstance(fields, list) or len(fields) > 10_000:
        raise NormalizationError("invalid_evaluator_result", "fields must be a bounded list")
    field_statuses: set[str] = set()
    for field in fields:
        if not isinstance(field, dict) or not valid_json_pointer(field.get("pointer")):
            raise NormalizationError("invalid_json_pointer", "field pointer is invalid")
        field_statuses.add(str(field.get("status", "")))
    if status == "passed" and field_statuses.intersection({"failed", "error"}):
        raise NormalizationError("contradictory_evaluator_result", "passed result has failed/error field")
    if status == "error" and score == 1:
        raise NormalizationError("contradictory_evaluator_result", "error result cannot score 1")
    if status == "not_applicable" and field_statuses.intersection({"passed", "failed"}):
        raise NormalizationError(
            "contradictory_evaluator_result",
            "not-applicable result has resolved pass/fail field",
        )
    if len(str(result.get("message", "")).encode()) > 16 * 1024:
        raise NormalizationError("oversized_evaluator_result", "message is too large")
    details = json.dumps(result.get("details"), ensure_ascii=False, allow_nan=False).encode()
    if len(details) > 1024 * 1024:
        raise NormalizationError("oversized_evaluator_result", "details are too large")


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
        encoded = json.dumps(response, ensure_ascii=False, allow_nan=False)
    except (TypeError, ValueError) as error:
        encoded = json.dumps(failure(evaluator_id, case_id, error), allow_nan=False)
    print(encoded, file=PROTOCOL_STDOUT, flush=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--callable", required=True)
    parser.add_argument("--check", action="store_true")
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

    if args.check:
        if startup_error is not None:
            raise SystemExit(1)
        return

    runner = PersistentAsyncRunner()
    try:
        for line in sys.stdin:
            if not line.strip():
                continue
            evaluator_id = ""
            case_id = ""
            try:
                request = json.loads(
                    line, object_pairs_hook=strict_object, parse_constant=reject_constant
                )
                if not isinstance(request, dict):
                    raise ValueError("request must be a JSON object")
                validate_request(request)
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
                validate_evaluator_result(result)
                normalized = normalize(result)
                response = {
                    **normalized,
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
