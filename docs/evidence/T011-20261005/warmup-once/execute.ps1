param([Parameter(Mandatory=$true)][string]$ExpectedConfigSha256)
$ErrorActionPreference='Stop'
$base='D:\Project\witvoice\.local\t011-warmup-once';$repo='D:\Project\witvoice'
$u=New-Object Text.UTF8Encoding($false)
function FileHash([string]$path){$f=[IO.File]::OpenRead($path);$h=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($h.ComputeHash($f))).Replace('-','').ToLowerInvariant()}finally{$h.Dispose();$f.Dispose()}}
if($PSVersionTable.PSVersion.Major -ne 5 -or [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName -ne 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'){throw 'Explicit system PS5 required'}
if((FileHash "$base\config.json") -ne $ExpectedConfigSha256){throw 'Configuration changed'}
$c=Get-Content "$base\config.json" -Raw|ConvertFrom-Json
if((FileHash $PSCommandPath) -ne $c.supervisor_sha256){throw 'Supervisor changed'}
if((FileHash $c.binary) -ne $c.binary_sha256){throw 'Binary changed'}
foreach($e in $c.source_sha256.PSObject.Properties){if((FileHash "$repo\$($e.Name)") -ne $e.Value){throw 'Frozen source changed'}}
if(Test-Path "$base\result.json"){throw 'Never overwrite or automatically retry this experiment'}
$si=New-Object Diagnostics.ProcessStartInfo
$si.FileName=$c.binary;$si.Arguments='--approve-real-warmup';$si.WorkingDirectory=$repo
$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
$si.EnvironmentVariables['PYTHONPATH']='D:\Project\witvoice\.local\t011-leader\python-deps'
$si.EnvironmentVariables['PYTHONNOUSERSITE']='1'
$p=New-Object Diagnostics.Process;$p.StartInfo=$si
$o=[IO.File]::Open("$base\stdout.raw",'CreateNew','Write','Read');$e=[IO.File]::Open("$base\stderr.raw",'CreateNew','Write','Read')
$w=[Diagnostics.Stopwatch]::StartNew();$started=$false;$timeout=$false
$r=[ordered]@{source_commit=$c.source_commit;binary_sha256=$c.binary_sha256;config_sha256=$ExpectedConfigSha256;owner='/root';host=$PSVersionTable.PSVersion.ToString();started_utc=[DateTime]::UtcNow.ToString('o');status='NOT_LAUNCHED';model='NOT_LAUNCHED';audio='NOT_RUN';network='NOT_RUN';pcm_saved=$false;exit=$null;timed_out=$false;owned_job_cleanup='UNKNOWN'}
try{
 if(-not $p.Start()){throw 'Probe launch failed'};$started=$true;$r.model='REAL_WARMUP_ATTEMPT'
 $a=$p.StandardOutput.BaseStream.CopyToAsync($o);$b=$p.StandardError.BaseStream.CopyToAsync($e)
 if(-not $p.WaitForExit(127000)){$timeout=$true;$p.Kill();if(-not $p.WaitForExit(750)){throw 'Owned probe termination unconfirmed'}}
 $r.exit=$p.ExitCode;$r.timed_out=$timeout
 if(-not $a.Wait(250) -or -not $b.Wait(250)){throw 'Scalar output drain unconfirmed'}
 $o.Dispose();$o=$null;$e.Dispose();$e=$null
 if((Get-Item "$base\stdout.raw").Length -gt 16384 -or (Get-Item "$base\stderr.raw").Length -gt 16384){throw 'Scalar log size exceeded'}
 $r.status='FAIL_REAL_WARMUP'
 if((Get-Item "$base\stdout.raw").Length -gt 0){
  $report=Get-Content "$base\stdout.raw" -Raw|ConvertFrom-Json;$r.report=$report
  if($report.cleanup_confirmed -eq $true -and $report.owned_active_processes -eq 0){$r.owned_job_cleanup='CONFIRMED_BY_REAL_OWNER'}
  # Frozen success predicate matches the reviewed probe; static backend labels are expectations until verified Ready.
  if(-not $timeout -and $r.exit -eq 0 -and $report.result -eq 'READY' -and $report.model_ready -eq $true -and $report.ready_received -eq $true -and $report.stopped_ack -eq $true -and $report.cleanup_confirmed -eq $true -and $report.owned_active_processes -eq 0 -and $report.policy_output_allowed_after_cleanup -eq $false -and $report.sink_present -eq $false -and $report.output_open -eq $false -and $report.media_exchanged -eq $false -and $report.automatic_retries -eq 0 -and $report.heartbeat_count -gt 0 -and $report.max_heartbeat_gap_ms -lt 500 -and $report.model_sha256 -eq '01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515'){$r.status='PASS_REAL_WARMUP_ONLY';$r.model='REAL_PREPARATION_VERIFIED'}
 }
}catch{$r.status='FAIL_REAL_WARMUP';$r.error=$_.Exception.Message}
finally{
 if($started -and -not $p.HasExited){$p.Kill();[void]$p.WaitForExit(250)}
 if($o){$o.Dispose()};if($e){$e.Dispose()};$p.Dispose()
 $r.finished_utc=[DateTime]::UtcNow.ToString('o');$r.wall_seconds=$w.Elapsed.TotalSeconds
 if($r.wall_seconds -gt 130){$r.status='FAIL_TOTAL_DEADLINE'}
 foreach($name in @('stdout','stderr')){if(Test-Path "$base\$name.raw"){$r[$name]=@{bytes=(Get-Item "$base\$name.raw").Length;sha256=(FileHash "$base\$name.raw")}}}
 $r.source_after_all_equal=$true
 foreach($entry in $c.source_sha256.PSObject.Properties){if((FileHash "$repo\$($entry.Name)") -ne $entry.Value){$r.source_after_all_equal=$false;$r.status='FAIL_SOURCE_CHANGED'}}
 [IO.File]::WriteAllText("$base\result.json",($r|ConvertTo-Json -Depth 20),$u)
}
$r|ConvertTo-Json -Depth 20
if($r.status -ne 'PASS_REAL_WARMUP_ONLY'){exit 1}