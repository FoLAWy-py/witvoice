$ErrorActionPreference='Stop';$root='D:\Project\witvoice';$u=New-Object Text.UTF8Encoding($false)
function FileHash([string]$path){$s=[IO.File]::OpenRead($path);$h=[Security.Cryptography.SHA256]::Create();try{([BitConverter]::ToString($h.ComputeHash($s))).Replace('-','').ToLowerInvariant()}finally{$h.Dispose();$s.Dispose()}}
$source=Get-Content "$root\.local\t011-finalize-warmup-once\execute.ps1" -Raw
$start=$source.IndexOf(' # Read closed resource evidence');$end=$source.IndexOf(' $r.finished_utc',$start)
if($start-lt0-or$end-lt$start){throw 'Missing actual resource reader'}
$block=[scriptblock]::Create($source.Substring($start,$end-$start))
$suffix=',"host_budget_bytes":6442450944,"device_budget_bytes":4294967296}'
$valid='{"host_private_bytes":123,"device_reserved_bytes":456'+$suffix
$cases=@(
 @{name='known';text=$valid;valid=$true;known=$true;budget=$true},
 @{name='host_over';text='{"host_private_bytes":6442450945,"device_reserved_bytes":1'+$suffix;valid=$true;known=$true;budget=$false},
 @{name='device_over';text='{"host_private_bytes":1,"device_reserved_bytes":4294967297'+$suffix;valid=$true;known=$true;budget=$false},
 @{name='unknown';text='{"host_private_bytes":null,"device_reserved_bytes":456'+$suffix;valid=$true;known=$false;budget=$false},
 @{name='duplicate';text=$valid.Replace('"host_private_bytes":123','"host_private_bytes":1,"host_private_bytes":123');valid=$false;known=$false;budget=$false},
 @{name='extra';text=$valid.Replace('}',',"path":"private"}');valid=$false;known=$false;budget=$false},
 @{name='boolean';text=$valid.Replace(':123',':true');valid=$false;known=$false;budget=$false},
 @{name='oversize';text=('a'*513);valid=$false;known=$false;budget=$false},
 @{name='partial';text=$valid.Substring(0,$valid.Length-1);valid=$false;known=$false;budget=$false}
)
$results=@();$started=[DateTime]::UtcNow.ToString('o')
foreach($case in $cases){
 $base="$root\.local\t011-resource-root\resource-cases\$($case.name)";[void][IO.Directory]::CreateDirectory($base)
 [IO.File]::WriteAllText("$base\resources.json",$case.text,$u)
 $r=[ordered]@{owned_job_cleanup='CONFIRMED_BY_REAL_OWNER';status='PASS_REAL_WARMUP_ONLY'}
 & $block
 if(($r.resource_diagnostic-eq'VALID_BOUNDED_SCALARS')-ne$case.valid-or$r.resource_known-ne$case.known-or$r.resource_within_budget-ne$case.budget){throw "Resource reader mismatch $($case.name)"}
 if(($r.status-eq'PASS_REAL_WARMUP_ONLY')-ne$case.budget){throw "Resource success gate mismatch $($case.name)"}
 $results+=@{case=$case.name;diagnostic=$r.resource_diagnostic;known=$r.resource_known;within_budget=$r.resource_within_budget;exit=0}
}
$result=@{host=$PSVersionTable.PSVersion.ToString();tests=$cases.Count;exit=0;started_utc=$started;finished_utc=[DateTime]::UtcNow.ToString('o');cases=$results;supervisor_sha256=(FileHash "$root\.local\t011-finalize-warmup-once\execute.ps1");model='NOT_RUN';GPU='NOT_RUN';audio='NOT_RUN';scope='Actual extracted resource reader/gate only; no supervisor main'}
[IO.File]::WriteAllText("$root\.local\t011-resource-root\resources-tests.json",($result|ConvertTo-Json -Depth 10),$u)
$result|ConvertTo-Json -Depth 10
