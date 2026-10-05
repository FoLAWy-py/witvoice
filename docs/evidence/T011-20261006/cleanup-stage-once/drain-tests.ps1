param([string]$RunId='final')
$ErrorActionPreference='Stop'
$startedUtc=[DateTime]::UtcNow.ToString('o')
$total=[Diagnostics.Stopwatch]::StartNew()
if($PSVersionTable.PSVersion.Major-ne5){throw 'SystemPS5.1 expected'}
$path='D:\Project\witvoice\.local\t011-cleanup-r2-root\execute-native.ps1'
$tokens=$null;$errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Supervisor syntax invalid'}
$functions=@($ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in @('Remaining','DrainUntil')},$true))
if($functions.Count-ne2){throw 'Exact two production deadline functions required'}
foreach($f in $functions){. ([scriptblock]::Create($f.Extent.Text))}
$results=@()
function Require($condition,[string]$what){if(-not$condition){throw $what}}
# Actual local anonymous pipe with its write end retained; no subprocess/model/audio/LAN.
$server=New-Object IO.Pipes.AnonymousPipeServerStream([IO.Pipes.PipeDirection]::In,[IO.HandleInheritability]::None)
$client=New-Object IO.Pipes.AnonymousPipeClientStream([IO.Pipes.PipeDirection]::Out,$server.ClientSafePipeHandle)
$sink=New-Object IO.MemoryStream
try{
    $copy=$server.CopyToAsync($sink)
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $status=DrainUntil @($copy) $clock 50
    Require ($status-eq'DEADLINE') 'Held real writer must hit shared deadline'
    Require (-not$copy.IsCompleted) 'Real held write handle did not retain read'
    Require ($clock.ElapsedMilliseconds-lt1000) 'Unexpected drain wallclock violation'
    $results+=@{case='real held write handle';status=$status;wall_ms=$clock.ElapsedMilliseconds;no_process=$true}
    $client.Dispose()
    Require ($copy.Wait(1000)) 'Pipe EOF cleanup incomplete'
}finally{$client.Dispose();$server.Dispose();$sink.Dispose()}
$done=New-Object 'Threading.Tasks.TaskCompletionSource[bool]';$done.SetResult($true)
$pending=New-Object 'Threading.Tasks.TaskCompletionSource[bool]'
$clock=[Diagnostics.Stopwatch]::StartNew()
$status=DrainUntil @($done.Task,$pending.Task) $clock 30
Require ($status-eq'DEADLINE') 'One completed stream must not renew second stream window'
Require (-not$pending.Task.IsCompleted) 'Pending fixture unexpectedly complete'
$pending.SetResult($true)
$results+=@{case='second stream remains pending';status=$status;wall_ms=$clock.ElapsedMilliseconds}
$clock=[Diagnostics.Stopwatch]::StartNew()
[Threading.Thread]::Sleep(20)
$status=DrainUntil @($done.Task) $clock 10
Require ($status-eq'DEADLINE') 'Expired absolute deadline must not be renewed even for completed tasks'
Require ((Remaining $clock 10 0)-eq0) 'Expired remaining time must be zero'
$results+=@{case='already expired deadline';status=$status;wall_ms=$clock.ElapsedMilliseconds}
$clock=[Diagnostics.Stopwatch]::StartNew()
$status=DrainUntil @($done.Task,$pending.Task) $clock 500
Require ($status-eq'COMPLETE') 'Both completed streams before deadline must succeed'
$results+=@{case='both complete';status=$status;wall_ms=$clock.ElapsedMilliseconds}
$broken=New-Object 'Threading.Tasks.TaskCompletionSource[bool]';$broken.SetException((New-Object IO.IOException('fixed test fixture')))
$clock=[Diagnostics.Stopwatch]::StartNew()
$status=DrainUntil @($broken.Task) $clock 500
Require ($status-eq'FAULTED') 'Stream fault must not be complete or release proof'
$results+=@{case='faulted stream';status=$status;wall_ms=$clock.ElapsedMilliseconds}
$output="D:\Project\witvoice\.local\t011-cleanup-r2-root\drain-tests-$RunId.json"
if(Test-Path $output){throw 'Never overwrite deadline evidence'}
function FileHash([string]$p){$s=[Security.Cryptography.SHA256]::Create();$f=[IO.File]::OpenRead($p);try{([BitConverter]::ToString($s.ComputeHash($f))).Replace('-','').ToLowerInvariant()}finally{$f.Dispose();$s.Dispose()}}
$report=[ordered]@{owner='/root';task='T011';kind='same production deadline functions extracted; real local anonymous held writer, no processes/audio/network';started_utc=$startedUtc;finished_utc=[DateTime]::UtcNow.ToString('o');wall_ms=$total.ElapsedMilliseconds;exit=0;host=$PSVersionTable.PSVersion.ToString();executable='C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe';argv=@('-NoProfile','-File',$PSCommandPath,'-RunId',$RunId);supervisor_sha256=(FileHash $path);tests_sha256=(FileHash $PSCommandPath);cases=$results}
[IO.File]::WriteAllText($output,($report|ConvertTo-Json -Depth 6),(New-Object Text.UTF8Encoding($false)))
Write-Output ($report|ConvertTo-Json -Depth 6)
