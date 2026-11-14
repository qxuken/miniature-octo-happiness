#!/usr/bin/env python3
"""Frozen near-cap process-kill and repeated-recovery qualification; never publishes."""
import argparse
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import uuid
import importlib.util

SPEC = importlib.util.spec_from_file_location('resilience_compound', Path(__file__).with_name('compound.py'))
C = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(C)
PROFILES = ['maintenance-copy', 'maintenance-publish', *['recovery-' + str(i) for i in range(6)]]
OVERLAYS = [*C.OVERLAYS, 'tools/resource-envelope/resilience.py', 'tools/resource-envelope/test_resilience.py']


def completion(profile, source):
    return {'complete': True, 'profile': profile, 'source_id': source, 'features': 4, 'abi': 1, 'format': 2}


def state(row, phase, revision, changed, file_bytes=None):
    C.require(row.get('phase') == phase and type(row.get('revision')) is int and row['revision'] == revision, 'wrong phase/revision')
    C.require(type(row.get('changed')) is bool and row['changed'] == changed and type(row.get('seq')) is int and row['seq'] == 4 and type(row.get('roots')) is int and row['roots'] == 64, 'wrong root/current state')
    C.require(C.integers(row.get('digests'), 4) and row['digests'] == C.digests(4, changed), 'independent root byte oracle')
    for key in ['live_bytes', 'peak_bytes', 'cap_bytes', 'rss_kib', 'file_bytes', 'elapsed_ns']:
        C.require(type(row.get(key)) is int and row[key] >= 0, 'invalid telemetry')
    C.require(0 < row['live_bytes'] <= row['peak_bytes'] <= row['cap_bytes'] == C.CORE and row['rss_kib'] > 0, 'invalid core/RSS')
    C.require(60 * C.MIB < row['file_bytes'] <= 64 * C.MIB, 'not near cap')
    if file_bytes is not None: C.require(row['file_bytes'] == file_bytes, 'wrong replacement image length')


