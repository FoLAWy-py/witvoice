import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[2]
dest = Path(__file__).resolve().parent
frozen = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
assert frozen.startswith('6c68c5d'), frozen
sha = lambda b: hashlib.sha256(b).hexdigest()
audio = json.loads((root / 'docs/evidence/T008-20261005/r1-fix-author/06-delivery-20261005T092004208631.json').read_text(encoding='utf-8'))['all_audio_source_sha256']
paths = {root / p for p in audio}
for directory in ['crates', 'apps', 'tests']:
    if (root / directory).exists():
        paths.update(p for p in (root / directory).rglob('*') if p.is_file() and p.suffix in ('.rs', '.toml'))
paths.update(root / p for p in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'] if (root / p).exists())
def snapshot():
    return {p.relative_to(root).as_posix(): sha(p.read_bytes()) for p in sorted(paths)}
initial = snapshot()
assert all(initial[p] == h for p, h in audio.items())
overrides = ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTDOC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER', 'RUSTUP_TOOLCHAIN', 'CARGO_BUILD_RUSTC', 'CARGO_BUILD_RUSTDOC', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS']
present = {name: os.environ[name] for name in overrides if name in os.environ}
assert not present, list(present)
env = os.environ.copy()
env.update(CARGO_HOME=r'D:\Software\WitvoiceToolchain\Rust\cargo', RUSTUP_HOME=r'D:\Software\WitvoiceToolchain\Rust\rustup', CARGO_TARGET_DIR=str(dest / 'target'), CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0')
env['PATH'] = r'D:\Software\WitvoiceToolchain\Rust\cargo\bin;' + env['PATH']
executable = r'D:\Software\WitvoiceToolchain\Rust\cargo\bin\cargo.exe'
commands = [
    ('01-workspace-tests', ['test', '--locked', '--workspace', '--all-targets', '--features', 'witvoice-node/process-tests', '--', '--nocapture']),
    ('02-fmt', ['fmt', '--all', '--check']),
    ('03-clippy', ['clippy', '--locked', '--workspace', '--all-targets', '--features', 'witvoice-node/process-tests', '--', '-D', 'warnings']),
    ('04-examples', ['build', '--locked', '--workspace', '--examples']),
]
records = []
for stem, argv in commands:
    before = snapshot()
    assert before == initial
    start = datetime.datetime.now(datetime.timezone.utc).isoformat()
    out = dest / (stem + '.stdout.txt')
    err = dest / (stem + '.stderr.txt')
    assert not out.exists() and not err.exists()
    with out.open('wb') as stdout, err.open('wb') as stderr:
        process = subprocess.run([executable] + argv, cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
    end = datetime.datetime.now(datetime.timezone.utc).isoformat()
    after = snapshot()
    record = {'auditor': '/root', 'frozen_commit': frozen, 'executable': executable, 'argv': argv, 'cwd': str(root), 'start_utc': start, 'end_utc': end, 'exit_code': process.returncode, 'source_before': before, 'source_after': after, 'source_equal': before == after == initial, 'environment': {name: env[name] for name in ['CARGO_HOME', 'RUSTUP_HOME', 'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL']}, 'audited_override_names': overrides, 'present_overrides': present, 'raw': {channel: {'file': p.name, 'bytes': p.stat().st_size, 'sha256': sha(p.read_bytes())} for channel, p in [('stdout', out), ('stderr', err)]}}
    if stem == '01-workspace-tests':
        text = out.read_text(encoding='utf-8')
        counts = [int(x) for x in re.findall(r'test result: ok\. (\d+) passed;', text)]
        record['test_partitions'] = counts
        record['effective_tests'] = sum(counts)
    (dest / (stem + '.json')).write_text(json.dumps(record, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    records.append({'metadata': stem + '.json', 'exit_code': process.returncode, 'source_equal': record['source_equal'], 'effective_tests': record.get('effective_tests')})
    print(json.dumps(records[-1]), flush=True)
    if process.returncode or not record['source_equal']:
        (dest / 'result.json').write_text(json.dumps({'status': 'FAILED', 'frozen_commit': frozen, 'runs': records, 'hardware_network_model_mac': 'NOT_RUN'}, indent=2) + '\n', encoding='utf-8')
        sys.exit(process.returncode or 1)
assert records[0]['effective_tests'] == 132, records[0]
(dest / 'result.json').write_text(json.dumps({'status': 'PASS_SOURCE_CHECKS_ONLY', 'frozen_commit': frozen, 'effective_tests': records[0]['effective_tests'], 'runs': records, 'audio_tests_subset': 54, 'platform_transport_tests_subset': 48, 'hardware_network_model_mac': 'NOT_RUN', 'formal_review': 'PENDING'}, indent=2) + '\n', encoding='utf-8')
