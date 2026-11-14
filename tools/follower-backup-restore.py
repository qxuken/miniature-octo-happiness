#!/usr/bin/env python3
"""Bounded native CLI backup/restore and fresh-stream reseed qualification."""
import argparse
import hashlib
import io
import json
import os
import platform
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', default='65d142e')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.name == 'nt':
        parser.error('this native keygen qualification supports macOS/Linux only')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    revision = subprocess.check_output(['git', 'rev-parse', args.source + '^{commit}'], cwd=ROOT, text=True).strip()
    archive = subprocess.check_output(['git', 'archive', revision], cwd=ROOT)
    fixture = (ROOT / 'tools/compatibility-driver.rs').read_bytes()
    evidence = {'source': revision, 'source_archive_sha256': hashlib.sha256(archive).hexdigest(),
                'fixture_sha256': hashlib.sha256(fixture).hexdigest(), 'runner_sha256': sha(Path(__file__)),
                'platform': platform.platform(),
                'rustc': subprocess.check_output(['rustc', '-Vv'], text=True), 'commands': []}
    counter = 0

    def run(binary, *arguments, success=True, expected_error=None):
        nonlocal counter
        command = [str(binary), *map(str, arguments)]
        evidence['commands'].append(command)
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        counter += 1
        (output / f'{counter:02}-command.log').write_text(result.stdout + result.stderr)
        if (result.returncode == 0) != success:
            raise RuntimeError(f'unexpected status {result.returncode}: {command}')
        if expected_error is not None and expected_error not in result.stdout + result.stderr:
            raise RuntimeError(f'missing expected diagnostic {expected_error}')
        return result.stdout

    with tempfile.TemporaryDirectory(prefix='weekend-backup-reseed-') as temp:
        temp = Path(temp)
        source = temp / 'source'
        source.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(source, filter='data')
        manifest = source / 'Cargo.toml'
        with manifest.open('a') as f:
            f.write('\n[[example]]\nname="backup_oracle"\npath="tools/backup-oracle.rs"\n')
        (source / 'tools/backup-oracle.rs').write_bytes(fixture)
        lock = sha(source / 'Cargo.lock')
        (output / 'Cargo.lock').write_bytes((source / 'Cargo.lock').read_bytes())
        env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_TARGET_DIR=str(temp / 'target'))
        env.pop('CARGO_BUILD_TARGET', None)
        command = ['cargo', 'build', '--offline', '--locked', '--manifest-path', str(manifest),
                   '-p', 'weekend-db', '--bin', 'weekend-admin', '--example', 'backup_oracle',
                   '-p', 'weekend-replicate', '--bin', 'weekend-replicate']
        evidence['commands'].append(command)
        with (output / 'build.log').open('w') as log:
            subprocess.run(command, env=env, cwd=source, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=300)
        if sha(source / 'Cargo.lock') != lock:
            raise RuntimeError('locked build changed lockfile')
        evidence['lock_sha256'] = lock
        bins = {}
        for name, relative in [('admin', 'weekend-admin'), ('cli', 'weekend-replicate'), ('oracle', 'examples/backup_oracle')]:
            path = temp / 'target/debug' / relative
            dest = output / name
            dest.write_bytes(path.read_bytes())
            dest.chmod(0o700)
            bins[name] = dest
        evidence['binary_sha256'] = {k: sha(v) for k, v in bins.items()}
    cli, admin, oracle = [bins[n] for n in ['cli', 'admin', 'oracle']]
    a, b = output / 'primary-credentials', output / 'follower-credentials'
    run(cli, 'keygen', a)
    run(cli, 'keygen', b)
    primary = output / 'original-primary.wdb'
    run(oracle, 'write', primary)
    old_stream, new_stream = os.urandom(16).hex(), os.urandom(16).hex()
    if old_stream == new_stream or int(old_stream, 16) == 0 or int(new_stream, 16) == 0:
        raise RuntimeError('invalid random stream identities')
    description = run(cli, 'describe', primary, old_stream)
    identity = re.search(r'file_identity=([0-9a-f]{32})', description).group(1)

    def receipt(text):
        match = re.search(r'follower durable_revision=(\d+) stream=([0-9a-f]{32}) digest=([0-9a-f]{64})', text)
        if not match:
            raise RuntimeError('missing durable receipt')
        return {'revision': int(match[1]), 'stream': match[2], 'digest': match[3]}

    def tree(directory):
        return {p.name: sha(p) for p in directory.iterdir() if p.is_file()}

    def ship(source, stream, follower, export, label, server_stream=None, reject=False):
        command = [str(cli), 'serve', str(follower), server_stream or stream, identity, '127.0.0.1:0', str(b), str(a / 'cert.der'), '--max-exchanges', '1']
        evidence['commands'].append(command)
        logpath = output / f'{label}-server.log'
        with logpath.open('w') as log:
            server = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 20
                address = None
                while time.monotonic() < deadline and server.poll() is None:
                    match = re.search(r'^ready (.+)$', logpath.read_text(), re.M)
                    if match:
                        address = match[1]
                        break
                    time.sleep(0.01)
                if address is None:
                    raise RuntimeError('no server readiness')
                sent = run(cli, 'ship', source, stream, export, address, a, b / 'cert.der', '--attempts', '1', success=not reject, expected_error='ForeignStore' if reject else None)
                if server.wait(timeout=30) != 0:
                    raise RuntimeError('server failed')
            finally:
                if server.poll() is None:
                    server.kill()
                    server.wait()
        if reject:
            return None
        ack = receipt(sent)
        if ack != {'revision': 4, 'stream': stream, 'digest': sha(export)}:
            raise RuntimeError('incorrect receipt')
        status = run(cli, 'status', follower, stream, identity)
        if receipt(status) != ack or 'requires_recovery=false' not in status:
            raise RuntimeError('reopened receipt differs')
        slot = re.search(r'selected_slot=Some\(([01])\)', status).group(1)
        selected = follower / f'replica-{slot}.wdb'
        if sha(selected) != ack['digest']:
            raise RuntimeError('selected file does not match receipt')
        run(oracle, 'read', selected)
        return ack, selected

    old_follower = output / 'old-follower'
    old_follower.mkdir(mode=0o700)
    run(cli, 'follower-init', old_follower, old_stream, identity)
    old_ack, selected = ship(primary, old_stream, old_follower, output / 'old-export.wdb', 'old')
    originals = {primary: sha(primary), **{p: sha(p) for p in old_follower.iterdir() if p.is_file()}}
    backup, restored = output / 'verified-backup.wdb', output / 'restored-primary.wdb'
    run(admin, 'backup', selected, backup)
    run(admin, 'restore', backup, restored)
    restored_before = sha(restored)
    run(admin, 'restore', backup, restored, success=False, expected_error='AlreadyExists')
    if sha(restored) != restored_before:
        raise RuntimeError('existing restore destination changed')
    for path in [backup, restored]:
        run(admin, 'check', path)
        run(oracle, 'read', path)
    before = tree(old_follower)
    run(cli, 'status', old_follower, new_stream, identity, success=False, expected_error='ForeignStore')
    ship(restored, new_stream, old_follower, output / 'rejected-export.wdb', 'rejected-enrollment', server_stream=old_stream, reject=True)
    if tree(old_follower) != before:
        raise RuntimeError('wrong enrollment changed old follower')
    if receipt(run(cli, 'status', old_follower, old_stream, identity)) != old_ack:
        raise RuntimeError('old receipt changed')
    new_follower = output / 'new-follower'
    new_follower.mkdir(mode=0o700)
    run(cli, 'describe', restored, new_stream)
    run(cli, 'follower-init', new_follower, new_stream, identity)
    new_ack, _ = ship(restored, new_stream, new_follower, output / 'new-export.wdb', 'new')
    for path, expected in originals.items():
        if sha(path) != expected:
            raise RuntimeError(f'original changed: {path}')
    evidence.update(verified=True, file_identity=identity, old_receipt=old_ack, new_receipt=new_ack,
                    retained_revision=2, retained_id=1, originals_unchanged=True,
                    wrong_enrollment_rejected_unchanged=True, backup_sha256=sha(backup), restored_sha256=sha(restored))
    (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(output / 'evidence.json')


if __name__ == '__main__':
    main()
