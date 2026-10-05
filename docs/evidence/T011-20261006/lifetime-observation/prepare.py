import pathlib,json,hashlib,subprocess,sys,shutil
sys.stdout.reconfigure(encoding='utf-8')
r=pathlib.Path(r'D:\Project\witvoice');out=r/'.local/t011-observation-root'
author=r/'.local/t011-lifetime-observation-author/delivery-index.json'
assert hashlib.sha256(author.read_bytes()).hexdigest()=='b52cbeeed84fcd55a6a2fcef87c1fe89d9a97276d2e55c66e6309f080e2c4bde'
d=json.loads(author.read_text(encoding='utf-8-sig'));print('AUTHOR_INDEX_SHA_VERIFIED', list(d))
cmd=[r'D:\Software\WitvoiceToolchain\Rust\cargo\bin\rustfmt.exe','--edition','2024','crates/platform/src/process.rs','crates/platform/src/lib.rs','crates/engines/src/supervisor.rs','crates/engines/tests/worker_supervision.rs']
p=subprocess.run(cmd,cwd=r,capture_output=True);(out/'format.stdout').write_bytes(p.stdout);(out/'format.stderr').write_bytes(p.stderr);print('FORMAT_EXIT',p.returncode);assert p.returncode==0
old=json.loads((r/'.local/t011-lifetime-root/frozen.json').read_text())
paths=list(old)+['crates/audio/examples/support/route_startup.rs','crates/audio/tests/route_startup.rs']
f={p:hashlib.sha256((r/p).read_bytes()).hexdigest() for p in paths};(out/'frozen.json').write_text(json.dumps(f,indent=2)+'\n',encoding='utf-8')
shutil.copyfile(r/'.local/t011-lifetime-root/checks.ps1',out/'checks.ps1')
print('FROZEN_CURRENT_INPUTS',len(f))
