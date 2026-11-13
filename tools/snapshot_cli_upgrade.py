#!/usr/bin/env python3
"""Finite private actual-CLI snapshot upgrade qualification; POSIX, offline builds."""
import argparse
from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import time
import compatibility as helpers

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
STREAM = '11' * 16

def require(ok, message):
    if not ok: raise RuntimeError(message)

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def inventory(root): return {str(p.relative_to(root)):sha(p) for p in sorted(root.rglob('*')) if p.is_file()}

def invoke(argv, output, evidence, label, *, cwd=None, env=None, timeout=45, success=True):
    argv=list(map(str,argv));record={'label':label,'argv':argv,'timeout':timeout,'exit':None}
    evidence['commands'].append(record)
    with (output/(label+'.stdout')).open('x') as stdout, (output/(label+'.stderr')).open('x') as stderr:
        child=subprocess.Popen(argv,cwd=cwd,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
        started=time.monotonic()
        try: child.wait(timeout=timeout)
        finally:
            helpers.stop(child)
            record.update(exit=child.returncode,elapsed=time.monotonic()-started)
    require((child.returncode==0)==success,label+' unexpected exit')
    return (output/(label+'.stdout')).read_text()

def ack(text):
    rows=[line for line in text.splitlines() if line.startswith('follower durable_revision=')]
    require(len(rows)==1,'missing/duplicate durable receipt')
    m=re.fullmatch(r'follower durable_revision=(0|[1-9][0-9]*) stream=([0-9a-f]{32}) digest=([0-9a-f]{64})',rows[0])
    require(m is not None and m[2]==STREAM and len(m[1])<=20,'invalid durable receipt')
    require(int(m[1]) <= (1 << 64)-1,'revision outside u64')
    return {'revision':int(m[1]),'digest':m[3]}

@contextmanager
def server(binary, directory, identity, credentials, pin, output, evidence, label):
    argv=list(map(str,[binary,'serve',directory,STREAM,identity,'127.0.0.1:0',credentials,pin,'--max-exchanges','20']))
    record={'label':label,'argv':argv,'exit':None};evidence['commands'].append(record)
    with (output/(label+'.stdout')).open('x') as stdout,(output/(label+'.stderr')).open('x') as stderr:
        child=subprocess.Popen(argv,stdout=stdout,stderr=stderr,start_new_session=True)
        try:
            deadline=time.monotonic()+10
            while True:
                text=(output/(label+'.stdout')).read_text()
                lines=text.splitlines()
                if lines and re.fullmatch(r'ready 127\.0\.0\.1:[0-9]+',lines[0]):break
                require(child.poll() is None,'server exited before readiness')
                require(time.monotonic()<deadline,'server readiness timeout')
                time.sleep(.02)
            yield lines[0].split()[1]
        finally:
            helpers.stop(child);record['exit']=child.returncode
            record['stopped_by_harness']=True

def prepare(output, historical, current):
    sources={};bindings={}
    for name,rev in [('historical',historical),('current',current)]:
        full=subprocess.check_output(['git','rev-parse','--verify',rev+'^{commit}'],cwd=ROOT,text=True,timeout=30).strip()
        archive=subprocess.check_output(['git','archive',full],cwd=ROOT,timeout=30)
        path=output/'source'/name;path.mkdir(parents=True)
        (output/(name+'.tar')).write_bytes(archive)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:tar.extractall(path,filter='data')
        sources[name]=path;bindings[name]={'commit':full,'archive_sha256':hashlib.sha256(archive).hexdigest(),'files':inventory(path)}
    return sources,bindings

def qualify(output,sources,bindings,evidence):
    logs=output/'logs';logs.mkdir();work=output/'work';work.mkdir(mode=0o700)
    env=helpers.child_env();evidence['sources']=bindings
    evidence['tool_hashes']={p.name:sha(p) for p in [Path(__file__),HERE/'snapshot-cli-upgrade-fixture.rs',HERE/'compatibility.py']}
    evidence['compiler']=invoke(['rustc','-Vv'],logs,evidence,'compiler',env=env)
    evidence['cargo']=invoke(['cargo','-V'],logs,evidence,'cargo',env=env)
    binaries={};fixtures={};evidence['binaries']={}
    for name,source in sources.items():
        require(inventory(source)==bindings[name]['files'],'prepared source mismatch')
        target=output/'build'/name;buildenv=dict(env,CARGO_TARGET_DIR=str(target),WEEKEND_DB_SOURCE_ID=bindings[name]['commit'])
        invoke(['cargo','build','--release','--offline','--locked','-p','weekend-replicate'],logs,evidence,name+'-build',cwd=source,env=buildenv,timeout=240)
        binaries[name]=target/'release/weekend-replicate';fixtures[name]=output/(name+'-fixture')
        rlibs=list((target/'release/deps').glob('libweekend_db*.rlib'))
        require(len(rlibs)==1,'ambiguous core library')
        invoke(['rustc','--edition=2024','-O',HERE/'snapshot-cli-upgrade-fixture.rs','--extern','weekend_db='+str(rlibs[0]),'-L','dependency='+str(target/'release/deps'),'-o',fixtures[name]],logs,evidence,name+'-fixture-build',env=buildenv,timeout=90)
        identity=invoke([fixtures[name],'identity'],logs,evidence,name+'-identity',env=env).strip()
        require(identity==bindings[name]['commit'],'linked runtime identity mismatch')
        evidence['binaries'][name]={'cli':sha(binaries[name]),'fixture':sha(fixtures[name]),'runtime_source_id':identity}
        require(inventory(source)==bindings[name]['files'],'source changed during build')
    sequence=0
    def call(argv,success=True):
        nonlocal sequence
        sequence+=1
        return invoke(argv,logs,evidence,f'command-{sequence:03}',env=env,success=success)
    old,new=binaries['historical'],binaries['current']
    source=work/'primary.wdb';follower=work/'follower';follower.mkdir()
    call([fixtures['historical'],'create',source])
    description=call([old,'describe',source,STREAM]);matches=re.findall(r'file_identity=([0-9a-f]{32})',description)
    require(len(matches)==1,'describe identity');identity=matches[0]
    call([old,'follower-init',follower,STREAM,identity])
    client=work/'client';receiver=work/'server';wrong=work/'wrong'
    for credentials in [client,receiver,wrong]:call([old,'keygen',credentials])
    def poll(binary,address,expected,local):
        text=call([binary,'poll',source,STREAM,address,client,receiver/'cert.der'])
        require(f'local verified_revision={local}\n' in text,'local observation mismatch')
        require(ack(text)==expected,'fresh durable receipt mismatch')
    def ship(binary,address,revision):
        export=work/f'export-{revision}.wdb'
        text=call([binary,'ship',source,STREAM,export,address,client,receiver/'cert.der','--attempts','1'])
        require(f'local captured_revision={revision} ' in text,'local capture missing')
        observed=ack(text);require(observed=={'revision':revision,'digest':sha(export)},'export/receipt mismatch')
        return observed,export
    def offline(binary,expected,export):
        text=call([binary,'status',follower,STREAM,identity]);require(ack(text)==expected,'reopened receipt mismatch')
        match=re.search(r'selected_slot=Some\(([01])\)',text);require(match is not None,'selected slot missing')
        selected=follower/f'replica-{match[1]}.wdb';require(selected.read_bytes()==export.read_bytes(),'selected export bytes differ')
        before=sha(selected)
        for fixture in fixtures.values():call([fixture,'check',selected,str(expected['revision'])])
        require(sha(selected)==before,'oracle changed selected image')
    with server(old,follower,identity,receiver,client/'cert.der',logs,evidence,'old-server') as address:
        first,export3=ship(old,address,3);poll(old,address,first,3)
    offline(new,first,export3)
    call([fixtures['current'],'advance',source,'4'])
    manifest=follower/'replica-manifest.wdb';before=sha(manifest)
    with server(old,follower,identity,receiver,client/'cert.der',logs,evidence,'old-server-new-client') as address:
        poll(new,address,first,4)
        # Actual reachable endpoint, but intentionally wrong client-side server pin.
        text=call([new,'ship',source,STREAM,work/'unconfirmed-export',address,client,wrong/'cert.der','--attempts','1'],False)
        require('local captured_revision=4 ' in text and 'follower durable_revision=' not in text,'failed ship conflated local and remote acknowledgment')
        poll(new,address,first,4)
    offline(old,first,export3);require(sha(manifest)==before,'failed exchange changed manifest')
    with server(old,follower,identity,receiver,client/'cert.der',logs,evidence,'retry-new-to-old') as address:
        second,export4=ship(new,address,4)
    offline(new,second,export4)
    # Upgrade receiver process while preserving enrollment/data, and keep old sender.
    call([fixtures['historical'],'advance',source,'5'])
    with server(new,follower,identity,receiver,client/'cert.der',logs,evidence,'new-server-old-client') as address:
        poll(old,address,second,5);third,export5=ship(old,address,5)
    offline(old,third,export5)
    before=sha(manifest)
    with server(new,follower,identity,receiver,client/'cert.der',logs,evidence,'both-new') as address:
        poll(new,address,third,5);poll(new,address,third,5)
    offline(new,third,export5);require(sha(manifest)==before,'repeat poll created manifest commit')
    for fixture in fixtures.values():call([fixture,'check',source,'5'])
    evidence['receipts']=[first,second,third];evidence['private_credentials_retained']=False
    # Never retain private keys in proof artifacts. Only public certificates/hashes survive.
    for directory in [client,receiver,wrong]:(directory/'key.der').unlink()
    for name,source in sources.items():require(inventory(source)==bindings[name]['files'],'source drift')
    require(evidence['tool_hashes']=={p.name:sha(p) for p in [Path(__file__),HERE/'snapshot-cli-upgrade-fixture.rs',HERE/'compatibility.py']},'tool drift')
    evidence['verified']=True

def cleanup_docker(name, volume):
    """Remove only this wrapper's owned pair; attempt both even after failure."""
    require(re.fullmatch(r'snapshot-cli-upgrade-[0-9a-f]{32}', name) is not None
            and volume == name + '-data', 'cleanup requires owned UUID pair')
    result = {'cleanup_errors': []}
    for argv in [['docker', 'rm', '-f', name], ['docker', 'volume', 'rm', volume]]:
        try:
            subprocess.run(argv, capture_output=True, timeout=30, check=True)
        except (OSError, subprocess.SubprocessError) as error:
            result['cleanup_errors'].append(type(error).__name__)
    for kind, identifier, argv in [
        ('container', name, ['docker', 'ps', '-a', '--format', '{{.Names}}']),
        ('volume', volume, ['docker', 'volume', 'ls', '--format', '{{.Name}}'])]:
        try:
            names = subprocess.check_output(argv, text=True, timeout=10).splitlines()
            result[kind + '_absent'] = identifier not in names
        except (OSError, subprocess.SubprocessError) as error:
            result[kind + '_absent'] = False
            result['cleanup_errors'].append(type(error).__name__)
    result['cleanup_verified'] = result['container_absent'] and result['volume_absent']
    return result

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--historical',default='315068fc38b99d8ec616fdb5a8ee81107b471534');parser.add_argument('--current',default='365daf6c8a6b7c85fb049fb5a0370b401dde2424');parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();require(os.name=='posix','POSIX qualification only')
    output=helpers.fresh_output(args.output);evidence={'verified':False,'commands':[]}
    try:
        sources,bindings=prepare(output,args.historical,args.current);qualify(output,sources,bindings,evidence)
    except BaseException as error:
        evidence['failure_type']=type(error).__name__;raise
    finally:
        # Also scrub credentials after an interrupted/failed scenario.
        for key in (output/'work').glob('*/key.der'):key.unlink()
        evidence['artifact_sha256']={str(p.relative_to(output)):sha(p) for p in output.rglob('*') if p.is_file() and 'build' not in p.relative_to(output).parts and 'source' not in p.relative_to(output).parts and p.name!='evidence.json'}
        (output/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
if __name__=='__main__':main()
