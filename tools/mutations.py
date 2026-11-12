#!/usr/bin/env python3
"""Replay safe guard mutations in an isolated copy of the current source tree."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
CASES = []


def add(name, file, old, new, test, target='--lib', occurrence=1, matches=None, package='weekend-db'):
    CASES.append(dict(name=name, file=file, old=old, new=new, test=test, target=target, occurrence=occurrence, matches=matches, package=package))


for field, condition in [
    ('depth-zero', 'self.max_depth == 0'), ('depth-upper', 'self.max_depth > 64'),
    ('entries', 'self.max_entries > 1_000_000'), ('writes', 'self.max_writes > 1_000_000'),
    ('observations', 'self.max_observations > 1_000_000'),
    ('bytes', 'self.max_bytes > isize::MAX as usize'),
    ('value-bytes', 'self.max_value_bytes > isize::MAX as usize'),
    ('transaction-bytes', 'self.max_transaction_bytes > isize::MAX as usize'),
]:
    add('memory-' + field, 'src/memory.rs', condition, 'false', 'allocation', '--test')
add('memory-capacity', 'src/memory.rs', 'needed > vec.capacity()', 'needed >= vec.capacity()', 'memory::tests::reservation_checks')
add('memory-abort-threshold', 'src/memory.rs', '>= isize::MAX as usize', '> isize::MAX as usize', 'memory::tests::shared_clone_aborts')
# Live-byte admission mutations never invalidate pointers or object ownership.
add('live-budget-equality', 'src/memory.rs', '*n <= self.0.limits.max_live_bytes',
    '*n < self.0.limits.max_live_bytes', 'memory::live_budget_tests')
add('live-budget-cap', 'src/memory.rs', '.filter(|n| *n <= self.0.limits.max_live_bytes)',
    '.filter(|_| true)', 'memory::live_budget_tests')
add('live-budget-overflow', 'src/memory.rs', 'used.checked_add(bytes)',
    'Some(used.wrapping_add(bytes))', 'memory::live_budget_tests')
add('live-budget-release', 'src/memory.rs', 'self.owner.0.used.fetch_sub(self.bytes, Ordering::AcqRel);',
    'self.owner.0.used.fetch_sub(0, Ordering::AcqRel);', 'memory::live_budget_tests')
add('live-budget-peak-sample', 'src/memory.rs', 'self.0.peak.fetch_max(live, Ordering::Relaxed).max(live)',
    'self.0.peak.load(Ordering::Relaxed)' , 'memory::live_budget_tests')
for name, old, new in [
    ('byte-null', 'buffer.is_null() && capacity != 0', 'buffer.is_null() || capacity != 0'),
    ('byte-capacity', 'capacity < bytes.len()', 'capacity <= bytes.len()'),
    ('byte-required', '*required = bytes.len();', '*required = 0;'),
    ('ids-null', 'buffer.is_null() && capacity!=0', 'buffer.is_null() || capacity!=0'),
    ('ids-capacity', 'capacity<ids.len()', 'capacity<=ids.len()'),
    ('ids-required', '*required=ids.len();', '*required=0;'),
    ('empty-input', 'if len == 0 {', 'if false {'),
    ('owner', 'if a == b {', 'if false {'),
]:
    add('ffi-' + name, 'src/ffi.rs', old, new, 'ffi::tests')
for name, occurrence in [('read', 1), ('write', 2)]:
    add('io-' + name + '-zero', 'src/storage/mod.rs', 'n == 0 || n > size', 'n <= 1 || n > size', 'transfer_tests', occurrence=occurrence)
    add('io-' + name + '-oversize', 'src/storage/mod.rs', 'n == 0 || n > size', 'n == 0 || n >= size', 'transfer_tests', occurrence=occurrence)
add('replacement-errors', 'src/storage/replacement.rs', 'apply(step)?;', 'let _ = apply(step);', 'namespace_publication_stops')
add('simulator-mask-boundary', 'src/storage/simulator.rs', 'index < 64 &&', 'index <= 64 &&', 'bytestore', '--test')
add('simulator-mask-order', 'src/storage/simulator.rs', '.enumerate().rev()', '.enumerate()', 'bytestore', '--test')

value_guard = 'weight > self.mem.limits().max_value_bytes\n            || value.node.depth >= self.mem.limits().max_depth'
for occurrence, operation in enumerate(['map-set', 'list-set', 'list-push', 'list-insert'], 1):
    for name, replacement in [
        ('weight', 'false\n            || value.node.depth >= self.mem.limits().max_depth'),
        ('depth', 'weight > self.mem.limits().max_value_bytes\n            || false'),
        ('weight-equality', 'weight >= self.mem.limits().max_value_bytes\n            || value.node.depth >= self.mem.limits().max_depth'),
    ]:
        add('value-' + operation + '-' + name, 'src/value.rs', value_guard, replacement,
            'values', '--test', occurrence=occurrence, matches=4)
add('tree-capacity-exists', 'src/tree.rs', '!exists && self.len >= mem.limits().max_entries',
    'self.len >= mem.limits().max_entries', 'tree::tests::map_capacity')
add('tree-capacity-bound', 'src/tree.rs', '!exists && self.len >= mem.limits().max_entries',
    '!exists', 'tree::tests::map_capacity')
add('tree-leaf-hash', 'src/tree.rs', 'return if *h == hash {',
    'return if true {', 'tree::tests::map_capacity')
add('tree-leaf-key', 'src/tree.rs', '.find(|(k, _)| k.as_slice() == key)',
    '.find(|_| true)', 'tree::tests::map_capacity')


# Both matches lie in production handshake admission, before the first test
# module. Each real-rustls test exercises both endpoints; mutate one operand at
# one endpoint only. The existing baseline rejects empty test selections.
for endpoint, occurrence in [('client', 1), ('server', 2)]:
    for operand, replacement in [
        ('handshaking', 'if tls.wants_write() {'),
        ('pending-write', 'if tls.is_handshaking() {'),
    ]:
        add('tls-' + endpoint + '-' + operand, 'apps/support/pinned_tls.rs',
            'if tls.is_handshaking() || tls.wants_write() {', replacement,
            'handshake_decision_tests', '--bins', occurrence=occurrence,
            matches=2, package='study-peer')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', action='append', help='Name prefix, repeatable, default all')
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    selected = [case for case in CASES if not args.case or any(case['name'].startswith(prefix) for prefix in args.case)]
    if not selected:
        parser.error('No cases selected')
    if args.list:
        print('\n'.join(case['name'] for case in selected))
        return
    output = args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix='weekend-mutations-'))
    output.mkdir(parents=True, exist_ok=True)
    checkout = output / 'source'
    checkout.mkdir(exist_ok=False)
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=ROOT).decode().split('\0')
    hashes = {}
    for name in names:
        source = ROOT / name
        if not name or not source.is_file() or source.resolve().is_relative_to(output):
            continue
        dest = checkout / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, dest)
        hashes[name] = hashlib.sha256(dest.read_bytes()).hexdigest()
    results = []
    baselines = set()
    for case in selected:
        command = ['cargo', 'test', '--offline', '--locked', '-p', case['package'], '--all-features', case['target'], case['test']]
        baseline = tuple(command)
        if baseline not in baselines:
            run = subprocess.run(command, cwd=checkout, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=120)
            (output / (case['name'] + '-baseline.log')).write_text(run.stdout)
            if run.returncode != 0 or 'test result: ok' not in run.stdout or 'running 0 tests' in run.stdout:
                raise RuntimeError('Baseline failed or matched no tests for ' + case['name'])
            baselines.add(baseline)
        path = checkout / case['file']
        original = path.read_text()
        production, marker, tests = original.partition('#[cfg(test)]\nmod ')
        parts = production.split(case['old'])
        n = case['occurrence']
        expected_matches = case['matches'] or (2 if case['file'] == 'src/storage/mod.rs' else 1)
        if len(parts) != expected_matches + 1 or len(parts) <= n:
            raise RuntimeError('Source expression changed for ' + case['name'])
        path.write_text(case['old'].join(parts[:n]) + case['new'] + case['old'].join(parts[n:]) + marker + tests)
        try:
            run = subprocess.run(command, cwd=checkout, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=120)
        finally:
            path.write_text(original)
        (output / (case['name'] + '.log')).write_text(run.stdout)
        caught = run.returncode == 101 and 'test result: FAILED' in run.stdout
        results.append(dict(case, caught=caught, status=run.returncode, command=command))
        (output / 'evidence.json').write_text(json.dumps({'source_sha256': hashes, 'results': results}, indent=2) + '\n')
        print(case['name'], 'caught' if caught else 'NOT CAUGHT', flush=True)
    print(output)
    raise SystemExit(int(any(not item['caught'] for item in results)))


if __name__ == '__main__':
    main()
