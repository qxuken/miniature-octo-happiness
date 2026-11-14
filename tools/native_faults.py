#!/usr/bin/env python3
"""Bounded real Linux ENOSPC/permission drills from an archived engine."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import uuid

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', default='sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08')
    parser.add_argument('--source', default='HEAD')
    parser.add_argument('--output', type=Path, help='New private evidence directory; default is a fresh temporary directory')
    args = parser.parse_args()
    requested = Path(os.path.abspath(args.output)) if args.output else None
    if requested and (os.path.lexists(requested) or not requested.parent.is_dir()):
        parser.error('output must be new with an existing parent')
    identity = subprocess.check_output(['git', 'rev-parse', '--verify', args.source + '^{commit}'], cwd=ROOT, text=True, timeout=30).strip()
    image = subprocess.check_output(['docker', 'image', 'inspect', args.image, '--format={{.Id}}'], text=True, timeout=30).strip()
    if not image.startswith('sha256:') or len(image) != 71:
        raise RuntimeError('Docker image identity missing')
    output = requested.parent.resolve() / requested.name if requested else Path(tempfile.mkdtemp(prefix='weekend-native-faults-'))
    if requested:
        output.mkdir(mode=0o700)
    source = output / 'source'
    source.mkdir()
    archive = output / 'source.tar'
    subprocess.run(['git', 'archive', '--output', str(archive), identity], cwd=ROOT, check=True, timeout=30)
    with tarfile.open(archive) as bundle:
        bundle.extractall(source, filter='data')
    # Exactly one reviewed harness overlay; never copy arbitrary untracked files.
    overlay = 'examples/native_faults.rs'
    shutil.copyfile(ROOT / overlay, source / overlay)
    script = r'''
set -eu
uname -a
rustc -Vv
findmnt -n -o FSTYPE,SIZE,OPTIONS /faults
test "$(findmnt -n -o FSTYPE /faults)" = tmpfs
cargo build --offline --locked --release --manifest-path /source/Cargo.toml --example native_faults --target-dir /build
sha256sum /build/release/examples/native_faults
mkdir /faults/db /faults/denied
chmod 700 /faults/denied
setpriv --reuid=10001 --regid=10001 --clear-groups /build/release/examples/native_faults permission /faults/denied
test ! -e /faults/denied/database.wdb
WEEKEND_PRIVATE_TMPFS=1 /build/release/examples/native_faults diskfull /faults/db
printf 'NATIVE_FAULTS_COMPLETED\n'
'''
    name = 'weekend-native-faults-' + uuid.uuid4().hex
    command = ['docker', 'run', '--rm', '--name', name, '--network=none',
               '--memory=2g', '--pids-limit=256', '--tmpfs', '/faults:rw,size=8m,mode=755',
               '--mount', f'type=bind,source={source},target=/source,readonly',
               '--mount', f'type=bind,source={Path.home() / ".cargo/registry"},target=/usr/local/cargo/registry,readonly',
               '-e', 'CARGO_INCREMENTAL=0', '-e', 'WEEKEND_DB_SOURCE_ID=' + identity,
               image, 'sh', '-c', script]
    evidence = {'engine_source': identity, 'source_archive_sha256': digest(archive),
                'driver_overlay': {overlay: digest(source / overlay)},
                'runner_sha256': digest(Path(__file__)), 'image': image, 'command': command,
                'source_files_sha256': {str(p.relative_to(source)): digest(p) for p in source.rglob('*') if p.is_file()},
                'launcher_python': platform.python_version(),
                'verified': False, 'scope': 'native Linux errors on bounded tmpfs; not durable device power loss'}
    (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    try:
        with (output / 'run.log').open('w') as log:
            result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=300)
        evidence['returncode'] = result.returncode
        log = (output / 'run.log').read_text()
        evidence['completion_marker'] = 'NATIVE_FAULTS_COMPLETED' in log.splitlines()
        evidence['verified'] = result.returncode == 0 and evidence['completion_marker']
        evidence['source_stable'] = evidence['source_files_sha256'] == {
            str(p.relative_to(source)): digest(p) for p in source.rglob('*') if p.is_file()}
        if not evidence['source_stable']:
            evidence['verified'] = False
    finally:
        # --rm may have removed it already. Independently confirm absence after
        # cleanup, and preserve failure evidence even if the daemon times out.
        try:
            subprocess.run(['docker', 'rm', '-f', name], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=30, check=False)
            names = subprocess.check_output(['docker', 'ps', '-a', '--format={{.Names}}'], text=True, timeout=30).splitlines()
            evidence['container_absent'] = name not in names
            if not evidence['container_absent']:
                evidence['verified'] = False
        except (OSError, subprocess.SubprocessError) as error:
            evidence['verified'] = False
            evidence['cleanup_error'] = str(error)
        (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    if not evidence['verified']:
        raise RuntimeError('native drill failed; inspect ' + str(output))
    print(output / 'evidence.json')


if __name__ == '__main__':
    main()
