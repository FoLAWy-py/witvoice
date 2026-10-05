import datetime, hashlib, json, pathlib, re, subprocess
ROOT = pathlib.Path('D:/Project/witvoice')
OUT = ROOT/'.local/t009-exact-mix-author'
def sha(raw): return hashlib.sha256(raw).hexdigest()
commands = [json.loads(p.read_text(encoding='utf8')) | {'metadata_path':str(p), 'metadata_sha256':sha(p.read_bytes())}
            for p in sorted(OUT.glob('0[1-5]-*.json'))]
paths = commands[-1]['source_after']
git = 'C:/Users/22198/.cache/codex-runtimes/codex-primary-runtime/dependencies/native/git/cmd/git.exe'
diff = subprocess.run([git,'diff','--',*paths],cwd=ROOT,capture_output=True)
assert diff.returncode == 0
(OUT/'source.diff').write_bytes(diff.stdout)
all_files = sorted(set((ROOT/'crates/audio').rglob('*.rs')) | {ROOT/'crates/audio/README.md',ROOT/'crates/audio/Cargo.toml'})
all_hashes = {p.relative_to(ROOT).as_posix():sha(p.read_bytes()) for p in all_files}
binary = {}
for name in ['route_probe', 'format_support_probe']:
 p = ROOT/'.local/t009-author/target/debug/examples'/f'{name}.exe'
 binary[name] = {'path':str(p),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes()),'execution':'NOT_RUN','build_metadata':commands[-1]['metadata_path']}
tests = pathlib.Path(commands[1]['stdout']['path']).read_text(encoding='utf8')
passed = sum(map(int,re.findall(r'test result: ok\. (\d+) passed',tests)))
index = {'task':'T009 exact-mix single-variable preparation','actor_uuid':'01a10767-80de-7e02-b3b0-73d1273a70bd',
 'status':'STOP_WRITES','created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'commands':commands,'source_final':all_hashes,'changed_source_final':paths,
 'diff':{'path':str(OUT/'source.diff'),'bytes':len(diff.stdout),'sha256':sha(diff.stdout),'command_exit':diff.returncode},
 'binaries':binary,'tests_passed':passed,'all_command_exit_codes':[m['exit_code'] for m in commands],
 'final_four_source_maps_equal':all(m['source_before']==paths and m['source_after']==paths for m in commands[1:]),
 'not_run':['native metadata query','Initialize','Start','render marker','capture','GPU','LAN'],
 'risks':['HRESULT exact symbol and root cause remain UNKNOWN; no driver attribution',
          'Exact mix layout is restricted to validated mono/stereo Float32 basic3/cb0 or extensible65534/cb22',
          'Prepare failure cleanup is UNKNOWN; no RAII-to-hardware cleanup PASS',
          'Early failure preserves structured actual_frames when known; capacity convenience fields may be null before successful owner construction',
          'Source and binaries require independent review and fresh leader metadata before future device diagnostic'],
 'bounds_unchanged':['1440 negotiated maximum','960 single commit','2s active marker','15s total','8 queue slots','480 block','epoch/gate/ticket','flags and strict underflow','STA/shared/event/zero periodicity/GUID_NULL'],
 'default_prepare':'unchanged basic descriptor path; no GetMixFormat on default',
 'mix_mode':'explicit --approve-capture-mix-probe changes capture descriptor only; render remains basic'}
p = OUT/'delivery-index.json'
p.write_text(json.dumps(index,ensure_ascii=True,indent=2),encoding='utf8')
print(json.dumps({'index':str(p),'index_sha256':sha(p.read_bytes()),'passed':passed,'exit_codes':index['all_command_exit_codes'], 'source_final':paths,'binaries':binary,'final_four_source_maps_equal':index['final_four_source_maps_equal']}))
