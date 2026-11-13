import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('scale', Path(__file__).with_name('run.py'))
scale = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scale)

class Controls(unittest.TestCase):
    def test_fresh_output_rejects_existing_and_dangling_before_commands(self):
        with tempfile.TemporaryDirectory() as tmp:
            existing = Path(tmp)/'existing'; existing.mkdir()
            dangling = Path(tmp)/'dangling'; dangling.symlink_to(Path(tmp)/'missing')
            for path in [existing, dangling]:
                with patch.object(sys, 'argv', ['run.py','--output',str(path)]), patch.object(scale, 'command') as cmd:
                    with self.assertRaises(SystemExit): scale.main()
                    cmd.assert_not_called()

    def test_build_overrides_removed_but_rustup_install_location_preserved(self):
        with patch.dict(os.environ, {'RUSTFLAGS':'bad','RUSTC_WRAPPER':'bad','CARGO_PROFILE_RELEASE_PANIC':'abort',
                                     'CARGO_BUILD_RUSTFLAGS':'bad','RUSTUP_HOME':'/toolchain','RUSTUP_TOOLCHAIN':'1.99.0'}, clear=True):
            env = scale.clean_env()
        self.assertEqual(env, {'RUSTUP_HOME':'/toolchain','RUSTUP_TOOLCHAIN':'1.99.0', 'CARGO_INCREMENTAL':'0'})

    def test_outcome_and_maintenance_oracle_rejects_early_limit_or_skipped_work(self):
        import copy
        phases = ['create'] + ['commit', 'retain', 'verify'] * 8 + [
            'reopen', 'verify-reopen', 'backup', 'restore', 'reopen-copy',
            'verify-copy', 'reopen-copy', 'verify-copy', 'compact',
            'reopen-compact', 'verify-compact']
        valid = []
        for phase in phases:
            valid += [{'event':'start','phase':phase}, {'event':'end','phase':phase,'ns':1}]
        valid += [{'event':'complete','verified':True,'width':128,'epochs':8,'shared':False,'limited_at':0}]
        scale.validate_events(valid, 'distinct', 128)
        early = copy.deepcopy(valid)
        early[-1].update(epochs=7, limited_at=8)
        wrong_width = copy.deepcopy(valid); wrong_width[-1]['width']=512
        wrong_shape = copy.deepcopy(valid); wrong_shape[-1]['shared']=True
        missing = copy.deepcopy(valid)
        index = next(i for i,e in enumerate(missing) if e.get('phase')=='backup')
        del missing[index:index+2]
        for invalid in [early, wrong_width, wrong_shape, missing, valid + [valid[-1]]]:
            with self.assertRaises(RuntimeError): scale.validate_events(invalid, 'distinct', 128)

    def test_history_roots_require_complete_maintenance_and_exact_history(self):
        import copy
        for roots in [1, 8, 32, 64]:
            phases = ['create'] + ['commit', 'retain', 'verify'] * roots + [
                'reopen', 'verify-reopen', 'backup', 'restore', 'reopen-copy',
                'verify-copy', 'reopen-copy', 'verify-copy', 'compact',
                'reopen-compact', 'verify-compact']
            events = []
            for phase in phases:
                events.extend([{'event':'start','phase':phase},
                               {'event':'end','phase':phase,'ns':1}])
            events.append({'event':'complete','verified':True,'width':8,
                           'epochs':roots,'shared':True,'limited_at':0})
            scale.validate_events(events, 'shared', 8, roots)
            wrong = copy.deepcopy(events); wrong[-1]['epochs'] = roots - 1
            limit = copy.deepcopy(events); limit[-1]['limited_at'] = roots
            missing = copy.deepcopy(events)
            i = next(i for i,e in enumerate(missing) if e.get('phase') == 'restore')
            del missing[i:i+2]
            short = events[:2] + events[8:]  # one complete commit/retain/verify omitted
            for invalid in [wrong, limit, missing, short]:
                with self.assertRaises(RuntimeError):
                    scale.validate_events(invalid, 'shared', 8, roots)

    def test_container_removal_failure_still_cleans_volume_and_checks_absence(self):
        import subprocess
        from types import SimpleNamespace
        for volumes, expected in [('', True), ('owned-data\n', False)]:
            effects = [subprocess.CalledProcessError(1, ['docker','rm']),
                       SimpleNamespace(stdout=''), SimpleNamespace(stdout=''),
                       SimpleNamespace(stdout=volumes)]
            with patch.object(scale, 'command', side_effect=effects) as cmd:
                result = scale.cleanup_docker('owned', 'owned-data')
            self.assertEqual(cmd.call_args_list[1].args[0], ['docker','volume','rm','owned-data'])
            self.assertEqual(result['container_absent'], True)
            self.assertEqual(result['volume_absent'], expected)
            self.assertEqual(result['attempts'][0]['ok'], False)
            self.assertEqual(cmd.call_count, 4)
        with patch.object(scale, 'command', side_effect=[None,None,OSError('daemon down')]):
            with self.assertRaises(OSError): scale.cleanup_docker('owned','owned-data')

    def test_instrumentation_unique_exact_sites_and_reject_reapplication(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); (root/'src/storage').mkdir(parents=True)
            for name in ['codec.rs','mod.rs']:
                (root/'src/storage'/name).write_bytes((scale.ROOT/'src/storage'/name).read_bytes())
            scale.instrument(root)
            text=(root/'src/storage/codec.rs').read_text()
            self.assertEqual(text.count('SERIAL_VALUE.fetch_add('),1)
            self.assertEqual(text.count('SERIAL_DATA.fetch_add('),1)
            # Same-node expression remains after the counter, preserving early-hit semantics.
            self.assertIn('Ordering::Relaxed); old.value.same_node(value)',text)
            with self.assertRaises(RuntimeError): scale.instrument(root)

    def test_instrumentation_accepts_legacy_data_and_rejects_ambiguous_or_missing_sites(self):
        legacy = '.find(|r| r.revision == head.data_revision)'
        indexed = 'fn compare(a: u64, b: u64) -> std::cmp::Ordering {\n        a.cmp(&b)\n    }'
        for site in [legacy, indexed, '', legacy + indexed, indexed + indexed]:
            with self.subTest(site=site), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp); (root/'src/storage').mkdir(parents=True)
                (root/'src/storage/codec.rs').write_text('old.value.same_node(value)\n' + site)
                (root/'src/storage/mod.rs').write_text('')
                if site in [legacy, indexed]:
                    scale.instrument(root)
                    text = (root/'src/storage/codec.rs').read_text()
                    self.assertEqual(text.count('SERIAL_DATA.fetch_add('), 1)
                    self.assertEqual(text.count('SERIAL_VALUE.fetch_add('), 1)
                    with self.assertRaises(RuntimeError): scale.instrument(root)
                else:
                    with self.assertRaises(RuntimeError): scale.instrument(root)

if __name__=='__main__': unittest.main()
