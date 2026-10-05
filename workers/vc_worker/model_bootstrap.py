"""Fixed direct interpreter launch; retain kernel PID identity across IPC.

Windows venv redirectors may launch a different Python PID. This entry uses the
fixed base interpreter and the existing audited model environment's package
directory directly, without executing site .pth files or weakening pipe auth.
"""
from pathlib import Path
import runpy
import sys


def main():
    expected = Path(r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe")
    if sys.version_info[:2] != (3, 12) or Path(sys.executable).resolve() != expected.resolve():
        raise RuntimeError("fixed direct model interpreter required")
    root = Path(__file__).resolve().parents[2]
    packages = root / ".local/venvs/meanvc2-m0/Lib/site-packages"
    if not packages.is_dir():
        raise RuntimeError("audited model environment unavailable")
    sys.path.insert(0, str(packages))
    runpy.run_path(str(root / "workers/vc_worker/runtime.py"), run_name="__main__")


if __name__ == "__main__":
    main()
