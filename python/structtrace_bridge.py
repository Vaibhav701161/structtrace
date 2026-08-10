"""Versioned JSONL bridge for StructTrace Python callables."""

from __future__ import annotations

import argparse
import asyncio
import base64
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

PROTOCOL = "structtrace.variant"
PROTOCOL_VERSION = 3
PROTOCOL_STDOUT = sys.stdout

# The bridge lives under `.structtrace/runtime`; user modules resolve from the
# configured project working directory rather than the bridge directory.
sys.path.insert(0, os.getcwd())


@dataclasses.dataclass(frozen=True)
class StructTraceEnvelope:
    """Explicit opt-in wrapper for an already constructed protocol response."""

    value: dict[str, Any]


@dataclasses.dataclass(frozen=True)
class StructTraceBase64:
    """Explicit opt-in wrapper for binary data."""

    value: bytes


class NormalizationError(ValueError):
    """A stable per-case normalization failure."""

    def __init__(self, kind: str, message: str) -> None:
        super().__init__(message)
        self.kind = kind


# User modules may explicitly import the wrapper from `structtrace_bridge` even
# though this file is executed as the worker entry point.
sys.modules.setdefault("structtrace_bridge", sys.modules[__name__])


class PersistentAsyncRunner:
    """One event loop shared for the complete persistent worker lifetime."""

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


def normalize(value: Any) -> Any:
    """Convert ordinary application value types into deterministic JSON values."""

    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return normalize(dataclasses.asdict(value))
    if isinstance(value, Enum):
        return normalize(value.value)
    if isinstance(value, StructTraceBase64):
        return {"$base64": base64.b64encode(value.value).decode("ascii")}
    if isinstance(value, bytes):
        raise NormalizationError("bytes_require_wrapper", "bytes require StructTraceBase64")
    if isinstance(value, float) and not math.isfinite(value):
        raise NormalizationError("non_finite_number", "non-finite numbers are not JSON")
    if isinstance(value, decimal.Decimal):
        if not value.is_finite():
            raise NormalizationError("non_finite_number", "non-finite Decimal is not JSON")
        return str(value)
    if isinstance(value, uuid.UUID):
        return str(value)
    if isinstance(value, datetime.datetime):
        if value.tzinfo is None or value.utcoffset() is None:
            raise NormalizationError(
                "naive_datetime", "datetime values must include an explicit timezone"
            )
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
    if not isinstance(request.get("case_id"), str) or not request["case_id"]:
        raise NormalizationError("protocol_error", "case_id must be a non-empty string")


def error_fingerprint(error: BaseException) -> str:
    material = f"{type(error).__module__}.{type(error).__qualname__}".encode()
    return hashlib.sha256(material).hexdigest()[:16]


def success(case_id: str, value: Any) -> dict[str, Any]:
    if isinstance(value, StructTraceEnvelope):
        reserved = {"protocol", "protocol_version", "case_id"}.intersection(value.value)
        if reserved:
            raise ValueError("StructTraceEnvelope contains reserved protocol fields")
        normalized = normalize(value.value)
        return {
            **normalized,
            "protocol": PROTOCOL,
            "protocol_version": PROTOCOL_VERSION,
            "case_id": case_id,
        }
    envelope: dict[str, Any] = {
        "protocol": PROTOCOL,
        "protocol_version": PROTOCOL_VERSION,
        "case_id": case_id,
        "status": "ok",
    }
    if isinstance(value, str):
        envelope["raw_output"] = value
    else:
        envelope["output"] = normalize(value)
    return envelope


def failure(case_id: str, error: BaseException, kind: str = "python_exception") -> dict[str, Any]:
    return {
        "protocol": PROTOCOL,
        "protocol_version": PROTOCOL_VERSION,
        "case_id": case_id,
        "status": "error",
        "error": {
            "kind": kind,
            "message": f"Python callable failed with {type(error).__name__}",
            "fingerprint": error_fingerprint(error),
        },
    }


def emit(response: dict[str, Any], case_id: str) -> None:
    try:
        encoded = json.dumps(response, ensure_ascii=False, allow_nan=False)
    except (TypeError, ValueError) as error:
        kind = error.kind if isinstance(error, NormalizationError) else "non_serializable_output"
        encoded = json.dumps(failure(case_id, error, kind), allow_nan=False)
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
    except Exception as error:  # noqa: BLE001 - converted into protocol evidence
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
            case_id = ""
            try:
                request = json.loads(
                    line, object_pairs_hook=strict_object, parse_constant=reject_constant
                )
                if not isinstance(request, dict):
                    raise ValueError("request must be a JSON object")
                validate_request(request)
                case_id = str(request.get("case_id", ""))
            except (json.JSONDecodeError, ValueError) as error:
                emit(failure(case_id, error, "protocol_error"), case_id)
                continue
            if startup_error is not None:
                emit(failure(case_id, startup_error, "startup_error"), case_id)
                continue
            case = {
                "input": request.get("input"),
                "metadata": request.get("metadata"),
            }
            try:
                with contextlib.redirect_stdout(sys.stderr):
                    value = runner.resolve(target(case))  # type: ignore[misc]
                response = success(case_id, value)
            except Exception as error:  # noqa: BLE001 - converted into protocol evidence
                if args.sensitive_tracebacks:
                    traceback.print_exc(file=sys.stderr)
                kind = error.kind if isinstance(error, NormalizationError) else "python_exception"
                response = failure(case_id, error, kind)
            emit(response, case_id)
    finally:
        runner.close()


if __name__ == "__main__":
    main()
