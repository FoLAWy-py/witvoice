[Console]::OutputEncoding=New-Object Text.UTF8Encoding($false)
$ErrorActionPreference='Stop'
$repo='D:\Project\witvoice'
$dir="$repo\.local\t011-warmup-author"
Set-Location -LiteralPath $repo
. "$repo\tools\dev\toolchain-env.ps1"
$env:CARGO_TARGET_DIR="$repo\.local\t016-diagnostics-author\target"
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_INCREMENTAL='0'
$utf8=New-Object Text.UTF8Encoding($false)
function Hash([string]$p){$s=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($s.ComputeHash([IO.File]::ReadAllBytes($p)))).Replace('-','').ToLower()}finally{$s.Dispose()}}
$source="$repo\crates\engines\examples\worker_warmup_probe.rs"
$overrideNames=@('RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTDOC')
$present=@($overrideNames|Where-Object {[Environment]::GetEnvironmentVariable($_)})
if($present.Count-ne 0){throw 'Build overrides present'}
$hostInfo=@{version=$PSVersionTable.PSVersion.ToString();process=(Get-Process -Id $PID).Path}
if($PSVersionTable.PSVersion.Major-ne 5){throw 'Explicit SystemPS5.1 host required'}
$cargo='D:\Software\WitvoiceToolchain\Rust\cargo\bin\cargo.exe'
$fmt='D:\Software\WitvoiceToolchain\Rust\rustup\toolchains\1.99.0-x86_64-pc-windows-msvc\bin\rustfmt.exe'
$commands=@(
 @{name='01-probe-rustfmt';exe=$fmt;argv=@('--edition','2024',$source)},
 @{name='02-engines-fmt-check';exe=$cargo;argv=@('fmt','--check','-p','witvoice-engines')},
 @{name='03-engines-clippy';exe=$cargo;argv=@('clippy','--locked','--offline','-p','witvoice-engines','--all-targets','--','-D','warnings')},
 @{name='04-warmup-probe-build';exe=$cargo;argv=@('build','--locked','--offline','-p','witvoice-engines','--example','worker_warmup_probe')}
)
$events=@()
foreach($c in $commands){
 $before=Hash $source
 $si=New-Object Diagnostics.ProcessStartInfo
 $si.FileName=$c.exe;$si.Arguments=($c.argv|ForEach-Object {if($_.Contains(' ')){'"'+$_+'"'}else{$_}})-join ' '
 $si.WorkingDirectory=$repo;$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $start=[DateTime]::UtcNow.ToString('o')
 $p=New-Object Diagnostics.Process;$p.StartInfo=$si;[void]$p.Start()
 $ot=$p.StandardOutput.ReadToEndAsync();$et=$p.StandardError.ReadToEndAsync();$p.WaitForExit()
 $out=$ot.GetAwaiter().GetResult();$err=$et.GetAwaiter().GetResult();$code=$p.ExitCode;$end=[DateTime]::UtcNow.ToString('o');$p.Dispose()
 $stem=Join-Path $dir $c.name
 [IO.File]::WriteAllText("$stem.stdout",$out,$utf8);[IO.File]::WriteAllText("$stem.stderr",$err,$utf8)
 $after=Hash $source
 $meta=[ordered]@{uuid='01a10767-4ff5-7f21-881a-146d094fbc55';executable=$c.exe;argv=$c.argv;cwd=$repo;execution_host=$hostInfo;start_utc=$start;end_utc=$end;exit_code=$code;env=@{CARGO_HOME=$env:CARGO_HOME;RUSTUP_HOME=$env:RUSTUP_HOME;CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR;CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS;CARGO_INCREMENTAL=$env:CARGO_INCREMENTAL};present_overrides=$present;source_path=$source;source_before_sha256=$before;source_after_sha256=$after;lock_sha256=(Hash "$repo\Cargo.lock");stdout=@{bytes=(Get-Item "$stem.stdout").Length;sha256=(Hash "$stem.stdout")};stderr=@{bytes=(Get-Item "$stem.stderr").Length;sha256=(Hash "$stem.stderr")};capture='Complete redirected stream text UTF8 without BOM';scope='Build/format/lint only. Warmup main/worker/model/GPU/audio/network NOT_RUN'}
 [IO.File]::WriteAllText("$stem.json",($meta|ConvertTo-Json -Depth 7),$utf8)
 $events+=@{path="$stem.json";sha256=(Hash "$stem.json");exit_code=$code}
 Write-Output ($meta|ConvertTo-Json -Depth 7 -Compress)
 if($code-ne 0){break}
 if($c.name-ne '01-probe-rustfmt'-and $before-ne $after){throw 'Source changed during check'}
}
$index=[ordered]@{uuid='01a10767-4ff5-7f21-881a-146d094fbc55';utc=[DateTime]::UtcNow.ToString('o');runner_sha256=(Hash "$dir\runner.ps1");source_path=$source;source_sha256=(Hash $source);events=$events;not_run=@('probe main','GPU/model/warmup','native worker startup','audio','LAN','Mac');writes='STOP_AFTER_DELIVERY'}
[IO.File]::WriteAllText("$dir\delivery-index.json",($index|ConvertTo-Json -Depth 7),$utf8)
Write-Output ('INDEX_SHA256='+(Hash "$dir\delivery-index.json"))