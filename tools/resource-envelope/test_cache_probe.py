"""Cache-probe patch-surface and evidence rejection controls."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('cache_probe', Path(__file__).with_name('cache_probe.py'))
probe = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(probe)


class CacheProbeControls(unittest.TestCase):
    def test_current_and_legacy_unique_sites_and_ambiguous_patch_rejection(self):
        root = probe.C.ROOT
        for legacy in [False, True]:
            with tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp); (tmp / 'src/storage').mkdir(parents=True)
                for name in ['src/storage/codec.rs', 'src/storage/mod.rs', 'src/memory.rs']:
                    (tmp / name).write_bytes((root / name).read_bytes())
                path = tmp / 'src/storage/codec.rs'; text = path.read_text()
                if legacy:
                    # Synthetic old patch surface; actual archived old Rust is
                    # separately compiled and exercised by the measured probe.
                    text = text.replace('        let slots = fork_vec(mem, &self.index.slots, 2 * MAX_OBJECTS)?;',
                                        '    cache.extend(old_cache.iter().cloned());')
                    text = text.replace('        let order = fork_vec(mem, &self.index.order, MAX_OBJECTS)?;',
                                        '    data_cache.extend(old_data.iter().cloned());')
                path.write_text(text); probe.instrument(tmp)
                self.assertEqual(path.read_text().count('CACHE_WORK'), 1)
                self.assertEqual((tmp / 'src/memory.rs').read_text().count('crate::storage::report_cache_work();'), 1)
                with self.assertRaises(AssertionError): probe.instrument(tmp)
        for text in ['no source match', (root / 'src/storage/codec.rs').read_text() +
                     '\n    cache.extend(old_cache.iter().cloned());']:
            with tempfile.TemporaryDirectory() as tmp:
                tmp = Path(tmp); (tmp / 'src/storage').mkdir(parents=True)
                (tmp / 'src/storage/codec.rs').write_text(text)
                with self.assertRaises(AssertionError): probe.instrument(tmp)

    def test_parser_rejects_missing_malformed_and_regressing_totals(self):
        line = 'CACHE_WORK ' + ' '.join(key + '=2' for key in probe.FIELDS) + '\n'
        self.assertEqual(probe.parse(line), dict.fromkeys(probe.FIELDS, 2))
        for text in ['', line + 'CACHE_WORK truncated\n', line.replace('=2', '=-1', 1),
                     line + line.replace('value_clones=2', 'value_clones=1')]:
            with self.assertRaises(AssertionError): probe.parse(text)


if __name__ == '__main__': unittest.main()
