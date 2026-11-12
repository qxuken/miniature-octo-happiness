#!/usr/bin/env python3
"""Measure Rust line/branch/condition coverage, or reject unavailable MC/DC."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
EXCLUSIONS = r'/(tests|examples|tools)/|/\.rustup/|/\.cargo/registry/|^/rustc/'


def output(*args, env=None):
    return subprocess.check_output([str(a) for a in args], cwd=ROOT, env=env, text=True)


def source_hashes(scope):
    names = output('git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z').split('\0')
    prefixes = ('src/', 'crates/') if scope == 'library' else ('src/', 'crates/', 'apps/', 'tests/', 'examples/', 'tools/')
    return {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
            for name in names if name and (ROOT / name).is_file()
            and not name.startswith(('fuzz/', 'probe/', 'tools/atomic-model/'))
            and ((name.endswith('.rs') and name.startswith(prefixes))
                 or Path(name).name in ('Cargo.toml', 'Cargo.lock'))}


def metric_counts(totals, metric):
    """Reject absent, malformed and impossible denominators instead of guessing."""
    value = totals.get(metric)
    if not isinstance(value, dict):
        raise RuntimeError(f'Missing requested metric: {metric}')
    count, covered = value.get('count'), value.get('covered')
    if type(count) is not int or type(covered) is not int or not 0 <= covered <= count:
        raise RuntimeError(f'Invalid counts for metric: {metric}')
    if count == 0:
        raise RuntimeError(f'Requested metric has a zero denominator: {metric}')
    return count, covered


def export_totals(exported):
    data = json.loads(exported).get('data')
    if not isinstance(data, list) or len(data) != 1 or not isinstance(data[0], dict) or not isinstance(data[0].get('totals'), dict):
        raise RuntimeError('Expected one merged coverage export with totals')
    return data[0]['totals']


def coverage_files(exported, scope):
    files = json.loads(exported)['data'][0].get('files')
    if not isinstance(files, list) or not files:
        raise RuntimeError('Coverage export contains no measured source files')
    prefixes = ('src', 'crates') if scope == 'library' else ('src', 'crates', 'apps')
    names = []
    for file in files:
        name = file.get('filename') if isinstance(file, dict) else None
        if not isinstance(name, str) or not any(Path(name).resolve().is_relative_to(ROOT / prefix) for prefix in prefixes):
            raise RuntimeError(f'Coverage includes source outside requested repository scope: {name}')
        names.append(name)
    return names


def validate_calibration(totals, mode, case):
    expected_branches = 2 if case == 'partial' else 6
    if metric_counts(totals, 'branches') != (6, expected_branches):
        raise RuntimeError('Calibration did not measure the expected six branch outcomes')
    if mode == 'mcdc':
        expected = {'partial': 0, 'branch-complete': 2, 'complete': 3}[case]
        if metric_counts(totals, 'mcdc') != (3, expected):
            raise RuntimeError('Calibration did not measure the expected independent condition effects')


def ensure_stable(evidence, scope):
    evidence['source_stable'] = (source_hashes(scope) == evidence['source_files_sha256']
        and hashlib.sha256((ROOT / 'tools/coverage-fixture.rs').read_bytes()).hexdigest()
        == evidence['fixture_sha256']
        and hashlib.sha256(Path(__file__).read_bytes()).hexdigest() == evidence['runner_sha256'])
    if not evidence['source_stable']:
        raise RuntimeError('Source changed during coverage measurement')


def calibrate(rustc, flags, cov, profdata, dest, mode, env):
    directory = dest / 'calibration'
    directory.mkdir(parents=True, exist_ok=True)
    binary = directory / ('fixture.exe' if os.name == 'nt' else 'fixture')
    subprocess.run([*rustc, *flags, str(ROOT / 'tools/coverage-fixture.rs'), '-o', str(binary)],
                   cwd=ROOT, env=env, check=True)
    evidence = {}
    for case in ['partial', 'branch-complete', 'complete']:
        raw = directory / (case + '.profraw')
        raw.unlink(missing_ok=True)
        profile = directory / (case + '.profdata')
        subprocess.run([str(binary), case], env={**env, 'LLVM_PROFILE_FILE': str(raw)}, check=True)
        subprocess.run([str(profdata), 'merge', '-sparse', str(raw), '-o', str(profile)], check=True)
        exported = output(cov, 'export', binary, f'-instr-profile={profile}')
        (directory / (case + '.json')).write_text(exported)
        totals = export_totals(exported)
        validate_calibration(totals, mode, case)
        evidence[case] = totals
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=['line', 'branch', 'condition', 'mcdc'], default='line')
    parser.add_argument('--toolchain', help='Defaults to active toolchain for line mode, nightly otherwise')
    parser.add_argument('--scope', choices=['workspace', 'library'], default='workspace')
    parser.add_argument('--fixture-only', action='store_true')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.fixture_only and args.mode == 'line':
        parser.error('The calibration fixture requires a branch, condition, or MC/DC mode')
    toolchain = args.toolchain or (None if args.mode == 'line' else 'nightly')
    selector = [f'+{toolchain}'] if toolchain else []
    rustc, cargo = ['rustc', *selector], ['cargo', *selector]
    compiler = output(*rustc, '-vV')
    host = next(line.split(': ', 1)[1] for line in compiler.splitlines() if line.startswith('host:'))
    dest = (args.output or ROOT / 'target' / ('coverage' if args.mode == 'line' else f'coverage-{args.mode}')).resolve()
    dest.mkdir(parents=True, exist_ok=True)
    flags = ['-C', 'instrument-coverage']
    if args.mode != 'line':
        flags += ['-Z', f'coverage-options={args.mode}']
    evidence = {'mode': args.mode, 'scope': args.scope, 'compiler': compiler, 'rustflags': flags,
                'features': 'all', 'source': output('git', 'rev-parse', 'HEAD').strip(),
                'dirty': bool(output('git', 'status', '--porcelain').strip()),
                'exclusions': EXCLUSIONS, 'status': 'incomplete',
                'source_files_sha256': source_hashes(args.scope),
                'fixture_sha256': hashlib.sha256((ROOT / 'tools/coverage-fixture.rs').read_bytes()).hexdigest(),
                'runner_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), 'commands': []}
    manifest = dest / 'evidence.json'
    try:
        # Fail before testing if rustc rejects the requested metric. LLVM alone
        # having a report column does not establish compiler instrumentation.
        probe = subprocess.run([*rustc, *flags, '--print', 'cfg'], cwd=ROOT, text=True, capture_output=True)
        evidence['probe'] = {'returncode': probe.returncode, 'stderr': probe.stderr}
        if probe.returncode:
            evidence['status'] = 'unsupported'
            raise RuntimeError(f'Requested {args.mode} instrumentation is unavailable:\n{probe.stderr}')
        llvm = Path(output(*rustc, '--print', 'sysroot').strip()) / 'lib/rustlib' / host / 'bin'
        suffix = '.exe' if os.name == 'nt' else ''
        cov, profdata = (llvm / (name + suffix) for name in ['llvm-cov', 'llvm-profdata'])
        if not cov.exists() or not profdata.exists():
            raise RuntimeError('Install matching llvm-tools-preview for the selected toolchain')
        evidence['llvm_cov'] = output(cov, '--version')
        profiles = dest / 'profiles'
        profiles.mkdir(parents=True, exist_ok=True)
        for path in profiles.glob('*.profraw'):
            path.unlink()
        env = os.environ.copy()
        evidence['replaced_flag_variables'] = [name for name in ['RUSTFLAGS', 'RUSTDOCFLAGS', 'CARGO_ENCODED_RUSTFLAGS'] if name in env]
        env.pop('CARGO_ENCODED_RUSTFLAGS', None)
        env.update({'CARGO_INCREMENTAL': '0', 'CARGO_TARGET_DIR': str(dest), 'RUSTFLAGS': ' '.join(flags),
                    'RUSTDOCFLAGS': ' '.join(flags), 'LLVM_PROFILE_FILE': str(profiles / '%p-%m.profraw')})
        if args.mode != 'line':
            evidence['calibration'] = calibrate(rustc, flags, cov, profdata, dest, args.mode, env)
        if args.fixture_only:
            ensure_stable(evidence, args.scope)
            evidence['status'] = 'calibrated'
            return
        scope = ['--workspace'] if args.scope == 'workspace' else ['-p', 'weekend-db', '--lib']
        command = [*cargo, 'test', '--offline', '--locked', *scope, '--all-features']
        evidence['commands'].append(command + ['--no-run', '--message-format=json'])
        artifacts = output(*evidence['commands'][-1], env=env)
        objects = sorted({item['executable'] for line in artifacts.splitlines()
                          if (item := json.loads(line)).get('reason') == 'compiler-artifact' and item.get('executable')})
        if not objects:
            raise RuntimeError('No executable test artifacts were collected')
        evidence['commands'].append(command)
        subprocess.run(command, cwd=ROOT, env=env, check=True)
        merged = dest / 'tests.profdata'
        subprocess.run([str(profdata), 'merge', '-sparse', *map(str, profiles.glob('*.profraw')), '-o', str(merged)], check=True)
        objects_args = [objects[0], f'-instr-profile={merged}', f'-ignore-filename-regex={EXCLUSIONS}']
        for obj in objects[1:]:
            objects_args += ['-object', obj]
        report_args = ['--show-branch-summary'] if args.mode != 'line' else []
        show_args = ['--show-branches=count'] if args.mode != 'line' else []
        if args.mode == 'mcdc':
            report_args += ['--show-mcdc-summary']
            show_args += ['--show-mcdc']
        report = output(cov, 'report', *objects_args, *report_args)
        (dest / 'report.txt').write_text(report)
        exported = output(cov, 'export', *objects_args)
        (dest / 'export.json').write_text(exported)
        evidence['totals'] = export_totals(exported)
        evidence['measured_files'] = coverage_files(exported, args.scope)
        evidence['objects'] = objects
        metric = 'mcdc' if args.mode == 'mcdc' else 'lines' if args.mode == 'line' else 'branches'
        metric_counts(evidence['totals'], metric)
        subprocess.run([str(cov), 'show', *objects_args, *show_args, '-format=html', f'-output-dir={dest / "html"}'], cwd=ROOT, check=True)
        ensure_stable(evidence, args.scope)
        evidence['status'] = 'measured'
        print(report)
        print(f'Metric {args.mode}, scope {args.scope}, all features. Evidence and HTML in {dest}')
    finally:
        manifest.write_text(json.dumps(evidence, indent=2) + '\n')


if __name__ == '__main__':
    main()
