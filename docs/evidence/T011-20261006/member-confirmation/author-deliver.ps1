$ErrorActionPreference='Stop'
Set-Location -LiteralPath 'D:\Project\witvoice'
$dir='D:\Project\witvoice\.local\t011-member-author'
$output=Join-Path $dir 'delivery-index.json'
if(Test-Path -LiteralPath $output){throw 'Unique delivery file already exists'}
function Record([string]$path){
 $file=Join-Path 'D:\Project\witvoice' $path
 [ordered]@{path=$path;bytes=(Get-Item -LiteralPath $file).Length;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$checks=@();$artifacts=@()
foreach($label in @('01-fmt','02-observation-tests','03-fmtcheck','04-fmt','05-observation-tests','06-fmtcheck')){
 $meta=Get-Content -LiteralPath (Join-Path $dir ($label+'.json')) -Raw | ConvertFrom-Json
 foreach($stream in @($meta.stdout,$meta.stderr)){
  if((Get-FileHash -LiteralPath $stream.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $stream.sha256){throw 'Raw stream SHA mismatch'}
 }
 $checks+=$meta
 foreach($extension in @('json','stdout','stderr')){$artifacts+=Record ('.local/t011-member-author/'+$label+'.'+$extension)}
}
$sources=@(Record 'crates/engines/src/supervisor.rs';Record 'crates/engines/tests/worker_supervision.rs')
foreach($source in $sources){foreach($check in @($checks[4],$checks[5])){
 $before=@($check.source_before | Where-Object {$_.path -eq $source.path})[0]
 $after=@($check.source_after | Where-Object {$_.path -eq $source.path})[0]
 if($before.sha256 -ne $source.sha256 -or $after.sha256 -ne $source.sha256){throw 'Final source not covered'}
}}
$result=[ordered]@{
 task='T011-minimal-exact-job-members-confirmation';actual_agent='/root/backend';actual_agent_uuid='01a10767-4ff5-7f21-881a-146d094fbc55';created_utc=[DateTime]::UtcNow.ToString('o');status='STOP_WRITES_STOP_CARGO';host=$PSVersionTable.PSVersion.ToString();
 source_before_edit=@(@{path='crates/engines/src/supervisor.rs';sha256='65082543ac42d3d18d03cdd60f384ec2d7c58c011cfc6c341ebeb255bd1c4f36'},@{path='crates/engines/tests/worker_supervision.rs';sha256='42ff54c4ebd9156865459198ab869c575772fc14f36fb1825b3dc262c21d263c'});
 sources=$sources;checks=$checks;artifacts=$artifacts;runner=Record '.local/t011-member-author/check.ps1';delivery_script=Record '.local/t011-member-author/deliver.ps1';unchanged_peer=Record 'tests/integration/worker_supervision_peer.py';
 implementation='Only cleanup termination switched to platform strict terminate_all_members_until using original shared3s deadline. cfg(test) member accessor captures retained exact-ownerJob handles via platform API; native tests no longer reopen PIDs with Process::observe. Reclaimed logs each heldPID and actual zero-wait Result before original unchanged success assertion, with no extra wait/sleep or relaxed predicates. Leader confirmed obsolete owned_process_ids had no callers and authorized removal to resolve actual same-source integration dead_code warning.';
 coverage='Final two source hashes are exactly bound by05 pure11test and06fmtcheck before/after. Existing11 observation cases retained; native8 filtered. New platform member-predicate tests and strictClippy are leader owned, not executed by backend.';
 results=[ordered]@{final_pure_tests_exit=0;final_pure_tests_passed=11;final_pure_tests_failed=0;native_tests_filtered_not_run=8;fmt_check_exit=0;strict_clippy='NOT_RUN_BY_BACKEND';platform_member_tests='NOT_RUN_BY_BACKEND';wholeworkspace='NOT_RUN';examples_build='NOT_RUN';Python_tests='NOT_RUN';model_GPU_audio_LAN='NOT_RUN';compiler_ICE_AV='NONE_OBSERVED';full_T011='IN_PROGRESS_NOT_APPROVED'};
 retained_warning=[ordered]@{command_label='02-observation-tests';exit=0;ordinary_warning='unused public owned_process_ids in integration same-source module';Clippy_run=$false;repair='Leader actual rg caller check033796 followed by approval; obsolete method removed,04format/05puretests/06format on final source';final05warning='NONE_IN_COMPLETE_RAW'};
 read_nonzero=[ordered]@{tool_chunk='75b2bf';exit=1;reason='rg new platform APIs had no matches while leader implementation pending';application_failure=$false;compiler_invoked=$false};
 risk='Earlier dad441b Stop/Released but retained member waitZERO None failure remains exact member UNKNOWN, not changed to PASS. New strict capture may reject historical members already exited or changed member history; retain failclosed refusal. No actual native member cleanup executed here, and no whole-task release/ready/VC/Mac proof.';
 source_bytes='SHA binds working file bytes, not asserted equal to Git LF normalization';public_evidence='Leader owns archive/ledger and independent approval'
}
[IO.File]::WriteAllText($output,($result | ConvertTo-Json -Depth 18)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
Record '.local/t011-member-author/delivery-index.json' | ConvertTo-Json
