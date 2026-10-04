"""Read-only Windows toolchain doctor. No installs, network or audio access.

Run with a real Python 3.11+ interpreter (the WindowsApps alias is not one).
Exit 0: version/discovery checks passed; 2: missing/failed/unknown prerequisites.
Neither result certifies compilation, audio, model, LAN or desktop acceptance.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
TIMEOUT = 15
OUTPUT_LIMIT = 16_384


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def probe(argv: list[str], *, timeout: float = TIMEOUT) -> dict:
    """Static callers supply argv; stdin is closed and a wedged probe is killed.

    Temporary output files bound memory even if an executable emits huge output.
    Only OUTPUT_LIMIT characters per stream are retained in diagnostics.
    """
    start = time.monotonic()
    result = {"argv": argv, "exit_code": None, "stdout": "", "stderr": ""}
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        try:
            with subprocess.Popen(
                argv, stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                env={**os.environ, "VSLANG": "1033"},
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            ) as proc:
                try:
                    result["exit_code"] = proc.wait(timeout=timeout)
                    result["status"] = "PASS" if proc.returncode == 0 else "FAILED"
                except subprocess.TimeoutExpired:
                    # Kill the still-live owned PID and descendants before its parent
                    # disappears; wrapper children must not continue compiling/writing.
                    tree_kill = Path(os.environ.get("SystemRoot", "C:/Windows")) / "System32/taskkill.exe"
                    if os.name == "nt" and tree_kill.is_file() and proc.poll() is None:
                        try:
                            cleanup = subprocess.run(
                                [str(tree_kill), "/PID", str(proc.pid), "/T", "/F"],
                                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, timeout=5,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                            )
                            result["tree_cleanup_exit_code"] = cleanup.returncode
                        except (OSError, subprocess.TimeoutExpired) as exc:
                            result["tree_cleanup_error"] = str(exc)
                    if proc.poll() is None:
                        proc.kill()
                    proc.wait()
                    result["status"] = "TIMEOUT"
        except OSError as exc:
            result["status"] = "UNKNOWN"
            result["error"] = str(exc)
        for key, handle in (("stdout", out), ("stderr", err)):
            handle.seek(0)
            raw = handle.read(OUTPUT_LIMIT + 1)
            result[key] = raw[:OUTPUT_LIMIT].decode("utf-8", errors="replace").strip()
            result[key + "_truncated"] = len(raw) > OUTPUT_LIMIT
    result["elapsed_seconds"] = round(time.monotonic() - start, 4)
    return result


def missing(reason: str) -> dict:
    return {"status": "MISSING", "exit_code": None, "reason": reason}


def find_exe(name: str, fallback: Path | None = None) -> str | None:
    found = shutil.which(name)
    if found:
        return found
    return str(fallback) if fallback and fallback.is_file() else None


def version(name: str, args: list[str], fallback: Path | None = None) -> dict:
    exe = find_exe(name, fallback)
    return probe([exe, *args]) if exe else missing(f"{name} absent from PATH and checked fallback")


def inventory() -> dict:
    shell = find_exe("pwsh.exe") or find_exe("powershell.exe")
    if not shell:
        return missing("No native PowerShell executable in PATH")
    run = probe([shell, "-NoLogo", "-NoProfile", "-NonInteractive", "-File",
                 str(Path(__file__).with_name("windows_inventory.ps1"))])
    if run["status"] == "PASS":
        try:
            run["data"] = json.loads(run["stdout"])
            if not isinstance(run["data"], dict):
                raise ValueError("Inventory must be a JSON object")
            if run["data"].get("errors") or run["data"].get("platform") != "Win32NT":
                run["status"] = "UNKNOWN"
        except (ValueError, TypeError) as exc:
            run["status"] = "UNKNOWN"
            run["error"] = f"Invalid inventory JSON: {exc}"
    return run


def node_cli(name: str, node: str | None) -> dict:
    # Known local wrappers point at JS entrypoints; never execute arbitrary .cmd text.
    wrapper = find_exe(name + ".cmd")
    if not wrapper:
        return missing(f"{name}.cmd absent from PATH")
    if not node:
        return missing(f"{name} found at {wrapper}, but node.exe is missing")
    base = Path(wrapper).parent
    entries = ([base / "node_modules/@openai/codex/bin/codex.js"] if name == "codex"
               else [base / "node_modules/pnpm/bin/pnpm.cjs"])
    if name == "pnpm" and len(base.parents) > 1:
        entries.append(base.parents[1] / "node/node_modules/pnpm/bin/pnpm.cjs")
    for entry in entries:
        if entry.is_file():
            run = probe([node, str(entry), "--version"])
            run["wrapper"] = wrapper
            return run
    return {"status": "UNKNOWN", "exit_code": None, "wrapper": wrapper,
            "reason": "Known JS entrypoint not found; wrapper was not executed",
            "checked": [str(p) for p in entries]}


def msvc() -> dict:
    installer = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
    vswhere = installer / "Microsoft Visual Studio/Installer/vswhere.exe"
    if not vswhere.is_file():
        return missing(f"Visual Studio discovery tool absent: {vswhere}; MSVC installation UNKNOWN")
    discovery = probe([str(vswhere), "-products", "*", "-requires",
                       "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-format", "json", "-utf8"])
    if discovery["status"] != "PASS":
        return discovery
    try:
        instances = json.loads(discovery["stdout"])
        if not isinstance(instances, list) or any(
            not isinstance(item, dict) or not isinstance(item.get("installationPath"), str)
            or not item["installationPath"] for item in instances
        ):
            raise ValueError("vswhere must return an array of objects with installationPath")
    except (ValueError, TypeError) as exc:
        discovery["status"] = "UNKNOWN"
        discovery["error"] = str(exc)
        return discovery
    compilers = []
    for item in instances:
        install = Path(item["installationPath"])
        for compiler in sorted(install.glob("VC/Tools/MSVC/*/bin/Hostx64/x64/cl.exe")):
            run = probe([str(compiler), "/Bv"])
            # cl /Bv without a source reports its version and returns 'no source' 2.
            banner = run["stdout"] + run["stderr"]
            if run["exit_code"] == 2 and "Microsoft" in banner and "19." in banner:
                run["status"] = "VERSION_EXECUTED_NOT_COMPILED"
            run["installation_version"] = item.get("installationVersion")
            compilers.append(run)
    status = "MISSING" if not compilers else (
        "DISCOVERED_NOT_COMPILED" if any(c["status"] == "VERSION_EXECUTED_NOT_COMPILED" for c in compilers)
        else "FAILED"
    )
    return {"status": status,
            "discovery": discovery, "compilers": compilers}


def assess(checks: dict) -> tuple[str, list[str]]:
    accepted = {"PASS", "DISCOVERED_NOT_COMPILED", "VERSION_EXECUTED_NOT_COMPILED"}
    blockers = [name for name, result in checks.items() if result.get("status") not in accepted]
    rust = checks.get("rustc", {})
    if rust.get("status") == "PASS" and "host: x86_64-pc-windows-msvc" not in rust.get("stdout", ""):
        blockers.append("rust_msvc_host")
    return ("BLOCKED" if blockers else "VERSION_BASELINE_ONLY"), blockers


def git_snapshot() -> dict:
    git = find_exe("git.exe")
    if not git:
        return {"commit": None, "dirty": None}
    revision = probe([git, "-C", str(ROOT), "rev-parse", "HEAD"])
    state = probe([git, "-C", str(ROOT), "status", "--porcelain"])
    return {"commit": revision["stdout"] if revision["status"] == "PASS" else None,
            "dirty": bool(state["stdout"]) if state["status"] == "PASS" else None}


def collect() -> dict:
    started = now()
    snapshot = git_snapshot()
    native = inventory()
    data = native.get("data", {})
    if not isinstance(data, dict):
        data = {}
    cargo_bin = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))) / "bin"
    node = find_exe("node.exe")
    checks = {
        "windows_powershell": native,
        "git": version("git.exe", ["--version"]),
        "rustc": version("rustc.exe", ["-Vv"], cargo_bin / "rustc.exe"),
        "cargo": version("cargo.exe", ["--version"], cargo_bin / "cargo.exe"),
        "rustup": version("rustup.exe", ["--version"], cargo_bin / "rustup.exe"),
        "node": version("node.exe", ["--version"]),
        "pnpm": node_cli("pnpm", node),
        "codex": node_cli("codex", node),
        "python": probe([sys.executable, "-c", "import sys; print(sys.version); print(sys.executable)"]),
        "msvc": msvc(),
        "windows_sdk": data.get("sdk", {"status": "UNKNOWN", "reason": "Inventory unavailable"}),
    }
    status, blockers = assess(checks)
    path_python = find_exe("python.exe")
    path_status = "ALIAS_NOT_EXECUTED" if path_python and "windowsapps" in path_python.lower() else "DISCOVERED_NOT_EXECUTED"
    git_repo = probe([find_exe("git.exe"), "-C", str(ROOT), "rev-parse", "--is-inside-work-tree"]) if find_exe("git.exe") else missing("git.exe missing")
    files = sorted(Path(__file__).parent.glob("*.py")) + sorted(Path(__file__).parent.glob("*.ps1"))
    return {
        "schema_version": 1, "run_id": "T001-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
        "task_id": "T001", "requirements": ["REQ-01", "REQ-31"], **snapshot,
        "evidence_level": "C", "kind": "WINDOWS_TOOLCHAIN_DISCOVERY",
        "environment": data, "started_at": started, "finished_at": now(),
        "checks": checks, "result": status, "blockers": blockers,
        "path_python": {"path": path_python, "status": path_status},
        "python_source": "CURRENT_INTERPRETER (may be bundled; see absolute path)",
        "git_repository": git_repo,
        "source_sha256": {str(p.relative_to(ROOT)).replace("\\", "/"): hashlib.sha256(p.read_bytes()).hexdigest() for p in files},
        "measurement_boundary": "Version/discovery subprocesses only; elapsed_seconds is probe wall time, never audio latency",
        "not_tested": ["native_compilation", "WASAPI", "virtual_cable", "microphone_permissions",
                       "real_model", "CUDA", "macOS", "LAN", "desktop", "installation"],
    }


def write_report(path: Path, report: dict) -> None:
    resolved = path.resolve()
    if not resolved.is_relative_to(ROOT / "docs/evidence"):
        raise ValueError("Report destination must be inside repository docs/evidence/")
    resolved.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation protects previous evidence and user files.
    with resolved.open("x", encoding="utf-8") as handle:
        json.dump(report, handle, ensure_ascii=False, indent=2)
        handle.write("\n")


def main() -> int:
    # Windows redirected stdout otherwise inherits a legacy code page.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="New JSON evidence path under docs/evidence/ (never overwrite)")
    args = parser.parse_args()
    report = collect()
    if args.output:
        write_report(args.output, report)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 2 if report["result"] == "BLOCKED" else 0


if __name__ == "__main__":
    raise SystemExit(main())
