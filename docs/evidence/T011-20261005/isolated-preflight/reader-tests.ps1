
$ErrorActionPreference='Stop'
$root='D:\Project\witvoice';$u=New-Object Text.UTF8Encoding($false)
function FileHash([string]$path){$f=[IO.File]::OpenRead($path);$h=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($h.ComputeHash($f))).Replace('-','').ToLowerInvariant()}finally{$h.Dispose();$f.Dispose()}}
$source=Get-Content "$root\.local\t011-isolated-warmup-once\execute.ps1" -Raw
$start=$source.IndexOf(' # Read scalar phase evidence');$end=$source.IndexOf(' $r.finished_utc',$start)
if($start -lt 0 -or $end -lt $start){throw 'Cannot locate actual reader'}
$block=[scriptblock]::Create($source.Substring($start,$end-$start))
$stages=@('imports_array_audio','imports_torch','imports_adapter','fixtures_check','model_load','placement_validate','convert_warmup','finalize')
$lines=@();$time=0
foreach($stage in $stages){foreach($edge in @('before','after')){$time++;$lines+=([ordered]@{stage=$stage;edge=$edge;elapsed_ns=$time}|ConvertTo-Json -Compress)}}
$valid=($lines-join"`n")+"`n"
$cases=@(
 @{name='complete';text=$valid;expect='VALID_BOUNDED_SCALARS';complete=$true},
 @{name='partial_progress';text=($lines[0..2]-join"`n")+"`n";expect='VALID_BOUNDED_SCALARS';complete=$false},
 @{name='partial_tail';text=$valid.TrimEnd("`n");expect='INVALID_OR_PARTIAL';complete=$false},
 @{name='duplicate';text='{"stage":"private","stage":"imports_array_audio","edge":"before","elapsed_ns":1}'+"`n";expect='INVALID_OR_PARTIAL';complete=$false},
 @{name='extra';text='{"stage":"imports_array_audio","edge":"before","elapsed_ns":1,"path":"private"}'+"`n";expect='INVALID_OR_PARTIAL';complete=$false},
 @{name='oversize';text=('a'*4097);expect='INVALID_OR_PARTIAL';complete=$false},
 @{name='overcount';text=$valid+$lines[0]+"`n";expect='INVALID_OR_PARTIAL';complete=$false},
 @{name='order';text=$lines[1]+"`n";expect='INVALID_OR_PARTIAL';complete=$false}
)
$results=@();$started=[DateTime]::UtcNow.ToString('o')
foreach($case in $cases){
 $base="$root\.local\t011-isolation-root\reader-cases\$($case.name)";[void][IO.Directory]::CreateDirectory($base)
 [IO.File]::WriteAllText("$base\prepare-stages.ndjson",$case.text,$u)
 $r=[ordered]@{owned_job_cleanup='CONFIRMED_BY_REAL_OWNER';status='FAIL_REAL_WARMUP'}
 & $block
 if($r.phase_diagnostic -ne $case.expect -or $r.phase_complete -ne $case.complete){throw "Actual reader mismatch $($case.name)"}
 $results+=@{case=$case.name;actual=$r.phase_diagnostic;complete=$r.phase_complete;exit=0}
}
$result=[ordered]@{host=$PSVersionTable.PSVersion.ToString();started_utc=$started;finished_utc=[DateTime]::UtcNow.ToString('o');supervisor_sha256=(FileHash "$root\.local\t011-isolated-warmup-once\execute.ps1");cases=$results;tests=8;exit=0;model='NOT_RUN';gpu='NOT_RUN';audio='NOT_RUN';scope='Actual reader block only; no supervisor main/model launch';prior_host_failure='8d17cf -1073740791; later readonly confirmed script absent; no test result'}
[IO.File]::WriteAllText("$root\.local\t011-isolation-root\reader-tests.json",($result|ConvertTo-Json -Depth 10),$u)
$result|ConvertTo-Json -Depth 10
