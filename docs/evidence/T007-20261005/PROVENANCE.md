# T007 slice provenance

Actual source author /root/audio_runtime; root integration executor /root.
Environment doctor-final.json under T001-20261004:Windows11Pro x64, ordinary
user, MSVC17.14.41/SDK10.0.26100,Rust/Cargo1.99.0. Current-process environment
only from tools/dev/toolchain-env.ps1; no system PATH/driver/firewall changes.

First slice source frozen4fd6be043b9c5a627df36328ff00e3ef03d61ace. Root metadata
and24-test workspace checks occurred before this commit while author stopped
writing; source hashes at command start were NOT_CAPTURED. After freeze actual
audio relay ran git diff --exit-code frozen -- crates/audio docs/evidence/T007-20261005
exit0, and all14 frozen Git reads exit0. Cache/working/Git texts matched after
explicit CRLF->LF normalization;8 source SHA values matched byte-for-byte,
5 shell evidence SHA differed only due newlines. This is a limited provenance
chain, not a retrospectively captured start hash or independent reviewer rerun.

Metadata command exit0 reads only COM endpoint state/format,33 endpoints8active;
raw UID JSON only.local, public record contains counts/SHA. No Initialize/Start,
recording/playback. The source file will change for S2 probe descriptors; do not
relabel this old metadata command as a new source/hardware measurement.

Independent slice review at4fd6be0 found one S2 for PCM24/32 descriptors. Actual
author repair adds2 native descriptor field/roundtrip tests; author now reports
15audio tests plus fmt/clippy0 after GPU lease released. Separate leader final
workspace integration commands are archived underfinal-*; wholeT007 remains
IN_PROGRESS, stream/removal/ASRC/real device route still unimplemented/NOT_RUN.
