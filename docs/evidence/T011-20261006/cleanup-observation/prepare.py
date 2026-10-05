from pathlib import Path
import hashlib
import json
import subprocess
root=Path('D:/Project/witvoice');base=root/'.local/t011-cleanup-r2-root';old=root/'.local/t011-supervisor-root'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
paths=list(json.loads((old/'frozen-inputs.json').read_text(encoding='utf-8'))['source_sha256'])
assert len(paths)==121
f={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'scope':'T011 closed cleanup observation only; native8/model STOPPED','source_sha256':{p:sha(root/p) for p in paths}}
assert not (base/'frozen-inputs.json').exists()
(base/'frozen-inputs.json').write_text(json.dumps(f,indent=2)+'\n',encoding='utf-8')
rust=(old/'verify.ps1').read_text(encoding='utf-8')
rust=rust.replace("@{name='workspace-tests';argv=@('test','--locked','--workspace','--all-targets','--features','witvoice-node/process-tests,witvoice-session/test-support','--verbose','--','--nocapture')},", "@{name='platform-error-tests';argv=@('test','--locked','--offline','-p','witvoice-platform','--lib','native_error_tests','--','--nocapture')},\n    @{name='cleanup-observation-tests';argv=@('test','--locked','--offline','-p','witvoice-engines','--test','worker_supervision','cleanup_observation','--','--nocapture')},")
rust=rust.replace("$Checks=@('workspace-tests','fmt','clippy','examples')", "$Checks=@('platform-error-tests','cleanup-observation-tests','fmt','clippy','examples')")
rust=rust.replace('One frozen T011 native supervisor integration; trusted isolated Python testpeer only; no model/GPU/audio/LAN; stop on failure.', 'One frozen T011 cleanup observation software integration; no native peer/model/GPU/audio/LAN; stop on failure.')
(base/'verify.ps1').write_text(rust,encoding='utf-8')
python=(old/'python-verify.ps1').read_text(encoding='utf-8').replace('python-tests-supervisor','python-tests-cleanup').replace('compile-supervisor','compile-cleanup')
(base/'python-verify.ps1').write_text(python,encoding='utf-8')
print(json.dumps({'source_commit':f['source_commit'],'source_count':121,'manifest_sha256':sha(base/'frozen-inputs.json'),'lock_sha256':sha(root/'Cargo.lock')}))
