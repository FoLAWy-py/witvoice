param([Parameter(Mandatory=$true)][ValidatePattern('^[a-f0-9]{64}$')][string]$ExpectedConfigSha256)
$ErrorActionPreference='Stop'
$repo='D:\Project\witvoice'
$base="$repo\.local\t009-raw-once"
$utf8=New-Object System.Text.UTF8Encoding($false)
function Hash([string]$path){ $stream=[IO.File]::OpenRead($path); $sha=[Security.Cryptography.SHA256]::Create(); try{ [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }finally{ $sha.Dispose(); $stream.Dispose() } }
if((Hash "$base\config.json") -ne $ExpectedConfigSha256){throw 'Configuration changed'}
$config=Get-Content "$base\config.json" -Raw|ConvertFrom-Json
if($config.executable -ne "$repo\.local\t009-raw-once\route_probe.exe" -or $config.scope -ne "$base\selection-private.json"){throw 'Unexpected executable or scope'}
if((Hash $config.executable) -ne $config.executable_sha256 -or (Hash $config.scope) -ne $config.scope_sha256){throw 'Binary or endpoint scope changed'}
foreach($entry in $config.source_sha256.PSObject.Properties){
    if($entry.Name.Contains('..') -or [IO.Path]::IsPathRooted($entry.Name)){throw 'Invalid source path'}
    if((Hash "$repo\$($entry.Name)") -ne $entry.Value){throw "Source changed: $($entry.Name)"}
}
if((Hash $PSCommandPath) -ne $config.supervisor_sha256){throw 'Supervisor changed'}
if(Test-Path "$base\result.json"){throw 'Existing execution result: no retry'}
$once=[IO.File]::Open("$base\once.json",'Open','ReadWrite','None')
$p=$null; $out=$null; $err=$null; $a=$null; $b=$null; $started=$false; $ended=$false; $consumed=$false
$result=[ordered]@{task='T009';authorization_id=$config.authorization_id;status='NOT_LAUNCHED';child_exit=$null;timed_out=$false;child_terminated=$false;error=$null;report=$null;cleanup='UNKNOWN';pcm_saved=$false;physical_capture='NEVER_STARTED';physical_render='NEVER_STARTED';os_buffer_erasure='UNKNOWN';source_commit=$config.source_commit;executable_sha256=$config.executable_sha256;scope_sha256=$config.scope_sha256;supervisor_sha256=$config.supervisor_sha256;config_sha256=$ExpectedConfigSha256}
try {
    $reader=New-Object IO.StreamReader($once,$utf8,$true,4096,$true)
    try{$grant=$reader.ReadToEnd()|ConvertFrom-Json}finally{$reader.Dispose()}
    if($grant.authorization_id -ne $config.authorization_id -or $grant.status -ne 'AUTHORIZED_NOT_EXECUTED'){throw 'Authorization unavailable; never retry'}
    $out=[IO.File]::Open("$base\stdout.raw",'CreateNew','Write','Read')
    $err=[IO.File]::Open("$base\stderr.raw",'CreateNew','Write','Read')
    $si=New-Object Diagnostics.ProcessStartInfo
    $si.FileName=$config.executable; $si.Arguments='"'+$config.scope+'" --approve-raw-capture-mix-probe'; $si.WorkingDirectory=$repo
    $si.UseShellExecute=$false; $si.CreateNoWindow=$true
    $si.RedirectStandardInput=$true; $si.RedirectStandardOutput=$true; $si.RedirectStandardError=$true
    $p=New-Object Diagnostics.Process; $p.StartInfo=$si
    $watch=[Diagnostics.Stopwatch]::StartNew()
    $result['started_utc']=[DateTime]::UtcNow.ToString('o')
    $grant.status='CONSUMED_BEFORE_LAUNCH'; $grant.consumed_utc=$result.started_utc
    $bytes=$utf8.GetBytes(($grant|ConvertTo-Json -Depth 10))
    $once.Position=0; $once.SetLength(0); $once.Write($bytes,0,$bytes.Length); $once.Flush($true)
    $consumed=$true
    if(-not $p.Start()){throw 'Process start failed; authorization consumed'}
    $started=$true; $result['process_id']=$p.Id; $p.StandardInput.Close()
    $a=$p.StandardOutput.BaseStream.CopyToAsync($out,4096)
    $b=$p.StandardError.BaseStream.CopyToAsync($err,4096)
    $remaining=[Math]::Max(0,13000-[int]$watch.ElapsedMilliseconds)
    if(-not $p.WaitForExit($remaining)){
        $result.timed_out=$true
        $p.Kill()
        $ended=$p.WaitForExit(750)
        $result.child_terminated=$ended
        $result.status='FAIL_WATCHDOG'; $result.error='Owned child exceeded deadline; normal Stop/erasure not confirmed'
    }else{$ended=$true}
    if($ended){$result.child_exit=$p.ExitCode}
    if(-not $a.Wait(250) -or -not $b.Wait(250)){throw 'Scalar log drain incomplete'}
    $out.Dispose(); $out=$null; $err.Dispose(); $err=$null
    if(-not $result.timed_out){
        if((Get-Item "$base\stdout.raw").Length -gt 16384 -or (Get-Item "$base\stderr.raw").Length -gt 16384){throw 'Unexpected scalar log size'}
        $r=Get-Content "$base\stdout.raw" -Raw|ConvertFrom-Json
        $result.report=$r
        $clean=$r.capture_closed -eq $true -and $r.render_closed -eq $true -and $r.watch_closed -eq $true -and $r.ack_ready -eq $true
        if($clean){$result.cleanup='CONFIRMED_BY_CHILD'}
        $passed=$p.ExitCode -eq 0 -and $r.status -eq 'ROUTE_MARKER_OBSERVED' -and $r.route_profile -eq 'VB_CABLE' -and $r.descriptor_mode -eq 'CAPTURE_RAW_EXACT_MIX_RENDER_BASIC' -and $r.run_ok -eq $true -and $clean -and $r.underflow_frames -eq 0 -and $r.discontinuity_packets -eq 0 -and $r.timestamp_error_packets -eq 0 -and $r.requested_seconds -eq 2 -and $r.requested_maximum_frames -eq 1440 -and $r.render_capacity_frames -gt 0 -and $r.render_capacity_frames -le 1440 -and $r.capture_capacity_frames -gt 0 -and $r.capture_capacity_frames -le 1440 -and $r.marker_amplitude -le 0.01 -and $r.marker_frames -le 96000 -and $r.capture_frames -le 96000 -and $r.process_wall_seconds -le 15 -and $r.active_wall_seconds -le 2 -and $r.match.correlation -ge 0.6 -and $r.physical_capture -eq 'NEVER_STARTED' -and $r.pcm_saved -eq $false
        $result.status=if($passed){'VB_CABLE_ROUTE_CLOSED_LOOP_PASS'}else{'FAIL_ROUTE_PROBE'}
        if(-not $passed){$result.error=if($r.error){$r.error}else{$r.run_error}}
    }
} catch {
    $result.error=$_.Exception.Message
    if($consumed -and $result.status -eq 'NOT_LAUNCHED'){$result.status='FAIL_CONSUMED'}
} finally {
    if($started -and -not $ended){
        try{if(-not $p.HasExited){$p.Kill()};$ended=$p.WaitForExit(250);$result.child_terminated=$ended}catch{$result['termination_error']=$_.Exception.Message}
    }
    if($out){$out.Dispose()};if($err){$err.Dispose()};if($p){$p.Dispose()}
    $once.Dispose()
    if($consumed){
        $result['finished_utc']=[DateTime]::UtcNow.ToString('o');$result['wall_seconds']=$watch.Elapsed.TotalSeconds
        if($result.wall_seconds -gt 15){$result.status='FAIL_TOTAL_DEADLINE'}
        foreach($name in @('stdout','stderr')){if(Test-Path "$base\$name.raw"){$result[$name]=@{bytes=(Get-Item "$base\$name.raw").Length;sha256=Hash "$base\$name.raw"}}}
        [IO.File]::WriteAllText("$base\result.json",($result|ConvertTo-Json -Depth 20),$utf8)
    }
}
$result|ConvertTo-Json -Depth 20
if($result.status -ne 'VB_CABLE_ROUTE_CLOSED_LOOP_PASS'){exit 1}
