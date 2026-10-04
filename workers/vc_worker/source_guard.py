"""Build an import snapshot from fixed Git blobs, never working-tree bytecode."""
from __future__ import annotations

import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess


def git(repo, *args):
    return subprocess.run(
        ["git", "-c", "core.fsmonitor=false", "-C", str(repo), *args],
        capture_output=True, timeout=15, check=True,
    ).stdout


def regular_path(path):
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
        raise RuntimeError("linked/reparse source path forbidden")
    return info


def snapshot_runtime(repo: Path, commit: str, destination: Path):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise RuntimeError("invalid fixed commit")
    regular_path(repo)
    regular_path(repo / "runtime")
    regular_path(destination)
    if not destination.is_dir() or any(destination.iterdir()):
        raise RuntimeError("snapshot destination must be empty")
    if git(repo, "rev-parse", "HEAD").decode().strip() != commit:
        raise RuntimeError("upstream commit mismatch")
    for args in (("diff", "--no-ext-diff", "--no-textconv", "--exit-code", "HEAD"),
                 ("diff", "--cached", "--no-ext-diff", "--no-textconv", "--exit-code", "HEAD")):
        git(repo, *args)
    entries = {}
    total = 0
    for record in git(repo, "ls-tree", "-r", "-l", "-z", commit, "--", "runtime", "src/config").split(b"\0"):
        if not record:
            continue
        meta, raw_name = record.split(b"\t", 1)
        mode, kind, oid, size = meta.split()
        name = PurePosixPath(raw_name.decode("utf-8"))
        length = int(size)
        permitted = name.parts[0] == "runtime" or name.parts[:2] == ("src", "config")
        if mode not in (b"100644", b"100755") or kind != b"blob" or name.is_absolute() or ".." in name.parts or not permitted or not 0 <= length <= 1024 * 1024:
            raise RuntimeError("unsupported source tree entry")
        total += length
        entries[name.as_posix()] = (oid.decode(), length)
    if total > 16 * 1024 * 1024 or "runtime/run_rt.py" not in entries:
        raise RuntimeError("invalid source tree budget/entrypoint")
    for folder, dirs, files in os.walk(repo / "runtime", followlinks=False):
        for child in dirs + files:
            path = Path(folder) / child
            info = regular_path(path)
            if stat.S_ISDIR(info.st_mode):
                continue
            name = path.relative_to(repo).as_posix()
            cache = path.parent.name == "__pycache__" and path.suffix == ".pyc"
            if not stat.S_ISREG(info.st_mode) or (name not in entries and not cache):
                raise RuntimeError("untracked/ignored runtime file forbidden: " + name)
    for name, (oid, size) in entries.items():
        data = git(repo, "cat-file", "blob", oid)
        if len(data) != size:
            raise RuntimeError("source blob size mismatch")
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as handle:
            handle.write(data)
    return {"commit": commit, "blob_count": len(entries), "bytes": total,
            "import_source": "fixed_git_blobs", "working_tree_bytecode_used": False}
