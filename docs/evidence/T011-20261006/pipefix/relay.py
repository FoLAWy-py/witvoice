import pathlib,json,hashlib,sys,subprocess
sys.stdout.reconfigure(encoding='utf-8');r=pathlib.Path(r'D:\Project\witvoice');b=r/'.local/t011-pipefix-root'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
kind=sys.argv[1];part=int(sys.argv[2]) if len(sys.argv)>2 else 0
if kind=='checks':
    f=json.loads((b/'frozen.json').read_text());data=[]
    for name in ['01-platform','02-observation','03-lifecycle','03b-audio','04-fmt','05-clippy','06-examples']:
        m=json.loads((b/(name+'.json')).read_text(encoding='utf-8-sig'))
        assert m['source_before']==m['source_after']==f and all(sha(r/p)==h for p,h in f.items())
        assert sha(b/(name+'.stdout'))==m['stdout_sha256'] and sha(b/(name+'.stderr'))==m['stderr_sha256']
        data.append({k:v for k,v in m.items() if k not in ['source_before','source_after']}|{'126_pre_post_current_equal':True,'metadata_sha256':sha(b/(name+'.json'))})
    text=json.dumps(data,indent=2)
elif kind=='configs':
    data=[]
    for name,wrapper in [('t011-pipefix-native-once','execute.ps1'),('t011-production-owner-once','execute.ps1'),('t009-drained-once','supervise_once.ps1')]:
        d=r/'.local'/name;c=json.loads((d/'config.json').read_text());assert all(sha(r/p)==h for p,h in c['source_sha256'].items())
        data.append({'directory':name,'config_sha':sha(d/'config.json'),'wrapper_sha':sha(d/wrapper),'126_current_equal':True,'unconsumed':not(d/'consumed.json').exists() and not(d/'result.json').exists(),'config':{k:v for k,v in c.items() if k!='source_sha256'}})
    text=json.dumps(data,indent=2)
elif kind=='diff':
    text=subprocess.check_output(['git','show','--format=fuller','--stat','HEAD'],cwd=r,text=True)+subprocess.check_output(['git','show','--format=','HEAD','--','crates/platform/src/windows.rs','crates/engines/examples/supervised_warmup_probe.rs'],cwd=r,text=True)
elif kind=='audio-diff':
    text=subprocess.check_output(['git','show','--format=','HEAD','--','crates/audio/examples/route_probe.rs','crates/audio/examples/support/route_drain.rs','crates/audio/tests/route_drain.rs'],cwd=r,text=True)
elif kind=='wrapperdiff':
    import difflib
    old=(r/'.local/t011-observation-native-once/execute.ps1').read_text(encoding='utf-8-sig')
    text='NATIVE WRAPPER FULL\n'+(r/'.local/t011-pipefix-native-once/execute.ps1').read_text()+'\nREAL WRAPPER DIFF\n'+''.join(difflib.unified_diff(old.splitlines(True),(r/'.local/t011-production-owner-once/execute.ps1').read_text().splitlines(True)))+'\nAUDIO WRAPPER DIFF\n'+''.join(difflib.unified_diff((r/'.local/t009-primed-once/supervise_once.ps1').read_text(encoding='utf-8-sig').splitlines(True),(r/'.local/t009-drained-once/supervise_once.ps1').read_text().splitlines(True)))
else:
    text=(b/(kind+'.stdout')).read_text(encoding='utf-8-sig')+'\nSTDERR\n'+(b/(kind+'.stderr')).read_text(encoding='utf-8-sig')
chunks=[text[i:i+14000] for i in range(0,len(text),14000)]
assert part<len(chunks);print('RELAY',kind,'part',part+1,'of',len(chunks),'full_chars',len(text));print(chunks[part])
