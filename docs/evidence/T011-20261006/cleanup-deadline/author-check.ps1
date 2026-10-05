
param([Parameter(Mandatory=$true)][string]$Label,[Parameter(Mandatory=$true)][ValidateSet('fmt','fmtcheck','tests','clippy')][string]$Kind)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
Set-Location -LiteralPath 'D:\Project\witvoice'
. tools/dev/toolchain-env.ps1
if($env:CARGO_HOME -ne 'D:\Software\WitvoiceToolchain\Rust\cargo' -or $env:RUSTUP_HOME -ne 'D:\Software\WitvoiceToolchain\Rust\rustup') { throw 'Audited D drive toolchain environment required' }
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_INCREMENTAL='0'
$env:CARGO_TARGET_DIR='D:\Project\witvoice\.local\t008-t016-r2-leader\target'
if($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS) { throw 'Unexpected compiler flags' }
$paths=@('crates/engines/src/supervisor.rs','crates/engines/tests/worker_supervision.rs','tests/integration/worker_supervision_peer.py','crates/platform/src/process.rs','crates/platform/src/lib.rs','crates/engines/src/lifecycle.rs','Cargo.lock')
function Snapshot {
 $a=@()
 foreach($p in $paths){$f=Join-Path 'D:\Project\witvoice' $p;$a+=@{path=$p;sha256=(Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $f).Length}}
 return $a
}
$before=Snapshot
switch($Kind){
 'fmt' {$binary=Join-Path $env:CARGO_HOME 'bin\rustfmt.exe';$argv=@('--edition','2024',$paths[0],$paths[1])}
 'fmtcheck' {$binary=Join-Path $env:CARGO_HOME 'bin\rustfmt.exe';$argv=@('--edition','2024','--check',$paths[0],$paths[1])}
 'tests' {$binary=Join-Path $env:CARGO_HOME 'bin\cargo.exe';$argv=@('test','--locked','--offline','-p','witvoice-engines','--test','worker_supervision','cleanup_observation','--','--test-threads=1')}
 'clippy' {$binary=Join-Path $env:CARGO_HOME 'bin\cargo.exe';$argv=@('clippy','--locked','--offline','-p','witvoice-engines','--all-targets','--','-D','warnings')}
}
$dir='D:\Project\witvoice\.local\t011-cleanup-deadline-author'
$stdout=Join-Path $dir ($Label+'.stdout');$stderr=Join-Path $dir ($Label+'.stderr');$meta=Join-Path $dir ($Label+'.json')
if((Test-Path -LiteralPath $stdout) -or (Test-Path -LiteralPath $stderr) -or (Test-Path -LiteralPath $meta)){throw 'Unique author artifact exists'}
$process=New-Object Diagnostics.Process
$process.StartInfo.FileName=$binary
$process.StartInfo.Arguments=($argv|ForEach-Object{'"'+($_ -replace '"','\"')+'"'} ) -join ' '
$process.StartInfo.UseShellExecute=$false
$process.StartInfo.CreateNoWindow=$true
$process.StartInfo.RedirectStandardOutput=$true
$process.StartInfo.RedirectStandardError=$true
$start=[DateTime]::UtcNow.ToString('o');[void]$process.Start()
$out=$process.StandardOutput.ReadToEndAsync();$err=$process.StandardError.ReadToEndAsync()
$process.WaitForExit();$end=[DateTime]::UtcNow.ToString('o')
[IO.File]::WriteAllText($stdout,$out.Result,(New-Object Text.UTF8Encoding($false)))
[IO.File]::WriteAllText($stderr,$err.Result,(New-Object Text.UTF8Encoding($false)))
$result=@{label=$Label;kind=$Kind;actual_agent_uuid='01a10767-4ff5-7f21-881a-146d094fbc55';binary=$binary;argv=$argv;cwd='D:\Project\witvoice';start_utc=$start;end_utc=$end;exit_code=$process.ExitCode;host=$PSVersionTable.PSVersion.ToString();env=@{CARGO_HOME=$env:CARGO_HOME;RUSTUP_HOME=$env:RUSTUP_HOME;CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR;CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS;CARGO_INCREMENTAL=$env:CARGO_INCREMENTAL;RUSTFLAGS=$env:RUSTFLAGS;CARGO_ENCODED_RUSTFLAGS=$env:CARGO_ENCODED_RUSTFLAGS};source_before=$before;source_after=(Snapshot);stdout=@{path=$stdout;sha256=(Get-FileHash -LiteralPath $stdout -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $stdout).Length};stderr=@{path=$stderr;sha256=(Get-FileHash -LiteralPath $stderr -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $stderr).Length};runner_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()}
[IO.File]::WriteAllText($meta,($result|ConvertTo-Json -Depth 16),(New-Object Text.UTF8Encoding($false)))
$result|ConvertTo-Json -Depth 16
exit $process.ExitCode

