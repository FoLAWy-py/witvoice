param([Parameter(Mandatory=$true)][string]$Label,[Parameter(Mandatory=$true)][ValidateSet('fmt','fmtcheck','platform','observation','lifecycle','clippy','examples')][string]$Kind)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
Set-Location -LiteralPath 'D:\Project\witvoice'
. tools/dev/toolchain-env.ps1
if($env:CARGO_HOME -ne 'D:\Software\WitvoiceToolchain\Rust\cargo' -or $env:RUSTUP_HOME -ne 'D:\Software\WitvoiceToolchain\Rust\rustup'){throw 'Audited D toolchain required'}
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0';$env:CARGO_TARGET_DIR='D:\Project\witvoice\.local\t008-t016-r2-leader\target'
if($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS){throw 'Unexpected compiler flags'}
$sources=@('crates/platform/src/process.rs','crates/engines/src/supervisor.rs','crates/engines/tests/worker_supervision.rs','tests/integration/worker_supervision_peer.py')
$paths=@($sources)+@('crates/platform/src/lib.rs','crates/engines/src/lifecycle.rs','Cargo.toml','Cargo.lock')
$manifests=@(& rg --files -g Cargo.toml -g '!node_modules' -g '!.local')
if($LASTEXITCODE -ne 0){throw 'Manifest inventory failed'}
$paths=@($paths+$manifests | Sort-Object -Unique)
function Snapshot {
 $list=@();foreach($path in $paths){$file=Join-Path 'D:\Project\witvoice' $path;$list+=@{path=$path;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $file).Length}};return $list
}
$before=Snapshot
$binary=Join-Path $env:CARGO_HOME 'bin\cargo.exe'
switch($Kind){
 'fmt' {$binary=Join-Path $env:CARGO_HOME 'bin\rustfmt.exe';$argv=@('--edition','2024')+$sources[0..2]}
 'fmtcheck' {$binary=Join-Path $env:CARGO_HOME 'bin\rustfmt.exe';$argv=@('--edition','2024','--check')+$sources[0..2]}
 'platform' {$argv=@('test','--locked','--offline','-p','witvoice-platform','--lib')}
 'observation' {$argv=@('test','--locked','--offline','-p','witvoice-engines','--test','worker_supervision','cleanup_observation','--','--test-threads=1')}
 'lifecycle' {$argv=@('test','--locked','--offline','-p','witvoice-engines','--test','lifecycle')}
 'clippy' {$argv=@('clippy','--locked','--offline','--workspace','--all-targets','--features','witvoice-node/process-tests','--','-D','warnings')}
 'examples' {$argv=@('build','--locked','--offline','--workspace','--examples')}
}
$dir='D:\Project\witvoice\.local\t011-lifetime-author'
$stdout=Join-Path $dir ($Label+'.stdout');$stderr=Join-Path $dir ($Label+'.stderr');$meta=Join-Path $dir ($Label+'.json')
if((Test-Path -LiteralPath $stdout) -or (Test-Path -LiteralPath $stderr) -or (Test-Path -LiteralPath $meta)){throw 'Unique check artifact exists'}
$process=New-Object Diagnostics.Process
$process.StartInfo.FileName=$binary
$process.StartInfo.Arguments=($argv | ForEach-Object{'"'+($_ -replace '"','\"')+'"'}) -join ' '
$process.StartInfo.UseShellExecute=$false;$process.StartInfo.CreateNoWindow=$true
$process.StartInfo.RedirectStandardOutput=$true;$process.StartInfo.RedirectStandardError=$true
$start=[DateTime]::UtcNow.ToString('o');[void]$process.Start()
$out=$process.StandardOutput.ReadToEndAsync();$err=$process.StandardError.ReadToEndAsync()
$process.WaitForExit();$end=[DateTime]::UtcNow.ToString('o')
[IO.File]::WriteAllText($stdout,$out.Result,[Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText($stderr,$err.Result,[Text.UTF8Encoding]::new($false))
$result=@{label=$Label;kind=$Kind;actual_agent_uuid='01a10767-4ff5-7f21-881a-146d094fbc55';binary=$binary;argv=$argv;cwd='D:\Project\witvoice';start_utc=$start;end_utc=$end;exit_code=$process.ExitCode;host=$PSVersionTable.PSVersion.ToString();env=@{CARGO_HOME=$env:CARGO_HOME;RUSTUP_HOME=$env:RUSTUP_HOME;CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR;CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS;CARGO_INCREMENTAL=$env:CARGO_INCREMENTAL;RUSTFLAGS=$env:RUSTFLAGS;CARGO_ENCODED_RUSTFLAGS=$env:CARGO_ENCODED_RUSTFLAGS};source_before=$before;source_after=(Snapshot);stdout=@{path=$stdout;sha256=(Get-FileHash -LiteralPath $stdout -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $stdout).Length};stderr=@{path=$stderr;sha256=(Get-FileHash -LiteralPath $stderr -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $stderr).Length};runner_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()}
[IO.File]::WriteAllText($meta,($result|ConvertTo-Json -Depth 16),[Text.UTF8Encoding]::new($false))
$result|ConvertTo-Json -Depth 16
exit $process.ExitCode
