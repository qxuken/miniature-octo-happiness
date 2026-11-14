#!/usr/bin/env python3
"""Private exact-package Linux C ABI fault drill; no host filesystem exhaustion."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import uuid

HERE=Path(__file__).resolve().parent
helper=HERE.parent/'compatibility.py'
if not helper.is_file():helper=HERE/'compatibility.py'
sys.path.insert(0,str(helper.parent))
import compatibility as helpers
IMAGE='sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08'
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def inventory(path):return {str(p.relative_to(path)):sha(p) for p in sorted(path.rglob('*')) if p.is_file()}
def verify(package):
    d=json.loads((package/'manifest.json').read_text())
    if d.get('verified') is not True or not re.fullmatch('[0-9a-f]{40}',d.get('source','')) or d.get('source_id')!=d['source']:raise ValueError('unverified exact-source package')
    hashes=d.get('sha256',{})
    if not hashes:raise ValueError('empty package inventory')
    for p in package.rglob('*'):
        if p.is_symlink():raise ValueError('package symlink')
    if set(inventory(package))!=set(hashes)|{'manifest.json'}:raise ValueError('package inventory')
    for name,digest in hashes.items():
        if Path(name).is_absolute() or '..' in Path(name).parts or sha(package/name)!=digest:raise ValueError('package hash/path')
    if not (package/'libweekend_db.so').is_file() or d.get('features')!='crdt,replication':raise ValueError('requires combined native Linux package')
    return d

def validate(row,mode):
    expected={'mode':mode,'status':0 if mode==0 else 14 if mode in [1,5] else 15,
              'visible':4 if mode==0 else 3,'reopened':4 if mode in [0,2,3] else 3,'final':4,
              'retained_revision':2,'hits':1 if mode in [1,2,3,4] else 0,
              'syncs':[3,1,2,3,1,0][mode], 'root_writes':1 if mode in [0,2,3,4] else 0,
              'witness_writes':1 if mode in [0,3] else 0,'native_errno':28 if mode==5 else 0,'verified':True}
    if set(row)!=set(expected) or any(type(row[k]) is not type(v) or row[k]!=v for k,v in expected.items()):raise ValueError('unexpected ABI/state/fault outcome')
    return row

def combine(exercise,recovery,mode):
    if set(exercise)!={'mode','status','visible','hits','syncs','root_writes','witness_writes','native_errno','verified'} or set(recovery)!={'mode','reopened','final','retained_revision','verified'}:
        raise ValueError('missing independent exercise/recovery phase')
    if type(exercise['mode']) is not int or type(recovery['mode']) is not int or exercise['mode']!=recovery['mode'] or exercise['verified'] is not True or recovery['verified'] is not True:
        raise ValueError('phase identity or completion mismatch')
    return validate({**exercise,**recovery},mode)

def worker(output):
    output.mkdir();e={'verified':False,'commands':[],'scope':'Linux C delivery ABI; separate fresh process recovers without shim. EIO is interposed, ENOSPC is kernel tmpfs; no power-loss/device or all-language claim'}
    try:
        package=output/'relocated-package';shutil.copytree('/seed/package',package);info=verify(package)
        inputs=output/'inputs';shutil.copytree('/seed/inputs',inputs)
        frozen=inventory(inputs);pkg_hash=sha(package/'manifest.json')
        env=helpers.child_env()
        env['PYTHONDONTWRITEBYTECODE']='1'
        for key in ['LD_PRELOAD','LD_LIBRARY_PATH','LD_DEBUG','QUALIFICATION_PRIVATE_TMPFS']:env.pop(key,None)
        def call(argv,label,extra=None):return helpers.run(argv,output,e,label,cwd=output,env={**env,**(extra or {})},timeout=90)
        e.update(source=info['source'],package_manifest_sha256=pkg_hash,inputs_sha256=frozen,
                 kernel=call(['uname','-a'],'kernel'),cc=call(['cc','--version'],'cc'),
                 filesystem=call(['findmnt','-T',output,'-n','-o','FSTYPE,SOURCE,TARGET'],'filesystem'))
        if e['filesystem'].split()[0]!='ext4':raise ValueError('private volume must be ext4')
        call(['python3','--version'],'python-version')
        call(['python3',inputs/'test_runner.py'],'controls')
        call(['python3','-O',inputs/'test_runner.py'],'controls-O')
        common=['cc','-std=c11','-O2','-UNDEBUG','-Wall','-Wextra','-Werror']
        for suffix,flags in [('',[]),('-disabled',['-DQUALIFICATION_BYPASS_INJECTION=1'])]:
            call(common+flags+['-shared','-fPIC',str(inputs/'faults.c'),'-ldl','-o',str(output/('faults'+suffix+'.so'))],'build-shim'+suffix)
        call(common+['-I',str(package),str(inputs/'consumer.c'),str(package/'libweekend_db.so'),'-Wl,-rpath,$ORIGIN/relocated-package','-ldl','-o',str(output/'consumer')],'build-consumer')
        before={n:sha(output/n) for n in ['consumer','faults.so','faults-disabled.so']};e['binary_sha256']=before
        records=[]
        for mode in range(6):
            directory=Path('/faults')/'case' if mode==5 else output/f'case-{mode}';directory.mkdir()
            text=call([output/'consumer',str(mode),directory,info['source']],f'case-{mode}',{'LD_PRELOAD':str(output/'faults.so'),'LD_DEBUG':'libs','QUALIFICATION_PRIVATE_TMPFS':'1' if mode==5 else '0'})
            rows=[json.loads(line) for line in text.splitlines() if line.startswith('{')]
            if len(rows)!=1:raise ValueError('missing/duplicate complete result')
            recovered=call([output/'consumer','recover',directory,info['source'],str(mode)],f'recover-{mode}',{'LD_DEBUG':'libs'})
            recovered_rows=[json.loads(line) for line in recovered.splitlines() if line.startswith('{')]
            if len(recovered_rows)!=1:raise ValueError('missing independent recovery result')
            records.append(combine(rows[0],recovered_rows[0],mode))
            if 'calling init: '+str(package/'libweekend_db.so') not in text:raise ValueError('relocated library not observed')
            if 'calling init: '+str(package/'libweekend_db.so') not in recovered or 'calling init: '+str(output/'faults.so') in recovered:raise ValueError('recovery loader/injector mismatch')
        negative=output/'disabled-case';negative.mkdir()
        p=subprocess.run([str(output/'consumer'),'1',str(negative),info['source']],cwd=output,env={**env,'LD_PRELOAD':str(output/'faults-disabled.so')},capture_output=True,text=True,timeout=30)
        (output/'disabled.stdout').write_text(p.stdout);(output/'disabled.stderr').write_text(p.stderr)
        if p.returncode!=-6 or 'status==expected' not in p.stderr:raise ValueError('disabled-injector control did not fail at ABI status check')
        e['disabled_injector']={'returncode':p.returncode,'expected_failure':True}
        if inventory(inputs)!=frozen or sha(package/'manifest.json')!=pkg_hash or before!={n:sha(output/n) for n in before}:raise ValueError('input/binary changed')
        verify(package);e.update(results=records,source_stable=True,verified=True)
    finally:
        e['artifact_sha256']=inventory(output)
        (output/'evidence.json').write_text(json.dumps(e,indent=2)+'\n')

def cleanup(name,volume):
    if not re.fullmatch('weekend-delivered-faults-[0-9a-f]{32}',name) or volume!=name+'-data':raise ValueError('cleanup requires owned UUID pair')
    for argv in [['docker','rm','-f',name],['docker','volume','rm',volume]]:
        try:subprocess.run(argv,capture_output=True,timeout=30,check=False)
        except (OSError,subprocess.SubprocessError):pass
    containers=subprocess.check_output(['docker','ps','-a','--format','{{.Names}}'],text=True,timeout=30).splitlines()
    volumes=subprocess.check_output(['docker','volume','ls','--format','{{.Name}}'],text=True,timeout=30).splitlines()
    return name not in containers and volume not in volumes

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--package',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--image',default=IMAGE);p.add_argument('--worker',action='store_true');a=p.parse_args()
    if a.worker:worker(a.output);return
    requested=Path(os.path.abspath(a.output))
    if os.path.lexists(requested) or not requested.parent.is_dir():p.error('output must be new with existing parent')
    if a.package is None:p.error('--package required')
    if not re.fullmatch('sha256:[0-9a-f]{64}',a.image):p.error('immutable already-local Docker image required')
    package=a.package.resolve();candidate=requested.parent.resolve()/requested.name
    if candidate==package or candidate.is_relative_to(package):p.error('output must be outside package')
    output=helpers.fresh_output(requested);e={'verified':False,'scope':'Private Linux exact-package C ABI fault qualification','commands':[]}
    name='weekend-delivered-faults-'+uuid.uuid4().hex;volume=name+'-data'
    try:
        info=verify(package)
        image=subprocess.check_output(['docker','image','inspect','--format','{{.Id}}',a.image],text=True,timeout=30).strip()
        if image!=a.image:raise ValueError('image mismatch')
        inputs=output/'inputs';inputs.mkdir()
        for n in ['consumer.c','faults.c','run.py','test_runner.py']:shutil.copy2(HERE/n,inputs/n)
        shutil.copy2(helper,inputs/'compatibility.py');shutil.copytree(package,output/'package')
        frozen=inventory(inputs);manifest=sha(package/'manifest.json')
        e.update(source=info['source'],image=image,container=name,volume=volume,inputs_sha256=frozen,package_manifest_sha256=manifest)
        try:
            subprocess.run(['docker','volume','create',volume],check=True,capture_output=True,timeout=30)
            argv=['docker','run','--pull','never','--name',name,'--network','none','--memory','2g','--cpus','2','--pids-limit','128','--ulimit','core=0','--tmpfs','/faults:rw,size=8m,mode=700','--mount','type=volume,source='+volume+',target=/work','--mount','type=bind,source='+str(output)+',target=/seed,readonly',image,'python3','/seed/inputs/run.py','--worker','--output','/work/result']
            helpers.run(argv,output,e,'docker',timeout=300)
        finally:
            try:subprocess.run(['docker','cp',name+':/work/result',str(output/'result')],check=True,capture_output=True,timeout=60)
            finally:e['owned_resources_absent']=cleanup(name,volume)
        d=json.loads((output/'result/evidence.json').read_text())
        if not d['verified'] or not e['owned_resources_absent']:raise ValueError('worker or cleanup unqualified')
        for name,digest in d['artifact_sha256'].items():
            if sha(output/'result'/name)!=digest:raise ValueError('copied proof mismatch: '+name)
        if d['inputs_sha256']!=frozen:raise ValueError('worker input mismatch')
        if inventory(inputs)!=frozen or sha(package/'manifest.json')!=manifest:raise ValueError('seed/package changed')
        verify(package);verify(output/'result/relocated-package')
        for mode,row in enumerate(d['results']):validate(row,mode)
        if len(d['results'])!=6:raise ValueError('missing cases')
        e.update(result_evidence_sha256=sha(output/'result/evidence.json'),verified=True)
    finally:(output/'evidence.json').write_text(json.dumps(e,indent=2)+'\n')
    print(json.dumps({'verified':True,'output':str(output)}))
if __name__=='__main__':main()
