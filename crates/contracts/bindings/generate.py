"""Generate UI types and Python constants only from Rust-exported schema.

Run export_schema first. --check compares without overwriting generated files.
Unknown schema constructs fail instead of silently emitting `any`.
"""
import argparse
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent


def ts_type(schema):
    if "$ref" in schema:
        return schema["$ref"].rsplit("/", 1)[-1]
    if "enum" in schema:
        return " | ".join(json.dumps(value) for value in schema["enum"])
    for union in ("anyOf", "oneOf"):
        if union in schema:
            return "(" + " | ".join(ts_type(part) for part in schema[union]) + ")"
    if "allOf" in schema:
        return "(" + " & ".join(ts_type(part) for part in schema["allOf"]) + ")"
    kind = schema.get("type")
    if isinstance(kind, list):
        return "(" + " | ".join(ts_type({**schema, "type": item}) for item in kind) + ")"
    if kind in ("integer", "number"):
        return "number"
    if kind in ("string", "boolean", "null"):
        return kind
    if kind == "array":
        return "Array<" + ts_type(schema["items"]) + ">"
    if kind == "object":
        required = schema.get("required", [])
        fields = [json.dumps(key) + ("" if key in required else "?") + ": " + ts_type(value)
                  for key, value in sorted(schema.get("properties", {}).items())]
        return "{ " + "; ".join(fields) + " }"
    raise ValueError("Unsupported schema: " + json.dumps(schema))


def generate(bundle):
    schemas = {key: value for key, value in bundle.items() if isinstance(value, dict) and "$schema" in value}
    definitions = {}
    for schema in schemas.values():
        for name, definition in schema.get("definitions", {}).items():
            if name in definitions and definitions[name] != definition:
                raise ValueError("Conflicting Rust definitions: " + name)
            definitions[name] = definition
    types = {**definitions, **schemas}
    content = "// Generated from Rust contracts. Do not hand edit. Runtime semantic validation remains in Node.\n"
    content += "export const PROTOCOL_VERSION = " + str(bundle["protocol_version"]) + " as const;\n"
    for name, schema in sorted(types.items()):
        content += "export type " + name + " = " + ts_type(schema) + ";\n"
    python = '# Generated from Rust contracts. Do not hand edit.\n'
    for key in ("protocol_version", "control_max_bytes", "profile_max_bytes"):
        python += key.upper() + " = " + str(bundle[key]) + "\n"
    return {"contracts.ts": content, "wire_constants.py": python,
            "media-golden.json": json.dumps(bundle["golden_media"], indent=2) + "\n",
            "worker-media.json": json.dumps(bundle["worker_media"], indent=2) + "\n"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--schema", type=Path, default=HERE / "schema.json")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    files = generate(json.loads(args.schema.read_text(encoding="utf-8")))
    for name, content in files.items():
        destination = HERE / name
        if args.check:
            if not destination.is_file() or destination.read_text(encoding="utf-8") != content:
                raise SystemExit("Generated binding drift: " + name)
        else:
            destination.write_text(content, encoding="utf-8", newline="\n")
    print("Rust-derived bindings " + ("consistent" if args.check else "generated"))


if __name__ == "__main__":
    main()
