$ErrorActionPreference='Stop'
$dir='D:\Project\witvoice\.local\t010-clippy-fix'
$source='D:\Project\witvoice\tests\integration\identity_pipeline.rs'
$exe='D:\Software\WitvoiceToolchain\Rust\rustup\toolchains\1.99.0-x86_64-pc-windows-msvc\bin\rustfmt.exe'
if(-not [IO.File]::Exists($exe)){throw 'Pinned rustfmt missing'}
$utf8=New-Object Text.UTF8Encoding($false)
$events=@()
foreach($step in @(@{name='01-rustfmt';argv=@('--edition','2024',$source)},@{name='02-rustfmt-check';argv=@('--edition','2024','--check',$source)})){
 $before=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLower()
 $psi=New-Object Diagnostics.ProcessStartInfo
 $psi.FileName=$exe
 $psi.Arguments=($step.argv | ForEach-Object { if($_.Contains(' ')){ '"'+$_+'"' }else{$_} }) -join ' '
 $psi.WorkingDirectory='D:\Project\witvoice'
 $psi.UseShellExecute=$false
 $psi.CreateNoWindow=$true
 $psi.RedirectStandardOutput=$true
 $psi.RedirectStandardError=$true
 $start=[DateTime]::UtcNow.ToString('o')
 $proc=New-Object Diagnostics.Process
 $proc.StartInfo=$psi
 [void]$proc.Start()
 $outTask=$proc.StandardOutput.ReadToEndAsync()
 $errTask=$proc.StandardError.ReadToEndAsync()
 $proc.WaitForExit()
 $outText=$outTask.GetAwaiter().GetResult()
 $errText=$errTask.GetAwaiter().GetResult()
 $exitCode=$proc.ExitCode
 $end=[DateTime]::UtcNow.ToString('o')
 $proc.Dispose()
 $outPath=Join-Path $dir ($step.name+'.stdout')
 $errPath=Join-Path $dir ($step.name+'.stderr')
 [IO.File]::WriteAllText($outPath,$outText,$utf8)
 [IO.File]::WriteAllText($errPath,$errText,$utf8)
 $event=[ordered]@{uuid='01a10767-4ff5-7f21-881a-146d094fbc55';executable=$exe;argv=$step.argv;cwd=$psi.WorkingDirectory;host_powershell=$PSVersionTable.PSVersion.ToString();start_utc=$start;end_utc=$end;exit_code=$exitCode;source_before_sha256=$before;source_after_sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLower();stdout=@{path=$outPath;bytes=(Get-Item -LiteralPath $outPath).Length;sha256=(Get-FileHash -LiteralPath $outPath -Algorithm SHA256).Hash.ToLower()};stderr=@{path=$errPath;bytes=(Get-Item -LiteralPath $errPath).Length;sha256=(Get-FileHash -LiteralPath $errPath -Algorithm SHA256).Hash.ToLower()};capture='Complete redirected stream text saved UTF8 without BOM; no truncation';scope='standalone rustfmt only; Cargo/tests/HW/network NOT_RUN'}
 $metaPath=Join-Path $dir ($step.name+'.json')
 [IO.File]::WriteAllText($metaPath,($event|ConvertTo-Json -Depth 6),$utf8)
 $events+=@{path=$metaPath;sha256=(Get-FileHash -LiteralPath $metaPath -Algorithm SHA256).Hash.ToLower();exit_code=$exitCode}
 if($exitCode -ne 0){break}
}
$index=[ordered]@{uuid='01a10767-4ff5-7f21-881a-146d094fbc55';utc=[DateTime]::UtcNow.ToString('o');source=@{path=$source;sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLower()};commands=$events;runner_sha256=(Get-FileHash -LiteralPath (Join-Path $dir 'run-format.ps1') -Algorithm SHA256).Hash.ToLower();not_run=@('Cargo','Clippy','tests','audio','network','GPU');writes='STOP_AFTER_DELIVERY'}
$indexPath=Join-Path $dir 'delivery-index.json'
[IO.File]::WriteAllText($indexPath,($index|ConvertTo-Json -Depth 6),$utf8)
Get-Content -LiteralPath $indexPath
foreach($e in $events){Get-Content -LiteralPath $e.path}
Write-Output ('INDEX_SHA256='+(Get-FileHash -LiteralPath $indexPath -Algorithm SHA256).Hash.ToLower())