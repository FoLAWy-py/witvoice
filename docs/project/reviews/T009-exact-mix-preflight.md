# T009 explicit capture mix: limited preflight and execution

Independent reviewer `/root/reviewer_t001`, actual thread
`01a1077c-556d-7152-892c-d9f92346ec0f`, read the complete five-source
31ba delta against 1a85, actual byte bindings, all five author metadata files
and ten raw logs, the fresh two-UID metadata query, and the concrete new
configuration and supervisor. No new mandatory S0–S3 finding. This is a limited
execution preflight; task formal review attempts remain zero.

Approved exactly one standing-authorized diagnostic:
`T009-EXACT-MIX-4b726af0-03d9-4553-ab95-a4416fe45145`.
Source `7ea5bd743d50c4a586cb0ab94b3b88866772ab59`, binary SHA256
`2022f4a42ab770d64bca8bf27281830df5db81215b80e7ce5a647361654842c0`,
configuration `e5d9ebd95ce3d9388a7cec59eb6b5c97778f64612396dc8272e40d1604029137`,
supervisor `08dcef3d3a4a6d154b6659659b053b8a113cac5ab6ef7aa365141e04ca3d19ff`,
private selection `f1b3241fdd3e887e5cef6d60d152320f7b6aeca2ed4c06adc156b6cd519baeca`.

Only capture uses the complete owned GetMixFormat descriptor; render stays
basic. Same verified CABLE pair, 48kHz stereo Float32, 1440-frame capacity,
single render commit <=960 frames, synthetic marker <=2 seconds at .01,
13-second child watchdog and 15-second total limit. No physical endpoints,
PCM files/upload, default changes, fallback, or automatic retry.

Fresh metadata returned S_OK for both basic and exact descriptors on both
endpoints. This does not prove Initialize works. Default device period 10ms;
minimum render 2ms and capture 3ms. Native Initialize/Start were NOT_RUN in
the metadata query.

Executed once at 13:57:27.7491669Z on 2026-10-05; finished in 0.320018s,
child exit 1, no timeout. Render prepared with 1056 frames. Capture Initialize
again returned -2005139430 / 0x887c001a before Start, marker render or capture
read. Capture capacity, cleanup and OS buffer erasure remain UNKNOWN.
Changing the descriptor did not resolve the fault; its exact cause remains
unverified. The authorization instance is consumed and immutable.

Reviewer independently read the result and raw errors and confirmed the
failure boundary. Evidence: `docs/evidence/T009-20261005/exact-mix-preflight`,
`mix-metadata`, `exact-mix-once`. T009 remains BLOCKED; no VB-CABLE route pass,
real VC, third-party application, long-run or Mac acceptance is claimed.
