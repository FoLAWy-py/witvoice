import hashlib,json,pathlib,re
root=pathlib.Path(r'D:\Project\witvoice')
proof=[]
for src,dst in [('.local/t011-native-once','docs/evidence/T011-20261005/native-interop'),('.local/t011-relink-final','docs/evidence/T011-20261005/native-integration/relink')]:
    target=root/dst;target.mkdir(parents=True,exist_ok=True)
    for p in sorted((root/src).iterdir()):
        if not p.is_file():continue
        data=p.read_bytes();q=target/p.name
        if q.exists() and q.read_bytes()!=data:raise RuntimeError('existing evidence differs')
        q.write_bytes(data)
        assert q.read_bytes()==data
        proof.append({'source':p.relative_to(root).as_posix(),'destination':q.relative_to(root).as_posix(),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
p=root/'.local/t011-pdb-archive/action.json'; q=root/'docs/evidence/T011-20261005/native-integration/relink/pdb-archive-action.json';q.write_bytes(p.read_bytes())
logs=root/'.local/t011-relink-final'
summary={'source_commit':'ff754fd988c69c2a089f8de74bf36fa97a2aa3c3','effective_tests':sum(map(int,re.findall(r'test result: ok\. (\d+) passed',(logs/'relink-workspace-tests.stdout').read_text()))),'commands':{},'original_failure_preserved':'docs/evidence/T011-20261005/native-integration/first-link-failure','cause':'UNKNOWN','model':'NOT_RUN','audio':'NOT_RUN','Mac':'NOT_RUN'}
for name in ['workspace-tests','fmt','clippy','examples']:
    m=json.loads((logs/f'relink-{name}.json').read_text()); assert m['exit']==0 and m['source_before_sha256']==m['source_after_sha256'] and len(m['source_before_sha256'])==101
    for stream in ['stdout','stderr']:
        d=(logs/f'relink-{name}.{stream}').read_bytes(); assert len(d)==m[stream]['bytes'] and hashlib.sha256(d).hexdigest()==m[stream]['sha256']
    summary['commands'][name]={'exit':m['exit'],'wall_seconds':m['wall_seconds'],'inputs':len(m['source_before_sha256'])}
for relative,obj in [('docs/evidence/T011-20261005/native-integration/relink/result.json',summary),('docs/evidence/T011-20261005/native-archive-proof.json',proof)]:
    (root/relative).write_text(json.dumps(obj,indent=2)+'\n',encoding='utf-8')
print(json.dumps(summary))