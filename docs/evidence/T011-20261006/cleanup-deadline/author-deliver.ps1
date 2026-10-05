$ErrorActionPreference='Stop'
Set-Location -LiteralPath 'D:\Project\witvoice'
$dir='D:\Project\witvoice\.local\t011-cleanup-deadline-author'
$output=Join-Path $dir 'delivery-index.json'
if(Test-Path -LiteralPath $output){throw 'Unique delivery file already exists'}
function Record([string]$path){
 $file=Join-Path 'D:\Project\witvoice' $path
 [ordered]@{path=$path;bytes=(Get-Item -LiteralPath $file).Length;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$checks=@();$artifacts=@()
foreach($label in @('01-fmt','02-observation-tests','03-fmtcheck')){
 $meta=Get-Content -LiteralPath (Join-Path $dir ($label+'.json')) -Raw | ConvertFrom-Json
 foreach($stream in @($meta.stdout,$meta.stderr)){
  if((Get-FileHash -LiteralPath $stream.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $stream.sha256){throw 'Raw stream SHA mismatch'}
 }
 $checks+=$meta
 foreach($extension in @('json','stdout','stderr')){$artifacts+=Record ('.local/t011-cleanup-deadline-author/'+$label+'.'+$extension)}
}
$sources=@(Record 'crates/engines/src/supervisor.rs';Record 'crates/engines/tests/worker_supervision.rs')
foreach($source in $sources){foreach($check in @($checks[1],$checks[2])){
 $before=@($check.source_before | Where-Object {$_.path -eq $source.path})[0]
 $after=@($check.source_after | Where-Object {$_.path -eq $source.path})[0]
 if($before.sha256 -ne $source.sha256 -or $after.sha256 -ne $source.sha256){throw 'Final source not covered'}
}}
$result=[ordered]@{
 task='T011-limited-cleanup-shared-deadline';actual_agent='/root/backend';actual_agent_uuid='01a10767-4ff5-7f21-881a-146d094fbc55';created_utc=[DateTime]::UtcNow.ToString('o');status='STOP_WRITES_STOP_CARGO';host=$PSVersionTable.PSVersion.ToString();
 source_before_edit=@(@{path='crates/engines/src/supervisor.rs';sha256='6a4e452bed1cf080647b4faabfd8e0fb6354d39b284e3acbaadfe97fb486ed11'},@{path='crates/engines/tests/worker_supervision.rs';sha256='99f7c42455462087974ce7b20b3266f19b073754f617e03bb1ea6f57d529690c'});
 sources=$sources;checks=$checks;artifacts=$artifacts;runner=Record '.local/t011-cleanup-deadline-author/check.ps1';delivery_script=Record '.local/t011-cleanup-deadline-author/deliver.ps1';unchanged_peer=Record 'tests/integration/worker_supervision_peer.py';
 implementation='Cleanup entry captures started and one deadline=started+3s, passes that same Instant to ProcessJob::terminate_until and remaining-only retained controller wait. No renewed timer, extra query, altered termination/query/order/predicate or capability gate. If remaining budget is zero or negative, helper returns Cleanup timeout without calling wait; controller scalar fields remain null. API error does not confirm policy or release owned resources. First cleanup error snapshot remains immutable.';
 test_scope='Three new deterministic cases use exactly production CleanupObservation::wait_until with bounded normalized/native-result fixtures and virtual timestamp arithmetic; no sleeping, Process/Job/Python instantiation or identity/device/network call. All previous8 observation tests retained. The shared helper test does not prove actual OS controller teardown timing.';
 results=[ordered]@{rustfmt_exit=0;pure_observation_tests_passed=11;pure_observation_tests_failed=0;native_tests_filtered_not_run=8;fmt_check_exit=0;strict_clippy='NOT_RUN_BY_BACKEND; leader integration';platform_lib_tests='NOT_RUN_BY_BACKEND; leader owned';wholeworkspace='NOT_RUN';examples_build='NOT_RUN';Python_tests='NOT_RUN';model_GPU_audio_LAN='NOT_RUN';compiler_ICE_AV='NONE_OBSERVED';full_T011='IN_PROGRESS_NOT_APPROVED'};
 risk='Frozen new source needs independent preflight and separately authorized actual native diagnostic. Earlier author09 predicate UNKNOWN and later once WaitController false remain preserved failures; no new native PASS or Job0 claimed by these pure tests. Native synchronous calls can have OS scheduling latency; software absolute wait budget does not imply hard realtime guarantees.';
 failures_this_slice=@();source_bytes='SHA binds working file bytes; Git LF-normalized blob SHA may differ';public_evidence='Leader owns archival/ledger and independent review; author does not approve own implementation'
}
[IO.File]::WriteAllText($output,($result | ConvertTo-Json -Depth 18)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
Record '.local/t011-cleanup-deadline-author/delivery-index.json' | ConvertTo-Json
