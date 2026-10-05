import pathlib, sys, json, hashlib
sys.stdout.reconfigure(encoding='utf-8')
r=pathlib.Path(r'D:\Project\witvoice')
paths={'process':'crates/platform/src/process.rs','supervisor':'crates/engines/src/supervisor.rs','tests':'crates/engines/tests/worker_supervision.rs','peer':'tests/integration/worker_supervision_peer.py','audio-spec':'docs/spec/02-audio-engine.md','acceptance':'docs/spec/07-tests-acceptance.md','audio-role':'docs/roles/audio_runtime.md'}
key=sys.argv[1]
if key in paths:
    p=r/paths[key];lines=p.read_text(encoding='utf-8').splitlines(True);part=int(sys.argv[2]) if len(sys.argv)>2 else 0
    # Line numbers make independent review locations reproducible. Small blocks.
    start=part*180;end=min(start+180,len(lines));print(json.dumps({'path':paths[key],'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'total_lines':len(lines),'start':start+1,'end':end}))
    for i in range(start,end): print(f'{i+1}: {lines[i]}',end='')
else:
    stem=r/'.local/t011-lifetime-root'/key
    m=json.loads(stem.with_suffix('.json').read_text(encoding='utf-8-sig'))
    print(json.dumps({k:v for k,v in m.items() if k not in ['source_before','source_after']},indent=2))
    print('SOURCE_COUNT',len(m['source_before']),'SAME',m['source_before']==m['source_after'])
    for ext in ['stdout','stderr']:
        print('FULL_'+ext);print(stem.with_suffix('.'+ext).read_text(encoding='utf-8'))
