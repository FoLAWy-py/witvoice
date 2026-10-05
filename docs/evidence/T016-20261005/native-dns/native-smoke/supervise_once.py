import datetime, hashlib, ipaddress, json, pathlib, subprocess, time, uuid
ROOT = pathlib.Path(__file__).resolve().parents[2]
OWN = ROOT / '.local/t016-authorized-native'
PUBLIC = ROOT / 'docs/evidence/T016-20261005/native-dns/native-smoke'
def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()
def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()
def main():
    once_path = OWN / 'once.json'
    once = json.loads(once_path.read_text(encoding='utf-8'))
    assert once['status'] == 'NOT_RUN', 'Once authorization already consumed; do not retry'
    config = json.loads((OWN / 'frozen-config.json').read_text(encoding='utf-8'))
    exe = pathlib.Path(config['executable']).resolve()
    assert exe.is_relative_to(ROOT / '.local') and exe.name == 'native_discovery_probe.exe'
    assert sha(exe) == config['executable_sha256']
    assert sha(ROOT / 'crates/transport/examples/native_discovery_probe.rs') == config['source_sha256']
    address = ipaddress.IPv4Address(config['private_ipv4'])
    assert any(address in ipaddress.IPv4Network(n) for n in ('10.0.0.0/8','172.16.0.0/12','192.168.0.0/16','169.254.0.0/16'))
    assert config['interface_index'] == 19 and config['static_preflight_complete']
    expected = f'{address}:40000'
    node = str(uuid.uuid4())
    PUBLIC.mkdir(parents=True, exist_ok=True)
    processes = []
    started = time.monotonic()
    once.update(status='CONSUMED', started_utc=utc(), node_id=node, scope='Two selected interface19 probes only')
    once_path.write_text(json.dumps(once, indent=2)+'\n', encoding='utf-8')
    error = None
    try:
        for operation, duration in [('browse',6500),('advertise',8000)]:
            argv = [str(exe),'--allow-lan','19',node,operation,expected,str(duration)]
            out = (OWN / f'{operation}.stdout').open('wb')
            err = (OWN / f'{operation}.stderr').open('wb')
            launched = time.monotonic()
            try:
                process = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.DEVNULL, stdout=out, stderr=err, creationflags=subprocess.CREATE_NO_WINDOW)
            except BaseException:
                out.close();err.close();raise
            processes.append(dict(operation=operation, process=process, launched=launched, argv=argv, out=out, err=err, timed_out=False))
        while any(x['process'].poll() is None for x in processes):
            now = time.monotonic()
            for item in processes:
                process = item['process']
                if process.poll() is None and (now-item['launched'] >= 9.5 or now-started >= 13.5):
                    item['timed_out'] = True
                    process.kill()
            time.sleep(0.01)
    except BaseException as exc:
        error = type(exc).__name__ + ': ' + str(exc)
    finally:
        for item in processes:
            process = item['process']
            if process.poll() is None:
                item['timed_out'] = True
                process.kill()
            try:
                process.wait(timeout=0.5)
            except subprocess.TimeoutExpired:
                error = 'Own probe termination unconfirmed'
            item['out'].close();item['err'].close()
    result = dict(authorization=once['authorization'], node_id=node, interface_index=19, executable_sha256=config['executable_sha256'], source_sha256=config['source_sha256'], start_utc=once['started_utc'], end_utc=utc(), elapsed_seconds=time.monotonic()-started, error=error, probes=[])
    for item in processes:
        operation = item['operation']
        out = OWN / f'{operation}.stdout'
        err = OWN / f'{operation}.stderr'
        raw_out = out.read_text(encoding='utf-8',errors='replace')
        raw_err = err.read_text(encoding='utf-8',errors='replace')
        (PUBLIC / f'{operation}.stdout.redacted').write_text(raw_out.replace(expected,'<private-endpoint>').replace(str(address),'<private-ipv4>'),encoding='utf-8')
        (PUBLIC / f'{operation}.stderr.redacted').write_text(raw_err.replace(expected,'<private-endpoint>').replace(str(address),'<private-ipv4>'),encoding='utf-8')
        result['probes'].append(dict(operation=operation, pid=item['process'].pid, argv=item['argv'], exit_code=item['process'].returncode, timed_out=item['timed_out'], raw_stdout_sha256=sha(out), raw_stderr_sha256=sha(err), stdout_bytes=out.stat().st_size, stderr_bytes=err.stat().st_size))
    (OWN / 'result.raw.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    public = json.dumps(result,indent=2).replace(expected,'<private-endpoint>').replace(str(address),'<private-ipv4>')
    (PUBLIC / 'supervisor.json').write_text(public+'\n',encoding='utf-8')
    once.update(status='EXECUTED_NOT_REUSABLE', ended_utc=result['end_utc'])
    once_path.write_text(json.dumps(once,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(dict(elapsed_seconds=result['elapsed_seconds'], error=error, probes=[{k:x[k] for k in ('operation','exit_code','timed_out')} for x in result['probes']])))
    return int(bool(error) or len(processes)!=2 or any(x['timed_out'] or x['exit_code']!=0 for x in result['probes']))
if __name__ == '__main__':
    raise SystemExit(main())
