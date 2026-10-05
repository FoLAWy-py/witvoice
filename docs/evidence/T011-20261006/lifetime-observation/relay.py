import pathlib,json,hashlib,subprocess,sys
sys.stdout.reconfigure(encoding='utf-8');r=pathlib.Path(r'D:\Project\witvoice');b=r/'.local/t011-observation-root'
mode=sys.argv[1]
if mode=='diff':
    raw=subprocess.check_output(['git','diff','41299a5f176bbdca8a0931c793391b587d49498d','39583276946ccd4ec99c0fcb8e30a9aac485472a','--','crates/platform/src/process.rs','crates/platform/src/lib.rs','crates/engines/src/supervisor.rs','crates/engines/tests/worker_supervision.rs'],cwd=r).decode();lines=raw.splitlines(True);part=int(sys.argv[2]);print('TOTAL_DIFF_LINES',len(lines),'PART',part,'DIFF_SHA',hashlib.sha256(raw.encode()).hexdigest());print(''.join(lines[part*190:(part+1)*190]))
else:
    m=json.loads((b/(mode+'.json')).read_text(encoding='utf-8-sig'));print(json.dumps({k:v for k,v in m.items() if k not in ['source_before','source_after']},indent=2));print('INPUTS',len(m['source_before']),'SAME',m['source_before']==m['source_after'],'CURRENT',all(hashlib.sha256((r/p).read_bytes()).hexdigest()==h for p,h in m['source_after'].items()))
    for ext in ['stdout','stderr']:print('FULL_'+ext+'\n'+(b/(mode+'.'+ext)).read_text())
