$ErrorActionPreference = 'Stop'
$env:PSModulePath = 'C:\Windows\System32\WindowsPowerShell\v1.0\Modules'
$taskRoot = 'D:\Project\witvoice'
$taskOutput = Join-Path $taskRoot '.local\t011-lifetime-observation-author'
$taskStart = [DateTime]::UtcNow.ToString('o')
$utf8 = New-Object System.Text.UTF8Encoding($false)
$taskPaths = @('crates/platform/src/process.rs','crates/platform/src/lib.rs','crates/engines/src/supervisor.rs','crates/engines/tests/worker_supervision.rs')
function GitText([string]$arguments) {
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = 'git.exe'
    $info.Arguments = $arguments
    $info.WorkingDirectory = $taskRoot
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = $utf8
    $info.StandardErrorEncoding = $utf8
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $info
    $started = [DateTime]::UtcNow.ToString('o')
    if (-not $process.Start()) { throw 'git start failed' }
    $out = $process.StandardOutput.ReadToEnd()
    $err = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $result = [ordered]@{argv=('git.exe ' + $arguments);start_utc=$started;end_utc=[DateTime]::UtcNow.ToString('o');exit=$process.ExitCode;stdout=$out;stderr=$err}
    $process.Dispose()
    if ($result.exit -ne 0) { throw ('read-only git failed: ' + $result.exit) }
    return $result
}
function TextSha([string]$body) {
    $hash = [System.Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($hash.ComputeHash($utf8.GetBytes($body)))).Replace('-','').ToLowerInvariant() }
    finally { $hash.Dispose() }
}
function FileSha([string]$path) {
    $hash = [System.Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($hash.ComputeHash([System.IO.File]::ReadAllBytes($path)))).Replace('-','').ToLowerInvariant() }
    finally { $hash.Dispose() }
}
$head = GitText 'rev-parse HEAD'
$sources = @()
foreach ($relative in $taskPaths) {
    $original = GitText ('show ' + $head.stdout.Trim() + ':' + $relative)
    $absolute = Join-Path $taskRoot $relative
    $sources += [ordered]@{path=$relative;baseline_git_ref=$head.stdout.Trim();baseline_git_utf8_lf_sha256=(TextSha $original.stdout);working_raw_sha256=(FileSha $absolute);working_bytes=([System.IO.File]::ReadAllBytes($absolute)).Length;baseline_command=$original}
}
$diff = GitText ('diff -- ' + ($taskPaths -join ' '))
[System.IO.File]::WriteAllText((Join-Path $taskOutput 'source.diff'),$diff.stdout,$utf8)
[System.IO.File]::WriteAllText((Join-Path $taskOutput 'git.stderr'),$diff.stderr,$utf8)
$index = [ordered]@{task='T011 lifetime failure observation only';agent='/root/backend';actual_uuid='01a10767-4ff5-7f21-881a-146d094fbc55';start_utc=$taskStart;end_utc=[DateTime]::UtcNow.ToString('o');status='STOP_WRITES';source_edits='apply_patch tool results in this thread; exact edit UTC not independently recorded';sources=$sources;diff=$diff;checks=[ordered]@{fmt='NOT_RUN';cargo_test='NOT_RUN';clippy='NOT_RUN';native_suite='NOT_RUN';model='NOT_RUN';gpu='NOT_RUN';audio='NOT_RUN';lan='NOT_RUN'};limitations=@('Diagnostic-only source not yet formatted or compiled.','Prior EOF versus Protocol cause remains UNKNOWN.','No native query or cleanup predicate was added or removed.','Three new pure snapshot tests are authored, not executed.');shared_dependency_mutations='NONE';prior_archive_failure=[ordered]@{tool_chunk='e34e8f';exit=1;reason='Get-FileHash cmdlet unavailable in SystemPS5.1 child';application_test='NOT_RUN';full_stdout_stderr='tool e34e8f in this author thread'};script_sha256=(FileSha $PSCommandPath)}
$indexPath = Join-Path $taskOutput 'delivery-index.json'
if ([System.IO.File]::Exists($indexPath)) { throw 'refuse to overwrite delivery index' }
[System.IO.File]::WriteAllText($indexPath,($index | ConvertTo-Json -Depth 15),$utf8)
[ordered]@{index=$indexPath;sha256=(FileSha $indexPath);sources=$sources | Select-Object path,working_raw_sha256,working_bytes;all_checks='NOT_RUN';status='STOP_WRITES'} | ConvertTo-Json -Depth 5
