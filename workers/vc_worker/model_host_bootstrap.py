"""Fixed -I -S child entry; native model stdout/stderr are NUL."""
from pathlib import Path
import argparse
import ctypes
import os
import sys


def main():
    expected = Path(r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe")
    if (sys.version_info[:2] != (3, 12)
            or Path(sys.executable).resolve() != expected.resolve()
            or not sys.flags.no_site or not sys.flags.isolated):
        raise RuntimeError("fixed isolated interpreter required")
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--prepare-diagnostic", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    packages = root / ".local/venvs/meanvc2-m0/Lib/site-packages"
    protocol_packages = root / ".local/t011-leader/python-deps"
    if not packages.is_dir() or not protocol_packages.is_dir():
        raise RuntimeError("fixed audited packages unavailable")
    # This private FD is never put in argv/environment or given Node credentials.
    result_fd = os.dup(1)
    os.set_inheritable(result_fd, False)
    null_fd = os.open(os.devnull, os.O_WRONLY)
    try:
        os.dup2(null_fd, 1)
        os.dup2(null_fd, 2)
        import msvcrt
        api = ctypes.WinDLL("kernel32", use_last_error=True).SetStdHandle
        api.argtypes = [ctypes.c_uint32, ctypes.c_void_p]
        api.restype = ctypes.c_int
        if not api(ctypes.c_uint32(-11).value, msvcrt.get_osfhandle(1)):
            raise OSError("native stdout redirection failed")
        if not api(ctypes.c_uint32(-12).value, msvcrt.get_osfhandle(2)):
            raise OSError("native stderr redirection failed")
    finally:
        os.close(null_fd)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    sys.path.insert(0, str(packages))
    sys.path.insert(0, str(protocol_packages))  # No site.addsitedir/.pth execution.
    try:
        from model_host import main as host_main
        return host_main(result_fd, args.prepare_diagnostic)
    finally:
        os.close(result_fd)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        # NUL stderr; malformed startup/EOF cannot produce Ready.
        raise SystemExit(1)
