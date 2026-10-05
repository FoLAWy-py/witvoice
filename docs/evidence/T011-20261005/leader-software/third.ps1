$env:PYTHONPATH='D:\Project\witvoice\.local\t011-leader\python-deps'
$ErrorActionPreference='Stop'
$repo='D:\Project\witvoice';Set-Location $repo
. "$repo\tools\dev\toolchain-env.ps1"
$env:CARGO_TARGET_DIR="$repo\.local\t008-t016-r2-leader\target";$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0'
$enc=New-Object Text.UTF8Encoding($false)
function Hash([string]$p){$s=[IO.File]::OpenRead($p);$h=[Security.Cryptography.SHA256]::Create();try{[BitConverter]::ToString($h.ComputeHash($s)).Replace('-','').ToLowerInvariant()}finally{$s.Dispose();$h.Dispose()}}
function Run([string]$name,[string]$exe,[string[]]$argv){
 $stem=Join-Path $PSScriptRoot $name;if(Test-Path "$stem.json"){throw 'Evidence exists'}
 $si=New-Object Diagnostics.ProcessStartInfo;$si.FileName=$exe;$si.Arguments=$argv -join ' ';$si.WorkingDirectory=$repo;$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $out=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$err=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read');$p=New-Object Diagnostics.Process;$p.StartInfo=$si;$watch=[Diagnostics.Stopwatch]::StartNew();$utc=[DateTime]::UtcNow.ToString('o')
 try{if(-not $p.Start()){throw 'Startfailed'};$a=$p.StandardOutput.BaseStream.CopyToAsync($out);$b=$p.StandardError.BaseStream.CopyToAsync($err);$p.WaitForExit();[void]$a.GetAwaiter().GetResult();[void]$b.GetAwaiter().GetResult();$exit=$p.ExitCode}finally{$out.Dispose();$err.Dispose();$p.Dispose()}
 $m=[ordered]@{task='T011';owner='/root';name=$name;exe=$exe;argv=$argv;cwd=$repo;host=$PSVersionTable.PSVersion.ToString();started_utc=$utc;finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds;exit=$exit;hardware='NOT_RUN';model='NOT_RUN';stdout=@{bytes=(Get-Item "$stem.stdout").Length;sha256=(Hash "$stem.stdout")};stderr=@{bytes=(Get-Item "$stem.stderr").Length;sha256=(Hash "$stem.stderr")}}
 [IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 12),$enc);Write-Output "$name exit=$exit";if($exit -ne 0){exit $exit}
}
Run '17-root-rustfmt' "$env:CARGO_HOME\bin\rustfmt.exe" @('--edition','2024','crates/contracts/src/worker.rs','crates/contracts/tests/worker_wire.rs','crates/contracts/src/bin/export_schema.rs')
Run '18-worker-wire' "$env:CARGO_HOME\bin\cargo.exe" @('test','--locked','-p','witvoice-contracts','--test','worker_wire','--','--nocapture')
Run '19-python-control' 'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' @('-m','unittest','discover','-s','workers/vc_worker','-p','test_control.py','-v')