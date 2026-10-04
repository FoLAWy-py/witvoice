# T006 first slice provenance

Actual /root/backend authored11 disjoint module files; root manifests/lockfile
integration by/root. Existing26 external packages stayed pinned; only3 local
workspace packages added. Root scoped fmt,19tests (11contracts+5session+3real
Windows processes),clippy exit0 in leader-checks.json. Tests run as ordinary
non-elevated user, using actual named pipes and independent Node/client processes;
no mic/playback/GPU/LAN or production model capability.

Author earlier check exit0,first fmt-check exit1 fixed,final checks exit0,
8module tests passed; final toolwall7.874s, clock before command17:46:21UTC.
Precise author shell start/end times and source start hashes NOT_CAPTURED;
do not backfill them. Root integration has its own real timestamps/logs and is
distinct from author checks. Source subsequently frozen by leader for independent
review; no claim of reviewer rerun.

WholeT006 remains IN_PROGRESS: worker Job supervision,Tauri launcher/UI Job
isolation,explicit complete shutdown/sleep/audio leases,different-user hardware
and Mac evidence are not implemented or not tested. Control-only Prepare blocks
without real resources; no Ready/Running or original-audio path is fabricated.
