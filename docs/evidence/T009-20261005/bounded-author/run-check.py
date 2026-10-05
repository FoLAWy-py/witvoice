import datetime
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(r'D:\Project\witvoice')
OUT = ROOT / '.local/t009-bounded-adaptation'
OUT.mkdir(parents=True, exist_ok=True)
OVERRIDES = ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTDOC',
             'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTFLAGS',
             'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER',
             'RUSTUP_TOOLCHAIN', 'CARGO_BUILD_RUSTC', 'CARGO_BUILD_RUSTDOC',
             'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS']
env = os.environ.copy()
present = [name for name in OVERRIDES if env.get(name)]
if present:
    raise SystemExit('Unexpected overrides: ' + ','.join(present))
env.update(CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0',
           CARGO_TARGET_DIR=str(ROOT / '.local/t009-author/target'))

def sha(data):
    return hashlib.sha256(data).hexdigest()

def sources():
    paths = [*(ROOT / 'crates/audio').glob('src/**/*.rs'),
             *(ROOT / 'crates/audio').glob('tests/**/*.rs'),
             *(ROOT / 'crates/audio').glob('examples/**/*.rs'),
             ROOT / 'crates/audio/README.md']
    return {str(path.relative_to(ROOT)).replace('\\', '/'): sha(path.read_bytes())
            for path in sorted(paths)}

label = sys.argv[1]
executable = pathlib.Path(env['CARGO_HOME']) / 'bin' / (
    'rustfmt.exe' if label == 'format-source' else 'cargo.exe')
args = sys.argv[2:]
stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%f')
stem = OUT / (label + '-' + stamp)
before = sources()
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
wall = time.perf_counter()
run = subprocess.run([str(executable), *args], cwd=ROOT, env=env, capture_output=True)
seconds = time.perf_counter() - wall
finished = datetime.datetime.now(datetime.timezone.utc).isoformat()
logs = {}
for kind in ['stdout', 'stderr']:
    data = getattr(run, kind)
    path = pathlib.Path(str(stem) + '.' + kind + '.txt')
    with path.open('xb') as file:
        file.write(data)
    logs[kind] = {'path': str(path.relative_to(ROOT)).replace('\\', '/'),
                  'bytes': len(data), 'sha256': sha(data)}
meta = {'task': 'T009', 'slice': 'bounded 1440-capacity/960-commit adaptation',
        'owner': '/root/audio_runtime',
        'agent_uuid': '01a10767-80de-7e02-b3b0-73d1273a70bd',
        'executable': str(executable), 'argv': args, 'cwd': str(ROOT),
        'started_utc': started, 'finished_utc': finished, 'wall_seconds': seconds,
        'exit': run.returncode,
        'environment': {key: env.get(key) for key in [
            'CARGO_HOME', 'RUSTUP_HOME', 'CARGO_TARGET_DIR',
            'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL']},
        'audited_override_names': OVERRIDES, 'present_overrides': present,
        'source_before_sha256': before, 'source_after_sha256': sources(),
        'logs': logs, 'hardware': 'NOT_RUN; all once permissions consumed'}
path = pathlib.Path(str(stem) + '.json')
with path.open('x', encoding='utf-8', newline='\n') as file:
    json.dump(meta, file, ensure_ascii=True, indent=2)
print(json.dumps({'metadata': str(path.relative_to(ROOT)).replace('\\', '/'),
                  'exit': run.returncode,
                  'log_bytes': {key: value['bytes'] for key, value in logs.items()},
                  'metadata_sha256': sha(path.read_bytes())}))
sys.exit(0 if run.returncode == 0 else 1)
