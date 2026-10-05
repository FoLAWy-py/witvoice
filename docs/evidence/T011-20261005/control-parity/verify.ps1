$env:PYTHONPATH$env:PYTHONPATH='D:\Project\witvoice\.local\t011-leader\python-deps;D:\Project\witvoice\workers\vc_worker'
$ErrorActionPreference$env:PYTHONPATH='Stop'
$repo$env:PYTHONPATH='D:\Project\witvoice';Set-Location $repo
. "$repo\tools\dev\toolchain-env.ps1"
$env:CARGO_TARGET_DIR$env:PYTHONPATH="$repo\.local\t008-t016-r2-leader\target";$env:CARGO_BUILD_JOBS$env:PYTHONPATH='1';$env:CARGO_INCREMENTAL$env:PYTHONPATH='0'
$enc$env:PYTHONPATH=New-Object Text.UTF8Encoding($false)
function Hash([string]$p){$s$env:PYTHONPATH=[IO.File]::OpenRead($p);$h$env:PYTHONPATH=[Security.Cryptography.SHA256]::Create();try{[BitConverter]::ToString($h.ComputeHash($s)).Replace('-','').ToLowerInvariant()}finally{$s.Dispose();$h.Dispose()}}
function Run([string]$name,[string]$exe,[string[]]$argv){
 $stem$env:PYTHONPATH=Join-Path $PSScriptRoot $name;if(Test-Path "$stem.json"){throw 'Evidence exists'}
 $si$env:PYTHONPATH=New-Object Diagnostics.ProcessStartInfo;$si.FileName$env:PYTHONPATH=$exe;$si.Arguments$env:PYTHONPATH=$argv -join ' ';$si.WorkingDirectory$env:PYTHONPATH=$repo;$si.UseShellExecute$env:PYTHONPATH=$false;$si.CreateNoWindow$env:PYTHONPATH=$true;$si.RedirectStandardOutput$env:PYTHONPATH=$true;$si.RedirectStandardError$env:PYTHONPATH=$true
 $out$env:PYTHONPATH=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$err$env:PYTHONPATH=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read');$p$env:PYTHONPATH=New-Object Diagnostics.Process;$p.StartInfo$env:PYTHONPATH=$si;$watch$env:PYTHONPATH=[Diagnostics.Stopwatch]::StartNew();$utc$env:PYTHONPATH=[DateTime]::UtcNow.ToString('o')
 try{if(-not $p.Start()){throw 'Startfailed'};$a$env:PYTHONPATH=$p.StandardOutput.BaseStream.CopyToAsync($out);$b$env:PYTHONPATH=$p.StandardError.BaseStream.CopyToAsync($err);$p.WaitForExit();[void]$a.GetAwaiter().GetResult();[void]$b.GetAwaiter().GetResult();$exit$env:PYTHONPATH=$p.ExitCode}finally{$out.Dispose();$err.Dispose();$p.Dispose()}
 $m$env:PYTHONPATH=[ordered]@{task$env:PYTHONPATH='T011';owner$env:PYTHONPATH='/root';name$env:PYTHONPATH=$name;exe$env:PYTHONPATH=$exe;argv$env:PYTHONPATH=$argv;cwd$env:PYTHONPATH=$repo;host$env:PYTHONPATH=$PSVersionTable.PSVersion.ToString();started_utc$env:PYTHONPATH=$utc;finished_utc$env:PYTHONPATH=[DateTime]::UtcNow.ToString('o');wall_seconds$env:PYTHONPATH=$watch.Elapsed.TotalSeconds;exit$env:PYTHONPATH=$exit;hardware$env:PYTHONPATH='NOT_RUN';model$env:PYTHONPATH='NOT_RUN';stdout$env:PYTHONPATH=@{bytes$env:PYTHONPATH=(Get-Item "$stem.stdout").Length;sha256$env:PYTHONPATH=(Hash "$stem.stdout")};stderr$env:PYTHONPATH=@{bytes$env:PYTHONPATH=(Get-Item "$stem.stderr").Length;sha256$env:PYTHONPATH=(Hash "$stem.stderr")}}
 [IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 12),$enc);Write-Output "$name exit$env:PYTHONPATH=$exit";if($exit -ne 0){exit $exit}
}
Run 'rust-unicode' "$env:CARGO_HOME\bin\cargo.exe" @('test','--locked','-p','witvoice-contracts','--test','worker_wire','--','--nocapture')
Run 'python-control' 'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' @('-m','unittest','discover','-s','workers/vc_worker','-p','test_control.py','-v')
Run 'python-repro-after' 'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' @('.local/t011-control-parity/before.py')