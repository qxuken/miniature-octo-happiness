#!/usr/bin/env python3
"""Build a pinned engine and run a Linux server with a native macOS client.

Uses synthetic data, a private Docker volume and temporary test listeners.
The listeners do not supply production authentication.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import uuid

ROOT = Path(__file__).resolve().parent.parent
DRIVER_FILES = [
    'tools/monthly_workload.rs', 'tools/monthly_workload.py',
    'tools/monthly-workload/Cargo.toml', 'tools/monthly-workload/Cargo.lock',
    'tools/monthly-workload/.gitignore',
]


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True, help='New evidence directory')
    parser.add_argument('--source', default='HEAD', help='Engine Git revision to archive and build')
    parser.add_argument('--image', default='rust:1-bookworm')
    parser.add_argument('--live-budget', action='store_true', help='Apply the 16 MiB core budget; requires an engine supporting it')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This qualification pairs a native macOS client with a Linux container')
    output = args.directory.resolve()
    if output.exists():
        parser.error('Evidence directory must not already exist')
    output.parent.mkdir(parents=True, exist_ok=True)
    identity = subprocess.check_output(
        ['git', 'rev-parse', args.source + '^{commit}'], cwd=ROOT, text=True).strip()
    server_port, client_port = free_port(), free_port()
    while client_port == server_port:
        client_port = free_port()
    name = 'weekend-month-' + uuid.uuid4().hex[:10]
    volume = name + '-data'
    features = ['--features', 'live-budget'] if args.live_budget else []
    with tempfile.TemporaryDirectory(prefix='weekend-cross-month-') as temporary:
        scratch = Path(temporary)
        source = scratch / 'source'
        source.mkdir()
        archive = scratch / 'source.tar'
        with archive.open('wb') as stream:
            subprocess.run(['git', 'archive', identity], cwd=ROOT, stdout=stream, check=True)
        with tarfile.open(archive) as bundle:
            bundle.extractall(source, filter='data')
        for filename in DRIVER_FILES:
            destination = source / filename
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / filename, destination)
        # These qualification files may postdate the pinned engine. Bind both.
        with tarfile.open(archive, 'a') as bundle:
            for filename in DRIVER_FILES:
                bundle.add(source / filename, arcname=filename)
        subprocess.run([
            'cargo', 'build', '--offline', '--locked', '--release',
            '--manifest-path', str(source / 'tools/monthly-workload/Cargo.toml'), *features,
        ], env={**os.environ, 'WEEKEND_DB_SOURCE_ID': identity}, check=True, timeout=180)
        try:
            subprocess.run([
                'docker', 'run', '-d', '--rm', '--name', name, '--memory=2g', '--pids-limit=256',
                '-p', f'127.0.0.1:{server_port}:{server_port}',
                '--mount', f'type=volume,source={volume},target=/data',
                '--mount', f'type=bind,source={Path.home() / ".cargo/registry"},target=/usr/local/cargo/registry,readonly',
                args.image, 'tail', '-f', '/dev/null',
            ], check=True)
            subprocess.run(['docker', 'exec', name, 'mkdir', '/work'], check=True)
            with archive.open('rb') as stream:
                subprocess.run(['docker', 'exec', '-i', name, 'tar', '-xf', '-', '-C', '/work'], stdin=stream, check=True)
            subprocess.run([
                'docker', 'exec', '-e', f'WEEKEND_DB_SOURCE_ID={identity}', name,
                'cargo', 'build', '--offline', '--locked', '--release', '--manifest-path',
                '/work/tools/monthly-workload/Cargo.toml', '--target-dir', '/build', *features,
            ], check=True, timeout=180)
            platform_info = '\n'.join(subprocess.check_output(
                ['docker', 'exec', name, *command], text=True) for command in [
                    ['uname', '-a'], ['rustc', '-Vv'],
                    ['findmnt', '-T', '/data', '-o', 'SOURCE,FSTYPE,OPTIONS'],
                    ['sha256sum', '/build/release/monthly-workload'],
                ])
            print(platform_info, flush=True)
            subprocess.run([
                'python3', str(ROOT / 'tools/monthly_workload.py'), '--directory', str(output),
                '--binary', str(source / 'tools/monthly-workload/target/release/monthly-workload'),
                '--engine-source', identity,
                '--server-command', json.dumps(['docker', 'exec', name, '/build/release/monthly-workload']),
                '--server-db', '/data/server.wdb',
                '--server-listen', f'0.0.0.0:{server_port}', '--server-address', f'127.0.0.1:{server_port}',
                '--client-listen', f'0.0.0.0:{client_port}', '--client-address', f'host.docker.internal:{client_port}',
            ], check=True, timeout=900)
            (output / 'linux-platform.txt').write_text(platform_info)
            shutil.copyfile(archive, output / 'qualification-source.tar')
            evidence = {
                'engine': identity, 'features': features, 'source_tar_sha256': digest(archive),
                'runner_sha256': digest(Path(__file__)),
                'driver_sha256': {filename: digest(source / filename) for filename in DRIVER_FILES},
                'docker_image': subprocess.check_output(
                    ['docker', 'image', 'inspect', args.image, '--format', '{{.Id}}'], text=True).strip(),
            }
            (output / 'qualification-source.json').write_text(json.dumps(evidence, indent=2) + '\n')
            subprocess.run(['docker', 'cp', f'{name}:/data', str(output / 'linux-data')], check=True)
            print('CROSS_PLATFORM_MONTH_PASSED', output, flush=True)
        finally:
            subprocess.run(['docker', 'rm', '-f', name], check=False, timeout=30)
            subprocess.run(['docker', 'volume', 'rm', volume], check=False, timeout=30)


if __name__ == '__main__':
    main()
