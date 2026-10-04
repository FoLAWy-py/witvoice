# T007 first slice review — one S2, whole task remains incomplete

Independent reviewer /root/reviewer_t001, frozen4fd6be043b9c5a627df36328ff00e3ef03d61ace.
Actual audio author /root/audio_runtime relayed8 complete source files plus6
public evidence files. All14 Git reads exit0, frozen range diff0, newline-only
normalization disclosed for5 shell evidence files. Reviewer no escalation,
file edits, tests, GPU/audio or independent measurement rerun.

No S0/S1 found in this metadata/conversion slice: no original-audio fallback,
nonfinite residue, conversion allocation or COM/task-memory cleanup defect was
identified. This is limited to the reviewed slice, not a whole application claim.

S2: wasapi.rs probe_format builds WAVEFORMATEX(tag1,cbSize0) even for PCM24/32.
[Microsoft WAVEFORMATEX](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex)
and [extensible formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/extensible-wave-format-descriptors)
require/use WAVEFORMATEXTENSIBLE for >16bit PCM descriptions. Current legal
AudioFormat can produce a descriptor that rejects a supported endpoint incorrectly.
Repair: pure owned descriptor builder; PCM24/32 use container=valid bits,
mono mask4/stereo mask3,PCM GUID,cbSize22; test exact fields and read roundtrip.
No Initialize/Start or additional hardware is needed for the repair.

WholeT007 cannot close: stream/capture/render/removal, notifications, ASRC,
resampling, clocks and hardware acceptance remain unimplemented/NOT_RUN as
recorded.13 slice tests and33 metadata endpoints are not physical/virtual-stream
evidence. This first independent review requires source repair before approval.
