"""Security regressions; isolated repos only, no model or audio hardware."""
import importlib.util
import marshal
from pathlib import Path
import subprocess
import tempfile
import unittest

from source_guard import snapshot_runtime

ROOT = Path(__file__).resolve().parents[2]


class SourceGuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / ".local/temp")
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / "repo"
        self.repo.mkdir()
        self.call("init", "--quiet")
        (self.repo / "runtime").mkdir()
        self.source = self.repo / "runtime/run_rt.py"
        self.source.write_text("VALUE = 1\n", encoding="utf-8")
        (self.repo / "src/config").mkdir(parents=True)
        (self.repo / "src/config/model.json").write_text('{"fixed": true}', encoding="utf-8")
        (self.repo / ".gitignore").write_text("runtime/ignored.py\n", encoding="utf-8")
        self.call("add", ".")
        self.call("-c", "user.name=Security test", "-c", "user.email=test@localhost", "commit", "--quiet", "-m", "fixture")
        self.commit = self.call("rev-parse", "HEAD").strip()
        self.dest = Path(self.temp.name) / "snapshot"
        self.dest.mkdir()

    def call(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True, capture_output=True, text=True, timeout=15).stdout

    def reject(self):
        with self.assertRaises((RuntimeError, subprocess.CalledProcessError)):
            snapshot_runtime(self.repo, self.commit, self.dest)
        self.assertFalse((self.dest / "runtime/run_rt.py").exists())

    def test_clean_exact_blob(self):
        proof = snapshot_runtime(self.repo, self.commit, self.dest)
        self.assertEqual((self.dest / "runtime/run_rt.py").read_bytes(), b"VALUE = 1\n")
        self.assertFalse(proof["working_tree_bytecode_used"])
        self.assertEqual((self.dest / "src/config/model.json").read_text(encoding="utf-8"), '{"fixed": true}')

    def test_unstaged_rejected(self):
        self.source.write_text("VALUE = 2\n", encoding="utf-8")
        self.reject()

    def test_staged_rejected(self):
        self.source.write_text("VALUE = 2\n", encoding="utf-8")
        self.call("add", "runtime/run_rt.py")
        self.reject()

    def test_staged_change_with_restored_worktree_rejected(self):
        self.source.write_text("VALUE = 2\n", encoding="utf-8")
        self.call("add", "runtime/run_rt.py")
        self.source.write_text("VALUE = 1\n", encoding="utf-8")
        self.reject()

    def test_untracked_rejected(self):
        (self.repo / "runtime/extra.py").write_text("raise RuntimeError()\n", encoding="utf-8")
        self.reject()

    def test_ignored_rejected(self):
        (self.repo / "runtime/ignored.py").write_text("raise RuntimeError()\n", encoding="utf-8")
        self.reject()

    def test_old_bytecode_never_imported(self):
        cache = self.repo / "runtime/__pycache__"
        cache.mkdir()
        pyc = importlib.util.MAGIC_NUMBER + b"\0" * 12 + marshal.dumps(compile("VALUE = 9\n", str(self.source), "exec"))
        (cache / Path(importlib.util.cache_from_source(str(self.source))).name).write_bytes(pyc)
        snapshot_runtime(self.repo, self.commit, self.dest)
        self.assertFalse((self.dest / "runtime/__pycache__").exists())
        spec = importlib.util.spec_from_file_location("isolated_test_source", self.dest / "runtime/run_rt.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertEqual(module.VALUE, 1)

    def test_wrong_commit_and_nonempty_destination_rejected(self):
        with self.assertRaises(RuntimeError):
            snapshot_runtime(self.repo, "0" * 40, self.dest)
        (self.dest / "keep").write_text("keep", encoding="utf-8")
        self.reject()
        self.assertEqual((self.dest / "keep").read_text(encoding="utf-8"), "keep")


if __name__ == "__main__":
    unittest.main()
