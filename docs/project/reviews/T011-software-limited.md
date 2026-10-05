# T011 limited worker codec and lifecycle review

Independent reviewer `/root/reviewer_t001`, UUID
`01a1077c-556d-7152-892c-d9f92346ec0f`, read eight complete frozen source
files and actual complete Rust wire/lifecycle and Python control/PCM logs.
Source baseline `7ea5bd743d50c4a586cb0ab94b3b88866772ab59`.

One S2 was reproduced: JSON escaped lone surrogate in a valid Ready string
was accepted by Python but rejected by Rust. The minimal recursive Unicode
scalar check is frozen in `6d1f85e4ef4d70558c088ad31fc33eea9f50bc11`.
Reviewer read the full three-file delta and complete actual result metadata
and raw streams. Rust wire 6 tests, Python control 5, reproduction after fix,
scoped rustfmt and strict contracts Clippy all exited zero. The three source
hashes and five unchanged inherited files are in
`docs/evidence/T011-20261005/control-parity/delivery-index.json`.
The reviewer explicitly confirmed that final frozen binding in a subsequent
report, correcting the earlier report's pending-binding statement.

The S2 is closed for this software slice; no remaining mandatory issue in
the read slice. Earlier baseline has 185 workspace tests/fmt/strict Clippy/
examples/default checks zero, but whole-workspace checks after the Unicode
delta were NOT_RUN at this review. The pure lifecycle is policy only: it
cannot prove a process Job terminated or released GPU resources.

Not included in this review: newly authored native worker pipe transport,
Python native client, actual Python process ownership/kill/leases, real model
warmup/Ready or heartbeat/media integration. T011 remains IN_PROGRESS and
formal task review attempt count remains zero; this report does not close
the task or advertise a new model/backend capability.