def check_rows(rows, profile, source):
    C.require(isinstance(rows, list) and len(rows) > 1, 'missing rows')
    C.require(type(rows[-1].get('complete')) is bool and all(type(rows[-1].get(k)) is int for k in ['features', 'abi', 'format']), 'wrong completion types')
    C.require(rows[-1] == completion(profile, source), 'wrong completion identity')
    if profile.startswith('maintenance-'):
        publish = profile == 'maintenance-publish'
        C.require(profile in PROFILES, 'wrong maintenance profile')
        count = 16 if publish else 36
        C.require(len(rows) == 1 + count * 3 + 2, 'incomplete maintenance cases')
        state(rows[0], 'control', 212, False, 63074304)
        C.require(rows[0]['detail'] == {'fresh_process': True} and type(rows[0]['detail']['fresh_process']) is bool, 'missing fresh control')
        expected = [(s, 'first') for s in range(9, 17)] if publish else [(s, 'first') for s in range(9)] + [(3, 'last'), (7, 'middle'), (7, 'last')] + [(s, n) for s in [1, 5, 6] for n in ['middle', 'last']]
        cursor = 1; counts = {}
        for step, selection in expected:
            for after in [False, True]:
                boundary, killed, retried = rows[cursor:cursor + 3]; cursor += 3
                C.require(type(boundary.get('step')) is int and boundary['step'] == step and type(boundary.get('count')) is int and boundary['count'] > 0, 'wrong boundary/count')
                C.require(type(boundary.get('occurrence')) is int and type(boundary.get('after')) is bool and boundary['after'] == after, 'wrong occurrence/edge')
                if step in [3, 7]: C.require(boundary['count'] == (2 if step == 3 else 3), 'missing durability barrier')
                C.require(counts.setdefault(step, boundary['count']) == boundary['count'], 'inconsistent calibration count')
                n = 0 if selection == 'first' else boundary['count'] // 2 if selection == 'middle' else boundary['count'] - 1
                C.require(boundary['occurrence'] == n and (selection == 'first' or boundary['count'] > (1 if step in [3, 7] else 2)), 'wrong sampled large-I/O occurrence')
                name = f's{step}_n{n}_' + ('after' if after else 'before')
                exchanged = step > 11 or (step == 11 and after)
                C.require(boundary.get('boundary') == name and type(boundary.get('signal')) is int and boundary['signal'] == 9 and type(boundary.get('exchanged')) is bool and boundary['exchanged'] == exchanged, 'missing kill/exchange evidence')
                C.require(type(boundary.get('orphans')) is int and 0 <= boundary['orphans'] <= 1, 'unexpected private namespace')
                state(killed, 'killed_' + name, 212, False, 63074304 if exchanged else 66154496)
                state(retried, 'retried_' + name, 212, False, 63074304)
                C.require(killed['detail'] == retried['detail'] == {'fresh_process': True} and type(killed['detail']['fresh_process']) is bool and type(retried['detail']['fresh_process']) is bool, 'not checked by fresh processes')
    else:
        C.require(profile in PROFILES, 'wrong recovery profile')
        initial = int(profile.split('-')[1]); count = 18
        C.require(len(rows) == count * 2 + 2, 'incomplete recovery cases')
        cursor = 0
        for second in range(9):
            for persist in [False, True]:
                recovered, retried = rows[cursor:cursor + 2]; cursor += 2
                survives = initial >= 3 or (initial == 2 and (persist or 3 <= second <= 7))
                revision = 212 + int(survives)
                name = f'i{initial}_f{second}_' + ('persist' if persist else 'drop')
                state(recovered, 'recovered_' + name, revision, survives)
                state(retried, 'retried_' + name, revision + 1, True)
                wanted = {'initial': initial, 'second': second, 'persist': persist, 'recovery_error': 'Limit' if second == 8 else 'Io', 'repeat_error': 'Io', 'acknowledged_floor': 213 if initial == 5 else 212}
                C.require(all(type(recovered.get('detail', {}).get(k)) is int for k in ['initial', 'second', 'acknowledged_floor']) and type(recovered['detail'].get('persist')) is bool, 'invalid failure types')
                C.require(recovered.get('detail') == wanted and revision >= wanted['acknowledged_floor'], 'wrong failure/floor')
                C.require(retried.get('detail') == {'fresh_process': True} and type(retried['detail']['fresh_process']) is bool, 'not independently reopened')
    C.require(type(rows[-2].get('cleanup')) is bool and type(rows[-2].get('source_unchanged')) is bool and type(rows[-2].get('cases')) is int, 'invalid cleanup types')
    C.require(rows[-2] == {'cleanup': True, 'cases': count, 'source_unchanged': True}, 'unretired cases or modified source')


