#!/usr/bin/env python3
"""Private tmpfs maintenance ENOSPC controller. No host filesystem is filled."""
import argparse
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import uuid

HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[1]
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
Q=load('space_quota',HERE/'run.py');M=load('space_maintenance',HERE/'maintenance.py');C=M.C
OVERLAYS=[*C.OVERLAYS,*['tools/quota/'+n for n in ['enospc.py','maintenance.py','probe.py','run.py','test_maintenance.py']]]

def worker(source,output,revision):
    source,output=Path(source).resolve(),Path(output).absolute();output.mkdir();logs=output/'logs';logs.mkdir()
    e={'verified':False,'commands':[],'source':revision,'source_hashes':C.SCALE.inventory(source)}
    env=C.SCALE.clean_env();env.update(WEEKEND_DB_SOURCE_ID=revision,CARGO_TARGET_DIR='/data/build',PYTHONDONTWRITEBYTECODE='1')
    for n in ['LD_PRELOAD','LD_LIBRARY_PATH','PYTHONPATH','PYTHONHOME']:env.pop(n,None)
    def call(argv,timeout=30,env=env,allowed=(0,)):return Q.invoke(argv,logs,e,'command-'+str(len(e['commands'])+1),timeout=timeout,env=env,allowed=allowed)
    try:
        e['rustc']=call(['rustc','-vV']);e['kernel']=call(['uname','-a'])
        e['filesystem']=call(['findmnt','-T','/faults','-n','-o','FSTYPE,SIZE,OPTIONS'])
        C.require(e['filesystem'].split()[0]=='tmpfs'and int(os.statvfs('/faults').f_blocks)*os.statvfs('/faults').f_frsize==256*M.MIB,'private 256 MiB tmpfs required')
        call(['cargo','build','--offline','--locked','--release','--manifest-path',source/'tools/resource-envelope/Cargo.toml','--features','compound','--bin','weekend-compound-resource'],timeout=120)
        guest=Path('/data/guest');guest.mkdir(mode=0o711)
        fixture=guest/'fixture';shutil.copyfile('/data/build/release/weekend-compound-resource',fixture);fixture.chmod(0o555);e['fixture_sha256']=Q.sha(fixture)
        probe=guest/'probe.py';shutil.copyfile(source/'tools/quota/probe.py',probe);probe.chmod(0o444)
        uid=60000;parent=guest/'seed-parent';parent.mkdir(mode=0o700);os.chown(parent,uid,uid)
        data=Path('/faults/data');data.mkdir(mode=0o700);os.chown(data,uid,uid)
        drop=['setpriv','--reuid',str(uid),'--regid',str(uid),'--clear-groups','--inh-caps=-all','--ambient-caps=-all','--bounding-set=-all','--no-new-privs']
        python=shutil.which('python3');e['guest_python']={'path':python,'sha256':Q.sha(Path(python).resolve())}
        e['_logs']=str(logs)
        try:images=M.exercise(call,drop,fixture,data,parent/'seed',probe,python,uid,env,revision,e,28,Q.validate_probe)
        finally:del e['_logs']
        snapshots=output/'images';snapshots.mkdir()
        for p in images:shutil.copyfile(p,snapshots/p.name)
        C.require(Q.sha(fixture)==e['fixture_sha256'],'fixture changed')
        C.require(C.SCALE.inventory(source)==e['source_hashes'],'source changed')
        e['verified']=True
    finally:
        e['artifacts']={str(p.relative_to(output)):Q.sha(p)for p in output.rglob('*')if p.is_file()};(output/'evidence.json').write_text(json.dumps(e,indent=2)+'\n')

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--revision');parser.add_argument('--output');parser.add_argument('--worker',nargs=3)
    args=parser.parse_args()
    if args.worker:worker(*args.worker);return
    C.require(args.revision and args.output,'explicit revision and fresh output required');out=C.BASE.HELPERS.fresh_output(args.output);e={'verified':False}
    try:
        rev=C.SCALE.command(['git','rev-parse','--verify',args.revision+'^{commit}'],cwd=ROOT,text=True).stdout.strip()
        archive=C.SCALE.command(['git','archive',rev],cwd=ROOT).stdout;(out/'source.tar').write_bytes(archive);src=out/'source';src.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive))as tar:tar.extractall(src,filter='data')
        for n in OVERLAYS:p=src/n;p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(ROOT/n,p)
        e.update(source=rev,source_hashes=C.SCALE.inventory(src),archive_sha256=Q.sha(out/'source.tar'),overlays={n:Q.sha(ROOT/n)for n in OVERLAYS})
        image='sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08';name='maintenance-space-'+uuid.uuid4().hex;volume=name+'-data';e.update(image=image,container=name,volume=volume)
        try:
            C.SCALE.command(['docker','volume','create',volume])
            argv=['docker','run','--name',name,'--network','none','--memory','2g','--cpus','2','--pids-limit','256','--tmpfs','/faults:rw,size=256m,mode=755','--mount',f'type=volume,src={volume},dst=/data','--mount',f'type=bind,src={src},dst=/src,readonly',image,'python3','/src/tools/quota/enospc.py','--worker','/src','/data/run',rev]
            import supervision
            argv=supervision.docker_command(argv,image,out)
            with (out/'docker.log').open('x')as log:result=subprocess.run(argv,stdout=log,stderr=subprocess.STDOUT,timeout=600)
        finally:
            try:copied=subprocess.run(['docker','cp',name+':/data/run',str(out/'run')],capture_output=True,text=True,timeout=120);e['copy']={'exit':copied.returncode,'stderr':copied.stderr}
            finally:
                e['cleanup']=C.SCALE.cleanup_docker(name,volume);C.require(e['cleanup']['container_absent']and e['cleanup']['volume_absent'],'Docker cleanup not verified')
        result.check_returncode();C.require(copied.returncode==0,'incomplete proof copy');run=json.loads((out/'run/evidence.json').read_text());C.require(run['verified'],'unverified worker')
        for n,h in run['artifacts'].items():C.require(Q.sha(out/'run'/n)==h,'copied proof changed')
        C.require(run['source_hashes']==e['source_hashes']==C.SCALE.inventory(src),'source changed')
        e.update(verified=True,run_sha256=Q.sha(out/'run/evidence.json'))
    finally:(out/'evidence.json').write_text(json.dumps(e,indent=2)+'\n')

if __name__=='__main__':main()
