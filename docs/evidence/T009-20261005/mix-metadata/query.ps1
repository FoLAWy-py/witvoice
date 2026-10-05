$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major -ne 5){throw 'System PS5 required'}
$base='D:\Project\witvoice\.local\t009-mix-metadata'
$exe='D:\Project\witvoice\.local\t009-author\target\debug\examples\format_support_probe.exe'
function Hash([string]$path){$s=[IO.File]::OpenRead($path);$h=[Security.Cryptography.SHA256]::Create();try{[BitConverter]::ToString($h.ComputeHash($s)).Replace('-','').ToLowerInvariant()}finally{$h.Dispose();$s.Dispose()}}
if((Hash $exe) -ne '1b8e90b8d1664ead28438e647e0cf59a533d8a7f5f2b00b99de19fe3aed50e3e'){throw 'Author binary changed'}
$u=New-Object Text.UTF8Encoding($false)
$whole=[Diagnostics.Stopwatch]::StartNew()
foreach($flow in @('Render','Capture')){
 $stem="$base\$flow"
 if(Test-Path "$stem.json"){throw 'Never overwrite/retry query'}
 $selection="$base\$flow-private.json"
 $si=New-Object Diagnostics.ProcessStartInfo
 $si.FileName=$exe;$si.Arguments='"'+$selection+'" --mix-query 48000:2:float32'
 $si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $p=New-Object Diagnostics.Process;$p.StartInfo=$si
 $out=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$err=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read')
 $utc=[DateTime]::UtcNow.ToString('o');$timer=[Diagnostics.Stopwatch]::StartNew();$timedout=$false
 try{
  if(-not $p.Start()){throw 'Query failed to start'}
  $a=$p.StandardOutput.BaseStream.CopyToAsync($out);$b=$p.StandardError.BaseStream.CopyToAsync($err)
  $remaining=[Math]::Max(0,[Math]::Min(4000,10000-[int]$whole.ElapsedMilliseconds))
  if(-not $p.WaitForExit($remaining)){$timedout=$true;$p.Kill();if(-not $p.WaitForExit(500)){throw 'Owned query kill unconfirmed'}}
  if(-not $a.Wait(250) -or -not $b.Wait(250)){throw 'Scalar drain incomplete'}
  $exit=$p.ExitCode
 }finally{if(-not $p.HasExited){$p.Kill();[void]$p.WaitForExit(250)};$p.Dispose();$out.Dispose();$err.Dispose()}
 $meta=[ordered]@{task='T009';owner='/root';flow=$flow;source_commit='31ba5c4b9cc841ffa84c4d883588e915ab1495a2';binary_sha256=(Hash $exe);selection_sha256=(Hash $selection);argv=@('PRIVATE_EXACT_UID_SELECTION','--mix-query','48000:2:float32');host=$PSVersionTable.PSVersion.ToString();started_utc=$utc;finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$timer.Elapsed.TotalSeconds;exit=$exit;timed_out=$timedout;initialize_start='NOT_RUN';pcm='NOT_ACCESSED';stdout=@{bytes=(Get-Item "$stem.stdout").Length;sha256=(Hash "$stem.stdout")};stderr=@{bytes=(Get-Item "$stem.stderr").Length;sha256=(Hash "$stem.stderr")}}
 [IO.File]::WriteAllText("$stem.json",($meta|ConvertTo-Json -Depth 10),$u)
 Get-Content "$stem.stdout" -Raw
 Write-Output "$flow exit=$exit timeout=$timedout"
 if($exit -ne 0 -or $timedout){exit 1}
}
