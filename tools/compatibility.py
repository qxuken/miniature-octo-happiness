#!/usr/bin/env python3
"""Qualify one pre-1 historical reader against current writer, offline, on native files."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot_profile(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    revisions = {name: capture('git', 'rev-parse', '--verify', revision + '^{commit}')
                 for name, revision in [('historical', args.historical), ('current', args.current)]}
    fixture_bytes = (ROOT / 'tools/compatibility-snapshot.rs').read_bytes()
    evidence = {'sources': revisions, 'fixture_sha256': hashlib.sha256(fixture_bytes).hexdigest(),
                'runner_sha256': digest(Path(__file__)), 'toolchain': capture('rustc', '-Vv'),
                'cargo': capture('cargo', '-V'), 'features': ['replication'],
                'profile': 'debug', 'commands': [], 'builds': {}, 'results': {},
                'scope': 'two frozen pre-1 checkpoints, caller-authorized private loopback; no TLS/rotation qualification'}
    with tempfile.TemporaryDirectory(prefix='weekend-snapshot-compatibility-') as temp:
        temp = Path(temp)
        env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_TARGET_DIR=str(temp / 'target'))
        env.pop('CARGO_BUILD_TARGET', None)
        bins = {}
        for name, revision in revisions.items():
            source = temp / name
            source.mkdir()
            archive = subprocess.check_output(['git', 'archive', revision], cwd=ROOT)
            with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
                tar.extractall(source, filter='data')
            manifest = source / 'Cargo.toml'
            manifest_hash = digest(manifest)
            with manifest.open('a') as file:
                file.write('\n[[example]]\nname="snapshot_compatibility"\npath="tools/compatibility-snapshot.rs"\nrequired-features=["replication"]\n')
            (source / 'tools/compatibility-snapshot.rs').write_bytes(fixture_bytes)
            lock = source / 'Cargo.lock'
            lock_hash = digest(lock)
            (output / (name + '-Cargo.lock')).write_bytes(lock.read_bytes())
            command = ['cargo', 'build', '--offline', '--locked', '-p', 'weekend-db',
                       '--example', 'snapshot_compatibility', '--features', 'replication',
                       '--manifest-path', str(manifest)]
            evidence['commands'].append(command)
            with (output / (name + '-build.log')).open('w') as log:
                subprocess.run(command, cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT,
                               check=True, timeout=180)
            if digest(lock) != lock_hash:
                raise RuntimeError('locked build changed historical lockfile')
            built = temp / 'target/debug/examples' / ('snapshot_compatibility' + ('.exe' if os.name == 'nt' else ''))
            binary = output / (name + ('-consumer.exe' if os.name == 'nt' else '-consumer'))
            binary.write_bytes(built.read_bytes())
            binary.chmod(0o700)
            bins[name] = binary
            evidence['builds'][name] = {'archive_sha256': hashlib.sha256(archive).hexdigest(),
                'original_manifest_sha256': manifest_hash, 'overlaid_manifest_sha256': digest(manifest),
                'original_lock_sha256': lock_hash, 'binary_sha256': digest(binary),
                'replication_source_sha256': {str(p.relative_to(source)): digest(p)
                                              for p in sorted((source / 'src/replication').rglob('*.rs'))}}
        for sender, receiver in [('historical', 'current'), ('current', 'historical')]:
            name = sender + '-to-' + receiver
            directory = output / name
            directory.mkdir()
            children = []
            logs = []
            try:
                for role, version in [('primary', sender), ('follower', receiver)]:
                    command = [str(bins[version]), role, str(directory)]
                    evidence['commands'].append(command)
                    log = (directory / (role + '.log')).open('w')
                    logs.append(log)
                    children.append(subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT))
                for child in children:
                    if child.wait(timeout=90) != 0:
                        raise RuntimeError(f'{name} consumer failed; inspect logs')
            finally:
                for child in children:
                    if child.poll() is None:
                        child.kill()
                        child.wait()
                for log in logs:
                    log.close()
            primary = json.loads((directory / 'primary.json').read_text())
            follower = json.loads((directory / 'follower.json').read_text())
            if primary != follower:
                raise RuntimeError('sender/receiver receipts differ')
            if primary['digest'] != digest(directory / 'export.wdb'):
                raise RuntimeError('receipt digest differs from independently hashed export')
            if primary['revision'] != 4 or primary['retained_revision'] != 2:
                raise RuntimeError('unexpected current/retained revision')
            evidence['results'][name] = dict(primary, independent_export_sha256=primary['digest'],
                reopened=True, duplicate_ack_equal=True, unsupported_rejected_without_file_changes=True,
                primary_advanced_locally_to=5)
    evidence['verified'] = True
    (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(output / 'evidence.json')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--historical', default=None)
    parser.add_argument('--current', default='HEAD', help='snapshot profile committed endpoint')
    parser.add_argument('--profile', choices=['storage', 'snapshot'], default='storage')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.historical = args.historical or ('dcd2402' if args.profile == 'snapshot' else 'a7dc343')
    if args.profile == 'snapshot':
        return snapshot_profile(args)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    historical = capture('git', 'rev-parse', '--verify', args.historical + '^{commit}')
    evidence = {
        'historical_source': historical,
        'current_source': capture('git', 'rev-parse', 'HEAD'),
        'current_dirty': bool(capture('git', 'status', '--porcelain')),
        'current_diff_sha256': hashlib.sha256(subprocess.check_output(['git', 'diff', 'HEAD', '--binary'], cwd=ROOT)).hexdigest(),
        'driver_sha256': digest(ROOT / 'tools/compatibility-driver.rs'),
        'qualification_sources': {name: digest(ROOT / name) for name in [
            'tools/compatibility.py', 'tools/compatibility-driver.rs',
            'crates/weekend-db-typed/tests/process_evolution.rs']},
        'current_core_files': {str(p.relative_to(ROOT)): digest(p)
                               for p in sorted([ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
                                                *ROOT.joinpath('src').rglob('*.rs')])},
        'toolchain': capture('rustc', '-Vv'),
        'cargo': capture('cargo', '-V'),
        'features': [], 'profile': 'debug', 'commands': [],
        'scope': 'pre-1 checkpoints only, native filesystem, same format, not a stable compatibility promise',
    }
    with tempfile.TemporaryDirectory(prefix='weekend-compatibility-') as temp:
        temp = Path(temp)
        source = temp / 'historical'
        source.mkdir()
        archive = subprocess.check_output(['git', 'archive', historical], cwd=ROOT)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(source, filter='data')
        env = os.environ.copy()
        # Avoid implicit targets and shared-cache locks affecting the two builds.
        env.pop('CARGO_BUILD_TARGET', None)
        env['CARGO_TARGET_DIR'] = str(temp / 'target')
        def run(*command):
            evidence['commands'].append([str(x) for x in command])
            with (output / 'commands.log').open('a') as log:
                subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
        bins = {}
        for name, dependency in [('historical', source), ('current', ROOT)]:
            project = temp / name / 'consumer' if name == 'historical' else temp / 'current'
            (project / 'src').mkdir(parents=True)
            (project / 'Cargo.toml').write_text(
                f'[package]\nname="compat-{name}"\nversion="0.0.0"\nedition="2024"\n[workspace]\n'
                f'[dependencies]\nweekend-db={{path={json.dumps(str(dependency))}}}\n')
            (project / 'src/main.rs').write_bytes((ROOT / 'tools/compatibility-driver.rs').read_bytes())
            run('cargo', 'generate-lockfile', '--offline', '--manifest-path', str(project / 'Cargo.toml'))
            run('cargo', 'build', '--offline', '--locked', '--manifest-path', str(project / 'Cargo.toml'))
            evidence[name + '_lock_sha256'] = digest(project / 'Cargo.lock')
            bins[name] = str(temp / 'target/debug' / (f'compat-{name}' + ('.exe' if os.name == 'nt' else '')))
        image = output / 'current.wdb'
        run(bins['current'], 'write', str(image))
        original = image.read_bytes()
        for name in ['current', 'historical']:
            run(bins[name], 'read', str(image))
            assert image.read_bytes() == original, f'{name} changed witnessed image while reading'
        unsupported = output / 'unsupported.wdb'
        changed = bytearray(original)
        changed[8:12] = (0xffffffff).to_bytes(4, 'little')
        unsupported.write_bytes(changed)
        for name in ['current', 'historical']:
            run(bins[name], 'reject', str(unsupported))
            assert unsupported.read_bytes() == changed, f'{name} wrote unsupported image'
        evidence['image_sha256'] = digest(image)
        evidence['results'] = {'current_revision': 4, 'retained_revision': 2,
                               'historical_read_passed': True, 'read_bytes_unchanged': True,
                               'unsupported_rejected_unchanged': True}
    (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(output / 'evidence.json')


if __name__ == '__main__':
    main()
