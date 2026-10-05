import datetime, hashlib, json, os, pathlib, subprocess, sys

ROOT = pathlib.Path('D:/Project/witvoice')
OUT = ROOT / '.local/t009-exact-mix-author'
CHANGED = ['crates/audio/src/wasapi.rs', 'crates/audio/src/stream/native.rs',
           'crates/audio/examples/format_support_probe.rs', 'crates/audio/examples/route_probe.rs',
           'crates/audio/README.md']
COMMANDS = {
 '01-format': ['rustfmt', '--edition', '2024', *CHANGED[:4]],
 '02-test': ['cargo', 'test', '--locked', '-p', 'witvoice-audio', '--all-targets'],
 '03-fmt': ['cargo', 'fmt', '--package', 'witvoice-audio', '--check'],
 '04-clippy': ['cargo', 'clippy', '--locked', '-p', 'witvoice-audio', '--all-targets', '--', '-D', 'warnings'],
 '05-build': ['cargo', 'build', '--locked', '-p', 'witvoice-audio', '--examples'],
}
def hashes():
 return {p: hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in CHANGED}
def utc():
 return datetime.datetime.now(datetime.timezone.utc).isoformat()
name = sys.argv[1]
argv = COMMANDS[name]
stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%f')
stem = OUT / (name+'-'+stamp)
env = dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0',
           CARGO_TARGET_DIR=str(ROOT/'.local/t009-author/target'))
quote = lambda s: "'"+s.replace("'", "''")+"'"
command = ". 'tools/dev/toolchain-env.ps1'; & "+' '.join(map(quote, argv))+"; exit $LASTEXITCODE"
meta = {'actor_uuid':'01a10767-80de-7e02-b3b0-73d1273a70bd', 'argv':argv,
        'executor':['C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe','-NoProfile','-Command',command],
        'env':{k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_TARGET_DIR']},
        'source_before':hashes(), 'started_utc':utc(), 'hardware':'NOT_RUN'}
with open(str(stem)+'.stdout.raw','wb') as stdout, open(str(stem)+'.stderr.raw','wb') as stderr:
 result = subprocess.run(meta['executor'], cwd=ROOT, env=env, stdout=stdout, stderr=stderr)
meta.update(finished_utc=utc(), exit_code=result.returncode, source_after=hashes())
for suffix in ['stdout','stderr']:
 raw = pathlib.Path(str(stem)+'.'+suffix+'.raw').read_bytes()
 meta[suffix] = {'path':str(stem)+'.'+suffix+'.raw','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
meta_path = str(stem)+'.json'
pathlib.Path(meta_path).write_text(json.dumps(meta,ensure_ascii=True,indent=2),encoding='utf8')
print(json.dumps({'metadata':meta_path,'exit_code':result.returncode,'stdout':meta['stdout'],'stderr':meta['stderr']}))
sys.exit(result.returncode)
