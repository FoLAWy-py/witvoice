$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1){throw 'SystemPS5.1 required'}
$repo='D:\Project\witvoice';Set-Location $repo
. "$repo\tools\dev\toolchain-env.ps1"
$env:CARGO_TARGET_DIR="$repo\.local\t008-t016-r2-leader\target"
$env:CARGO_BUILD_JOBS='1';$env:CARGO_INCREMENTAL='0'
$u=New-Object Text.UTF8Encoding($false)
function Hash([string]$path){$s=[IO.File]::OpenRead($path);$h=[Security.Cryptography.SHA256]::Create();try{return ([BitConverter]::ToString($h.ComputeHash($s))).Replace('-','').ToLowerInvariant()}finally{$h.Dispose();$s.Dispose()}}
$scope=Get-Content "$PSScriptRoot\frozen-inputs.json" -Raw|ConvertFrom-Json
function Inputs {foreach($v in $scope.source_sha256.PSObject.Properties){if((Hash "$repo\$($v.Name)")-ne$v.Value){throw "Source changed $($v.Name)"}}}
$commands=@(
    @{name='default-feature-check';argv=@('check','--locked','--workspace','--all-targets');expected=0},
    @{name='test-support-nondebug-guard';argv=@('rustc','--locked','-p','witvoice-session','--lib','--features','test-support','--','-C','debug-assertions=off');expected=101}
)
foreach($c in $commands){
    Inputs
    $stem="$PSScriptRoot\$($c.name)"
    $si=New-Object Diagnostics.ProcessStartInfo;$si.FileName="$env:CARGO_HOME\bin\cargo.exe";$si.Arguments=$c.argv-join' ';$si.WorkingDirectory=$repo
    $si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
    $out=[IO.File]::Open("$stem.stdout",'CreateNew','Write','Read');$err=[IO.File]::Open("$stem.stderr",'CreateNew','Write','Read')
    $p=New-Object Diagnostics.Process;$p.StartInfo=$si;$start=[DateTime]::UtcNow.ToString('o')
    try{if(-not$p.Start()){throw 'Cargo did not start'};$a=$p.StandardOutput.BaseStream.CopyToAsync($out);$b=$p.StandardError.BaseStream.CopyToAsync($err);$p.WaitForExit();[void]$a.GetAwaiter().GetResult();[void]$b.GetAwaiter().GetResult();$code=$p.ExitCode}finally{$out.Dispose();$err.Dispose();$p.Dispose()}
    Inputs
    $raw=[IO.File]::ReadAllText("$stem.stderr")
    $recognized=$code-eq$c.expected
    if($c.expected-ne0){$recognized=$recognized-and$raw.Contains('test-support is forbidden in release builds')-and-not$raw.Contains('STATUS_ACCESS_VIOLATION')-and-not$raw.Contains('internal compiler error')}
    $m=[ordered]@{source_commit=$scope.source_commit;argv=$c.argv;exit=$code;expected_exit=$c.expected;expected_rejection_recognized=$recognized;actual_host_version=$PSVersionTable.PSVersion.ToString();actual_host_path=[Diagnostics.Process]::GetCurrentProcess().MainModule.FileName;started_utc=$start;finished_utc=[DateTime]::UtcNow.ToString('o');source_all72_equal=$true;stdout_sha256=(Hash "$stem.stdout");stderr_sha256=(Hash "$stem.stderr");scope='Compile defaultfeatures and test-support rejection with debugassertionsoff; NOT a full release installer or hardware test'}
    [IO.File]::WriteAllText("$stem.json",($m|ConvertTo-Json -Depth 10),$u)
    Write-Output "$($c.name) actualexit=$code recognized=$recognized"
    if(-not$recognized){exit 1}
}
