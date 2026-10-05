import pathlib,json,hashlib,shutil,datetime,sys
sys.stdout.reconfigure(encoding='utf-8')
r=pathlib.Path(r'D:\Project\witvoice');out=r/'docs/evidence/T011-20261006/lifetime';out.mkdir(parents=True,exist_ok=True)
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
f=json.loads((r/'.local/t011-lifetime-root/frozen.json').read_text())
assert all(sha(r/p)==h for p,h in f.items())
for x in (r/'.local/t011-lifetime-root').iterdir():
    if x.is_file() and x.suffix in ['.ps1','.py','.json','.stdout','.stderr']:shutil.copyfile(x,out/x.name)
author=r/'.local/t011-lifetime-author';a=out/'author';a.mkdir(exist_ok=True)
for x in author.iterdir():
    if x.is_file():shutil.copyfile(x,a/x.name)
now=datetime.datetime.now(datetime.timezone.utc).isoformat()
s=json.loads((r/'docs/project/STATE.json').read_text(encoding='utf-8-sig'))
s['last_tested_commit']='2c15bd04bafbe7bacd61d5b5c7f74f409d761cea'
s['execution_status']='T011_LIFETIME_ROOT_CHECKS_PASSED_NATIVE_PREFLIGHT'
s['active_tasks']=[{'task_id':'T011','agent':'/root','mode':'INTEGRATION_NATIVE_PREFLIGHT','path_whitelist':['crates/platform/src/process.rs','crates/engines/src/supervisor.rs','.local/t011-lifetime-root/','.local/t011-lifetime-native-once/','docs/project/','docs/evidence/T011-20261006/lifetime/'],'dependencies':['T002','T006','T010'],'contract':'docs/project/T011-lifetime-ledger-assignment.md','acceptance_commands':'01-platform/02-observation/03-lifecycle/04-fmt/05-clippy/06-examples; then independently-approved 19-case fixed peer suite','resources':['sole root source writer','no GPU/audio/LAN']}]
s['lifetime_root_checks']={'commit':s['last_tested_commit'],'pure_tests':56,'all_six_exit':0,'source_count':121,'source_maps_equal':True,'evidence':out.relative_to(r).as_posix(),'native':'NOT_RUN','real_supervisor_model':'NOT_RUN','third_party':'NOT_RUN'}
s['agent_runtime']['current_execution']={'root':'sole integrator','backend':'STOP','reviewer_t001':'readonly native preflight','audio_startup':{'uuid':'01a10db4-bb4d-7a83-bcc9-c7e06ad82d8f','status':'STOP_READONLY_DELIVERED','mode':'native audio_runtime role dispatched; custom TOML loading not independently proven'},'source_writers':1,'GPU_audio_LAN_leases':0}
(r/'docs/project/STATE.json').write_text(json.dumps(s,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
t=json.loads((r/'docs/project/TASKS.json').read_text(encoding='utf-8-sig'));task=next(x for x in t['tasks'] if x['id']=='T011');task['status']='IN_PROGRESS';task['execution_note']='2c15bd0 root lifetime ledger integration: 56pure/fmtall/strictworkspaceClippy/examples exit0/121frozen equal. Eight native/newowner real model NOT_RUN; independent preflight pending, not T011 DONE.'
for p in ['docs/evidence/T011-20261006/lifetime/frozen.json','docs/evidence/T011-20261006/lifetime/author/delivery-index.json']:
    if p not in task['evidence']:task['evidence'].append(p)
(r/'docs/project/TASKS.json').write_text(json.dumps(t,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
h=r/'docs/project/HANDOFF.md';h.write_text('当前整合2c15bd0：T011生命周期账本root56纯测试/fmtall/严格workspaceClippy/examples均0，121源码前后相等；八native一次独立预检中，真实新owner模型/流式VC/FFmpeg接收未执行。T009 RAW Initialize/Start成功但首capture discontinuity失败；下一最小软件为gate未arm的有界capture预滚动，marker阶段仍零容忍。LAN/UI后置，next_task=T011。历史保留。\n\n'+h.read_text(encoding='utf-8-sig'),encoding='utf-8')
b=r/'docs/project/BLOCKERS.md';b.write_text('当前B005：RAW同一VB-CABLE两端Initialize/Start成功，容量各1056帧，但首capture discontinuity使该次FAILED_MUTED；三项close均确认。default/exactmix旧0x887c001a原因UNKNOWN仍保留。下一软件动作是显式准备期/gate closed预滚动，不将原失败改PASS；真实VC/第三方尚未执行。T011新生命周期账本root56pure及checks0，原生八项和新owner模型尚待。\n\n'+b.read_text(encoding='utf-8-sig'),encoding='utf-8')
p=r/'.local/t011-lifetime-native-once';cfg=json.loads((p/'config.json').read_text());assert cfg['source_sha256']==f and not(p/'consumed.json').exists()
assert sha(p/'execute.ps1')==cfg['supervisor_sha256'];assert sha(pathlib.Path(cfg['binary']))==cfg['binary_sha256']
print(json.dumps({'utc':now,'frozen_map_equal_121':True,'config_sha256':sha(p/'config.json'),'binary_verified':cfg['binary_sha256'],'wrapper_verified':cfg['supervisor_sha256'],'unconsumed':True,'config':cfg},indent=2))
