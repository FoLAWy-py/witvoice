$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major-ne5-or$PSVersionTable.PSVersion.Minor-ne1){throw 'Explicit system PowerShell5.1 required'}
$ErrorActionPreference='Stop'
Set-Location 'D:\Project\witvoice'
. '.\tools\dev\toolchain-env.ps1'
$env:CARGO_TARGET_DIR='D:\Project\witvoice\.local\t008-t016-r2-leader\target'
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0'
$env:PYTHONNOUSERSITE='1'
$env:PYTHONPATH='D:\Project\witvoice\.local\t011-leader\python-deps'
function FileHash([string]$p){$f=[IO.File]::OpenRead($p);$s=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($s.ComputeHash($f))).Replace('-','').ToLowerInvariant()}finally{$f.Dispose();$s.Dispose()}}
$scope=Get-Content "$PSScriptRoot\frozen-inputs.json" -Raw|ConvertFrom-Json
function Inputs{$m=[ordered]@{};foreach($p in $scope.source_sha256.PSObject.Properties){$h=FileHash (Join-Path $PWD $p.Name);if($h-ne$p.Value){throw "Frozen source changed: $($p.Name)"};$m[$p.Name]=$h};return $m}
$commands=@(

@{name='python-tests-supervisor';exe='C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe';args=@('-m','unittest','discover','-s','workers/vc_worker','-p','test_*.py','-v')},
@{name='compile-supervisor';exe='C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe';args=@('-m','py_compile','workers/vc_worker/audit_assets.py','workers/vc_worker/control.py','workers/vc_worker/evidence_checks.py','workers/vc_worker/feasibility.py','workers/vc_worker/model_bootstrap.py','workers/vc_worker/model_host.py','workers/vc_worker/model_host_bootstrap.py','workers/vc_worker/model_process.py','workers/vc_worker/native_interop_probe.py','workers/vc_worker/prepare_config.py','workers/vc_worker/prepare_diagnostics.py','workers/vc_worker/process_memory.py','workers/vc_worker/protocol.py','workers/vc_worker/runtime.py','workers/vc_worker/source_guard.py','workers/vc_worker/test_control.py','workers/vc_worker/test_evidence_checks.py','workers/vc_worker/test_model_process.py','workers/vc_worker/test_prepare_diagnostics.py','workers/vc_worker/test_process_memory.py','workers/vc_worker/test_protocol.py','workers/vc_worker/test_runtime.py','workers/vc_worker/test_source_guard.py','workers/vc_worker/test_windows_pipe.py','workers/vc_worker/warmup.py','workers/vc_worker/windows_pipe.py','tests/integration/worker_terminal_peer.py','tests/integration/worker_supervision_peer.py')})
foreach($c in $commands){
$stem=Join-Path $PSScriptRoot $c.name;if(Test-Path "$stem.json"){throw 'Never overwrite results'};$before=Inputs
$si=New-Object Diagnostics.ProcessStartInfo;$si.FileName=$c.exe;$si.Arguments=$c.args-join' ';$si.WorkingDirectory=$PWD.Path;$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
$p=New-Object Diagnostics.Process;$p.StartInfo=$si;$o=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$e=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read');$t=[DateTime]::UtcNow.ToString('o');$w=[Diagnostics.Stopwatch]::StartNew()
try{[void]$p.Start();$a=$p.StandardOutput.BaseStream.CopyToAsync($o);$b=$p.StandardError.BaseStream.CopyToAsync($e);$p.WaitForExit();[void]$a.GetAwaiter().GetResult();[void]$b.GetAwaiter().GetResult();$code=$p.ExitCode}finally{$o.Dispose();$e.Dispose();$p.Dispose()}
$after=Inputs;$m=[ordered]@{owner='/root';host=$PSVersionTable.PSVersion.ToString();executable=$c.exe;argv=$c.args;started_utc=$t;finished_utc=[DateTime]::UtcNow.ToString('o');exit=$code;wall_seconds=$w.Elapsed.TotalSeconds;source_before=$before;source_after=$after;model='NOT_RUN';GPU='NOT_RUN';audio='NOT_RUN';stdout=@{bytes=(Get-Item "$stem.stdout").Length;sha256=(FileHash "$stem.stdout")};stderr=@{bytes=(Get-Item "$stem.stderr").Length;sha256=(FileHash "$stem.stderr")}}
[IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 20),(New-Object Text.UTF8Encoding($false)));Write-Output "$($c.name) exit=$code";if($code-ne0){exit $code}
}