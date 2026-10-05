param([Parameter(Mandatory=$true)][string]$ExpectedConfigSha256)
$ErrorActionPreference='Stop'
$base='D:\Project\witvoice\.local\t011-budget8-warmup-once';$repo='D:\Project\witvoice'
$u=New-Object Text.UTF8Encoding($false)
function FileHash([string]$path){$f=[IO.File]::OpenRead($path);$h=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($h.ComputeHash($f))).Replace('-','').ToLowerInvariant()}finally{$h.Dispose();$f.Dispose()}}
if($PSVersionTable.PSVersion.Major -ne 5 -or [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName -ne 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'){throw 'Explicit system PS5 required'}
if((FileHash "$base\config.json") -ne $ExpectedConfigSha256){throw 'Configuration changed'}
$c=Get-Content "$base\config.json" -Raw|ConvertFrom-Json
if((FileHash $PSCommandPath) -ne $c.supervisor_sha256){throw 'Supervisor changed'}
if((FileHash $c.binary) -ne $c.binary_sha256){throw 'Binary changed'}
foreach($e in $c.source_sha256.PSObject.Properties){if((FileHash "$repo\$($e.Name)") -ne $e.Value){throw 'Frozen source changed'}}
if((Test-Path "$base\resources.json") -or (Test-Path "$base\prepare-stages.ndjson") -or (Test-Path "$base\model-process-private.json")){throw 'Diagnostic identity or phases already consumed'}
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
  if(-not $timeout -and $r.exit -eq 0 -and $report.result -eq 'READY' -and $report.model_ready -eq $true -and $report.ready_received -eq $true -and $report.model_job_membership_verified -eq $true -and $report.stopped_ack -eq $true -and $report.cleanup_confirmed -eq $true -and $report.owned_active_processes -eq 0 -and $report.policy_output_allowed_after_cleanup -eq $false -and $report.sink_present -eq $false -and $report.output_open -eq $false -and $report.media_exchanged -eq $false -and $report.automatic_retries -eq 0 -and $report.heartbeat_count -gt 1 -and ($report.max_heartbeat_gap_ms -is [int] -or $report.max_heartbeat_gap_ms -is [long]) -and $report.max_heartbeat_gap_ms -lt 500 -and $report.model_sha256 -eq '01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515'){$r.status='PASS_REAL_WARMUP_ONLY';$r.model='REAL_PREPARATION_VERIFIED'}
 }
}catch{$r.status='FAIL_REAL_WARMUP';$r.error=$_.Exception.Message}
finally{
 if($started -and -not $p.HasExited){$p.Kill();[void]$p.WaitForExit(250)}
 if($o){$o.Dispose()};if($e){$e.Dispose()};$p.Dispose()

 # Read scalar phase evidence only after the owned Job has been queried empty.
 $r.phase_diagnostic='UNAVAILABLE';$r.phase_complete=$false
 $phasePath="$base\prepare-stages.ndjson"
 if($r.owned_job_cleanup -eq 'CONFIRMED_BY_REAL_OWNER' -and (Test-Path -LiteralPath $phasePath)){
  $f=$null
  try{
   $f=[IO.File]::Open($phasePath,'Open','Read','Read')
   $buf=New-Object byte[] 4097;$n=0
   while($n -lt 4097){$count=$f.Read($buf,$n,4097-$n);if($count -eq 0){break};$n+=$count}
   if($n -gt 4096){throw 'Closed diagnostic byte limit'}
   $decoder=New-Object Text.UTF8Encoding($false,$true);$phaseText=$decoder.GetString($buf,0,$n)
   $stages=@('imports_array_audio','imports_torch','imports_adapter','fixtures_check','model_load','placement_validate','convert_warmup','finalize')
   if($n -gt 0 -and -not $phaseText.EndsWith("`n")){throw 'Partial diagnostic record'}
   $records=@();$lastTime=[long]0
   foreach($line in ($phaseText -split "`n")){
    if($line -eq ''){continue}
    if($records.Count -ge 16){throw 'Closed diagnostic record limit'}
    # Exact writer shape also rejects duplicate keys, extra fields and text.
    if($line -cnotmatch '^\{"stage":"(imports_array_audio|imports_torch|imports_adapter|fixtures_check|model_load|placement_validate|convert_warmup|finalize)","edge":"(before|after)","elapsed_ns":[0-9]+\}$'){throw 'Closed diagnostic encoding'}
    $item=$line|ConvertFrom-Json
    $keys=@($item.PSObject.Properties.Name|Sort-Object)
    if(($keys -join ',') -ne 'edge,elapsed_ns,stage'){throw 'Closed diagnostic shape'}
    $index=$records.Count;$edge=if($index%2 -eq 0){'before'}else{'after'}
    if($item.stage -ne $stages[[int][Math]::Floor($index/2)] -or $item.edge -ne $edge){throw 'Closed diagnostic order'}
    if(($item.elapsed_ns -isnot [int] -and $item.elapsed_ns -isnot [long]) -or $item.elapsed_ns -lt $lastTime -or $item.elapsed_ns -lt 0){throw 'Closed diagnostic clock'}
    $lastTime=$item.elapsed_ns;$records+=@($item)
   }
   # All16 successful before/after records are diagnostic completeness only.
   $r.phase_diagnostic='VALID_BOUNDED_SCALARS';$r.phase_records=$records
   $r.phase_complete=($records.Count -eq 16)
   $r.phase_file=@{bytes=$n;sha256=(FileHash $phasePath)}
  }catch{$r.phase_diagnostic='INVALID_OR_PARTIAL';$r.phase_complete=$false}
  finally{if($f){$f.Dispose()}}
 }
 if($r.status -eq 'PASS_REAL_WARMUP_ONLY' -and -not $r.phase_complete){$r.status='FAIL_DIAGNOSTIC_INCOMPLETE'}

 # Read closed resource evidence only after owned Job is confirmed empty.
 $r.resource_diagnostic='UNAVAILABLE';$r.resource_known=$false;$r.resource_within_budget=$false
 $resourcePath="$base\resources.json"
 if($r.owned_job_cleanup -eq 'CONFIRMED_BY_REAL_OWNER' -and (Test-Path -LiteralPath $resourcePath)){
  $resourceStream=$null
  try{
   $resourceStream=[IO.File]::Open($resourcePath,'Open','Read','Read')
   $buffer=New-Object byte[] 513;$size=0
   while($size-lt513){$read=$resourceStream.Read($buffer,$size,513-$size);if($read-eq0){break};$size+=$read}
   if($size-gt512){throw 'Resource diagnostic byte limit'}
   $decoder=New-Object Text.UTF8Encoding($false,$true);$encoded=$decoder.GetString($buffer,0,$size)
   if($encoded-cnotmatch '^\{"host_private_bytes":(null|[0-9]+),"device_reserved_bytes":(null|[0-9]+),"host_budget_bytes":8589934592,"device_budget_bytes":4294967296\}$'){throw 'Closed resource diagnostic encoding'}
   $resource=$encoded|ConvertFrom-Json
   foreach($field in @('host_private_bytes','device_reserved_bytes')){
    $value=$resource.$field
    if($null-ne$value-and($value-isnot[int]-and$value-isnot[long])){throw 'Resource diagnostic integer'}
    if($null-ne$value-and$value-lt0){throw 'Resource diagnostic sign'}
   }
   $r.resource_diagnostic='VALID_BOUNDED_SCALARS';$r.resources=$resource
   $r.resource_known=($null-ne$resource.host_private_bytes-and$null-ne$resource.device_reserved_bytes)
   $r.resource_within_budget=($r.resource_known-and$resource.host_private_bytes-le8589934592-and$resource.device_reserved_bytes-le4294967296)
   $r.resource_file=@{bytes=$size;sha256=(FileHash $resourcePath)}
  }catch{$r.resource_diagnostic='INVALID_OR_PARTIAL';$r.resource_known=$false;$r.resource_within_budget=$false}
  finally{if($resourceStream){$resourceStream.Dispose()}}
 }
 if($r.status-eq'PASS_REAL_WARMUP_ONLY'-and-not$r.resource_within_budget){$r.status='FAIL_RESOURCE_DIAGNOSTIC_INCOMPLETE_OR_OVER_BUDGET'}

 $r.finished_utc=[DateTime]::UtcNow.ToString('o');$r.wall_seconds=$w.Elapsed.TotalSeconds
 if($r.wall_seconds -gt 130){$r.status='FAIL_TOTAL_DEADLINE'}
 foreach($name in @('stdout','stderr')){if(Test-Path "$base\$name.raw"){$r[$name]=@{bytes=(Get-Item "$base\$name.raw").Length;sha256=(FileHash "$base\$name.raw")}}}
 $r.source_after_all_equal=$true
 foreach($entry in $c.source_sha256.PSObject.Properties){if((FileHash "$repo\$($entry.Name)") -ne $entry.Value){$r.source_after_all_equal=$false;$r.status='FAIL_SOURCE_CHANGED'}}
 [IO.File]::WriteAllText("$base\result.json",($r|ConvertTo-Json -Depth 20),$u)
}
$r|ConvertTo-Json -Depth 20
if($r.status -ne 'PASS_REAL_WARMUP_ONLY'){exit 1}