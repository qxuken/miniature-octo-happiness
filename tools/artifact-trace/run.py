#!/usr/bin/env python3
"""Private, bounded, exact-package operation trace qualification. No network or publication."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import subprocess
import sys
import time
import tomllib
import uuid
import model

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
TIMEOUT=300

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inventory(root):
    return {p.relative_to(root).as_posix():sha(p) for p in sorted(root.rglob('*')) if p.is_file()}

def fresh_output(path):
    # Check the caller's final entry before resolving symlinks in its parent.
    original=Path(os.path.abspath(path))
    if os.path.lexists(original): raise FileExistsError('output entry already exists: '+str(original))
    return original.parent.resolve()/original.name

def verify(package):
    manifest=package/'manifest.json'
    info=json.loads(manifest.read_text())
    hashes=info['sha256']
    if not info.get('verified') or not hashes or not re.fullmatch('[0-9a-f]{40}',info['source']) or info.get('source_id')!=info['source']:
        raise ValueError('package is not a verified exact-source package')
    files=set()
    for p in package.rglob('*'):
        if p.is_symlink(): raise ValueError('package symlink rejected')
        if p.is_file(): files.add(p.relative_to(package).as_posix())
    if files != set(hashes)|{'manifest.json'}:
        raise ValueError('package inventory mismatch')
    for name,digest in hashes.items():
        path=Path(name)
        if path.is_absolute() or '..' in path.parts or sha(package/path)!=digest:
            raise ValueError('package hash/path mismatch: '+name)
    return info

def clean_env(source):
    env=dict(os.environ)
    for key in list(env):
        if key!='RUSTUP_HOME' and (key.startswith(('RUST','CARGO_BUILD_','CARGO_TARGET_','CARGO_PROFILE_')) or key in ['CARGO_ENCODED_RUSTFLAGS','LLVM_PROFILE_FILE','LD_PRELOAD','DYLD_INSERT_LIBRARIES','DYLD_LIBRARY_PATH','LD_LIBRARY_PATH','CARGO_HOME']):
            env.pop(key)
    env.update(CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true',WEEKEND_DB_SOURCE_ID=source)
    return env

def run(command,out,label,env=None,cwd=None,allowed_codes=(0,)):
    """Each child has a deadline and its own process group; logs never mix runs."""
    command=list(map(str,command))
    stdout=out/(label+'.stdout');stderr=out/(label+'.stderr')
    started=time.monotonic()
    with stdout.open('xb') as so,stderr.open('xb') as se:
        def limits():
            resource.setrlimit(resource.RLIMIT_FSIZE,(64*1024*1024,64*1024*1024))
        process=subprocess.Popen(command,stdout=so,stderr=se,env=env,cwd=cwd,start_new_session=True,preexec_fn=limits)
        try: code=process.wait(timeout=TIMEOUT)
        except BaseException:
            os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=30);raise
    record={'command':command,'exit':code,'seconds':time.monotonic()-started,
            'stdout_sha256':sha(stdout),'stderr_sha256':sha(stderr)}
    (out/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
    if stdout.stat().st_size>16*1024*1024 or stderr.stat().st_size>16*1024*1024:
        raise RuntimeError('command log exceeded 16 MiB')
    if code not in allowed_codes: raise RuntimeError(f'{label} failed ({code}): {stderr}')
    return stdout.read_text()

def cleanup_commands(container,volume):
    match=re.fullmatch(r'weekend-trace-([0-9a-f]{32})',container)
    if not match or volume!='weekend-trace-data-'+match[1]:
        raise ValueError('cleanup requires a matching private UUID container/volume pair')
    return [['docker','rm','-f',container],['docker','volume','rm',volume]]

def cleanup(container,volume,logs):
    for index,command in enumerate(cleanup_commands(container,volume)):
        run(command,logs,'cleanup-'+str(index),allowed_codes=(0,1))
    for kind,name in [('container',container),('volume',volume)]:
        label='absence-'+kind
        run(['docker',kind,'inspect',name],logs,label,allowed_codes=(0,1))
        record=json.loads((logs/(label+'.json')).read_text())
        error=(logs/(label+'.stderr')).read_text().lower()
        if record['exit']!=1 or 'no such' not in error:
            raise RuntimeError('cannot verify removal of private '+kind)

def docker_local(out,image,source_id,evidence):
    logs=out/'docker-logs';logs.mkdir()
    nonce=uuid.uuid4().hex;name='weekend-trace-'+nonce;volume='weekend-trace-data-'+nonce
    image_id=run(['docker','image','inspect','--format={{.Id}}',image],logs,'image-inspect').strip()
    if image_id!=image: raise ValueError('Docker image identity mismatch')
    source=out/'relocated-source/rust-source'
    run(['cargo','vendor','--offline','--locked','--manifest-path',source/'examples/rust_package/Cargo.toml',out/'vendor'],logs,'vendor',clean_env(source_id),cwd=source)
    seed=out/'seed';seed.mkdir()
    for item in ['inputs','context.json','trace.bin','expected.txt','relocated-package','relocated-source','vendor']:
        path=out/item
        if path.is_dir(): shutil.copytree(path,seed/item)
        else: shutil.copy2(path,seed/item)
    (seed/'seed-manifest.json').write_text(json.dumps(inventory(seed),indent=2)+'\n')
    evidence.update(docker_image=image_id,docker_container=name,docker_volume=volume,native_evidence_root='linux-result',linux_profile='private-ext4-volume')
    try:
        run(['docker','volume','create',volume],logs,'volume-create')
        try:
            run(['docker','run','--pull','never','--name',name,'--network','none','--memory=2g','--cpus=2','--pids-limit=256',
                 '--env','PYTHONDONTWRITEBYTECODE=1','--env','TMPDIR=/private-volume/tmp','--workdir','/private-volume',
                 '--mount',f'type=bind,source={seed},target=/seed,readonly','--mount',f'type=volume,source={volume},target=/private-volume',
                 image_id,'python3','/seed/inputs/docker_worker.py',source_id],logs,'linux')
        finally:
            # Stopped containers retain their volume proofs until this fresh copy.
            run(['docker','container','inspect',name],logs,'container-state',allowed_codes=(0,1))
            state=json.loads((logs/'container-state.json').read_text())
            if state['exit']==0:
                if (out/'linux-result').exists(): raise ValueError('copyback destination already exists')
                run(['docker','cp',name+':/private-volume/run/.',out/'linux-result'],logs,'copyback')
    finally:
        cleanup(name,volume,logs)
        evidence['docker_cleanup_verified']=True
    result=json.loads((out/'linux-result/native-result.json').read_text())
    if result['original_checkout_available']: raise ValueError('Docker can access the original checkout')
    for package in ['relocated-package','relocated-source']: verify(out/'linux-result'/package)
    frozen=json.loads((seed/'seed-manifest.json').read_text())
    if {k:v for k,v in inventory(seed).items() if k!='seed-manifest.json'}!=frozen:
        raise ValueError('readonly host seed changed')
    for name,digest in frozen.items():
        if sha(out/'linux-result'/name)!=digest: raise ValueError('copyback frozen input mismatch: '+name)
    return result

def local(out,package,source,source_id,odin):
    env=clean_env(source_id)
    logs=out/'logs';logs.mkdir()
    library=package/('libweekend_db.dylib' if sys.platform=='darwin' else 'libweekend_db.so')
    if not library.is_file(): raise ValueError('package is not for the executing native platform')
    # The copied inventory remains immutable; the explicit driver overlay lives separately.
    build_source=out/'source-overlay';shutil.copytree(source/'rust-source',build_source)
    if (out/'vendor').is_dir():
        config=build_source/'.cargo/config.toml'
        if config.exists(): raise ValueError('vendor config would overwrite packaged config')
        config.parent.mkdir(exist_ok=True)
        config.write_text('[source.crates-io]\nreplace-with = "trace-vendor"\n[source.trace-vendor]\ndirectory = '+json.dumps(str(out/'vendor'))+'\n')
    driver=build_source/'examples/rust_package/src/main.rs'
    original_main=sha(driver);shutil.copyfile(out/'inputs/rust/src/main.rs',driver)
    for manifest in build_source.rglob('Cargo.toml'):
        data=tomllib.loads(manifest.read_text())
        if 'package' in data and data['package'].get('publish') is not False:
            raise ValueError('all packaged Rust crates must be private')
    lock=build_source/'examples/rust_package/Cargo.lock';original_lock=sha(lock)
    manifest=build_source/'examples/rust_package/Cargo.toml'
    run(['rustc','-Vv'],logs,'rustc-version',env)
    run(['cargo','-V'],logs,'cargo-version',env)
    run(['cc','--version'],logs,'cc-version',env)
    run([sys.executable,'--version'],logs,'python-version',env)
    metadata=json.loads(run(['cargo','metadata','--offline','--locked','--format-version=1','--manifest-path',manifest],logs,'metadata',env,cwd=build_source))
    for p in metadata['packages']:
        if p['source'] is None and (not Path(p['manifest_path']).resolve().is_relative_to(build_source) or p.get('publish')!=[]):
            raise ValueError('Rust dependency escapes delivered source or is publishable')
    target=out/'cargo-target'
    run(['cargo','build','--release','--locked','--offline','--manifest-path',manifest,'--target-dir',target],logs,'build-rust',env,cwd=build_source)
    if sha(lock)!=original_lock: raise ValueError('locked dependency inventory changed')
    original_root=Path(json.loads((out/'context.json').read_text())['original_checkout'])
    for dep in target.rglob('*.d'):
        text=dep.read_text().replace('\\ ',' ')
        # Match only the original checkout's production directories; the isolated
        # output itself is allowed to be under that checkout's ignored target/.
        for name in ['src/','crates/','examples/']:
            if str(original_root/name)+'/' in text:
                raise ValueError('compiler consumed original checkout: '+str(dep))
    binary=out/'consumer-rust';shutil.copy2(target/'release/private-rust-consumer',binary)
    c_binary=out/'consumer-c'
    rpath='@loader_path/relocated-package' if sys.platform=='darwin' else '$ORIGIN/relocated-package'
    run(['cc','-std=c11','-O2','-UNDEBUG','-Wall','-Wextra','-Werror','-I',package,out/'inputs/consumer.c',library,'-Wl,-rpath,'+rpath,'-o',c_binary],logs,'build-c',env)
    binaries=[binary,c_binary]
    if odin:
        if not (package/'bindings/odin/weekend_db.odin').is_file(): raise ValueError('Odin bindings absent in exact package')
        run([odin,'version'],logs,'odin-version',env)
        help_text=run([odin,'build','-help'],logs,'odin-build-help',env)
        if '-o:speed' not in help_text: raise ValueError('installed Odin does not document speed optimization')
        executable=out/'consumer-odin'
        relative=os.path.relpath(library,package/'bindings/odin')
        run([odin,'build',out/'inputs/odin','-o:speed','-out:'+str(executable),'-collection:weekend='+str(package/'bindings'),'-define:WEEKEND_DB_LIBRARY='+relative,'-extra-linker-flags:-Wl,-rpath,'+rpath],logs,'build-odin',env)
        binaries.append(executable)
    expected=(out/'expected.txt').read_text()
    for binary in binaries:
        runtime_env=dict(env)
        if binary.name!='consumer-rust':
            runtime_env['DYLD_PRINT_LIBRARIES' if sys.platform=='darwin' else 'LD_DEBUG']='1' if sys.platform=='darwin' else 'libs'
        actual=run([binary,out/'trace.bin',out/(binary.name+'.db'),source_id],logs,'run-'+binary.name,runtime_env,cwd=out)
        if actual!=expected:
            raise ValueError('model mismatch: '+binary.name)
        if binary.name!='consumer-rust':
            loader=(logs/('run-'+binary.name+'.stderr')).read_text()
            loaded=str(library) if sys.platform=='darwin' else 'calling init: '+str(library)
            if loaded not in loader: raise ValueError('runtime loader did not confirm relocated library: '+binary.name)
            run(['otool','-L',binary] if sys.platform=='darwin' else ['readelf','-d',binary],logs,'dependencies-'+binary.name,env)
    return {'languages':[b.name.removeprefix('consumer-') for b in binaries],
            'consumer_profiles':{'rust':'cargo --release; runtime assert! enabled','c':'-O2 -UNDEBUG; assert enabled',**({'odin':'-o:speed; ODIN_DISABLE_ASSERT=false'} if odin else {})},
            'tool_binary_sha256':{name:sha(Path(shutil.which(name)).resolve()) for name in ['rustc','cargo','cc',sys.executable,*([str(odin)] if odin else [])]},
            'original_checkout_available':original_root.exists(),
            'overlay':{'path':'examples/rust_package/src/main.rs','original_sha256':original_main,'driver_sha256':sha(driver),'original_lock_sha256':original_lock},
            'consumer_sha256':{b.name:sha(b) for b in binaries},'library_sha256':sha(library)}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package',type=Path,required=True)
    parser.add_argument('--source-package',type=Path,help='Exact same-source package carrying rust-source (defaults to --package)')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--odin',type=Path)
    parser.add_argument('--docker-image',help='Immutable already-local Linux image ID; network is disabled')
    args=parser.parse_args()
    out=fresh_output(args.output)
    package=args.package.resolve();source=(args.source_package or args.package).resolve()
    if out==package or out.is_relative_to(package) or out==source or out.is_relative_to(source): parser.error('output must be outside inputs')
    package_info=verify(package);source_info=verify(source)
    if package_info['source']!=source_info['source'] or package_info['source_id']!=source_info['source_id']:
        raise ValueError('native/source package identities differ')
    if not (source/'rust-source').is_dir(): raise ValueError('source package has no exact Rust source')
    out.mkdir(parents=True,exist_ok=False)
    evidence={'verified':False,'schema':1,'source':package_info['source'],'source_id':package_info['source_id'],
              'package_manifest_sha256':sha(package/'manifest.json'),'source_manifest_sha256':sha(source/'manifest.json'),
              'trace_cells':len(model.TRACE),'native_windows_verified':False,
              'tool_source_sha256_before':{k:v for k,v in inventory(HERE).items() if '__pycache__/' not in k}}
    try:
        shutil.copytree(HERE,out/'inputs',ignore=shutil.ignore_patterns('__pycache__'))
        (out/'context.json').write_text(json.dumps({'original_checkout':str(ROOT)})+'\n')
        (out/'trace.bin').write_bytes(model.encode());(out/'expected.txt').write_text(model.expected(model.decode(model.encode())))
        shutil.copytree(package,out/'staged-package');(out/'staged-package').rename(out/'relocated-package')
        shutil.copytree(source,out/'staged-source');(out/'staged-source').rename(out/'relocated-source')
        relocated=out/'relocated-package';relocated_source=out/'relocated-source'
        verify(relocated);verify(relocated_source)
        if args.docker_image:
            if args.odin: raise ValueError('Linux Odin is not qualified by this runner')
            if not re.fullmatch(r'sha256:[0-9a-f]{64}',args.docker_image): raise ValueError('immutable Docker image ID required')
            evidence.update(docker_local(out,args.docker_image,package_info['source_id'],evidence))
        else:
            evidence['native_evidence_root']='.'
            evidence.update(local(out,relocated,relocated_source,package_info['source_id'],args.odin))
        verify(relocated);verify(relocated_source);verify(package);verify(source)
        if sha(package/'manifest.json')!=evidence['package_manifest_sha256'] or sha(source/'manifest.json')!=evidence['source_manifest_sha256']:
            raise ValueError('input manifest changed during run')
        evidence['tool_source_sha256_after']={k:v for k,v in inventory(HERE).items() if '__pycache__/' not in k}
        if evidence['tool_source_sha256_after']!=evidence['tool_source_sha256_before']:
            raise ValueError('tool source changed during qualification')
        native=out/evidence['native_evidence_root']
        evidence['transcript_sha256']={language:sha(native/'logs'/('run-consumer-'+language+'.stdout')) for language in evidence['languages']}
        if set(evidence['transcript_sha256'].values())!={sha(out/'expected.txt')}:
            raise ValueError('copied transcript mismatch')
        evidence['verified']=True
    finally:
        # Build products remain reviewable, but are omitted from the compact
        # evidence hash inventory; consumers, sources, inputs and logs are hashed.
        evidence['sha256']={k:v for k,v in inventory(out).items() if 'cargo-target/' not in k and k!='evidence.json'}
        (out/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps({'verified':True,'output':str(out),'languages':evidence['languages']}))

if __name__=='__main__': main()
