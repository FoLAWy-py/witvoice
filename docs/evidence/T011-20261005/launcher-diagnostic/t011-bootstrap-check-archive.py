import pathlib,json,hashlib
r=pathlib.Path(r'D:\Project\witvoice');dst=r/'docs/evidence/T011-20261005/warmup-once';dst.mkdir(exist_ok=True)
for name in ['execute.ps1','config.json','result.json','stdout.raw','stderr.raw']:
    p=r/'.local/t011-warmup-once'/name;q=dst/name;assert not q.exists();q.write_bytes(p.read_bytes())
b=r/'.local/t008-t016-r2-leader/target/debug/examples/worker_warmup_probe.exe'; data=b.read_bytes();assert hashlib.sha256(data).hexdigest()=='caf2901e2b895398396a01b9a45b1c283835425f2fdb1dbd91a3288fc7e3b101';(r/'.local/t011-warmup-once/executed-worker-warmup-probe.exe').write_bytes(data)
dst=r/'docs/evidence/T011-20261005/launcher-diagnostic';dst.mkdir(exist_ok=True)
for folder in ['.local/t011-warmup-import-diagnostic','.local/t011-bootstrap-check']:
    for p in (r/folder).iterdir():
        if p.is_file():
            q=dst/(p.parent.name+'-'+p.name);q.write_bytes(p.read_bytes())
print('First failed warmup immutable evidence and exact executed binary preserved; software PID diagnosis archived')