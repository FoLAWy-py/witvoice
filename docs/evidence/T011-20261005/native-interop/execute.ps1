param([Parameter(Mandatory=$true)][string]$ExpectedConfigSha256)
$ErrorActionPreference='Stop'
$base='D:\Project\witvoice\.local\t011-native-once'
$repo='D:\Project\witvoice'
$u=New-Object Text.UTF8Encoding($false)
function Hash([string]$path){$s=[IO.File]::OpenRead($path);$h=[Security.Cryptography.SHA256]::Create();try{[BitConverter]::ToString($h.ComputeHash($s)).Replace('-','').ToLowerInvariant()}finally{$h.Dispose();$s.Dispose()}}
if($PSVersionTable.PSVersion.Major -ne 5){throw 'Explicit system PS5 required'}
if((Hash "$base\config.json") -ne $ExpectedConfigSha256){throw 'Configuration changed'}
$c=Get-Content "$base\config.json" -Raw|ConvertFrom-Json
if((Hash $PSCommandPath) -ne $c.supervisor_sha256){throw 'Supervisor changed'}
if((Hash $c.binary) -ne $c.binary_sha256){throw 'Binary changed'}
foreach($p in $c.source_sha256.PSObject.Properties){if((Hash "$repo\$($p.Name)") -ne $p.Value){throw 'Frozen source changed'}}
if(Test-Path "$base\result.json"){throw 'Never overwrite or automatically retry this run'}
$si=New-Object Diagnostics.ProcessStartInfo
$si.FileName=$c.binary;$si.Arguments='--approve-native-interop';$si.WorkingDirectory=$repo
$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
$p=New-Object Diagnostics.Process;$p.StartInfo=$si
$out=[IO.File]::Open("$base\stdout.raw",'CreateNew','Write','Read');$err=[IO.File]::Open("$base\stderr.raw",'CreateNew','Write','Read')
$watch=[Diagnostics.Stopwatch]::StartNew();$utc=[DateTime]::UtcNow.ToString('o');$timeout=$false;$started=$false
$result=[ordered]@{source_commit=$c.source_commit;binary_sha256=$c.binary_sha256;config_sha256=$ExpectedConfigSha256;owner='/root';host=$PSVersionTable.PSVersion.ToString();started_utc=$utc;model='NOT_RUN';audio='NOT_RUN';network='NOT_RUN';pcm_saved=$false;status='NOT_LAUNCHED';exit=$null;timed_out=$false;owned_job_cleanup='UNKNOWN'}
try{
 if(-not $p.Start()){throw 'Probe launch failed'}
 $started=$true
 $a=$p.StandardOutput.BaseStream.CopyToAsync($out);$b=$p.StandardError.BaseStream.CopyToAsync($err)
 if(-not $p.WaitForExit(13000)){$timeout=$true;$p.Kill();if(-not $p.WaitForExit(750)){throw 'Owned probe termination unconfirmed'}}
 if(-not $a.Wait(250) -or -not $b.Wait(250)){throw 'Scalar output drain unconfirmed'}
 $result.exit=$p.ExitCode;$result.timed_out=$timeout
 $out.Dispose();$out=$null;$err.Dispose();$err=$null
 if((Get-Item "$base\stdout.raw").Length -gt 16384 -or (Get-Item "$base\stderr.raw").Length -gt 16384){throw 'Scalar log size exceeded'}
 if(-not $timeout -and $result.exit -eq 0){
  $r=Get-Content "$base\stdout.raw" -Raw|ConvertFrom-Json;$result.report=$r
  if($r.status -eq 'NATIVE_PYTHON_TRANSPORT_ONLY_PASS' -and $r.control_and_media -eq $true -and $r.source_samples -eq 2560 -and $r.owned_job_empty -eq $true -and $r.model -eq 'NOT_RUN' -and $r.audio -eq 'NOT_RUN' -and $r.pcm_saved -eq $false){$result.status='PASS_NATIVE_TRANSPORT_ONLY';$result.owned_job_cleanup='CONFIRMED_BY_REAL_OWNER'}
 }
 if($result.status -ne 'PASS_NATIVE_TRANSPORT_ONLY'){$result.status='FAIL_NATIVE_TRANSPORT'}
}catch{$result.status='FAIL_NATIVE_TRANSPORT';$result.error=$_.Exception.Message}
finally{
 if($started -and -not $p.HasExited){$p.Kill();[void]$p.WaitForExit(250)}
 if($out){$out.Dispose()};if($err){$err.Dispose()};$p.Dispose()
 $result.finished_utc=[DateTime]::UtcNow.ToString('o');$result.wall_seconds=$watch.Elapsed.TotalSeconds
 if($result.wall_seconds -gt 15){$result.status='FAIL_TOTAL_DEADLINE'}
 foreach($name in @('stdout','stderr')){if(Test-Path "$base\$name.raw"){$result[$name]=@{bytes=(Get-Item "$base\$name.raw").Length;sha256=(Hash "$base\$name.raw")}}}
 $result.source_after_all_equal=$true
 foreach($entry in $c.source_sha256.PSObject.Properties){if((Hash "$repo\$($entry.Name)") -ne $entry.Value){$result.source_after_all_equal=$false;$result.status='FAIL_SOURCE_CHANGED'}}
 [IO.File]::WriteAllText("$base\result.json",($result|ConvertTo-Json -Depth 15),$u)
}
$result|ConvertTo-Json -Depth 15
if($result.status -ne 'PASS_NATIVE_TRANSPORT_ONLY'){exit 1}
