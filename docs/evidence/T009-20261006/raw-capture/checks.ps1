$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major -ne 5){throw 'System PowerShell5 required'}
$repo='D:\Project\witvoice';Set-Location $repo
. "$repo\tools\dev\toolchain-env.ps1"
$env:CARGO_TARGET_DIR="$repo\.local\t008-t016-r2-leader\target"
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0'
& "$env:CARGO_HOME\bin\rustfmt.exe" --edition 2024 crates/audio/src/stream/native.rs crates/audio/examples/route_probe.rs
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& "$repo\.local\t009-raw-root\check-runner.ps1"
exit $LASTEXITCODE
