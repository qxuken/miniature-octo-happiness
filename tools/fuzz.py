#!/usr/bin/env python3
"""Run discovered cargo-fuzz targets with bounded ASan execution and evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def capture(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', action='append', help='Repeat to select targets, default all manifest bins')
    parser.add_argument('--runs', type=int, default=1000)
    parser.add_argument('--seconds', type=int, default=60)
    parser.add_argument('--rss-mb', type=int, default=2048)
    parser.add_argument('--max-len', type=int, default=262144)
    parser.add_argument('--seed', type=int, default=1)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if os.name != 'posix':
        parser.error('This ASan runner currently requires Linux or macOS')
    if min(args.runs, args.seconds, args.rss_mb, args.max_len) <= 0:
        parser.error('Execution budgets must be positive')
    available = [item['name'] for item in tomllib.loads((ROOT / 'fuzz/Cargo.toml').read_text())['bin']]
    targets = args.target or available
    if set(targets) - set(available):
        parser.error('Unknown target')
    if args.output:
        output = args.output.resolve()
        output.mkdir(parents=True, exist_ok=False)
    else:
        parent = ROOT / 'target/fuzz-evidence'
        parent.mkdir(parents=True, exist_ok=True)
        output = Path(tempfile.mkdtemp(dir=parent))
    evidence = {'source': capture('git', 'rev-parse', 'HEAD'),
                'dirty': bool(capture('git', 'status', '--porcelain')),
                'toolchain': capture('rustc', '+nightly', '-Vv'),
                'cargo_fuzz': capture('cargo', 'fuzz', '--version'),
                'lockfile_sha256': hashlib.sha256((ROOT / 'fuzz/Cargo.lock').read_bytes()).hexdigest(),
                'seed_sha256': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                                for path in (ROOT / 'fuzz/seeds').rglob('*') if path.is_file()},
                'targets': []}
    failed = False
    for name in targets:
        directory = output / name
        corpus = directory / 'corpus'
        artifacts = directory / 'artifacts'
        corpus.mkdir(parents=True)
        artifacts.mkdir()
        command = ['cargo', '+nightly', 'fuzz', 'run', '--sanitizer', 'address',
                   '--target-dir', str(ROOT / 'target/fuzz-build'), name, str(corpus)]
        seeds = ROOT / 'fuzz/seeds' / name
        if seeds.is_dir():
            command.append(str(seeds))
        command += ['--', f'-runs={args.runs}', f'-max_total_time={args.seconds}',
                    f'-rss_limit_mb={args.rss_mb}', f'-max_len={args.max_len}',
                    f'-seed={args.seed}', '-timeout=10', f'-artifact_prefix={artifacts}/']
        started = time.monotonic()
        with (directory / 'run.log').open('w') as log:
            process = subprocess.Popen(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                status = process.wait(timeout=args.seconds + 600)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                status = 124
        evidence['targets'].append({'name': name, 'command': command, 'status': status,
                                    'elapsed_seconds': time.monotonic() - started})
        if hashlib.sha256((ROOT / 'fuzz/Cargo.lock').read_bytes()).hexdigest() != evidence['lockfile_sha256']:
            evidence['lockfile_changed'] = True
            failed = True
        (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
        print(f'{name}: status {status}, evidence {directory}', flush=True)
        failed |= status != 0
    raise SystemExit(int(failed))


if __name__ == '__main__':
    main()
