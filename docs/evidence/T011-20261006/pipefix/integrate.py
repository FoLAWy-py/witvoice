import pathlib,json,shutil,datetime,re,hashlib
r=pathlib.Path(r'D:\Project\witvoice');b=r/'.local/t011-pipefix-root';out=r/'docs/evidence/T011-20261006/pipefix';out.mkdir(parents=True,exist_ok=True)
for p in b.iterdir():
    if p.is_file():shutil.copyfile(p,out/p.name)
counts={}
for name in ['01-platform','02-observation','03-lifecycle','03b-audio']:
    counts[name]=sum(map(int,re.findall(r'test result: ok\. (\d+) passed',(b/(name+'.stdout')).read_text())))
assert sum(counts.values())==145
note='e1bd4b2 integrated strict IPC HRESULT_FROM_WIN32 error preservation + production WorkerSupervisor control-only real warmup diagnostic + explicit bounded capture drain. Root actual 145 pure tests (35+11+16+83), fmt/strictworkspaceClippy with process-tests/test-support/examples exit0, all126 pre/post/current bytes equal. Native/model/audio new once configurations frozen and NOT_RUN pending independent preflight. Old consumed failures remain failed. No VC or FFmpeg receipt. nextT011 native suite, then real production owner if suite passes; independent T009 finite drain once. No background promise.'
(out/'RESULTS.md').write_text(note+'\n',encoding='utf-8')
p=r/'docs/project/STATE.json';s=json.loads(p.read_text(encoding='utf-8-sig'))
s['last_tested_commit']='e1bd4b205a2ab75b9ca5353d9f9562235afdef00';s['last_tested_scope']=note;s['execution_status']='FROZEN_T011_NATIVE_AND_T009_DRAIN_PREFLIGHT';s['next_task']='T011'
s['active_tasks']=[];s['agent_runtime']['current_execution'].update(root='sole integrator',backend='STOP',audio_startup='STOP_DELIVERED',source_writers=0,reviewer_t001='READONLY_FINAL_PREFLIGHT')
s['agent_runtime']['current_execution']['stream_worker']={'uuid':'01a10dd9-54b7-76a2-a3af-608091142321','status':'STOP_READONLY_T012_PREPARATION','model_audio_runs':False,'changes':False}
s['latest_integrated_software']={'source_commit':s['last_tested_commit'],'pure_tests':145,'counts':counts,'checks':'ALL7_EXIT0','inputs':126,'equal':True,'evidence':'docs/evidence/T011-20261006/pipefix/','native_model_audio':'NOT_RUN'}
p.write_text(json.dumps(s,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
p=r/'docs/project/TASKS.json';t=json.loads(p.read_text(encoding='utf-8-sig'))
for task in t['tasks']:
    if task['id'] in ['T009','T011']:
        task['execution_note']=note;ep='docs/evidence/T011-20261006/pipefix/RESULTS.md'
        if ep not in task['evidence']:task['evidence'].append(ep)
p.write_text(json.dumps(t,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
p=r/'docs/project/HANDOFF.md';p.write_text(note+'\n\n'+p.read_text(encoding='utf-8-sig'),encoding='utf-8')
print(json.dumps({'counts':counts,'total':sum(counts.values()),'source_writers':0,'GPU_audio_LAN':0,'new_once':'NOT_RUN','next_task':'T011'},indent=2))
