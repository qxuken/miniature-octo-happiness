#!/usr/bin/env python3
"""Report compatible parent-executable coverage separately, never a pooled percentage."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('coverage_helpers', ROOT / 'tools/coverage.py')
cov = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cov)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source_snapshot():
    return {**cov.source_hashes('workspace'), **{
        name: digest(ROOT / name) for name in
        ['tools/coverage.py', 'tools/coverage-executables.py', 'tools/coverage-fixture.rs']}}


def require_stable(before, current):
    if before != current:
        raise RuntimeError('Source or qualification tools changed during measurement')


def libtest_target(package, target):
    kind = target['kind']
    libraries = {'lib', 'rlib', 'dylib', 'cdylib', 'staticlib', 'proc-macro'}
    section = 'lib' if set(kind) <= libraries else 'bin' if kind == ['bin'] else 'test' if kind == ['test'] else None
    if section is None:
        return False
    manifest = tomllib.loads(Path(package['manifest_path']).read_text())
    entries = [manifest.get('lib', {})] if section == 'lib' else manifest.get(section, [])
    # Cargo marks custom harness artifacts as test-profile too. Never execute them.
    return not any(entry.get('harness') is False and
                   (section == 'lib' or entry.get('name') == target['name']) for entry in entries)


def artifacts(messages, packages):
    result = {}
    for line in messages.splitlines():
        item = json.loads(line)
        if item.get('reason') != 'compiler-artifact' or not item.get('executable'):
            continue
        target = item['target']
        package = packages[item['package_id']]
        label = package['name'] + '::' + target['name']
        eligible = (item.get('profile', {}).get('test') is True
                    and libtest_target(package, target))
        obj = str(Path(item['executable']).resolve())
        result[obj] = {'label': label, 'object': obj, 'kind': target['kind'],
                       'profile_test': item.get('profile', {}).get('test'),
                       'cwd': str(Path(package['manifest_path']).parent),
                       'eligible': eligible, 'object_sha256': digest(obj)}
    return sorted(result.values(), key=lambda item: (item['label'], item['object']))


def split_profiles(directory, pid):
    parent, other = [], []
    for path in sorted(directory.glob('*.profraw')):
        match = re.fullmatch(r'(\d+)-(.+)\.profraw', path.name)
        if not match:
            raise RuntimeError('Unexpected raw profile filename: ' + path.name)
        (parent if int(match[1]) == pid else other).append(path)
    if not parent:
        raise RuntimeError('No raw profile from the executed parent PID')
    return parent, other


class EmptyTests(RuntimeError):
    pass


def test_count(code, text):
    matches = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed;', text)
    if code != 0 or not matches or any(int(failed) for _, failed in matches):
        raise RuntimeError('Test executable failed or did not produce a successful libtest result')
    # Child programs may print their own summaries; libtest's parent summary is last.
    count = int(matches[-1][0])
    if count == 0:
        raise EmptyTests('Test executable ran zero tests')
    return count


def profiles(paths):
    result = []
    for path in paths:
        match = re.fullmatch(r'(\d+)-(.+)\.profraw', path.name)
        result.append({'path': str(path), 'pid': int(match[1]) if match else None,
                       'name_valid': match is not None, 'sha256': digest(path)})
    return result


def execute(item, dest, env, llvm, mode, timeout):
    dest.mkdir()
    raw = dest / 'profiles'
    raw.mkdir()
    result = {**item, 'status': 'incomplete', 'child_profiles_qualified': False}
    binary = item['object']
    try:
        if digest(binary) != item['object_sha256']:
            raise RuntimeError('Executable changed before execution')
        with (dest / 'tests.log').open('w') as log:
            process = subprocess.Popen([binary], cwd=item['cwd'], start_new_session=True, env={**env, 'LLVM_PROFILE_FILE': str(raw / '%p-%m.profraw')},
                                       stdout=log, stderr=subprocess.STDOUT)
            result['pid'] = process.pid
            try:
                code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=10)
                raise RuntimeError('Test executable timed out; its process group was killed')
        result['test_exit'] = code
        result['tests_passed'] = test_count(code, (dest / 'tests.log').read_text())
        parent, children = split_profiles(raw, process.pid)
        result['parent_profiles'] = profiles(parent)
        result['child_profiles_unqualified'] = profiles(children)
        merged = dest / 'parent.profdata'
        # Reject merge warnings too; no silent counter reconciliation.
        cov.checked_llvm([llvm / 'llvm-profdata', 'merge', '-sparse', *parent, '-o', merged], result, 'merge', dest)
        result['profile_sha256'] = digest(merged)
        args = [binary, '-instr-profile=' + str(merged), '-ignore-filename-regex=' + cov.EXCLUSIONS]
        report = cov.checked_llvm([llvm / 'llvm-cov', 'report', *args, '--show-branch-summary'], result, 'report', dest)
        (dest / 'report.txt').write_text(report)
        exported = cov.checked_llvm([llvm / 'llvm-cov', 'export', *args], result, 'export', dest)
        (dest / 'export.json').write_text(exported)
        result['totals_for_this_object_only'] = cov.export_totals(exported)
        cov.metric_counts(result['totals_for_this_object_only'], 'branches')
        result['measured_files'] = cov.coverage_files(exported, 'workspace')
        cov.checked_llvm([llvm / 'llvm-cov', 'show', *args, '--show-branches=count', '-format=html',
                          '-output-dir=' + str(dest / 'html')], result, 'html', dest)
        if digest(binary) != item['object_sha256']:
            raise RuntimeError('Executable changed during measurement')
        result['status'] = 'measured-parent-object'
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        result['status'] = ('unqualified-empty-tests' if isinstance(error, EmptyTests) else
                            'unqualified-empty-production-scope' if 'zero denominator' in str(error) else 'rejected')
        result['error'] = str(error)
    finally:
        result['all_raw_profiles'] = profiles(sorted(raw.glob('*.profraw')))
        (dest / 'evidence.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New evidence directory')
    parser.add_argument('--toolchain', default='nightly-2026-10-09')
    parser.add_argument('--mode', choices=['branch', 'condition'], default='condition')
    parser.add_argument('--select', action='append', help='Exact package::target label; default all eligible libtest artifacts')
    parser.add_argument('--timeout', type=int, default=180)
    args = parser.parse_args()
    if args.timeout < 1 or args.timeout > 1800:
        parser.error('timeout must be 1..1800 seconds')
    if os.name != 'posix':
        parser.error('this qualification driver requires POSIX process groups')
    requested = Path(os.path.abspath(args.output))
    if requested.exists() or requested.is_symlink() or not requested.parent.is_dir():
        parser.error('output must be new with an existing parent')
    dest = requested.parent.resolve() / requested.name
    dest.mkdir()
    evidence = {'status': 'incomplete', 'source': cov.output('git', 'rev-parse', 'HEAD').strip(),
                'dirty': bool(cov.output('git', 'status', '--porcelain').strip()),
                'source_files_sha256': source_snapshot(), 'results': [],
                'scope': 'Separate parent executable mappings; children/build profiles and unexecuted artifacts unqualified; no projectwide union or aggregate percentage',
                'exclusions': cov.EXCLUSIONS, 'source_stable': False}
    env = os.environ.copy()
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    rustc, cargo = ['rustc', '+' + args.toolchain], ['cargo', '+' + args.toolchain]
    flags = ['-C', 'instrument-coverage', '-Z', 'coverage-options=' + args.mode]
    evidence['flags'] = flags
    try:
        evidence['compiler'] = cov.output(*rustc, '-vV')
        host = next(line.split(': ', 1)[1] for line in evidence['compiler'].splitlines() if line.startswith('host:'))
        runtime = Path(cov.output(*rustc, '--print', 'sysroot').strip()) / 'lib/rustlib' / host / 'lib'
        llvm = runtime.parent / 'bin'
        # Cargo normally supplies these for test executables linked to dynamic std/deps.
        loader = 'DYLD_FALLBACK_LIBRARY_PATH' if 'apple-darwin' in host else 'LD_LIBRARY_PATH'
        env[loader] = os.pathsep.join([str(runtime), str(dest / 'build/debug/deps'), env.get(loader, '')])
        evidence['runtime_library_path'] = {loader: env[loader]}
        evidence['llvm_cov'] = cov.output(llvm / 'llvm-cov', '--version')
        build_profiles = dest / 'build-profiles'
        build_profiles.mkdir()
        env.update(CARGO_TARGET_DIR=str(dest / 'build'), CARGO_INCREMENTAL='0', RUSTFLAGS=' '.join(flags),
                   RUSTDOCFLAGS=' '.join(flags), LLVM_PROFILE_FILE=str(build_profiles / '%p-%m.profraw'))
        evidence['calibration'] = cov.calibrate(rustc, flags, llvm / 'llvm-cov', llvm / 'llvm-profdata', dest, args.mode, env, evidence)
        metadata = json.loads(cov.output(*cargo, 'metadata', '--offline', '--locked', '--no-deps', '--format-version=1', env=env))
        packages = {p['id']: p for p in metadata['packages']}
        command = [*cargo, 'test', '--offline', '--locked', '--workspace', '--all-features', '--no-run', '--message-format=json']
        evidence['build_command'] = command
        built = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=600)
        (dest / 'build.jsonl').write_text(built.stdout)
        (dest / 'build.stderr').write_text(built.stderr)
        built.check_returncode()
        inventory = artifacts(built.stdout, packages)
        selected = [item for item in inventory if item['eligible'] and (not args.select or item['label'] in args.select)]
        if not selected or (args.select and set(args.select) - {item['label'] for item in selected}):
            raise RuntimeError('Selected labels did not identify eligible libtest executables')
        evidence['artifacts'] = [{**item, 'selected': item in selected,
            'unqualified_reason': None if item in selected else 'not selected' if item['eligible'] else 'not an eligible libtest artifact'} for item in inventory]
        for n, item in enumerate(selected):
            require_stable(evidence['source_files_sha256'], source_snapshot())
            evidence['results'].append(execute(item, dest / f'object-{n:03}', env, llvm, args.mode, args.timeout))
        require_stable(evidence['source_files_sha256'], source_snapshot())
        evidence['source_stable'] = True
        evidence['build_profiles_unqualified'] = profiles(sorted(build_profiles.glob('*.profraw')))
        evidence['status'] = 'completed-separated-reports' if all(r['status'] == 'measured-parent-object' for r in evidence['results']) else 'degraded-separated-reports'
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        evidence['status'] = 'rejected'
        evidence['error'] = str(error)
    finally:
        (dest / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    return int(evidence['status'] != 'completed-separated-reports')


if __name__ == '__main__':
    raise SystemExit(main())
