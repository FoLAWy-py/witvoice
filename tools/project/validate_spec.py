#!/usr/bin/env python3
"""Validate the SPEC bundle and project ledger, NOT the application or its hardware.

Requires Python 3.11+ for tomllib. No network requests or user-data modifications.
Run from any current directory: python tools/project/validate_spec.py
"""
from __future__ import annotations
import json
import re
import sys
from pathlib import Path
try:
    import tomllib
except ImportError:
    raise SystemExit("This documentation validator requires Python 3.11+.")

ROOT = Path(__file__).resolve().parents[2]
ERRORS: list[str] = []
def require(condition: bool, message: str) -> None:
    if not condition:
        ERRORS.append(message)

def read_json(relative: str) -> dict:
    try:
        return json.loads((ROOT / relative).read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise SystemExit(f"Cannot load {relative}: {exc}") from exc

def local_existing_file(value: object) -> bool:
    if not isinstance(value, str) or not value:
        return False
    p = (ROOT / value).resolve()
    try:
        p.relative_to(ROOT)
    except ValueError:
        return False
    return p.is_file()


def main() -> int:
    task_doc = read_json("docs/project/TASKS.json")
    tasks = task_doc.get("tasks", [])
    state = read_json("docs/project/STATE.json")
    req_doc = read_json("docs/project/REQUIREMENTS.json")
    reqs = req_doc.get("requirements", [])
    expected_ids = {f"T{i:03}" for i in range(1, 36)}
    expected_reqs = {f"REQ-{i:02}" for i in range(1, 33)}
    expected_ms = {f"M{i}" for i in range(7)}
    expected_roles = {"leader", "frontend", "backend", "audio_runtime", "ml_engine", "hci", "reviewer"}
    ids = [t.get("id") for t in tasks]
    require(len(tasks) == 35 and set(ids) == expected_ids and len(set(ids)) == len(ids), "Exactly T001–T035 must exist once.")
    require(task_doc.get("top_level_task_limit") == 35, "The finite task limit must remain 35 unless the approved SPEC changes.")
    require({r.get("id") for r in reqs} == expected_reqs and len(reqs) == 32, "Exactly REQ-01–REQ-32 must exist once.")
    require(set(state.get("milestones", {})) == expected_ms, "Only M0–M6 are valid milestones.")
    covered: set[str] = set()
    by_id = {t["id"]: t for t in tasks}
    for task in tasks:
        tid = task["id"]
        require(task.get("milestone") in expected_ms, f"{tid}: unknown milestone")
        require(task.get("owner_role") in expected_roles, f"{tid}: unknown role")
        require(task.get("status") in {"TODO", "IN_PROGRESS", "REVIEW", "DONE", "BLOCKED"}, f"{tid}: invalid status")
        require(bool(task.get("acceptance")), f"{tid}: acceptance is empty")
        require(bool(task.get("allowed_paths")), f"{tid}: no path scope")
        require(set(task.get("depends_on", [])) <= expected_ids, f"{tid}: unknown dependency")
        require(tid not in task.get("depends_on", []), f"{tid}: self dependency")
        covered.update(task.get("requirements", []))
        require(set(task.get("requirements", [])) <= expected_reqs, f"{tid}: unknown requirement")
        if task.get("status") == "DONE":
            require(bool(task.get("evidence")), f"{tid}: DONE without evidence")
            require(all(local_existing_file(p) for p in task.get("evidence", [])), f"{tid}: evidence path missing or outside repository")
            require(local_existing_file(task.get("review")), f"{tid}: DONE requires an existing independent review report path")
            require(all(by_id[d].get("status") == "DONE" for d in task.get("depends_on", [])), f"{tid}: dependency not DONE")
    require(covered == expected_reqs, "Every requirement must map to at least one task.")
    # Ensure the dependency graph is acyclic.
    visiting: set[str] = set()
    visited: set[str] = set()
    def visit(tid: str) -> None:
        if tid in visiting:
            ERRORS.append(f"Dependency cycle at {tid}")
            return
        if tid in visited or tid not in by_id:
            return
        visiting.add(tid)
        for dep in by_id[tid].get("depends_on", []):
            visit(dep)
        visiting.remove(tid)
        visited.add(tid)
    for tid in ids:
        visit(tid)
    for req in reqs:
        actual = {t["id"] for t in tasks if req["id"] in t.get("requirements", [])}
        require(actual == set(req.get("task_ids", [])), f"{req['id']}: traceability map is stale")
    roles_found: set[str] = set()
    for path in sorted((ROOT / ".codex" / "agents").glob("*.toml")):
        try:
            cfg = tomllib.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            ERRORS.append(f"Invalid TOML {path.name}: {exc}")
            continue
        role = cfg.get("name")
        roles_found.add(role)
        require(role == path.stem, f"{path.name}: filename/name mismatch")
        require(bool(cfg.get("description")) and bool(cfg.get("developer_instructions")), f"{path.name}: missing required role fields")
        require((ROOT / "docs" / "roles" / f"{role}.md").is_file(), f"{role}: missing role guide")
        if role == "reviewer":
            require(cfg.get("sandbox_mode") == "read-only", "Reviewer must default to read-only.")
    require(roles_found == expected_roles, "Exactly the seven documented roles must be configured.")
    try:
        cfg = tomllib.loads((ROOT / ".codex/config.toml").read_text(encoding="utf-8"))
        require(cfg.get("agents", {}).get("max_concurrent_threads_per_session") == 4, "Project concurrency must be four subagents.")
        require("model" not in cfg, "Base config should not force an unverified account model.")
    except (OSError, ValueError) as exc:
        ERRORS.append(f"Invalid base TOML: {exc}")
    for ms in expected_ms:
        ms_tasks = [t for t in tasks if t.get("milestone") == ms]
        require(len(ms_tasks) == 5, f"{ms}: expected five top-level tasks")
        if state.get("milestones", {}).get(ms, {}).get("status") == "DONE":
            require(all(t["status"] == "DONE" for t in ms_tasks), f"{ms}: marked DONE before all tasks")
    project_status = state.get("project_status")
    require(project_status in {"NOT_STARTED", "IN_PROGRESS", "WINDOWS_DELIVERED", "BLOCKED", "COMPLETE"}, "Invalid project status")
    if project_status in {"WINDOWS_DELIVERED", "COMPLETE"}:
        needed = [t for t in tasks if project_status == "COMPLETE" or t["milestone"] != "M6"]
        require(all(t["status"] == "DONE" for t in needed), "Delivery status set before required tasks are DONE.")
    if project_status in {"WINDOWS_DELIVERED", "COMPLETE"}:
        require(state.get("hardware", {}).get("windows") == "VERIFIED", "Delivery requires real Windows evidence.")
        required_ms = expected_ms if project_status == "COMPLETE" else expected_ms - {"M6"}
        require(all(state["milestones"][m].get("status") == "DONE" for m in required_ms), "Delivery requires closed milestone gates.")
    if project_status == "COMPLETE":
        require(all(q.get("status") == "VERIFIED" for q in reqs), "COMPLETE requires all 32 requirements verified.")
        require(state.get("completion_claim_allowed") is True, "COMPLETE needs an explicit final completion gate.")
        require(state.get("hardware", {}).get("macos") == "VERIFIED", "COMPLETE requires real Mac evidence.")
        require(state.get("hardware", {}).get("lan_pair") == "VERIFIED", "COMPLETE requires real bilateral LAN evidence.")
    require((ROOT / "AGENTS.md").stat().st_size <= 8192, "Root instructions must remain short; detailed SPEC belongs in docs.")
    plan = (ROOT / "docs/PROJECT_PLAN.md").read_text(encoding="utf-8")
    plan_ids = set(re.findall(r"^\| (T\d{3}) \|", plan, flags=re.MULTILINE))
    require(plan_ids == expected_ids, "Human-readable PLAN and TASKS differ.")
    if ERRORS:
        print("SPEC/ledger consistency: FAIL")
        for error in ERRORS:
            print(f"- {error}")
        return 1
    print("SPEC/ledger consistency: PASS")
    print("7 milestones | 35 tasks | 32 traced requirements | 7 agent roles")
    print("TOML syntax checked; installed Codex configuration compatibility is NOT tested.")
    print("Application code, Windows/Mac builds, audio, models and LAN performance are NOT tested.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
