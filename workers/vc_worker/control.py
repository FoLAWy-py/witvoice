"""Validate worker JSON against the Rust-exported schema; never resolve remote refs."""
from __future__ import annotations
import json
from pathlib import Path
import re
from jsonschema import Draft7Validator

ROOT = Path(__file__).resolve().parents[2]
BUNDLE = json.loads((ROOT / "crates/contracts/bindings/schema.json").read_text(encoding="utf-8"))


def _local_refs(value):
    if isinstance(value, dict):
        if "$ref" in value and not value["$ref"].startswith("#/definitions/"):
            raise RuntimeError("nonlocal worker schema reference forbidden")
        for child in value.values(): _local_refs(child)
    elif isinstance(value, list):
        for child in value: _local_refs(child)


SCHEMAS = {name: BUNDLE[name] for name in ("WorkerRequest", "WorkerResponse")}
for schema in SCHEMAS.values():
    _local_refs(schema)
    Draft7Validator.check_schema(schema)
VALIDATORS = {name: Draft7Validator(schema) for name, schema in SCHEMAS.items()}


def _semantic(schema, value, definitions):
    if "$ref" in schema:
        name = schema["$ref"].rsplit("/", 1)[-1]
        if name == "PreparedCapabilities":
            if any(not value[key] for key in ("engine_id", "backend", "conditioning_schema", "capability_test_run_id",
                                               "native_input_rate", "native_output_rate", "chunk_samples")):
                raise ValueError("unready worker capabilities")
        if name == "Id" and not re.fullmatch(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}", value):
            raise ValueError("invalid canonical worker identifier")
        if name == "Sha256" and not re.fullmatch(r"[0-9a-f]{64}", value):
            raise ValueError("invalid worker asset digest")
        if name == "DecimalU64":
            if not re.fullmatch(r"0|[1-9][0-9]{0,19}", value) or int(value) > (1 << 64) - 1:
                raise ValueError("invalid worker u64")
        return _semantic(definitions[name], value, definitions)
    if schema.get("format") in ("uint16", "uint32"):
        maximum = (1 << int(schema["format"][4:])) - 1
        if type(value) is not int or not 0 <= value <= maximum:
            raise ValueError("worker integer overflow")
    if schema.get("type") == "object":
        for key, child in schema.get("properties", {}).items():
            if key in value: _semantic(child, value[key], definitions)
    for union in ("oneOf", "anyOf", "allOf"):
        for part in schema.get(union, []):
            # Matching is selected using the authoritative Rust shape, not a second enum list.
            probe = {**part, "definitions": definitions}
            if Draft7Validator(probe).is_valid(value):
                _semantic(part, value, definitions)


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result: raise ValueError("duplicate worker JSON field")
        result[key] = value
    return result


def decode_control(payload: bytes, schema_name: str, expected_binding=None):
    if schema_name not in SCHEMAS or not 1 <= len(payload) <= BUNDLE["control_max_bytes"]:
        raise ValueError("worker control bound or direction")
    try:
        def invalid_constant(_): raise ValueError("nonfinite worker JSON")
        value = json.loads(payload, object_pairs_hook=_object, parse_constant=invalid_constant)
        if not VALIDATORS[schema_name].is_valid(value):
            raise ValueError("invalid worker control shape")
        _semantic(SCHEMAS[schema_name], value, SCHEMAS[schema_name].get("definitions", {}))
        if value["protocol_version"] != BUNDLE["protocol_version"]:
            raise ValueError("unsupported worker protocol")
        binding = value["binding"]
        if not int(binding["session_tag"]) or not binding["epoch"]:
            raise ValueError("invalid worker binding")
        if expected_binding is not None and binding != expected_binding:
            raise ValueError("stale worker control")
        if schema_name == "WorkerResponse" and value["event"]["kind"] == "Ready":
            cap = value["event"]["args"]["capabilities"]
            wire = BUNDLE["worker_media"]
            if (cap["engine_id"], cap["backend"], cap["native_input_rate"], cap["native_output_rate"], cap["chunk_samples"]) != (
                    wire["engine_id"], wire["backend_label"], wire["native_rate"], wire["native_rate"], wire["chunk_samples"]):
                raise ValueError("unverified worker capability shape")
        return value
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("invalid worker JSON") from error
