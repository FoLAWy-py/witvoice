import pathlib,json,hashlib,subprocess,re,shutil,uuid,sys
sys.stdout.reconfigure(encoding='utf-8');r=pathlib.Path(r'D:\Project\witvoice');b=r/'.local/t011-pipefix-root'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
f=json.loads((b/'frozen.json').read_text());assert all(sha(r/p)==h for p,h in f.items())
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=r,text=True).strip()
for name in ['01-platform','02-observation','03-lifecycle','03b-audio','04-fmt','05-clippy','06-examples']:
    m=json.loads((b/(name+'.json')).read_text(encoding='utf-8-sig'));assert m['exit']==0 and m['source_before']==m['source_after']==f
    assert m['stdout_sha256']==sha(b/(name+'.stdout')) and m['stderr_sha256']==sha(b/(name+'.stderr'))
for stem,real in [('t011-pipefix-native-once',False),('t011-production-owner-once',True)]:
    out=r/'.local'/stem;assert not out.exists();out.mkdir()
    wrapper=(r/'.local/t011-observation-native-once/execute.ps1').read_text(encoding='utf-8-sig')
    if real:
        wrapper=wrapper.replace("model_GPU_audio_LAN='NOT_RUN'","model_GPU='FIXED_REAL_MEANVC2_CUDA_WARMUP';audio_LAN='NOT_STARTED'").replace('single native testpeer cleanup stage observation only; no T011DONE or VC claim','fixed real production owner lifecycle warmup only; no VC or third-party receipt claim')
        binary=r/'.local/t008-t016-r2-leader/target/debug/examples/supervised_warmup_probe.exe';argv=['--approve-fixed-model-warmup'];limit=134000
    else:
        raw=(b/'02-observation.stderr').read_text();binary=r/re.search(r'Running tests[\\/]worker_supervision\.rs \(([^\r\n]+\.exe)\)',raw)[1];argv=['--nocapture','--test-threads=1'];limit=30000
    assert binary.is_file();(out/'execute.ps1').write_text(wrapper,encoding='utf-8')
    cfg={'source_commit':commit,'source_sha256':f,'binary':str(binary),'binary_sha256':sha(binary),'argv':argv,'max_ms':limit,'automatic_retry':False,'model_GPU_audio_LAN':real,'only_fixed_testpeer':not real,'purpose':'One new Win32-preserving source integration suite' if not real else 'One fixed model production owner warmup/Ready hold/Stop cleanup; host gate8GiB, fixed assets; no media or device','requires_independent_preflight':True,'supervisor_sha256':sha(out/'execute.ps1')}
    (out/'config.json').write_text(json.dumps(cfg,indent=2)+'\n',encoding='utf-8')
    print(stem,json.dumps({'config':sha(out/'config.json'),'binary':sha(binary),'wrapper':sha(out/'execute.ps1'),'unconsumed':not(out/'consumed.json').exists(),'inputs':len(f)},indent=2))
out=r/'.local/t009-drained-once';assert not out.exists();out.mkdir()
shutil.copyfile(r/'.local/t009-primed-once/selection-private.json',out/'selection-private.json')
shutil.copyfile(r/'.local/t008-t016-r2-leader/target/debug/examples/route_probe.exe',out/'route_probe.exe')
wrapper=(r/'.local/t009-primed-once/supervise_once.ps1').read_text(encoding='utf-8-sig').replace('t009-primed-once','t009-drained-once').replace('--approve-primed-raw-capture-mix-probe','--approve-drained-primed-raw-capture-mix-probe').replace('CAPTURE_RAW_EXACT_MIX_PRIMED_RENDER_BASIC','CAPTURE_RAW_EXACT_MIX_PRIMED_DRAINED_RENDER_BASIC')
wrapper=wrapper.replace('$r.run_ok -eq $true -and', '$r.capture_drain.maximum_packets_per_pass -le 4 -and $r.capture_drain.maximum_frames_per_pass -le 5760 -and $r.run_ok -eq $true -and')
(out/'supervise_once.ps1').write_text(wrapper,encoding='utf-8');aid='T009-DRAINED-'+str(uuid.uuid4())
cfg={'authorization_id':aid,'source_commit':commit,'source_dirty_audio':False,'executable':str(out/'route_probe.exe'),'executable_sha256':sha(out/'route_probe.exe'),'scope':str(out/'selection-private.json'),'scope_sha256':sha(out/'selection-private.json'),'supervisor_sha256':sha(out/'supervise_once.ps1'),'source_sha256':f}
(out/'config.json').write_text(json.dumps(cfg,indent=2)+'\n',encoding='utf-8')
(out/'once.json').write_text(json.dumps({'authorization_id':aid,'status':'AUTHORIZED_NOT_EXECUTED','consumed_utc':None,'human_authorization':'standing equivalent finite same-pair synthetic VB-CABLE authorization','scope':'new bounded drain source; capture-only unarmed gate prep<=100ms/governed markercombinedactive<=2s/.01/1440/commit960/overall15/no retry/physical/PCMdisk/upload/default/driver/security changes'},indent=2)+'\n',encoding='utf-8')
print('t009-drained-once',json.dumps({'config':sha(out/'config.json'),'binary':sha(out/'route_probe.exe'),'wrapper':sha(out/'supervise_once.ps1'),'scope':sha(out/'selection-private.json'),'authorization':aid,'inputs':len(f),'unconsumed':True},indent=2))
