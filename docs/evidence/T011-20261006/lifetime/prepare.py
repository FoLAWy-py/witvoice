import hashlib, json, pathlib, sys
sys.stdout.reconfigure(encoding='utf-8')
root = pathlib.Path(r'D:\Project\witvoice')
out = root / '.local/t011-lifetime-root'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
delivery = root / '.local/t011-lifetime-author/delivery-index.json'
d = json.loads(delivery.read_text(encoding='utf-8-sig'))
assert sha(delivery) == '05928f1ce6948c1ee0e2bd0c055de2a73723811c1217b889345050521f548588'
for x in d['sources']: assert sha(root / x['path']) == x['sha256']
old = json.loads((root / '.local/t011-member-once/config.json').read_text(encoding='utf-8-sig'))
paths = list(old['source_sha256'])
assert len(paths) == 121
frozen = {p: sha(root / p) for p in paths}
(out / 'frozen.json').write_text(json.dumps(frozen, indent=2)+'\n', encoding='utf-8')
checks = []
for label in ['09-platform-final', '10-observation-final', '11-lifecycle-final', '12-fmtcheck-final', '13-clippy-final', '14-examples-final']:
    m = json.loads((root / '.local/t011-lifetime-author' / (label+'.json')).read_text(encoding='utf-8-sig'))
    for stream in ['stdout', 'stderr']:
        x=m[stream]; p=pathlib.Path(x['path']); assert p.stat().st_size == x['bytes'] and sha(p)==x['sha256']
    assert m['exit_code']==0
    checks.append({'label':label,'argv':m['argv'],'exit':m['exit_code']})
print(json.dumps({'delivery_sha256':sha(delivery),'sources':d['sources'],'author_checks_bytes_verified':checks,'root_frozen_count':len(frozen)},indent=2))
