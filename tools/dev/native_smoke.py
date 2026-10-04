"""Compile and run fixed Windows C and Rust programs, without audio/network."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import tempfile

import doctor


def batch_path(path: Path) -> str:
    value = str(path.resolve())
    if any(c in value for c in '\r\n"%&|<>^!'):
        raise ValueError("Unsafe characters in native probe path")
    return '"' + value + '"'


def run() -> dict:
    started = doctor.now()
    local = doctor.ROOT / ".local/native-smoke"
    local.mkdir(parents=True, exist_ok=True)
    # Keep binaries outside Git for reproducibility; each run has a fresh directory.
    work = Path(tempfile.mkdtemp(prefix="run-", dir=local))
    c_source = work / "hello.c"
    c_exe = work / "hello-c.exe"
    rust_source = work / "hello.rs"
    rust_exe = work / "hello-rust.exe"
    c_source.write_text('#include <windows.h>\n#include <stdio.h>\nint main(void) {\n'
                        ' puts("witvoice-native-msvc-ok");\n return GetCurrentProcessId() == 0;\n}\n', encoding="ascii")
    rust_source.write_text('fn main() { println!("witvoice-native-rust-ok"); }\n', encoding="ascii")
    checks = {}
    versions = doctor.msvc()
    compilers = versions.get("compilers", [])
    valid = [c for c in compilers if c["status"] == "VERSION_EXECUTED_NOT_COMPILED"]
    if valid:
        # cl.exe lives at VC/Tools/MSVC/<ver>/bin/Hostx64/x64/cl.exe.
        cl = Path(valid[-1]["argv"][0])
        install = cl.parents[7]
        vcvars = install / "VC/Auxiliary/Build/vcvars64.bat"
        batch = work / "compile.cmd"
        batch.write_text('@echo off\ncall ' + batch_path(vcvars) + '\n'
                         'if errorlevel 1 exit /b %errorlevel%\n'
                         + batch_path(cl) + ' /nologo /W4 /WX /Fe:' + batch_path(c_exe)
                         + ' /Fo:' + batch_path(work / "hello.obj") + ' ' + batch_path(c_source)
                         + '\nexit /b %errorlevel%\n', encoding="utf-8")
        cmd = doctor.find_exe("cmd.exe")
        checks["msvc_compile"] = doctor.probe([cmd, "/d", "/c", str(batch)], timeout=60) if cmd else doctor.missing("cmd.exe missing")
    else:
        checks["msvc_compile"] = doctor.missing("No executable MSVC compiler version discovered")
    if checks["msvc_compile"]["status"] == "PASS" and c_exe.is_file():
        checks["msvc_execute"] = doctor.probe([str(c_exe)])
        if checks["msvc_execute"]["stdout"] != "witvoice-native-msvc-ok":
            checks["msvc_execute"]["status"] = "FAILED"
    else:
        checks["msvc_execute"] = {"status": "SKIPPED", "reason": "C compile did not produce a runnable binary"}
    rustc = doctor.find_exe("rustc.exe")
    checks["rust_compile"] = doctor.probe([rustc, "--edition=2024", "--target", "x86_64-pc-windows-msvc",
                                          str(rust_source), "-o", str(rust_exe)], timeout=60) if rustc else doctor.missing("rustc.exe missing")
    if checks["rust_compile"]["status"] == "PASS" and rust_exe.is_file():
        checks["rust_execute"] = doctor.probe([str(rust_exe)])
        if checks["rust_execute"]["stdout"] != "witvoice-native-rust-ok":
            checks["rust_execute"]["status"] = "FAILED"
    else:
        checks["rust_execute"] = {"status": "SKIPPED", "reason": "Rust compile did not produce a runnable binary"}
    return {"schema_version": 1, "task_id": "T001", "requirements": ["REQ-01", "REQ-31"],
            **doctor.git_snapshot(),
            "evidence_level": "C", "kind": "WINDOWS_NATIVE_COMPILE_AND_EXECUTE",
            "started_at": started, "finished_at": doctor.now(), "checks": checks,
            "result": "PASS" if all(c["status"] == "PASS" for c in checks.values()) else "BLOCKED",
            "artifacts": {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in work.iterdir() if p.is_file()},
            "measurement_boundary": "Compiler/linker and fixed executable execution, no product runtime",
            "not_tested": ["WASAPI", "virtual_cable", "real_model", "macOS", "LAN", "Tauri"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run()
    doctor.write_report(args.output, result)
    print(json.dumps(result, ensure_ascii=True, indent=2))
    return 0 if result["result"] == "PASS" else 2


if __name__ == "__main__":
    raise SystemExit(main())
