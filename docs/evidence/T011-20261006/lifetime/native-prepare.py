import json, pathlib, hashlib, shutil, re, subprocess, sys
sys.stdout.reconfigure(encoding='utf-8')
r=pathlib.Path(r'D:\Project\witvoice'); p=r/'.local/t011-lifetime-native-once';p.mkdir(exist_ok=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert not (p/'config.json').exists()
shutil.copyfile(r/'.local/t011-member-once/execute.ps1',p/'execute.ps1')
stderr=(r/'.local/t011-lifetime-root/02-observation.stderr').read_text()
binary=r/re.search(r'Running tests[\\/]worker_supervision\.rs \(([^\r\n]+\.exe)\)',stderr)[1]
assert binary.is_file() and 'target/debug/deps' in binary.as_posix()
cfg={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=r,text=True).strip(),'source_sha256':json.loads((r/'.local/t011-lifetime-root/frozen.json').read_text()),'binary':str(binary),'binary_sha256':sha(binary),'argv':['--nocapture','--test-threads=1'],'max_ms':60000,'automatic_retry':False,'model_GPU_audio_LAN':False,'only_fixed_testpeer':True,'purpose':'One bounded current-lifetime native suite, 8 native plus 11 pure cases; stop on failed suite; no model/endpoints','requires_independent_preflight':True,'supervisor_sha256':sha(p/'execute.ps1')}
(p/'config.json').write_text(json.dumps(cfg,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in cfg.items() if k!='source_sha256'},indent=2));print('source_count',len(cfg['source_sha256']),'config_sha256',sha(p/'config.json'))
