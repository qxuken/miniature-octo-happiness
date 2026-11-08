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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--historical', default='a7dc343')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
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
