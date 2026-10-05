import pathlib,json,hashlib,subprocess,re,shutil,sys
sys.stdout.reconfigure(encoding='utf-8');r=pathlib.Path(r'D:\Project\witvoice');out=r/'.local/t011-observation-native-once';out.mkdir(exist_ok=True)
assert not(out/'config.json').exists()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
f=json.loads((r/'.local/t011-observation-root/frozen.json').read_text());assert all(sha(r/p)==h for p,h in f.items())
shutil.copyfile(r/'.local/t011-member-once/execute.ps1',out/'execute.ps1')
raw=(r/'.local/t011-observation-root/02-observation.stderr').read_text();binary=r/re.search(r'Running tests[\\/]worker_supervision\.rs \(([^\r\n]+\.exe)\)',raw)[1];assert binary.is_file()
cfg={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=r,text=True).strip(),'source_sha256':f,'binary':str(binary),'binary_sha256':sha(binary),'argv':['--exact','eof_partial_frame_heartbeat_timeout_and_memory_pressure_retire_actual_job','--nocapture','--test-threads=1'],'max_ms':20000,'automatic_retry':False,'model_GPU_audio_LAN':False,'only_fixed_testpeer':True,'purpose':'One new closed scalar EOF observation case with unchanged release predicates/assertions; cannot retry unchanged failures','requires_independent_preflight':True,'supervisor_sha256':sha(out/'execute.ps1')}
(out/'config.json').write_text(json.dumps(cfg,indent=2)+'\n',encoding='utf-8');print(json.dumps({'config_sha256':sha(out/'config.json'),'unconsumed':not(out/'consumed.json').exists(),'source_count':len(f),'config':cfg},indent=2))
