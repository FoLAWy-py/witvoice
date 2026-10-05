$ErrorActionPreference='Stop'
$base='D:\Project\witvoice\.local\t009-t010-final'
$systemHost='C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
$si=New-Object Diagnostics.ProcessStartInfo
$si.FileName=$systemHost
$si.Arguments='-NoLogo -NoProfile -NonInteractive -File "'+$base+'\verify.ps1" -RunId final-ps51'
$si.WorkingDirectory='D:\Project\witvoice'
$si.UseShellExecute=$false;$si.CreateNoWindow=$true
$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
$out=[IO.File]::Open("$base\host.stdout",'CreateNew','Write','Read')
$err=[IO.File]::Open("$base\host.stderr",'CreateNew','Write','Read')
$p=New-Object Diagnostics.Process;$p.StartInfo=$si
$start=[DateTime]::UtcNow.ToString('o')
try {
    if(-not$p.Start()){throw 'Explicit system child did not start'}
    $a=$p.StandardOutput.BaseStream.CopyToAsync($out)
    $b=$p.StandardError.BaseStream.CopyToAsync($err)
    $p.WaitForExit()
    [void]$a.GetAwaiter().GetResult();[void]$b.GetAwaiter().GetResult()
    $code=$p.ExitCode
}finally{$out.Dispose();$err.Dispose();$p.Dispose()}
$m=[ordered]@{requested_child=$systemHost;outer_host_version=$PSVersionTable.PSVersion.ToString();started_utc=$start;finished_utc=[DateTime]::UtcNow.ToString('o');exit=$code;no_hardware=$true;no_retry=$true}
[IO.File]::WriteAllText("$base\host.json",($m|ConvertTo-Json),(New-Object Text.UTF8Encoding($false)))
Get-Content "$base\host.stdout"
Get-Content "$base\host.stderr"
exit $code
