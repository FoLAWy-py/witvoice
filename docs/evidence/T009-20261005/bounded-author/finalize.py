import datetime
import hashlib
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(r'D:\Project\witvoice')
OUT = ROOT / '.local/t009-bounded-adaptation'
GIT = r'C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\native\git\cmd\git.exe'
BASE = '75473156a14cd9b1fe0f26b4f56bde5de80ee520'
CHANGED = ['crates/audio/src/stream/native.rs', 'crates/audio/src/stream/raw_tests.rs',
           'crates/audio/examples/route_probe.rs', 'crates/audio/README.md']

def sha(data):
    return hashlib.sha256(data).hexdigest()

paths = [*(ROOT / 'crates/audio').glob('src/**/*.rs'),
         *(ROOT / 'crates/audio').glob('tests/**/*.rs'),
         *(ROOT / 'crates/audio').glob('examples/**/*.rs'), ROOT / 'crates/audio/README.md']
sources = {str(path.relative_to(ROOT)).replace('\\', '/'): sha(path.read_bytes())
           for path in sorted(paths)}
commands = []
for path in OUT.glob('*.json'):
    if not path.name.startswith(('format-source-', 'test-', 'fmt-check-', 'clippy-', 'build-')):
        continue
    meta = json.loads(path.read_bytes())
    for raw in meta['logs'].values():
        data = (ROOT / raw['path']).read_bytes()
        assert len(data) == raw['bytes'] and sha(data) == raw['sha256']
    commands.append({'metadata_path': str(path.relative_to(ROOT)).replace('\\', '/'),
                     'metadata_raw_sha256': sha(path.read_bytes()), **meta})
commands.sort(key=lambda command: command['started_utc'])
final = []
for label in ['test-', 'fmt-check-', 'clippy-', 'build-']:
    matching = [command for command in commands
                if pathlib.Path(command['metadata_path']).name.startswith(label)]
    command = matching[-1]
    assert command['exit'] == 0
    assert command['source_before_sha256'] == sources
    assert command['source_after_sha256'] == sources
    final.append(command['metadata_path'])
test = next(command for command in commands if command['metadata_path'] == final[0])
stdout = (ROOT / test['logs']['stdout']['path']).read_text(encoding='utf-8')
counts = [int(count) for count in re.findall(r'test result: ok\. (\d+) passed;', stdout)]
assert sum(counts) == 67
assert '0 failed' in stdout
protected = [path for path in sources if path not in CHANGED]
unchanged = subprocess.run([GIT, 'diff', '--exit-code', BASE, '--', *protected],
                           cwd=ROOT, capture_output=True)
assert unchanged.returncode == 0 and not unchanged.stdout and not unchanged.stderr
diff = subprocess.run([GIT, 'diff', '--check', '--', *CHANGED], cwd=ROOT, capture_output=True)
assert diff.returncode == 0 and not diff.stdout and not diff.stderr
binary = ROOT / '.local/t009-author/target/debug/examples/route_probe.exe'
binary_data = binary.read_bytes()
index = {'task': 'T009', 'slice': '1440 negotiated capacity / 960 single governed commit',
         'owner': '/root/audio_runtime', 'agent_uuid': '01a10767-80de-7e02-b3b0-73d1273a70bd',
         'status': 'AUTHOR_DELIVERED_STOP_WRITES', 'baseline_audio_source': BASE,
         'changed_source_sha256': {path: sources[path] for path in CHANGED},
         'all_audio_source_sha256': sources, 'source_count': len(sources),
         'commands': commands, 'command_count': len(commands),
         'nonzero_commands': [command['metadata_path'] for command in commands if command['exit']],
         'final_checks_same_source': final,
         'effective_tests': 67, 'test_target_counts': counts,
         'protected_audio_source_count': len(protected),
         'protected_git_diff': {'argv': [GIT, 'diff', '--exit-code', BASE, '--', *protected],
                                'exit': unchanged.returncode, 'stdout_bytes': 0, 'stderr_bytes': 0},
         'source_diff_check': {'exit': diff.returncode, 'stdout_bytes': 0, 'stderr_bytes': 0},
         'new_binary': {'path': str(binary.relative_to(ROOT)).replace('\\', '/'),
                        'bytes': len(binary_data), 'sha256': sha(binary_data),
                        'build_metadata': final[-1], 'executed': False},
         'tests_added': ['production render budget 0/960/961/1056/1440, padding/scratch bounds and zero allocations',
                         'actual shared production packet ABI single commit with notification/mute and ACK through release',
                         'stereo capture through real decode branch 1440 bound, short destination/overbound full silence'],
         'risk_notes': ['render1056 is prior leader device measurement; capture actual remains UNKNOWN',
                        'CLI missing subcommand, strict test-only lint, and ordinary E0277 failures retained without overwritten raw logs',
                        'no compiler ICE/AV observed in these commands; other scopes tool stability still UNKNOWN',
                        'OS capacity is not queue target, measured latency, continuity, model quality or successful cleanup',
                        'newly built executable was not run; all existing hardware once permissions spent',
                        'external watchdog needed for synchronous COM deadline; on-kill cleanup/OS erasure UNKNOWN',
                        'allocation tests cover Rust/ABI injection paths, not real OS audio services',
                        'a readonly old check-runner file was read outside this lease before implementation; disclosed to leader, never executed or modified'],
         'not_run': ['Initialize', 'Start', 'capture', 'render', 'metadata probe', 'GPU', 'LAN',
                     'actual VB-CABLE continuity', 'native cleanup measurement', 'third-party app', 'Mac'],
         'independent_review': 'PENDING', 'root_integration': 'NOT_RUN by author'}
stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%f')
path = OUT / ('delivery-' + stamp + '.json')
with path.open('x', encoding='utf-8', newline='\n') as file:
    json.dump(index, file, ensure_ascii=True, indent=2)
print(json.dumps({'index': str(path.relative_to(ROOT)).replace('\\', '/'),
                  'index_sha256': sha(path.read_bytes()), 'command_count': len(commands),
                  'nonzero_commands': index['nonzero_commands'], 'source_count': len(sources),
                  'changed_source_sha256': index['changed_source_sha256'], 'final_checks': final,
                  'effective_tests': 67, 'new_binary': index['new_binary']}))
