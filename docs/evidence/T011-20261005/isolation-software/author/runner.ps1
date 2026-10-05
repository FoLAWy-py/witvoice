[Console]::OutputEncoding=New-Object Text.UTF8Encoding($false)
$ProgressPreference='SilentlyContinue'
$ErrorActionPreference='Stop'
$repo='D:\Project\witvoice'
$dir="$repo\.local\t011-isolation-author"
$utf=New-Object Text.UTF8Encoding($false)
Set-Location -LiteralPath $repo
$python='C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
function Hash([string]$p){$s=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($s.ComputeHash([IO.File]::ReadAllBytes($p)))).Replace('-','').ToLower()}finally{$s.Dispose()}}
function Sources { $result=@{};foreach($name in @('model_process.py','model_host.py','model_host_bootstrap.py','test_model_process.py')){$result[$name]=(Hash "$repo\workers\vc_worker\$name")};return $result }
$hostInfo=@{version=$PSVersionTable.PSVersion.ToString();process=(Get-Process -Id $PID).Path}
if($PSVersionTable.PSVersion.Major-ne 5){throw 'SystemPS5.1 required'}
$testCode="import sys;sys.path[:0]=[r'D:\Project\witvoice\workers\vc_worker',r'D:\Project\witvoice\.local\t011-leader\python-deps'];import unittest;result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromName('test_model_process'));sys.exit(0 if result.wasSuccessful() else 1)"
$compileCode="import py_compile;from pathlib import Path;root=Path(r'D:\Project\witvoice');names=('model_process.py','model_host.py','model_host_bootstrap.py','test_model_process.py');[py_compile.compile(str(root/'workers/vc_worker'/name),cfile=str(root/'.local/t011-isolation-author'/(name+'.pyc')),doraise=True) for name in names];print('4 assigned Python sources compiled; model/main NOT_RUN')"
$commands=@(
 @{name='01-software-tests';argv=@('-I','-S','-B','-c',$testCode)},
 @{name='02-four-source-compile';argv=@('-I','-S','-B','-c',$compileCode)}
)
$events=@()
foreach($c in $commands){
 $before=Sources
 $si=New-Object Diagnostics.ProcessStartInfo
 $si.FileName=$python
 $si.Arguments=($c.argv|ForEach-Object {if($_.Contains(' ')){'"'+$_+'"'}else{$_}})-join ' '
 $si.WorkingDirectory=$repo;$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $start=[DateTime]::UtcNow.ToString('o')
 $p=New-Object Diagnostics.Process;$p.StartInfo=$si;[void]$p.Start()
 $ot=$p.StandardOutput.ReadToEndAsync();$et=$p.StandardError.ReadToEndAsync();$p.WaitForExit()
 $out=$ot.GetAwaiter().GetResult();$err=$et.GetAwaiter().GetResult();$code=$p.ExitCode;$end=[DateTime]::UtcNow.ToString('o');$p.Dispose()
 $stem=Join-Path $dir $c.name
 [IO.File]::WriteAllText("$stem.stdout",$out,$utf);[IO.File]::WriteAllText("$stem.stderr",$err,$utf)
 $after=Sources
 $meta=[ordered]@{task='T011 software model process isolation';uuid='01a10767-4ff5-7f21-881a-146d094fbc55';executable=$python;argv=$c.argv;cwd=$repo;execution_host=$hostInfo;start_utc=$start;end_utc=$end;exit_code=$code;source_before_sha256=$before;source_after_sha256=$after;stdout=@{bytes=(Get-Item "$stem.stdout").Length;sha256=(Hash "$stem.stdout")};stderr=@{bytes=(Get-Item "$stem.stderr").Length;sha256=(Hash "$stem.stderr")};capture='Complete redirect stream text UTF8 withoutBOM';scope='Only test_model_process and four py_compile outputs; test subprocess executes only finite native GIL Sleep; modelhost/realmodel/Torch/GPU/audio/LAN NOT_RUN'}
 [IO.File]::WriteAllText("$stem.json",($meta|ConvertTo-Json -Depth 8),$utf)
 $events+=@{path="$stem.json";sha256=(Hash "$stem.json");exit_code=$code}
 Write-Output ($meta|ConvertTo-Json -Depth 8 -Compress)
 if($code-ne 0){break}
 foreach($key in $before.Keys){if($before[$key]-ne $after[$key]){throw 'Source changed during check'}}
}
$index=[ordered]@{uuid='01a10767-4ff5-7f21-881a-146d094fbc55';utc=[DateTime]::UtcNow.ToString('o');runner_sha256=(Hash "$dir\runner.ps1");sources=(Sources);events=$events;not_run=@('model_host_bootstrap main','real model/Torch/GPU','Node owned specificJob inheritance witness','audio','LAN','Mac');writes='STOP_AFTER_DELIVERY'}
[IO.File]::WriteAllText("$dir\delivery-index.json",($index|ConvertTo-Json -Depth 8),$utf)
Write-Output ('INDEX_SHA256='+(Hash "$dir\delivery-index.json"))
