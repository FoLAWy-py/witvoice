import pathlib,json,datetime,shutil
r=pathlib.Path(r'D:\Project\witvoice');p=r/'docs/project/STATE.json';s=json.loads(p.read_text(encoding='utf-8-sig'))
assert s.get('audio_diagnostic_lease',{}).get('status')!='ACQUIRED';assert s.get('native_worker_test_lease',{}).get('status')!='ACQUIRED'
s['native_worker_test_lease']={'status':'ACQUIRED','owner':'/root','task':'T011','max_ms':20000,'scope':'single exact new closed EOF observation; fixed Pythonpeer, no Torch/model/GPU/audio/LAN','review':'docs/project/reviews/T011-observation-preflight.md','acquired_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};s['execution_status']='T011_CLOSED_EOF_OBSERVATION_ONCE_RUNNING';s['last_tested_commit']='39583276946ccd4ec99c0fcb8e30a9aac485472a'
p.write_text(json.dumps(s,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
out=r/'docs/evidence/T011-20261006/lifetime-observation';out.mkdir(parents=True,exist_ok=True)
for x in (r/'.local/t011-observation-root').iterdir():
    if x.is_file():shutil.copyfile(x,out/x.name)
a=out/'author';a.mkdir(exist_ok=True)
for x in (r/'.local/t011-lifetime-observation-author').iterdir():
    if x.is_file():shutil.copyfile(x,a/x.name)
shutil.copyfile(r/'docs/project/reviews/T011-observation-preflight.md',out/'preflight.md')
f=r/'docs/evidence/T009-20261006/ffmpeg-identity';f.mkdir(exist_ok=True)
for name in ['identity.cpp','build.cmd','build.stdout','build.stderr','build.json','identity.stderr','result.json']:
    shutil.copyfile(r/'.local/ffmpeg-identity'/name,f/name)
