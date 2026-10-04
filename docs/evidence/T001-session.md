# T001 session baseline

Task: T001; requirements: REQ-01, REQ-31; dependencies: none.
Leader: /root. Write scope: tools/dev/, docs/evidence/; leader alone integrates docs/project/.
Contract: read-only native Windows doctor; no installation, downloads, recording, firewall changes or automatic repair. Missing/failed/unknown probes must never become PASS.
Acceptance: run docs validator, doctor, doctor negative-path tests; independent reviewer reads frozen source hashes and complete outputs. No application or hardware acceptance claimed.

Initial inventory: repository supplied as a directory without .git. No reset/clean performed. Existing source material preserved. PowerShell 7.6.5; Windows 11 Pro 10.0.26200 x64; i9-14900HX; RTX 4060 Laptop detected (not model/backend verification). Node v24.13.0, pnpm 11.19.0, Git 2.52.0.windows.1, Codex CLI 0.153.4. PATH python is a WindowsApps alias which exits 9009; bundled Python 3.12.14 runs.

Execution limitation: default exec_command and node_repl fail before execution with apply deny-read ACLs. Leader's scoped commands run after automatic review using require_escalated; no global permission/configuration change. Child agents must not escalate. This is not an automatic approval rejection.

Actual custom role probes dispatched, no second leader: /root/reviewer, /root/backend, /root/audio_runtime, /root/ml_engine. These canonical runtime IDs are returned by collaboration; no UUID is exposed. Write leases: leader tools/dev/ and ledger only, children read-only. Hardware/GPU leases: none. Frontend/HCI probes follow once slots are released.

Initial docs validator: bundled Python tools/project/validate_spec.py, exit 0, SPEC/ledger consistency PASS. This does not test installed Codex config, application, audio, model or Mac.
