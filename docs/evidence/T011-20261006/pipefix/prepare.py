import pathlib,json,hashlib,subprocess,shutil,sys
sys.stdout.reconfigure(encoding='utf-8');r=pathlib.Path(r'D:\Project\witvoice');out=r/'.local/t011-pipefix-root'
paths=['crates/platform/src/windows.rs','crates/engines/examples/supervised_warmup_probe.rs','crates/audio/examples/route_probe.rs','crates/audio/examples/support/route_drain.rs','crates/audio/tests/route_drain.rs']
p=subprocess.run([r'D:\Software\WitvoiceToolchain\Rust\cargo\bin\rustfmt.exe','--edition','2024']+paths,cwd=r,capture_output=True)
(out/'format.stdout').write_bytes(p.stdout);(out/'format.stderr').write_bytes(p.stderr);assert p.returncode==0
old=json.loads((r/'.local/t011-observation-root/frozen.json').read_text(encoding='utf-8'))
all_paths=list(old)+[p for p in paths if p not in old]
f={p:hashlib.sha256((r/p).read_bytes()).hexdigest() for p in all_paths}
(out/'frozen.json').write_text(json.dumps(f,indent=2)+'\n',encoding='utf-8')
s=(r/'.local/t011-observation-root/checks.ps1').read_text(encoding='utf-8-sig')
s=s.replace("@{name='04-fmt'", "@{name='03b-audio';argv=@('test','--locked','--offline','-p','witvoice-audio','--all-targets')},\n@{name='04-fmt'")
(out/'checks.ps1').write_text(s,encoding='utf-8')
print(json.dumps({'inputs':len(f),'format_exit':p.returncode,'frozen_sha':hashlib.sha256((out/'frozen.json').read_bytes()).hexdigest()},indent=2))
