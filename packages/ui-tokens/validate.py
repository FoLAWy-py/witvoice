"""T021 static tokens/coverage checks. Never certifies rendered desktop UI."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
ROLES = {"canvas", "surface", "elevated", "text", "secondary", "border",
         "accent", "accent_text", "focus", "success", "warning", "danger"}


def luminance(color: str) -> float:
    values = [int(color[index:index + 2], 16) / 255 for index in (1, 3, 5)]
    linear = [value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4
              for value in values]
    return sum(weight * value for weight, value in zip((0.2126, 0.7152, 0.0722), linear))


def contrast(left: str, right: str) -> float:
    lighter, darker = sorted((luminance(left), luminance(right)), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def stylesheet(data: dict) -> str:
    result = ["/* Generated from tokens.json by validate.py --write-css. */", ":root {"]
    for name, value in data["layout"].items():
        suffix = "px" if name.endswith("_px") else "ms" if name.endswith("_ms") else ""
        result.append(f"  --vc-{name.replace('_', '-')}: {value}{suffix};")
    result += ['  --vc-font-ui: "Segoe UI", "Microsoft YaHei UI", system-ui, sans-serif;', "}"]
    for theme in ("dark", "light"):
        colors = data["themes"][theme]
        selector = ':root, [data-vc-theme="dark"]' if theme == "dark" else '[data-vc-theme="light"]'
        result += [selector + " {", f"  color-scheme: {theme};"]
        result += [f"  --vc-{role.replace('_', '-')}: {value};" for role, value in colors.items()]
        result.append("}")
    result += ["@media (prefers-reduced-motion: reduce) {", "  :root {",
               "    --vc-motion-fast-ms: 0ms;", "    --vc-motion-normal-ms: 0ms;", "  }", "}"]
    return "\n".join(result) + "\n"


def check(data: dict) -> tuple[list[str], list[dict]]:
    errors, measurements = [], []
    if set(data["themes"]) != {"dark", "light"}:
        errors.append("Both dark and light themes required")
    for theme, colors in data["themes"].items():
        if set(colors) != ROLES or any(not re.fullmatch(r"#[0-9a-f]{6}", value) for value in colors.values()):
            errors.append(f"{theme}: invalid semantic color roles")
            continue
        pairs = [(fg, bg, 4.5) for fg in ("text", "secondary", "accent", "success", "warning", "danger")
                 for bg in ("canvas", "surface", "elevated")]
        pairs += [("accent_text", "accent", 4.5)]
        pairs += [(fg, bg, 3.0) for fg in ("border", "focus") for bg in ("canvas", "surface", "elevated")]
        for fg, bg, minimum in pairs:
            ratio = contrast(colors[fg], colors[bg])
            passed = ratio >= minimum  # Never round for threshold comparison.
            measurements.append({"theme": theme, "foreground": fg, "background": bg,
                                 "contrast": ratio, "minimum": minimum, "passed": passed})
            if not passed:
                errors.append(f"{theme} {fg}/{bg}: {ratio:.6f} < {minimum}")
    layout = data["layout"]
    if layout["text_size_px"] < 14 or layout["control_min_px"] < 32:
        errors.append("Body/control size below SPEC05 minimum")
    if (layout["window_min_width_px"], layout["window_min_height_px"]) != (1024, 680):
        errors.append("Window baseline drift")
    if layout["focus_width_px"] < 2 or layout["focus_offset_px"] < 2:
        errors.append("Focus outline lacks minimum visible geometry")
    # Check intended state coverage against the actual exported contract, not a copied enum.
    schema = json.loads((ROOT / "crates/contracts/bindings/schema.json").read_text(encoding="utf-8"))
    states = schema["SessionState"]["enum"]
    coverage = (ROOT / "docs/design/states.md").read_text(encoding="utf-8")
    missing = [state for state in states if not re.search(r"^\| " + re.escape(state) + r" \|", coverage, re.MULTILINE)]
    if missing:
        errors.append("Missing authoritative state coverage: " + ", ".join(missing))
    if abs(contrast("#ffffff", "#000000") - 21) > 1e-12:
        errors.append("Contrast calculation reference failed")
    return errors, measurements


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write-css", action="store_true")
    args = parser.parse_args()
    started = datetime.now(timezone.utc).isoformat()
    data = json.loads((HERE / "tokens.json").read_text(encoding="utf-8"))
    errors, measurements = check(data)
    generated = stylesheet(data)
    destination = HERE / "tokens.css"
    if args.write_css and not errors:
        destination.write_text(generated, encoding="utf-8", newline="\n")
    if not destination.is_file() or destination.read_text(encoding="utf-8") != generated:
        errors.append("Generated CSS drift; run --write-css after reviewing token changes")
    print(json.dumps({"started_at": started, "finished_at": datetime.now(timezone.utc).isoformat(),
                      "result": "FAIL" if errors else "PASS_STATIC_TOKEN_CHECKS_ONLY",
                      "errors": errors, "contrast_pairs": measurements,
                      "desktop_render_keyboard_zoom": "NOT_RUN", "owner_quality_signoff": "NOT_RUN"}, indent=2))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
