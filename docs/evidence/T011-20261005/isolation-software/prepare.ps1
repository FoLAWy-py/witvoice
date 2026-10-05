$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.ToString() -ne '5.1.26100.7462'){throw 'System5.1 required'}
Set-Location 'D:\Project\witvoice'
. '.\tools\dev\toolchain-env.ps1'
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0'
$env:CARGO_TARGET_DIR='D:\Project\witvoice\.local\t008-t016-r2-leader\target'
function Run([string]$name,[string]$exe,[string[]]$argv){
 $stem=Join-Path $PSScriptRoot $name
 $si=New-Object Diagnostics.ProcessStartInfo;$si.FileName=$exe;$si.Arguments=$argv-join' ';$si.WorkingDirectory=$PWD.Path;$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $p=New-Object Diagnostics.Process;$p.StartInfo=$si
 $o=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$e=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read');$t=[DateTime]::UtcNow.ToString('o');$w=[Diagnostics.Stopwatch]::StartNew()
 try{[void]$p.Start();$a=$p.StandardOutput.BaseStream.CopyToAsync($o);$b=$p.StandardError.BaseStream.CopyToAsync($e);$p.WaitForExit();[void]$a.GetAwaiter().GetResult();[void]$b.GetAwaiter().GetResult();$code=$p.ExitCode}finally{$o.Dispose();$e.Dispose();$p.Dispose()}
 $m=@{host=$PSVersionTable.PSVersion.ToString();executable=$exe;argv=$argv;started_utc=$t;finished_utc=[DateTime]::UtcNow.ToString('o');exit=$code;wall_seconds=$w.Elapsed.TotalSeconds;GPU='NOT_RUN';model='NOT_RUN';audio='NOT_RUN'}
 [IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 10),(New-Object Text.UTF8Encoding($false)))
 Write-Output "$name exit=$code";if($code-ne0){exit $code}
}
[IO.File]::Copy((Join-Path $PWD 'Cargo.lock'),(Join-Path $PSScriptRoot 'before-Cargo.lock'),$false)
Run 'lock' "$env:CARGO_HOME\bin\cargo.exe" @('generate-lockfile','--offline')
Run 'format' "$env:CARGO_HOME\bin\rustfmt.exe" @('--edition','2024','crates/engines/examples/worker_warmup_probe.rs','crates/platform/tests/model_job.rs')
Run 'model-job' "$env:CARGO_HOME\bin\cargo.exe" @('test','--locked','-p','witvoice-platform','--test','model_job','--','--nocapture')
