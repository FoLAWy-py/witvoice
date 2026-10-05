$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major-ne5-or$PSVersionTable.PSVersion.Minor-ne1){throw 'Explicit systemPS5.1 required'}
$repo='D:\Project\witvoice';Set-Location $repo
function FileHash([string]$path){$f=[IO.File]::OpenRead($path);$s=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($s.ComputeHash($f))).Replace('-','').ToLowerInvariant()}finally{$f.Dispose();$s.Dispose()}}
$cfg=Get-Content "$PSScriptRoot\config.json" -Raw|ConvertFrom-Json
if((FileHash $PSCommandPath)-ne$cfg.supervisor_sha256-or(FileHash $cfg.binary)-ne$cfg.binary_sha256){throw 'Frozen executable changed'}
function Inputs{$m=[ordered]@{};foreach($p in $cfg.source_sha256.PSObject.Properties){$h=FileHash "$repo\$($p.Name)";if($h-ne$p.Value){throw "Frozen source changed: $($p.Name)"};$m[$p.Name]=$h};return $m}
if(Test-Path "$PSScriptRoot\consumed.json"){throw 'Never reuse a consumed once'}
$before=Inputs
[IO.File]::WriteAllText("$PSScriptRoot\consumed.json",('{"started_utc":"'+[DateTime]::UtcNow.ToString('o')+'","automatic_retry":false}'),(New-Object Text.UTF8Encoding($false)))
$si=New-Object Diagnostics.ProcessStartInfo;$si.FileName=$cfg.binary;$si.Arguments=$cfg.argv-join' ';$si.WorkingDirectory=$repo
$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
$o=[IO.File]::Open("$PSScriptRoot\stdout.raw",'CreateNew','Write','Read');$e=[IO.File]::Open("$PSScriptRoot\stderr.raw",'CreateNew','Write','Read')
$p=New-Object Diagnostics.Process;$p.StartInfo=$si;$start=[DateTime]::UtcNow.ToString('o');$w=[Diagnostics.Stopwatch]::StartNew()
try{
    if(-not$p.Start()){throw 'Diagnostic process did not launch'}
    $out=$p.StandardOutput.BaseStream.CopyToAsync($o);$err=$p.StandardError.BaseStream.CopyToAsync($e)
    $remaining=[Math]::Max(0,[int]$cfg.max_ms-3000-[int]$w.ElapsedMilliseconds)
    $timeout=-not$p.WaitForExit($remaining)
    if($timeout){$p.Kill();$remaining=[Math]::Max(0,[int]$cfg.max_ms-[int]$w.ElapsedMilliseconds);if(-not$p.WaitForExit($remaining)){throw 'Test process cleanup incomplete'}}
    [void]$out.GetAwaiter().GetResult();[void]$err.GetAwaiter().GetResult();$code=$p.ExitCode
}finally{$o.Dispose();$e.Dispose();$p.Dispose()}
$after=Inputs
$result=[ordered]@{task='T011';owner='/root';source_commit=$cfg.source_commit;config_sha256=(FileHash "$PSScriptRoot\config.json");supervisor_sha256=(FileHash $PSCommandPath);binary=$si.FileName;binary_sha256=(FileHash $si.FileName);argv=$cfg.argv;started_utc=$start;finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$w.Elapsed.TotalSeconds;exit=$code;supervisor_timeout=$timeout;automatic_retries=0;host=$PSVersionTable.PSVersion.ToString();source_before_sha256=$before;source_after_sha256=$after;model_GPU_audio_LAN='NOT_RUN';stdout=@{bytes=(Get-Item "$PSScriptRoot\stdout.raw").Length;sha256=(FileHash "$PSScriptRoot\stdout.raw")};stderr=@{bytes=(Get-Item "$PSScriptRoot\stderr.raw").Length;sha256=(FileHash "$PSScriptRoot\stderr.raw")};interpretation='single native testpeer cleanup stage observation only; no T011DONE or VC claim'}
[IO.File]::WriteAllText("$PSScriptRoot\result.json",($result|ConvertTo-Json -Depth 20),(New-Object Text.UTF8Encoding($false)))
$consoleResult=[ordered]@{}
foreach($key in $result.Keys){if($key -notin @('source_before_sha256','source_after_sha256')){$consoleResult[$key]=$result[$key]}}
$consoleResult['source_count']=$before.Count
$consoleResult['both_maps_equal_frozen']=($before.Count-eq@($cfg.source_sha256.PSObject.Properties).Count-and$after.Count-eq$before.Count)
Write-Output ($consoleResult|ConvertTo-Json -Depth 20)
exit $code
