from pathlib import Path
import hashlib
import json
import re
root=Path('D:/Project/witvoice');base=root/'.local/t011-member-root'
once=root/'.local/t011-member-once'
assert not once.exists(), 'One fresh fixed diagnostic only'
f=json.loads((base/'frozen-inputs.json').read_text(encoding='utf-8'))
raw=(base/'integration-cleanup-observation-tests.stderr').read_text(encoding='utf-8')
matches=re.findall(r'Running tests[\\/]worker_supervision\.rs \(([^\r\n]+\.exe)\)',raw)
assert len(matches)==1
binary=(root/matches[0]).resolve()
assert binary.parent == (root/'.local/t008-t016-r2-leader/target/debug/deps').resolve()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert binary.is_file() and all(sha(root/p)==v for p,v in f['source_sha256'].items())
once.mkdir()
cfg={'source_commit':f['source_commit'],'source_sha256':f['source_sha256'],
     'binary':str(binary),'binary_sha256':sha(binary),
     'argv':['--exact','authenticated_dual_channels_ready_stop_ack_and_actual_tree_zero','--nocapture','--test-threads=1'],
     'max_ms':15000,'automatic_retry':False,'model_GPU_audio_LAN':False,
     'only_fixed_testpeer':True,'purpose':'One new stage/scalar diagnostic after independent frozen preflight, not repeat unchanged source',
     'requires_independent_preflight':True}
(once/'config.json').write_text(json.dumps(cfg,indent=2,ensure_ascii=True)+'\n',encoding='ascii')
script=(base/'execute-native.ps1').read_bytes()
(once/'execute.ps1').write_bytes(script)
cfg['supervisor_sha256']=sha(once/'execute.ps1')
(once/'config.json').write_text(json.dumps(cfg,indent=2,ensure_ascii=True)+'\n',encoding='ascii')
print(json.dumps({'config_sha256':sha(once/'config.json'),'supervisor_sha256':cfg['supervisor_sha256'],'binary_sha256':cfg['binary_sha256'],'source_commit':f['source_commit']}))
