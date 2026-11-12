#!/usr/bin/env python3
"""Private frozen-checkpoint storage/snapshot qualification, offline and bounded."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import tarfile
import time

ROOT = Path(__file__).resolve().parent.parent
IDENTITY = r'''fn main() {
    println!(r#"{{"identity_kind":"engine_version_api","source_id":"{}","features":{},"abi_version":{},"format_version":{}}}"#,
        weekend_db::version::SOURCE_ID,weekend_db::version::FEATURES,
        weekend_db::version::ABI_VERSION,weekend_db::version::FORMAT_VERSION);
}
'''
BUILD_IDENTITY = r'''fn main() {
    let database = weekend_db::Database::new().unwrap();
    println!(r#"{{"identity_kind":"qualification_build_stamp","source_id":"{}","features":{},"empty_revision":{}}}"#,
        env!("WEEKEND_DB_SOURCE_ID"),cfg!(feature="replication") as u32,database.snapshot().revision());
}
'''


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def capture(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True, timeout=30).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_hashes(source):
    return {str(p.relative_to(source)): digest(p) for p in sorted(source.rglob('*')) if p.is_file()}


def fresh_output(path):
    candidate = Path(os.path.abspath(path))
    if os.path.lexists(candidate):
        raise ValueError('output already exists, including dangling symlinks')
    candidate.parent.mkdir(parents=True, exist_ok=True)
    # Resolve parents only; a final-component symlink race must fail at mkdir.
    output = candidate.parent.resolve() / candidate.name
    output.mkdir(mode=0o700, exist_ok=False)
    return output


def child_env():
    env = os.environ.copy()
    for name in list(env):
        if (name in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS',
                     'RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_TARGET')
                or name.startswith('CARGO_PROFILE_')
                or (name.startswith('CARGO_TARGET_') and name.endswith('_RUSTFLAGS'))):
            env.pop(name, None)
    env['CARGO_INCREMENTAL'] = '0'
    return env


def stop(child):
    if child.poll() is None:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=5)


def run(command, output, evidence, label, *, cwd=ROOT, env=None, timeout=30):
    command = [str(arg) for arg in command]
    log_path = output / (label + '.log')
    record = {'command': command, 'cwd': str(cwd), 'timeout_seconds': timeout,
              'log': str(log_path), 'returncode': None}
    evidence['commands'].append(record)
    started = time.monotonic()
    with log_path.open('x') as log:
        child = subprocess.Popen(command, cwd=cwd, env=env, stdout=log,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            record['timed_out'] = True
            raise
        finally:
            stop(child)
            record.update(returncode=child.returncode, elapsed_seconds=time.monotonic() - started)
    require(child.returncode == 0, f'{label} failed; inspect its log')
    return log_path.read_text()


def build_endpoints(args, output, evidence):
    revisions = {name: capture('git', 'rev-parse', '--verify', revision + '^{commit}')
                 for name, revision in [('historical', args.historical), ('current', args.current)]}
    evidence['sources'] = revisions
    fixture_name = 'compatibility-snapshot.rs' if args.profile == 'snapshot' else 'compatibility-driver.rs'
    fixture = (ROOT / 'tools' / fixture_name).read_bytes()
    evidence.update(fixture_sha256=hashlib.sha256(fixture).hexdigest(),
                    runner_sha256=digest(Path(__file__)), toolchain=capture('rustc', '-Vv'),
                    cargo=capture('cargo', '-V'), features=['replication'] if args.profile == 'snapshot' else [],
                    profile='debug', builds={})
    bins = {}
    for name, revision in revisions.items():
        source = output / 'source' / name
        source.mkdir(parents=True)
        archive = subprocess.check_output(['git', 'archive', '--format=tar', revision], cwd=ROOT, timeout=30)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(source, filter='data')
        manifest, lock = source / 'Cargo.toml', source / 'Cargo.lock'
        manifest_hash, lock_hash = digest(manifest), digest(lock)
        require('publish = false' in manifest.read_text(), 'engine manifest must remain private')
        (output / (name + '-Cargo.lock')).write_bytes(lock.read_bytes())
        fixture_path = source / 'tools/qualification-fixture.rs'
        identity_path = source / 'tools/qualification-identity.rs'
        require(not fixture_path.exists() and not identity_path.exists(), 'overlay paths already exist')
        fixture_path.write_bytes(fixture)
        native_identity = (source / 'src/version.rs').is_file()
        require(native_identity or args.profile == 'snapshot', 'storage endpoint lacks native artifact identity API')
        identity_fixture = IDENTITY if native_identity else BUILD_IDENTITY
        identity_path.write_text(identity_fixture)
        with manifest.open('a') as file:
            file.write('\n[[example]]\nname="qualification_fixture"\npath="tools/qualification-fixture.rs"\n')
            if args.profile == 'snapshot':
                file.write('required-features=["replication"]\n')
            file.write('\n[[example]]\nname="qualification_identity"\npath="tools/qualification-identity.rs"\n')
        before = source_hashes(source)
        env = child_env()
        env.update(CARGO_TARGET_DIR=str(output / 'build' / name), WEEKEND_DB_SOURCE_ID=revision)
        command = ['cargo', 'build', '--offline', '--locked', '-p', 'weekend-db',
                   '--example', 'qualification_fixture', '--example', 'qualification_identity',
                   '--manifest-path', manifest]
        if args.profile == 'snapshot':
            command.extend(['--features', 'replication'])
        run(command, output, evidence, name + '-build', cwd=source, env=env, timeout=180)
        require(digest(lock) == lock_hash, 'locked build changed original lockfile')
        require(source_hashes(source) == before, 'build changed frozen source')
        extension = '.exe' if os.name == 'nt' else ''
        example_dir = output / 'build' / name / 'debug/examples'
        binary = output / (name + '-consumer' + extension)
        binary.write_bytes((example_dir / ('qualification_fixture' + extension)).read_bytes())
        binary.chmod(0o700)
        identity_binary = example_dir / ('qualification_identity' + extension)
        identity = json.loads(run([identity_binary], output, evidence, name + '-identity', env=env))
        expected_identity = {'source_id': revision, 'features': 1 if args.profile == 'snapshot' else 0}
        if native_identity:
            expected_identity.update(identity_kind='engine_version_api', abi_version=1, format_version=2)
        else:
            expected_identity.update(identity_kind='qualification_build_stamp', empty_revision=0)
        require(identity == expected_identity, 'runtime artifact identity mismatch')
        bins[name] = binary
        evidence['builds'][name] = {'archive_sha256': hashlib.sha256(archive).hexdigest(),
            'original_manifest_sha256': manifest_hash, 'overlaid_manifest_sha256': digest(manifest),
            'original_lock_sha256': lock_hash, 'binary_sha256': digest(binary),
            'identity_binary_sha256': digest(identity_binary), 'runtime_identity': identity,
            'identity_fixture_sha256': hashlib.sha256(identity_fixture.encode()).hexdigest(),
            'source_sha256': before, 'source_unchanged': True, 'source_directory': str(source)}
    return bins


def storage_cases(output, bins, evidence):
    evidence['results'] = {}
    for writer in ('current', 'historical'):
        directory = output / (writer + '-writer')
        directory.mkdir()
        image = directory / 'current.wdb'
        run([bins[writer], 'write', image], directory, evidence, 'write')
        original = image.read_bytes()
        require(len(original) >= 12, 'writer produced an incomplete image')
        for reader in ('current', 'historical'):
            run([bins[reader], 'read', image], directory, evidence, reader + '-read')
            require(image.read_bytes() == original, f'{reader} changed witnessed image while reading')
        unsupported = directory / 'unsupported.wdb'
        changed = bytearray(original)
        changed[8:12] = (0xffffffff).to_bytes(4, 'little')
        unsupported.write_bytes(changed)
        for reader in ('current', 'historical'):
            run([bins[reader], 'reject', unsupported], directory, evidence, reader + '-reject')
            require(unsupported.read_bytes() == changed, f'{reader} wrote unsupported image')
        evidence['results'][writer + '-writer'] = {'current_revision': 4, 'retained_revision': 2,
            'readers': ['current', 'historical'], 'read_bytes_unchanged': True,
            'unsupported_rejected_unchanged': True, 'image_sha256': digest(image),
            'unsupported_image_sha256': digest(unsupported)}


def snapshot_cases(output, bins, evidence):
    evidence['results'] = {}
    for sender, receiver in [('historical', 'current'), ('current', 'historical')]:
        name = sender + '-to-' + receiver
        directory = output / name
        directory.mkdir()
        children, logs, records = [], [], []
        deadline = time.monotonic() + 90
        try:
            for role, version in [('primary', sender), ('follower', receiver)]:
                command = [str(bins[version]), role, str(directory)]
                record = {'command': command, 'log': str(directory / (role + '.log')),
                          'timeout_seconds': 90, 'returncode': None}
                evidence['commands'].append(record)
                records.append(record)
                log = (directory / (role + '.log')).open('x')
                logs.append(log)
                children.append(subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT,
                                                  start_new_session=True, env=child_env()))
            for child in children:
                child.wait(timeout=max(0.001, deadline - time.monotonic()))
                require(child.returncode == 0, f'{name} consumer failed; inspect logs')
        finally:
            for child, record in zip(children, records):
                stop(child)
                record['returncode'] = child.returncode
            for log in logs:
                log.close()
        primary = json.loads((directory / 'primary.json').read_text())
        follower = json.loads((directory / 'follower.json').read_text())
        require(primary == follower, 'sender/receiver receipts differ')
        require(primary['digest'] == digest(directory / 'export.wdb'), 'receipt differs from independent export hash')
        require(primary['revision'] == 4 and primary['retained_revision'] == 2, 'unexpected snapshot revisions')
        evidence['results'][name] = dict(primary, independent_export_sha256=primary['digest'],
            reopened=True, duplicate_ack_equal=True, unsupported_rejected_without_file_changes=True,
            primary_advanced_locally_to=5)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--historical', default=None)
    parser.add_argument('--current', default='HEAD', help='committed current endpoint for both profiles')
    parser.add_argument('--profile', choices=['storage', 'snapshot'], default='storage')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(argv)
    require(os.name == 'posix', 'bounded process-group qualification currently requires macOS/Linux')
    args.historical = args.historical or ('dcd2402' if args.profile == 'snapshot' else 'a7dc343')
    output = fresh_output(args.output)
    evidence = {'verified': False, 'commands': [], 'requested_sources': {
        'historical': args.historical, 'current': args.current}, 'qualification_profile': args.profile,
        'scope': 'two frozen pre-1 checkpoints, original locked dependencies, native files; '
                 'snapshot adds caller-owned private loopback, not TLS/rotation qualification'}
    try:
        bins = build_endpoints(args, output, evidence)
        if args.profile == 'snapshot':
            snapshot_cases(output, bins, evidence)
        else:
            storage_cases(output, bins, evidence)
        for build in evidence['builds'].values():
            require(source_hashes(Path(build['source_directory'])) == build['source_sha256'],
                    'frozen source changed during qualification')
        evidence['verified'] = True
    except Exception as error:
        evidence['failure'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(output / 'evidence.json')


if __name__ == '__main__':
    main()
