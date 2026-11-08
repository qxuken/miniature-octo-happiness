#!/usr/bin/env python3
"""Build pinned engine sources and test a Linux server with a macOS client.

Fresh synthetic data only. Private Docker volume and container are always removed.
The temporary client test listener is reachable locally; no production auth claim.
"""
from pathlib import Path
import argparse,hashlib,json,os,shutil,socket,subprocess,sys,tarfile,tempfile,uuid
root=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description="Run an accelerated month with a native macOS client and Linux Docker server")
parser.add_argument('--directory',type=Path,required=True,help='New evidence directory')
parser.add_argument('--source',default='HEAD',help='Engine Git revision to archive and build')
parser.add_argument('--image',default='rust:1-bookworm')
args=parser.parse_args()
if sys.platform!='darwin':parser.error('This qualification pairs a native macOS client with a Linux container')
identity=subprocess.check_output(['git','rev-parse',args.source+'^{commit}'],cwd=root,text=True).strip()
def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0));return sock.getsockname()[1]
server_port=free_port();client_port=free_port()
while client_port==server_port:client_port=free_port()
name='weekend-month-'+uuid.uuid4().hex[:10]
volume=name+'-data'
output=args.directory.resolve()
if output.exists():parser.error('Evidence directory must not already exist')
output.parent.mkdir(parents=True,exist_ok=True)
scratch=Path(tempfile.mkdtemp(prefix='weekend-cross-month-'))
source=scratch/'source';source.mkdir()
archive=scratch/'source.tar'
with archive.open('wb') as f:
    subprocess.run(['git','archive',identity],cwd=root,stdout=f,check=True)
with tarfile.open(archive) as t:t.extractall(source,filter='data')
files=['tools/monthly_workload.rs','tools/monthly_workload.py','tools/monthly-workload/Cargo.toml','tools/monthly-workload/Cargo.lock','tools/monthly-workload/.gitignore']
for f in files:
    dest=source/f;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(root/f,dest)
with tarfile.open(archive,'a') as t:
    for f in files:t.add(source/f,arcname=f)
env={**os.environ,'WEEKEND_DB_SOURCE_ID':identity}
subprocess.run(['cargo','build','--offline','--locked','--release','--manifest-path',str(source/'tools/monthly-workload/Cargo.toml')],env=env,check=True,timeout=180)
try:
    subprocess.run(['docker','run','-d','--rm','--name',name,'--memory=2g','--pids-limit=256','-p',f'127.0.0.1:{server_port}:{server_port}','--mount',f'type=volume,source={volume},target=/data','--mount',f'type=bind,source={Path.home()/".cargo/registry"},target=/usr/local/cargo/registry,readonly',args.image,'tail','-f','/dev/null'],check=True)
    subprocess.run(['docker','exec',name,'mkdir','/work'],check=True)
    with archive.open('rb') as f:subprocess.run(['docker','exec','-i',name,'tar','-xf','-','-C','/work'],stdin=f,check=True)
    subprocess.run(['docker','exec','-e',f'WEEKEND_DB_SOURCE_ID={identity}',name,'cargo','build','--offline','--locked','--release','--manifest-path','/work/tools/monthly-workload/Cargo.toml','--target-dir','/build'],check=True,timeout=180)
    platform_info=subprocess.check_output(['docker','exec',name,'sh','-c','uname -a; rustc -Vv; findmnt -T /data -o SOURCE,FSTYPE,OPTIONS; sha256sum /build/release/monthly-workload'],text=True)
    print(platform_info,flush=True)
    subprocess.run(['python3',str(root/'tools/monthly_workload.py'),'--directory',str(output),'--binary',str(source/'tools/monthly-workload/target/release/monthly-workload'),'--engine-source',identity,'--server-command',json.dumps(['docker','exec',name,'/build/release/monthly-workload']),'--server-db','/data/server.wdb','--server-listen',f'0.0.0.0:{server_port}','--server-address',f'127.0.0.1:{server_port}','--client-listen',f'0.0.0.0:{client_port}','--client-address',f'host.docker.internal:{client_port}'],check=True,timeout=900)
    (output/'linux-platform.txt').write_text(platform_info)
    shutil.copyfile(archive,output/'qualification-source.tar')
    (output/'qualification-source.json').write_text(json.dumps({'engine':identity,'source_tar_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'driver_sha256':{f:hashlib.sha256((source/f).read_bytes()).hexdigest() for f in files},'docker_image':subprocess.check_output(['docker','image','inspect',args.image,'--format','{{.Id}}'],text=True).strip()},indent=2)+'\n')
    subprocess.run(['docker','cp',f'{name}:/data',str(output/'linux-data')],check=True)
    print('CROSS_PLATFORM_MONTH_PASSED',output,flush=True)
finally:
    subprocess.run(['docker','rm','-f',name],check=False)
    subprocess.run(['docker','volume','rm',volume],check=False)
    shutil.rmtree(scratch)
