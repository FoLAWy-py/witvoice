from pathlib import Path
import hashlib
import json
import subprocess

root = Path('D:/Project/witvoice')
target = root / '.local/t011-supervisor-root'
old = root / '.local/t011-budget8-root'
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

paths = list(json.loads((old / 'frozen-inputs.json').read_text(encoding='utf-8'))['source_sha256'])
paths += ['crates/engines/src/supervisor.rs', 'crates/engines/tests/worker_supervision.rs', 'tests/integration/worker_supervision_peer.py']
assert len(paths) == len(set(paths)) == 121
manifest = {'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
            'scope': 'T011 native software supervision only; no new model/GPU/audio/LAN execution',
            'source_sha256': {p: sha(root / p) for p in paths}}
assert not (target / 'frozen-inputs.json').exists(), 'Never overwrite a frozen integration'
(target / 'frozen-inputs.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')

rust = (old / 'verify.ps1').read_text(encoding='utf-8')
rust = rust.replace('One frozen integration of CR0006 human-approved8GiB host gate, device4GiB unchanged; no model/GPU/audio/network execution; stop on failure.',
                    'One frozen T011 native supervisor integration; trusted isolated Python testpeer only; no model/GPU/audio/LAN; stop on failure.')
rust = rust.replace("task_scope=@('T009','T011')", "task_scope=@('T011')")
(target / 'verify.ps1').write_text(rust, encoding='utf-8')

python = (old / 'python-verify.ps1').read_text(encoding='utf-8')
start = python.index('$files=@(')
end = python.index('$commands=@(', start)
python = python[:start] + '''$scope=Get-Content "$PSScriptRoot\\frozen-inputs.json" -Raw|ConvertFrom-Json
function Inputs{$m=[ordered]@{};foreach($p in $scope.source_sha256.PSObject.Properties){$h=FileHash (Join-Path $PWD $p.Name);if($h-ne$p.Value){throw "Frozen source changed: $($p.Name)"};$m[$p.Name]=$h};return $m}
''' + python[end:]
python = python.replace('python-tests-budget8', 'python-tests-supervisor').replace('compile-budget8', 'compile-supervisor')
python = python.replace("'tests/integration/worker_terminal_peer.py')", "'tests/integration/worker_terminal_peer.py','tests/integration/worker_supervision_peer.py')")
python = "$ErrorActionPreference='Stop'\nif($PSVersionTable.PSVersion.Major-ne5-or$PSVersionTable.PSVersion.Minor-ne1){throw 'Explicit system PowerShell5.1 required'}\n" + python
(target / 'python-verify.ps1').write_text(python, encoding='utf-8')
print(json.dumps({'source_count': len(paths), 'manifest_sha256': sha(target/'frozen-inputs.json'), 'Cargo.lock_sha256': sha(root/'Cargo.lock'), 'source_commit': manifest['source_commit']}))
