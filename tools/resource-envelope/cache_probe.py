#!/usr/bin/env python3
"""Disposable cache-copy/rebuild work probe. Never use its timings or RSS."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shutil

SPEC = importlib.util.spec_from_file_location('cache_growth', Path(__file__).with_name('growth_probe.py'))
GROWTH = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(GROWTH)
C = GROWTH.COMPOUND
FIELDS = ['value_clones', 'data_clones', 'value_rebuild_entries', 'data_sort_entries',
          'data_compares', 'value_slots_copied', 'data_positions_copied', 'data_shifted']


def instrument(root):
    path = Path(root) / 'src/storage/codec.rs'; text = path.read_text()
    C.require('CACHE_WORK' not in text, 'already instrumented')
    def add(needle, increments, after=False):
        nonlocal text
        C.require(text.count(needle) == 1, 'unique probe site required: ' + needle)
        prefix = ''.join(f'CACHE_{key.upper()}.fetch_add(({value}) as u64, std::sync::atomic::Ordering::Relaxed);\n' for key, value in increments)
        text = text.replace(needle, needle + "\n" + prefix if after else prefix + needle)
    old = '    cache.extend(old_cache.iter().cloned());'
    new = '        let slots = fork_vec(mem, &self.index.slots, 2 * MAX_OBJECTS)?;'
    C.require((old in text) != (new in text), 'exactly one supported cache policy required')
    if old in text:
        add(old, [('value_clones', 'old_cache.len()')])
        add('    data_cache.extend(old_data.iter().cloned());', [('data_clones', 'old_data.len()')])
    else:
        add(new, [('value_clones', 'entries.len()')])
        add(new, [('value_slots_copied', 'slots.len()')], after=True)
        add('        let order = fork_vec(mem, &self.index.order, MAX_OBJECTS)?;', [('data_clones', 'entries.len()')])
        add('        let order = fork_vec(mem, &self.index.order, MAX_OBJECTS)?;',
            [('data_positions_copied', 'order.len()')], after=True)
    add('        for (i, old) in cache.iter().enumerate() {', [('value_rebuild_entries', 'cache.len()')])
    add('        index.order.sort_unstable_by(|a, b| {', [('data_sort_entries', 'cache.len()')])
    add('        a.cmp(&b)\n', [('data_compares', '1')])
    add('        self.order[position..].rotate_right(1);', [('data_shifted', 'self.order.len() - position - 1')])
    text += '\n' + '\n'.join(f'static CACHE_{key.upper()}: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);' for key in FIELDS)
    text += '\npub(super) fn report_cache_work() { eprintln!("CACHE_WORK ' + ' '.join(key + '={}' for key in FIELDS) + '", ' + ', '.join('CACHE_' + key.upper() + '.load(std::sync::atomic::Ordering::Relaxed)' for key in FIELDS) + '); }\n'
    path.write_text(text)
    path = Path(root) / 'src/storage/mod.rs'
    path.write_text(path.read_text() + '\npub(crate) fn report_cache_work() { codec::report_cache_work(); }\n')
    GROWTH.instrument(root)
    path = Path(root) / 'src/memory.rs'; text = path.read_text()
    needle = 'VEC_NEW_BYTES.load(Ordering::Relaxed));'
    C.require(text.count(needle) == 1, 'unique domain reporter required')
    path.write_text(text.replace(needle, needle + '\n        crate::storage::report_cache_work();'))


def parse(text):
    pattern = 'CACHE_WORK ' + ' '.join(key + r'=(\d+)' for key in FIELDS)
    lines = [line for line in text.splitlines() if line.startswith('CACHE_WORK')]
    C.require(bool(lines) and all(re.fullmatch(pattern, line) for line in lines), 'missing/malformed cache work')
    records = [dict(zip(FIELDS, map(int, re.fullmatch(pattern, line).groups()))) for line in lines]
    for key in FIELDS:
        C.require([r[key] for r in records] == sorted(r[key] for r in records), 'nonmonotonic cache work')
    return records[-1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--reference', required=True, type=Path)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    reference = json.loads(args.reference.read_text())
    GROWTH.validate_reference(reference, args.revision, C.SCALE.inventory(args.source))
    output = C.BASE.HELPERS.fresh_output(args.output); original = args.source.resolve()
    C.require(not output.is_relative_to(original) and not original.is_relative_to(output), 'source/output overlap')
    source = output / 'source'; shutil.copytree(original, source); instrument(source)
    C.worker(source, output / 'run', args.revision + '+cache-probe')
    measured = json.loads((output / 'run/evidence.json').read_text())
    summary = {'verified': False, 'scope': 'instrumented cache/allocation work only; exclude latency/RSS', 'profiles': {}}
    for profile, run in measured['runs'].items():
        other = reference['runs'][profile]; C.require(len(run['rows']) == len(other['rows']), 'different phase count')
        for actual, expected in zip(run['rows'][:-1], other['rows'][:-1]):
            for key in ['phase', 'revision', 'seq', 'changed', 'roots', 'digests', 'live_bytes', 'file_bytes']:
                C.require(actual[key] == expected[key], 'instrumentation changed state/layout: ' + key)
        log = (output / 'run' / (profile + '.stderr')).read_text()
        domains = GROWTH.parse(log)
        summary['profiles'][profile] = {'cache': parse(log), 'domains': domains,
            'copy_domain_peaks': [r['peak'] for r in domains if r['cap'] == 256 * 1024 * 1024]}
    summary.update(verified=True, source_hashes=measured['source_hashes'], binary_sha256=measured['binary_sha256'],
                   run_sha256=C.SCALE.sha(output / 'run/evidence.json'), reference_sha256=C.SCALE.sha(args.reference))
    (output / 'cache.json').write_text(json.dumps(summary, indent=2) + '\n')


if __name__ == '__main__': main()
