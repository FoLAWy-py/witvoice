$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major-ne5){throw 'SystemPS5 required'}
$repo='D:\Project\witvoice';Set-Location $repo
. tools/dev/toolchain-env.ps1
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0';$env:CARGO_TARGET_DIR="$repo\.local\t008-t016-r2-leader\target"
if($env:RUSTFLAGS-or$env:CARGO_ENCODED_RUSTFLAGS){throw 'Unexpected compiler override'}
$frozen=Get-Content "$PSScriptRoot\frozen.json" -Raw|ConvertFrom-Json
function Inputs{$m=[ordered]@{};foreach($v in $frozen.PSObject.Properties){$h=(Get-FileHash -LiteralPath "$repo\$($v.Name)" -Algorithm SHA256).Hash.ToLowerInvariant();if($h-ne$v.Value){throw "Source changed $($v.Name)"};$m[$v.Name]=$h};return $m}
$checks=@(
@{name='01-platform';argv=@('test','--locked','--offline','-p','witvoice-platform','--lib')},
@{name='02-observation';argv=@('test','--locked','--offline','-p','witvoice-engines','--test','worker_supervision','cleanup_observation','--','--test-threads=1')},
@{name='03-lifecycle';argv=@('test','--locked','--offline','-p','witvoice-engines','--test','lifecycle')},
@{name='03b-audio';argv=@('test','--locked','--offline','-p','witvoice-audio','--all-targets')},
@{name='04-fmt';argv=@('fmt','--all','--check')},
@{name='05-clippy';argv=@('clippy','--locked','--offline','--workspace','--all-targets','--features','witvoice-node/process-tests,witvoice-session/test-support','--','-D','warnings')},
@{name='06-examples';argv=@('build','--locked','--offline','--workspace','--examples')})
foreach($c in $checks){
$stem=Join-Path $PSScriptRoot $c.name;if(Test-Path "$stem.json"){throw 'No overwrite'}
$before=Inputs;$start=[DateTime]::UtcNow.ToString('o');$w=[Diagnostics.Stopwatch]::StartNew()
$si=New-Object Diagnostics.ProcessStartInfo;$si.FileName="$env:CARGO_HOME\bin\cargo.exe";$si.Arguments=$c.argv-join' ';$si.WorkingDirectory=$repo;$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
$o=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$e=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read');$p=New-Object Diagnostics.Process;$p.StartInfo=$si
try{if(-not$p.Start()){throw 'Check launch failed'};$ot=$p.StandardOutput.BaseStream.CopyToAsync($o);$et=$p.StandardError.BaseStream.CopyToAsync($e);if(-not$p.WaitForExit(180000)){$p.Kill();throw 'Check timeout'};if(-not[Threading.Tasks.Task]::WaitAll(@($ot,$et),2000)){throw 'Drain timeout'};$code=$p.ExitCode}finally{$o.Dispose();$e.Dispose();$p.Dispose()}
$after=Inputs
$m=[ordered]@{command=@($si.FileName)+$c.argv;cwd=$repo;start_utc=$start;end_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$w.Elapsed.TotalSeconds;exit=$code;source_before=$before;source_after=$after;host=$PSVersionTable.PSVersion.ToString();native_model_audio_LAN='NOT_RUN';stdout_sha256=(Get-FileHash "$stem.stdout").Hash.ToLowerInvariant();stderr_sha256=(Get-FileHash "$stem.stderr").Hash.ToLowerInvariant();target=$env:CARGO_TARGET_DIR;jobs=1;incremental=0}
[IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 10),(New-Object Text.UTF8Encoding($false)))
Write-Output "$($c.name) exit=$code";if($code-ne0){exit $code}
}
