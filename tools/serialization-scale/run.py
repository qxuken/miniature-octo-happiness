#!/usr/bin/env python3
"""Bounded paired serialization experiment; never edits the production checkout."""
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
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
FILES = ['Cargo.toml', 'Cargo.lock', 'src/main.rs', 'run.py', 'test_runner.py']

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inventory(root):
    return {str(p.relative_to(root)): sha(p) for p in sorted(root.rglob('*')) if p.is_file()}

def command(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, capture_output=True, timeout=kwargs.pop("timeout", 120), **kwargs)

def clean_env():
    env = dict(os.environ)
    for key in list(env):
        if key not in ['RUSTUP_HOME', 'RUSTUP_TOOLCHAIN'] and (key.startswith(('RUST', 'CARGO_PROFILE_', 'CARGO_TARGET_')) or key in ['CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_BUILD_RUSTC', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_TARGET', 'LLVM_PROFILE_FILE', 'LD_PRELOAD', 'DYLD_INSERT_LIBRARIES']):
            env.pop(key)
    env['CARGO_INCREMENTAL'] = '0'
    return env

def instrument(root):
    path = root / 'src/storage/codec.rs'
    text = path.read_text()
    if 'SERIAL_VALUE.fetch_add(' in text or 'SERIAL_DATA.fetch_add(' in text:
        raise RuntimeError('instrumentation already applied')
    data_sites = [
        ('.find(|r| r.revision == head.data_revision)', '.find(|r| { SERIAL_DATA.fetch_add(1, std::sync::atomic::Ordering::Relaxed); r.revision == head.data_revision })'),
        ('fn compare(a: u64, b: u64) -> std::cmp::Ordering {\n        a.cmp(&b)\n    }',
         'fn compare(a: u64, b: u64) -> std::cmp::Ordering {\n        SERIAL_DATA.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n        a.cmp(&b)\n    }'),
    ]
    matching = [site for site in data_sites if site[0] in text]
    if len(matching) != 1:
        raise RuntimeError('expected exactly one linear or indexed DATA comparison site')
    replacements = [
        ('old.value.same_node(value)', '{ SERIAL_VALUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed); old.value.same_node(value) }'),
        matching[0],
    ]
    for old, new in replacements:
        if text.count(old) != 1:
            raise RuntimeError('instrumentation source pattern is not unique: ' + old)
        text = text.replace(old, new)
    text += '\nstatic SERIAL_VALUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);\nstatic SERIAL_DATA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);\npub(super) fn serialization_probe_counts() -> (u64,u64) { (SERIAL_VALUE.load(std::sync::atomic::Ordering::Relaxed), SERIAL_DATA.load(std::sync::atomic::Ordering::Relaxed)) }\n'
    path.write_text(text)
    path = root / 'src/storage/mod.rs'
    path.write_text(path.read_text() + '\n/// Disposable experiment only; not a shipped API.\npub fn serialization_probe_counts() -> (u64,u64) { codec::serialization_probe_counts() }\n')

def validate_events(events, shape, width, roots=None):
    limited = roots is None and shape == 'distinct' and width == 2048
    epochs = roots if roots is not None else (7 if limited else 8)
    phases = ['create'] + ['commit', 'retain', 'verify'] * epochs
    if limited:
        phases += ['commit', 'verify-limit']
    phases += ['reopen', 'verify-reopen', 'backup', 'restore',
               'reopen-copy', 'verify-copy', 'reopen-copy', 'verify-copy',
               'compact', 'reopen-compact', 'verify-compact']
    if len(events) != 2 * len(phases) + 1:
        raise RuntimeError('missing or extra workload phases')
    for i, phase in enumerate(phases):
        start, end = events[2*i:2*i+2]
        if start != {'event': 'start', 'phase': phase} or set(end) != {'event', 'phase', 'ns'} or end['event'] != 'end' or end['phase'] != phase or type(end['ns']) is not int or end['ns'] < 0:
            raise RuntimeError('invalid workload phase sequence')
    final = events[-1]
    expected = {'event': 'complete', 'verified': True, 'width': width, 'epochs': epochs,
                'shared': shape == 'shared', 'limited_at': 8 if limited else 0}
    for key, value in expected.items():
        if type(final.get(key)) is not type(value) or final[key] != value:
            raise RuntimeError('unexpected completed workload outcome: ' + key)
    if limited:
        headroom = final.get('limit_headroom')
        if type(headroom) is not int or not 0 <= headroom < width * 4096:
            raise RuntimeError('missing independent image exhaustion evidence')
    return final


def observe(binary, shape, width, directory, env, roots=None):
    samples = []
    stop = threading.Event()
    start = time.monotonic()
    log = directory.with_suffix('.jsonl')
    with log.open('w') as output:
        argv = [str(binary), shape, str(width), str(directory)] + ([] if roots is None else [str(roots)])
        process = subprocess.Popen(argv, stdout=output,
                                   stderr=subprocess.STDOUT, env=env, start_new_session=True)
        def sample():
            while not stop.is_set():
                try:
                    if os.uname().sysname == 'Linux':
                        text = Path(f'/proc/{process.pid}/status').read_text()
                        rss = int(next(line.split()[1] for line in text.splitlines() if line.startswith('VmRSS:'))) * 1024
                    else:
                        rss = int(command(['ps', '-o', 'rss=', '-p', process.pid], text=True, timeout=5).stdout.strip()) * 1024
                    samples.append({'seconds': time.monotonic() - start, 'rss_bytes': rss})
                except (OSError, ValueError, StopIteration, subprocess.SubprocessError):
                    pass
                stop.wait(0.02)
        sampler = threading.Thread(target=sample, daemon=True)
        sampler.start()
        try:
            status = process.wait(timeout=180)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=10)
            raise RuntimeError('bounded workload timed out')
        finally:
            stop.set()
            sampler.join(timeout=10)
    if status:
        raise RuntimeError(f'workload exited {status}: {log}')
    events = [json.loads(line) for line in log.read_text().splitlines()]
    validate_events(events, shape, width, roots)
    if not samples:
        raise RuntimeError(f'workload/oracle/RSS failed: {log}')
    return {'shape': shape, 'width': width, 'requested_roots': roots, 'exit': status, 'elapsed_seconds': time.monotonic()-start,
            'events': events, 'rss_samples': samples, 'sampled_peak_rss_bytes': max(s['rss_bytes'] for s in samples),
            'log_sha256': sha(log)}

def worker(source, output, axis="breadth"):
    cases = [(8, s, r) for r in [1,8,32,64] for s in ["distinct","shared"]] if axis == "history" else [(w,s,None) for w in [128,512,2048] for s in ["distinct","shared"]]
    output.mkdir()
    env = clean_env()
    evidence = {'verified': False, 'source_hashes': inventory(source), 'runs': [],
                'rustc': command(['rustc', '-vV'], text=True, env=env).stdout,
                'cargo': command(['cargo', '-V'], text=True, env=env).stdout,
                'platform': list(os.uname()), 'axis': axis, 'scope': 'paired fixed breadth/revisions; sampled process RSS, not a maximum bound'}
    evidence['filesystem'] = command(['findmnt','-T',output,'-n','-o','FSTYPE,SOURCE,TARGET'] if os.uname().sysname == 'Linux' else ['df','-P',output], text=True).stdout
    evidence['python'] = command(['python3','--version'], text=True).stdout
    configs = set()
    for start in [source, Path.cwd(), Path(env.get('CARGO_HOME', str(Path.home()/'.cargo')))]:
        for parent in [start, *start.parents]:
            for candidate in [parent/'.cargo/config', parent/'.cargo/config.toml', parent/'config', parent/'config.toml']:
                if candidate.is_file(): configs.add(candidate)
    evidence['cargo_configuration_sha256'] = {str(p):sha(p) for p in sorted(configs)}
    evidence['removed_environment_names'] = sorted(set(os.environ) - set(env))
    try:
        for variant in ['clean', 'instrumented']:
            copy = output / variant / 'source'
            shutil.copytree(source, copy)
            if variant == 'instrumented':
                instrument(copy)
            evidence[variant + '_source_hashes'] = inventory(copy)
            build = output / variant / 'build'
            runenv = {**env, 'CARGO_TARGET_DIR': str(build)}
            args = ['cargo', 'build', '--offline', '--locked', '--release', '--manifest-path', str(copy/'tools/serialization-scale/Cargo.toml')]
            if variant == 'instrumented':
                args += ['--features', 'probe']
            result = command(args, env=runenv, text=True, cwd=copy)
            (output / variant / 'build.log').write_text(result.stdout + result.stderr)
            binary = build / 'release/weekend-serialization-scale'
            evidence[variant + '_binary_sha256'] = sha(binary)
            for width, shape, roots in cases:
                suffix = f'{shape}-{width}' + ('' if roots is None else f'-roots-{roots}')
                result = observe(binary, shape, width, output/variant/suffix, runenv, roots)
                result['variant'] = variant
                evidence['runs'].append(result)
            if sha(binary) != evidence[variant + '_binary_sha256']:
                raise RuntimeError('binary changed during run')
            if inventory(copy) != evidence[variant + '_source_hashes']:
                raise RuntimeError('source changed during run')
        if inventory(source) != evidence['source_hashes']:
            raise RuntimeError('base source changed')
        for width, shape, roots in cases:
            pair = [r for r in evidence['runs'] if r['width']==width and r['shape']==shape and r['requested_roots']==roots]
            a,b = [r['events'][-1] for r in pair]
            for key in ['width','epochs','limited_at','shared','before_bytes','after_bytes','verified']:
                if a[key] != b[key]: raise RuntimeError('paired oracle differs: '+key)
        evidence['verified'] = True
    finally:
        (output / 'evidence.json').write_text(json.dumps(evidence, indent=2)+'\n')

def cleanup_docker(name, volume):
    # Removal failure must not skip the independently owned volume cleanup.
    attempts = []
    for argv in [['docker','rm','-f',name], ['docker','volume','rm',volume]]:
        try:
            command(argv)
            attempts.append({'argv':argv, 'ok':True})
        except (OSError, subprocess.SubprocessError):
            attempts.append({'argv':argv, 'ok':False})
    # Successful live-daemon queries, not removal return codes, prove absence.
    containers = command(['docker','ps','-a','--format','{{.Names}}'],text=True).stdout.splitlines()
    volumes = command(['docker','volume','ls','--format','{{.Name}}'],text=True).stdout.splitlines()
    return {'attempts':attempts, 'container_absent':name not in containers,
            'volume_absent':volume not in volumes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', nargs=2, metavar=('SOURCE','OUTPUT'))
    parser.add_argument('--output', type=Path)
    parser.add_argument('--revision', default='HEAD')
    parser.add_argument('--docker-image')
    parser.add_argument('--axis', choices=['breadth','history'], default='breadth')
    args=parser.parse_args()
    if args.worker:
        worker(Path(args.worker[0]).resolve(), Path(args.worker[1]).absolute(), args.axis);return
    if args.output is None: parser.error('--output required')
    output=Path(os.path.abspath(args.output))
    if output.exists() or output.is_symlink() or not output.parent.is_dir(): parser.error('output must be fresh with existing parent')
    output.mkdir()
    revision=command(['git','rev-parse',args.revision],cwd=ROOT,text=True).stdout.strip()
    archive=command(['git','archive',revision],cwd=ROOT).stdout
    (output/'source.tar').write_bytes(archive)
    source=output/'source';source.mkdir()
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar: tar.extractall(source,filter='data')
    for name in FILES:
        target=source/'tools/serialization-scale'/name;target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(ROOT/'tools/serialization-scale'/name,target)
    evidence={'source_hashes': inventory(source), 'axis':args.axis, 'source':revision,'archive_sha256':hashlib.sha256(archive).hexdigest(),'overlay_hashes':{name:sha(ROOT/'tools/serialization-scale'/name) for name in FILES},'verified':False}
    try:
        if args.docker_image:
            image=command(['docker','image','inspect','--format','{{.Id}}',args.docker_image],text=True).stdout.strip()
            name='serialization-scale-'+uuid.uuid4().hex
            volume = name + '-data'
            evidence.update(image=image,container=name,volume=volume,filesystem='private volume; worker records actual filesystem')
            try:
                command(['docker','volume','create',volume])
                argv=['docker','run','--name',name,'--network','none','--memory','2g','--cpus','2','--pids-limit','128','--mount',f'type=volume,src={volume},dst=/data','--mount',f'type=bind,src={source},dst=/src,readonly',image,'python3','/src/tools/serialization-scale/run.py','--worker','/src','/data/results','--axis',args.axis]
                result=subprocess.run(argv,capture_output=True,text=True,timeout=900)
                (output/'docker.log').write_text(result.stdout+result.stderr)
                command(['docker','cp',name+':/data/results',output/'run'])
                result.check_returncode()
            finally:
                cleanup = cleanup_docker(name, volume)
                evidence['cleanup'] = cleanup
                evidence['owned_container_volume_absent'] = cleanup['container_absent'] and cleanup['volume_absent']
                if not evidence['owned_container_volume_absent']:
                    raise RuntimeError('owned Docker resources remain')
        else:
            worker(source,output/'run',args.axis)
        if inventory(source) != evidence['source_hashes']:
            raise RuntimeError('archived source overlay changed')
        evidence['verified']=json.loads((output/'run/evidence.json').read_text())['verified']
        evidence['run_evidence_sha256']=sha(output/'run/evidence.json')
    finally:
        (output/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
if __name__=='__main__': main()
