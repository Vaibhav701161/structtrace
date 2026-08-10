"""Versioned JSONL bridge for StructTrace Python callables."""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import dataclasses
import hashlib
import importlib
import inspect
import json
import os
import pathlib
import sys
import traceback
from collections.abc import Mapping
from enum import Enum
from typing import Any, Callable

PROTOCOL = "structtrace.variant"
PROTOCOL_VERSION = 2
PROTOCOL_STDOUT = sys.stdout

# The bridge lives under `.structtrace/runtime`; user modules resolve from the
# configured project working directory rather than the bridge directory.
sys.path.insert(0, os.getcwd())


@dataclasses.dataclass(frozen=True)
class StructTraceEnvelope:
    """Explicit opt-in wrapper for an already constructed protocol response."""

    value: dict[str, Any]


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


def normalize(value: Any) -> Any:
    """Convert ordinary application value types into deterministic JSON values."""

    if dataclasses.is_dataclass(value) and not isinstance(value, type):
        return normalize(dataclasses.asdict(value))
    if isinstance(value, Enum):
        return normalize(value.value)
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
        return {str(key): normalize(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [normalize(item) for item in value]
    return value


def error_fingerprint(error: BaseException) -> str:
    material = f"{type(error).__module__}.{type(error).__qualname__}".encode()
    return hashlib.sha256(material).hexdigest()[:16]


def success(case_id: str, value: Any) -> dict[str, Any]:
    if isinstance(value, StructTraceEnvelope):
        reserved = {"protocol", "protocol_version", "case_id"}.intersection(value.value)
        if reserved:
            raise ValueError("StructTraceEnvelope contains reserved protocol fields")
        return {
            **value.value,
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
        encoded = json.dumps(response, ensure_ascii=False)
    except (TypeError, ValueError) as error:
        encoded = json.dumps(failure(case_id, error, "non_serializable_output"))
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
    except Exception as error:  # noqa: BLE001 - converted into protocol evidence
        startup_error = error
        if args.sensitive_tracebacks:
            traceback.print_exc(file=sys.stderr)

    runner = PersistentAsyncRunner()
    try:
        for line in sys.stdin:
            case_id = ""
            try:
                request = json.loads(line)
                if not isinstance(request, dict):
                    raise ValueError("request must be a JSON object")
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
                response = failure(case_id, error)
            emit(response, case_id)
    finally:
        runner.close()


if __name__ == "__main__":
    main()
