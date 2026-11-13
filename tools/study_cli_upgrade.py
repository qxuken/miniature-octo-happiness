#!/usr/bin/env python3
"""Private pre-1 old/new StudyProgress CLI ALPN upgrade qualification."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
OLD = 'fb80989d98bd977743168ee10418a434a67a0261'
NEW = '365daf6'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(path):
    return {str(p.relative_to(path)): digest(p) for p in sorted(path.rglob('*')) if p.is_file()}


def clean_env():
    env = os.environ.copy()
    for key in list(env):
        if key not in ['RUSTUP_HOME', 'RUSTUP_TOOLCHAIN'] and (
            key.startswith(('RUST', 'CARGO_PROFILE_', 'CARGO_TARGET_')) or
            key in ['CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_RUSTFLAGS','CARGO_BUILD_RUSTC',
                    'CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER',
                    'CARGO_BUILD_TARGET','LD_PRELOAD','DYLD_INSERT_LIBRARIES','LLVM_PROFILE_FILE']):
            env.pop(key)
    env['CARGO_INCREMENTAL'] = '0'
    return env


def capture(argv, **kw):
    return subprocess.check_output([str(x) for x in argv], timeout=30, **kw)


def execute(argv, log, *, cwd=None, env=None, timeout=300):
    with log.open('x') as out:
        child = subprocess.Popen([str(x) for x in argv], cwd=cwd, env=env, stdout=out,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            status = child.wait(timeout=timeout)
            if status:
                raise RuntimeError(f'command failed ({status}), see {log.name}')
        finally:
            # The leader may already have exited while descendants remain alive.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            finally:
                child.wait(timeout=10)


def fixture(source, scenario):
    helpers = (source/'apps/study-peer/tests/credential_rotation.rs').read_text()
    boundary = '#[test]\nfn persistent_study_progress_survives_interrupted_credential_cutover()'
    if helpers.count(boundary) != 1:
        raise RuntimeError('helper extraction boundary differs')
    helpers = helpers.split(boundary)[0]
    old = '        log: &Log,\n    ) -> Self {'
    if helpers.count(old) != 1 or helpers.count('Command::new(BIN)') != 1:
        raise RuntimeError('helper executable seam differs')
    helpers = helpers.replace(old, '        (log, binary): (&Log, &str),\n    ) -> Self {')
    helpers = helpers.replace('Command::new(BIN)', 'Command::new(binary)')
    return '#![allow(dead_code)]\n' + helpers + scenario.read_text()


def worker(seed, work):
    work.mkdir()
    output = work/'result'; output.mkdir()
    env = clean_env()
    runtime = work/'runtime'; runtime.mkdir(mode=0o700)
    env['TMPDIR'] = str(runtime)
    record = {'verified':False, 'sources':{}, 'compiler':capture(['rustc','-vV'],env=env,text=True),
              'cargo':capture(['cargo','-V'],env=env,text=True),
              'python':capture(['python3','--version'],text=True),
              'filesystem':capture(['findmnt','-T',work,'-n','-o','FSTYPE,SOURCE,TARGET'] if os.uname().sysname=='Linux' else ['df','-P',work],text=True)}
    try:
        for label in ['old','new']:
            source = work/label
            shutil.copytree(seed/label, source)
            before = inventory(source)
            build = work/(label+'-build')
            runenv = {**env, 'CARGO_TARGET_DIR':str(build)}
            execute(['cargo','build','--offline','--locked','--release','-p','study-peer'],output/(label+'-build.log'),cwd=source,env=runenv)
            binary = build/'release/study-peer'
            shutil.copy2(binary, output/(label+'-study-peer'))
            record['sources'][label] = {'files':before, 'binary_sha256':digest(binary)}
            if before != inventory(source):
                raise RuntimeError('build changed archived inputs')
        source = work/'new'
        generated = source/'apps/study-peer/tests/cli_upgrade.rs'
        generated.write_text(fixture(source, seed/'study-cli-upgrade-fixture.rs'))
        record['fixture_sha256'] = digest(generated)
        runenv = {**env, 'CARGO_TARGET_DIR':str(work/'new-build'),
                  'STUDY_OLD_BINARY':str(output/'old-study-peer'),
                  'STUDY_NEW_BINARY':str(output/'new-study-peer'),
                  'STUDY_UPGRADE_EVIDENCE':str(output/'proof')}
        execute(['cargo','test','--offline','--locked','--release','-p','study-peer','--test','cli_upgrade','--','--nocapture'],output/'test.log',cwd=source,env=runenv,timeout=180)
        execute(['cargo','clippy','--offline','--locked','--release','-p','study-peer','--test','cli_upgrade','--','-D','warnings'],output/'clippy.log',cwd=source,env=runenv)
        proof = (output/'proof/events.log').read_text()
        for marker in ['PASS old-old', 'PASS reachable mixed-version ALPN', 'PASS complete causal records']:
            if marker not in proof:
                raise RuntimeError('missing scenario oracle')
        generated.unlink()
        for label in ['old','new']:
            if inventory(work/label) != record['sources'][label]['files']:
                raise RuntimeError('archived source changed')
            if digest(output/(label+'-study-peer')) != record['sources'][label]['binary_sha256']:
                raise RuntimeError('binary changed')
        if list(runtime.iterdir()):
            raise RuntimeError('private runtime cleanup incomplete')
        record['verified'] = True
    finally:
        # Owned runtime is outside copied proof; remove secrets even after a failed fixture.
        shutil.rmtree(runtime)
        record['artifacts'] = inventory(output)
        (output/'evidence.json').write_text(json.dumps(record,indent=2)+'\n')


def verify_copy(result, sources, expected_fixture):
    if any(p.is_symlink() for p in result.rglob('*')):
        raise RuntimeError('copied evidence contains symlink')
    proof = json.loads((result/'evidence.json').read_text())
    if proof.get('verified') is not True:
        raise RuntimeError('worker did not qualify')
    actual = inventory(result)
    actual.pop('evidence.json')
    if actual != proof.get('artifacts'):
        raise RuntimeError('copied artifact inventory differs')
    for label in ['old','new']:
        source = proof['sources'][label]
        if source['files'] != sources[label]['files']:
            raise RuntimeError('worker source differs from frozen seed')
        if source['binary_sha256'] != digest(result/(label+'-study-peer')):
            raise RuntimeError('copied binary differs')
    if proof['fixture_sha256'] != expected_fixture:
        raise RuntimeError('worker fixture differs')
    return digest(result/'evidence.json')


def cleanup(name, volume):
    for argv in [['docker','rm','-f',name], ['docker','volume','rm',volume]]:
        try:
            subprocess.run(argv, timeout=30, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        except (OSError, subprocess.SubprocessError):
            pass
    containers = capture(['docker','ps','-a','--format','{{.Names}}'],text=True).splitlines()
    volumes = capture(['docker','volume','ls','--format','{{.Name}}'],text=True).splitlines()
    if name in containers or volume in volumes:
        raise RuntimeError('owned resources remain')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path)
    parser.add_argument('--new',default=NEW)
    parser.add_argument('--docker-image')
    parser.add_argument('--worker',nargs=2)
    args = parser.parse_args()
    if args.worker:
        worker(Path(args.worker[0]),Path(args.worker[1])); return
    if args.output is None:
        parser.error('--output required')
    output = Path(os.path.abspath(args.output))
    if output.exists() or output.is_symlink() or not output.parent.is_dir():
        parser.error('output must be fresh with an existing parent')
    output.mkdir(mode=0o700)
    seed=output/'seed';seed.mkdir()
    record={'verified':False, 'source':{}, 'runner_sha256':digest(Path(__file__))}
    try:
        for label,revision in [('old',OLD),('new',args.new)]:
            exact=capture(['git','rev-parse',revision],cwd=ROOT,text=True).strip()
            archive=capture(['git','archive',exact],cwd=ROOT)
            (output/(label+'.tar')).write_bytes(archive)
            source=seed/label;source.mkdir()
            with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
                tar.extractall(source,filter='data')
            record['source'][label]={'revision':exact,'archive_sha256':hashlib.sha256(archive).hexdigest(),'files':inventory(source)}
        # The state/wire container model is identical; transport implementation is not.
        for path in ['examples/support/study.rs','src/crdt.rs', 'apps/study-peer/src/native/protocol.rs']:
            if (seed/'old'/path).read_bytes() != (seed/'new'/path).read_bytes():
                raise RuntimeError('unqualified state/wire source change: '+path)
        if inventory(seed/'old/src/crdt') != inventory(seed/'new/src/crdt'):
            raise RuntimeError('unqualified CRDT container change')
        for name in ['study_cli_upgrade.py','study-cli-upgrade-fixture.rs']:
            shutil.copy2(ROOT/'tools'/name,seed/name)
        record['fixture_source_sha256']=digest(seed/'study-cli-upgrade-fixture.rs')
        if args.docker_image:
            image=capture(['docker','image','inspect','--format','{{.Id}}',args.docker_image],text=True).strip()
            name='study-upgrade-'+uuid.uuid4().hex;volume=name+'-data'
            record.update(image=image,container=name,volume=volume)
            try:
                capture(['docker','volume','create',volume])
                execute(['docker','run','--name',name,'--network','none','--cpus','2','--memory','3g','--pids-limit','256',
                         '--mount',f'type=bind,src={seed},dst=/seed,readonly',
                         '--mount',f'type=bind,src={Path.home()}/.cargo/registry,dst=/usr/local/cargo/registry,readonly',
                         '--mount',f'type=volume,src={volume},dst=/data',image,
                         'python3','/seed/study_cli_upgrade.py','--worker','/seed','/data/work'],output/'docker.log',timeout=900)
            finally:
                try:
                    subprocess.run(['docker','cp',name+':/data/work/result',str(output/'result')],timeout=60,check=False,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                finally:
                    cleanup(name,volume)
                record['owned_resources_absent']=True
        else:
            worker(seed,output/'work')
            shutil.copytree(output/'work/result',output/'result')
        for label in ['old','new']:
            if inventory(seed/label) != record['source'][label]['files']:
                raise RuntimeError('frozen seed changed')
        expected_fixture=hashlib.sha256(fixture(seed/'new',seed/'study-cli-upgrade-fixture.rs').encode()).hexdigest()
        record['result_sha256']=verify_copy(output/'result',record['source'],expected_fixture)
        record['verified']=True
    finally:
        (output/'evidence.json').write_text(json.dumps(record,indent=2)+'\n')

if __name__=='__main__':
    main()
