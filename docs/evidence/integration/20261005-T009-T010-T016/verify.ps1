param([string]$RunId='first')
$ErrorActionPreference='Stop'
$repo='D:\Project\witvoice'
Set-Location $repo
. "$repo\tools\dev\toolchain-env.ps1"
$env:CARGO_TARGET_DIR="$repo\.local\t008-t016-r2-leader\target"
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0'
$u=New-Object Text.UTF8Encoding($false)
$scope=Get-Content "$PSScriptRoot\frozen-inputs.json" -Raw|ConvertFrom-Json
$overrideNames=@('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTDOC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_RUSTFLAGS','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER','RUSTUP_TOOLCHAIN','CARGO_BUILD_RUSTC','CARGO_BUILD_RUSTDOC','CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS')
foreach($n in $overrideNames){if([Environment]::GetEnvironmentVariable($n)){throw "Unexpected override $n"}}
function Inputs {
    $map=[ordered]@{}
    foreach($p in $scope.source_sha256.PSObject.Properties){
        $h=(Get-FileHash -LiteralPath "$repo\$($p.Name)" -Algorithm SHA256).Hash.ToLowerInvariant()
        if($h-ne$p.Value){throw "Frozen source changed: $($p.Name)"};$map[$p.Name]=$h
    }
    return $map
}
$commands=@(
    @{name='workspace-tests';argv=@('test','--locked','--workspace','--all-targets','--features','witvoice-node/process-tests,witvoice-session/test-support','--verbose','--','--nocapture')},
    @{name='fmt';argv=@('fmt','--all','--check')},
    @{name='clippy';argv=@('clippy','--locked','--workspace','--all-targets','--features','witvoice-node/process-tests,witvoice-session/test-support','--','-D','warnings')},
    @{name='examples';argv=@('build','--locked','--workspace','--examples')}
)
foreach($c in $commands){
    $stem=Join-Path $PSScriptRoot "$RunId-$($c.name)"
    if(Test-Path "$stem.json"){throw 'Never overwrite evidence'}
    $before=Inputs
    $si=New-Object Diagnostics.ProcessStartInfo
    $si.FileName="$env:CARGO_HOME\bin\cargo.exe";$si.Arguments=$c.argv-join' ';$si.WorkingDirectory=$repo
    $si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
    $out=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$err=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read')
    $p=New-Object Diagnostics.Process;$p.StartInfo=$si
    $started=[DateTime]::UtcNow.ToString('o');$watch=[Diagnostics.Stopwatch]::StartNew()
    try{
        if(-not$p.Start()){throw 'Cargo did not start'}
        $a=$p.StandardOutput.BaseStream.CopyToAsync($out);$b=$p.StandardError.BaseStream.CopyToAsync($err)
        $p.WaitForExit();[void]$a.GetAwaiter().GetResult();[void]$b.GetAwaiter().GetResult();$code=$p.ExitCode
    }finally{$out.Dispose();$err.Dispose();$p.Dispose()}
    $after=Inputs
    $m=[ordered]@{owner='/root';task_scope=@('T009','T010','T016');source_commit=$scope.source_commit;executable=$si.FileName;argv=$c.argv;cwd=$repo;started_utc=$started;finished_utc=[DateTime]::UtcNow.ToString('o');wall_seconds=$watch.Elapsed.TotalSeconds;exit=$code;source_before_sha256=$before;source_after_sha256=$after;target=$env:CARGO_TARGET_DIR;jobs=1;incremental=0;present_overrides=@();hardware='NOT_RUN';hypothesis='Reuse already successful workspace Windows27feature cache; compiler reliability remains UNKNOWN';stdout=@{bytes=(Get-Item "$stem.stdout").Length;sha256=(Get-FileHash "$stem.stdout").Hash.ToLowerInvariant()};stderr=@{bytes=(Get-Item "$stem.stderr").Length;sha256=(Get-FileHash "$stem.stderr").Hash.ToLowerInvariant()}}
    [IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 20),$u)
    Write-Output "$($c.name) exit=$code wall=$($watch.Elapsed.TotalSeconds)"
    if($code-ne0){exit $code}
}
