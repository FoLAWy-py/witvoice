$ErrorActionPreference='Stop'
$repo='D:\Project\witvoice'
$utf8=New-Object Text.UTF8Encoding($false)
function Hash([string]$path){$stream=[IO.File]::OpenRead($path);$sha=[Security.Cryptography.SHA256]::Create();try{[BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose();$stream.Dispose()}}
function Inputs {$h=[ordered]@{};foreach($f in Get-ChildItem "$repo\crates\audio" -File -Recurse){$key=$f.FullName.Substring($repo.Length+1).Replace('\','/');$h[$key]=Hash $f.FullName};foreach($p in @('Cargo.toml','Cargo.lock')){$h[$p]=Hash "$repo\$p"};return $h}
$checks=@(
@{name='audio-tests';argv=@('test','--locked','--offline','-p','witvoice-audio','--lib','--tests')},
@{name='audio-fmt';argv=@('fmt','-p','witvoice-audio','--check')},
@{name='audio-clippy';argv=@('clippy','--locked','--offline','-p','witvoice-audio','--all-targets','--','-D','warnings')},
@{name='audio-build';argv=@('build','--locked','--offline','-p','witvoice-audio','--examples')})
foreach($c in $checks){
$stem=Join-Path $PSScriptRoot $c.name
if(Test-Path "$stem.json"){throw 'Existing result; no overwrite'}
$before=Inputs;$start=[DateTime]::UtcNow.ToString('o');$watch=[Diagnostics.Stopwatch]::StartNew()
$si=New-Object Diagnostics.ProcessStartInfo;$si.FileName="$env:CARGO_HOME\bin\cargo.exe";$si.Arguments=$c.argv-join' ';$si.WorkingDirectory=$repo;$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
$o=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$e=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read');$p=New-Object Diagnostics.Process;$p.StartInfo=$si
try{if(-not$p.Start()){throw 'Cargo start failed'};$ot=$p.StandardOutput.BaseStream.CopyToAsync($o);$et=$p.StandardError.BaseStream.CopyToAsync($e);if(-not$p.WaitForExit(180000)){$p.Kill();throw 'Check timeout'};if(-not$ot.Wait(2000)-or-not$et.Wait(2000)){throw 'Log drain timeout'};$code=$p.ExitCode}finally{$o.Dispose();$e.Dispose();$p.Dispose()}
$after=Inputs
$m=[ordered]@{command=@($si.FileName)+$c.argv;cwd=$repo;start_utc=$start;end_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds;exit=$code;source_before=$before;source_after=$after;host=$PSVersionTable.PSVersion.ToString();hardware='NOT_RUN';stdout_sha256=Hash "$stem.stdout";stderr_sha256=Hash "$stem.stderr"}
[IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 10),$utf8)
Write-Output "$($c.name) exit=$code"
if($code-ne0){exit $code}
}