def worker(source, output, revision):
    source, output = Path(source).resolve(), Path(output).absolute()
    # Existing native/concurrency/copy/fault profiles generate and verify the
    # near-cap source, with exact same engine/compiler identity as new profiles.
    C.worker(source, output, revision)
    baseline = json.loads((output / 'evidence.json').read_text())
    evidence = {'verified': False, 'revision': revision, 'source_hashes': baseline['source_hashes'], 'binary_sha256': baseline['binary_sha256'], 'baseline_sha256': C.SCALE.sha(output / 'evidence.json'), 'runs': {}}
    binary = output / 'build/release/weekend-compound-resource'; image = output / 'native-data/source.wdb'; image_hash = C.SCALE.sha(image)
    saved_env = dict(os.environ); clean_env = C.SCALE.clean_env()
    try:
        os.environ.clear(); os.environ.update(clean_env)
        for profile in PROFILES:
            arguments = [image, 'resilience-recovery', profile.split('-')[1]] if profile.startswith('recovery-') else [image, profile]
            evidence['runs'][profile] = C.observe(binary, output / (profile + '-data'), output / (profile + '.json'), profile, revision, arguments,
                checker=lambda rows, p=profile: check_rows(rows, p, revision))
        C.require(C.SCALE.sha(image) == image_hash and C.SCALE.sha(binary) == evidence['binary_sha256'], 'source image or binary changed')
        C.require(C.SCALE.inventory(source) == evidence['source_hashes'], 'frozen sources changed')
        evidence.update(verified=True, image_sha256=image_hash)
    finally:
        os.environ.clear(); os.environ.update(saved_env)
        evidence['artifacts'] = {str(p.relative_to(output)): C.SCALE.sha(p) for p in output.rglob('*') if p.is_file() and 'build' not in p.relative_to(output).parts and p.name != 'resilience.json'}
        (output / 'resilience.json').write_text(json.dumps(evidence, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision'); parser.add_argument('--output'); parser.add_argument('--docker-image')
    parser.add_argument('--worker', nargs=3, metavar=('SOURCE', 'OUTPUT', 'REVISION'))
    args = parser.parse_args()
    if args.worker: worker(*args.worker); return
    C.require(args.revision and args.output, 'revision and fresh output required')
    output = C.BASE.HELPERS.fresh_output(args.output); evidence = {'verified': False}
    try:
        revision = C.SCALE.command(['git', 'rev-parse', '--verify', args.revision + '^{commit}'], cwd=C.ROOT, text=True).stdout.strip()
        archive = C.SCALE.command(['git', 'archive', revision], cwd=C.ROOT).stdout
        (output / 'source.tar').write_bytes(archive); source = output / 'source'; source.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as packed: packed.extractall(source, filter='data')
        for name in OVERLAYS:
            p = source / name; p.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(C.ROOT / name, p)
        for name in ['tools/serialization-scale/run.py', 'tools/sqlite-baseline/run.py']:
            C.require((source / name).read_bytes() == (C.ROOT / name).read_bytes(), 'shared helper/archive mismatch')
        evidence.update(revision=revision, source_hashes=C.SCALE.inventory(source), archive_sha256=C.SCALE.sha(output / 'source.tar'), overlays={name: C.SCALE.sha(C.ROOT / name) for name in OVERLAYS})
        if args.docker_image:
            image = C.SCALE.command(['docker', 'image', 'inspect', '--format', '{{.Id}}', args.docker_image], text=True).stdout.strip()
            name = 'resilience-' + uuid.uuid4().hex; volume = name + '-data'
            evidence.update(image=image, container=name, volume=volume)
            try:
                C.SCALE.command(['docker', 'volume', 'create', volume])
                argv = ['docker', 'run', '--name', name, '--network', 'none', '--memory', '2g', '--cpus', '2', '--pids-limit', '256', '--mount', f'type=volume,src={volume},dst=/data', '--mount', f'type=bind,src={source},dst=/src,readonly', image, 'python3', '/src/tools/resource-envelope/resilience.py', '--worker', '/src', '/data/run', revision]
                import supervision
                argv=supervision.docker_command(argv,image,output)
                with (output / 'docker.log').open('x') as log: result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT, timeout=1200)
            finally:
                try:
                    copied = subprocess.run(['docker', 'cp', name + ':/data/run', str(output / 'run')], capture_output=True, text=True, timeout=120)
                    evidence['copy'] = {'exit': copied.returncode, 'stderr': copied.stderr}
                finally:
                    evidence['cleanup'] = C.SCALE.cleanup_docker(name, volume)
                    C.require(evidence['cleanup']['container_absent'] and evidence['cleanup']['volume_absent'], 'unverified Docker cleanup')
            result.check_returncode(); C.require(copied.returncode == 0, 'incomplete evidence copy')
        else: worker(source, output / 'run', revision)
        run = json.loads((output / 'run/resilience.json').read_text()); C.require(run['verified'], 'unverified resilience worker')
        C.require(set(run['runs']) == set(PROFILES), 'missing profile')
        for profile, report in run['runs'].items(): check_rows(report['rows'], profile, revision)
        for name, digest in run['artifacts'].items(): C.require(C.SCALE.sha(output / 'run' / name) == digest, 'copied proof changed')
        C.require(C.SCALE.sha(output / 'run/build/release/weekend-compound-resource') == run['binary_sha256'], 'binary changed')
        C.require(C.SCALE.inventory(source) == evidence['source_hashes'] == run['source_hashes'], 'source changed')
        evidence.update(verified=True, worker_sha256=C.SCALE.sha(output / 'run/resilience.json'))
    finally: (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')


if __name__ == '__main__': main()
