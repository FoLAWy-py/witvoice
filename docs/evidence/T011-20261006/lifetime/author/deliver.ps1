$ErrorActionPreference='Stop'
Set-Location -LiteralPath 'D:\Project\witvoice'
$dir='D:\Project\witvoice\.local\t011-lifetime-author'
$output=Join-Path $dir 'delivery-index.json'
if(Test-Path -LiteralPath $output){throw 'Unique delivery index exists'}
function Record([string]$path){$file=Join-Path 'D:\Project\witvoice' $path;[ordered]@{path=$path;bytes=(Get-Item -LiteralPath $file).Length;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()}}
$labels=@('01-fmt','02-platform-pure','03-observation-pure','04-lifecycle-pure','05-fmtcheck','06-workspace-clippy','07-workspace-examples','08-fmt-final','09-platform-final','10-observation-final','11-lifecycle-final','12-fmtcheck-final','13-clippy-final','14-examples-final')
$checks=@();$artifacts=@()
foreach($label in $labels){
 $meta=Get-Content -LiteralPath (Join-Path $dir ($label+'.json')) -Raw | ConvertFrom-Json
 if($meta.kind -ne 'fmt'){foreach($before in $meta.source_before){
  $after=@($meta.source_after | Where-Object {$_.path -eq $before.path})[0]
  if($before.sha256 -ne $after.sha256){throw 'Check input changed while command ran'}
 }}
 foreach($stream in @($meta.stdout,$meta.stderr)){if((Get-FileHash -LiteralPath $stream.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $stream.sha256){throw 'Raw SHA mismatch'}}
 $checks+=$meta
 foreach($extension in @('json','stdout','stderr')){$artifacts+=Record ('.local/t011-lifetime-author/'+$label+'.'+$extension)}
}
$sources=@(Record 'crates/platform/src/process.rs';Record 'crates/engines/src/supervisor.rs';Record 'crates/engines/tests/worker_supervision.rs';Record 'tests/integration/worker_supervision_peer.py')
foreach($source in $sources){foreach($check in $checks[8..13]){
 $before=@($check.source_before | Where-Object {$_.path -eq $source.path})[0];$after=@($check.source_after | Where-Object {$_.path -eq $source.path})[0]
 if($before.sha256 -ne $source.sha256 -or $after.sha256 -ne $source.sha256){throw 'Final four source bytes not covered'}
}}
foreach($label in @('09-platform-final','10-observation-final','11-lifecycle-final')){
 $expected=@{'09-platform-final'=29;'10-observation-final'=11;'11-lifecycle-final'=16}[$label]
 $raw=Get-Content -LiteralPath (Join-Path $dir ($label+'.stdout')) -Raw
 if($raw -notmatch ('test result: ok\. '+$expected+' passed; 0 failed;')){throw 'Actual test result mismatch'}
}
$index=[ordered]@{
 task='T011-exact-job-lifetime-ledger';actual_agent='/root/backend';actual_agent_uuid='01a10767-4ff5-7f21-881a-146d094fbc55';created_utc=[DateTime]::UtcNow.ToString('o');status='STOP_WRITES_STOP_CARGO';host=$PSVersionTable.PSVersion.ToString();
 source_before_edit=@(@{path='crates/platform/src/process.rs';sha256='cad36cbc6fead6b53353becfeaa97c7dcc322798eb59f461a55ac5241e0f2767'},@{path='crates/engines/src/supervisor.rs';sha256='286f7d8e36657d8469203ea2b6de37b5e406a415d3be7f44466de3c5e5bbe55d'},@{path='crates/engines/tests/worker_supervision.rs';sha256='40aba769b5add6039f68a30ab20b14ba0eeae7a4240147fecec3cb0a36c441cd'},@{path='tests/integration/worker_supervision_peer.py';sha256='79981208e2f8e66c7a0a0c307a4a5c9727701797810022d2e341e95470e7b667'});
 sources=$sources;changed_source_paths=@('crates/platform/src/process.rs','crates/engines/src/supervisor.rs');unchanged_allowed_paths=@('crates/engines/tests/worker_supervision.rs','tests/integration/worker_supervision_peer.py');checks=$checks;artifacts=$artifacts;runner=Record '.local/t011-lifetime-author/check.ps1';delivery_script=Record '.local/t011-lifetime-author/deliver.ps1';
 implementation='Each OwnedWorker unique ProcessJob explicitly enables a private<=8 member ledger only on a fresh Job. Suspended controller retained exact kernel handle before Resume; copied handles never inherit. Active new members opened once, exact Job verified and creation-time identity retained. Historical handles remain after natural exit; already recorded PID is never reopened. Stable before/after total,active,IDs and lifetime union coverage required; first missing identity/error freezes. Staged+retained slots bounded before OpenProcess. Supervisor captures before/after auth, before poll/Stop, and before closing IPC. Strict kill still occurs when capture failed, all lifetime handles wait only remaining shared3s, Job0/IDsempty/controller/policy confirmation remain. Generic/UI launch and generic terminate APIs remain untracked by default.';
 pure_production_tests='Six new platform tests drive actual shared predicates: naturally exited retained identity; changed creation/PID reuse; duplicate/zero/eight-capacity plus pre-open slot and checked-add overflow; missing historical/new total; exact native-vsWin32 error replay; immutable first ledger error. Existing bounded stable snapshot and shared deadline tests retained. No pure test is presented as a performed kernel capture/kill/cleanup.';
 final_coverage='09-14 bind all four allowed source hashes before/after, unchanged manifests and lock. 01-07 successful earlier version results retained; final capacity pre-open guard added after them, so only09-14 are claimed final. 08 formats final. No failures, retries or warnings were observed; second checks follow a real source change, not a blind retry.';
 final_results=[ordered]@{platform_pure=29;cleanup_observation_pure=11;lifecycle_pure=16;total_pure=56;native_worker_cases_filtered=8;fmtcheck_exit=0;strict_workspace_clippy_exit=0;workspace_examples_build_exit=0;all_checks_exit=0;compiler_ICE_AV='NONE_OBSERVED';native_process_fixture='NOT_RUN';Python_peer='NOT_RUN_UNCHANGED';model_GPU_audio_LAN='NOT_RUN';whole_workspace_tests='NOT_RUN';Mac='NOT_RUN';full_T011='IN_PROGRESS_NOT_SELF_APPROVED'};
 risks=@('Lifetime coverage is supported only for previously retained exact handles. Birth-and-exit between snapshots remains UNKNOWN and quarantines; no watcher/completionport/guaranteed birth coverage was added.','Stable capture can conservatively reject a racing membership transition. No retry, changed member budget or relaxed predicate hides that rejection.','Actual same-source native eight cases require new frozen independent preflight; none were executed by this author. Prior consumed native passes/failures and root warmup evidence do not replace this verification.','Synchronous native accounting/handle calls have OS latency; shared3s wait budget does not certify hard realtime. This owner is exclusively non-realtime.','All working byte hashes are distinct from any Git LF-normalization proof unless separately established by leader.');
 failures=@();publication='Only leader archives public evidence/state and approves integrated freeze via independent reviewer'
}
[IO.File]::WriteAllText($output,($index|ConvertTo-Json -Depth 18)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
Record '.local/t011-lifetime-author/delivery-index.json' | ConvertTo-Json
