
$ErrorActionPreference='Stop';$root='D:\Project\witvoice';$u=New-Object Text.UTF8Encoding($false)
$source=Get-Content "$root\.local\t011-budget8-warmup-once\execute.ps1" -Raw
$line=@($source -split [char]10|Where-Object { $_.TrimStart().StartsWith('if(-not $timeout') })[0]
if(-not $line){throw 'No actual success predicate'}
$predicate=[scriptblock]::Create($line)
$baseline=@{model_job_membership_verified=$true;result='READY';model_ready=$true;ready_received=$true;stopped_ack=$true;cleanup_confirmed=$true;owned_active_processes=0;policy_output_allowed_after_cleanup=$false;sink_present=$false;output_open=$false;media_exchanged=$false;automatic_retries=0;heartbeat_count=2;max_heartbeat_gap_ms=100;model_sha256='01caafec9a3991a5514df9412952d24ae3370406358a01e20e03df41f2f5d515'}
$cases=@(
 @{name='known_ready';key='';value=0;pass=$true},
 @{name='unknown_job';key='model_job_membership_verified';value=$null;pass=$false},
 @{name='wrong_job';key='model_job_membership_verified';value=$false;pass=$false},
 @{name='unknown_gap';key='max_heartbeat_gap_ms';value=$null;pass=$false},
 @{name='first_ack_unknown';key='heartbeat_count';value=1;pass=$false},
 @{name='deadline';key='max_heartbeat_gap_ms';value=500;pass=$false},
 @{name='not_ready';key='ready_received';value=$false;pass=$false},
 @{name='no_stop';key='stopped_ack';value=$false;pass=$false},
 @{name='not_empty';key='owned_active_processes';value=1;pass=$false},
 @{name='open_output';key='output_open';value=$true;pass=$false},
 @{name='media';key='media_exchanged';value=$true;pass=$false},
 @{name='retry';key='automatic_retries';value=1;pass=$false}
)
$result=@()
foreach($case in $cases){
 $report=$baseline.Clone();if($case.key){$report[$case.key]=$case.value}
 $timeout=$false;$r=[ordered]@{exit=0;status='FAIL_REAL_WARMUP';model='UNVERIFIED'}
 & $predicate
 $passed=$r.status -eq 'PASS_REAL_WARMUP_ONLY'
 if($passed -ne $case.pass){throw "Actual predicate mismatch $($case.name)"}
 $result+=@{case=$case.name;actual_accept=$passed;exit=0}
}
$metadata=@{host=$PSVersionTable.PSVersion.ToString();tests=12;exit=0;cases=$result;model='NOT_RUN';gpu='NOT_RUN';audio='NOT_RUN';scope='Actual extracted frozen success predicate; no supervisor main'}
[IO.File]::WriteAllText("$root\.local\t011-budget8-root\predicate-tests.json",($metadata|ConvertTo-Json -Depth 10),$u)
$metadata|ConvertTo-Json -Depth 10
