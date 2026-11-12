"""Fail-closed coverage oracle checks; no compiler instrumentation is mocked as measured."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('coverage_runner', Path(__file__).with_name('coverage.py'))
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)


class CoverageOracles(unittest.TestCase):
    def totals(self, case, mcdc=True):
        value = {'branches': {'count': 6, 'covered': 2 if case == 'partial' else 6}}
        if mcdc:
            value['mcdc'] = {'count': 3, 'covered': {'partial': 0, 'branch-complete': 2, 'complete': 3}[case]}
        return value

    def test_exact_independent_counts_and_complete_branches_are_distinct(self):
        for case in ['partial', 'branch-complete', 'complete']:
            coverage.validate_calibration(self.totals(case), 'mcdc', case)
        # Full branch coverage must not substitute for the missing B effect.
        with self.assertRaises(RuntimeError):
            coverage.validate_calibration(self.totals('branch-complete'), 'mcdc', 'complete')
        with self.assertRaises(RuntimeError):
            coverage.validate_calibration(self.totals('complete'), 'mcdc', 'branch-complete')

    def test_partial_mcdc_cannot_accept_missing_or_arbitrary_counts(self):
        for covered in [None, -1, 1, 2, 3, 4, True, '0']:
            totals = self.totals('partial')
            totals['mcdc']['covered'] = covered
            with self.subTest(covered=covered), self.assertRaises(RuntimeError):
                coverage.validate_calibration(totals, 'mcdc', 'partial')

    def test_requested_metric_absent_or_zero_is_not_measurement(self):
        for metric in ['lines', 'branches', 'mcdc']:
            for value in [None, {}, {'count': 0, 'covered': 0}, {'count': 2, 'covered': 3}, {'count': True, 'covered': 1}]:
                with self.subTest(metric=metric, value=value), self.assertRaises(RuntimeError):
                    coverage.metric_counts({metric: value}, metric)

    def test_branch_and_condition_do_not_claim_mcdc(self):
        for mode in ['branch', 'condition']:
            for case in ['partial', 'branch-complete', 'complete']:
                totals = self.totals(case, mcdc=False)
                totals['mcdc'] = {'count': 0, 'covered': 0}
                coverage.validate_calibration(totals, mode, case)
        with self.assertRaises(RuntimeError):
            coverage.validate_calibration(self.totals('partial', mcdc=False), 'mcdc', 'partial')

    def test_incomplete_branch_calibration_rejects(self):
        for case in ['branch-complete', 'complete']:
            with self.assertRaises(RuntimeError):
                coverage.validate_calibration(self.totals('partial'), 'condition', case)

    def test_export_cannot_silently_ignore_other_data_sets(self):
        totals = self.totals('complete')
        self.assertEqual(coverage.export_totals(json.dumps({'data': [{'totals': totals}]})), totals)
        for data in [[], [None], [{'totals': totals}, {'totals': totals}], [{}]]:
            with self.subTest(data=data), self.assertRaises(RuntimeError):
                coverage.export_totals(json.dumps({'data': data}))

    def test_remapped_sysroot_and_wrong_target_are_outside_measured_scope(self):
        import re
        self.assertRegex('/rustc/hash/library/std/src/thread_local.rs', coverage.EXCLUSIONS)
        self.assertIsNone(re.search(coverage.EXCLUSIONS, str(coverage.ROOT / 'src/lib.rs')))
        def exported(name):
            return json.dumps({'data': [{'files': [{'filename': name}]}]})
        local = str(coverage.ROOT / 'src/lib.rs')
        self.assertEqual(coverage.coverage_files(exported(local), 'library'), [local])
        for name in ['/rustc/hash/library/std/src/thread_local.rs', '/outside/source.rs', str(coverage.ROOT / 'apps/example.rs')]:
            with self.subTest(name=name), self.assertRaises(RuntimeError):
                coverage.coverage_files(exported(name), 'library')
        with self.assertRaises(RuntimeError):
            coverage.coverage_files(json.dumps({'data': [{'files': []}]}), 'workspace')

    def test_fixture_only_and_library_scope_bind_fixture_and_runner(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'tools').mkdir()
            fixture = root / 'tools/coverage-fixture.rs'
            fixture.write_text('first')
            evidence = {'source_files_sha256': {'src/lib.rs': 'hash'},
                        'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(),
                        'runner_sha256': hashlib.sha256(Path(coverage.__file__).read_bytes()).hexdigest()}
            with patch.object(coverage, 'ROOT', root), patch.object(coverage, 'source_hashes', return_value={'src/lib.rs': 'hash'}):
                coverage.ensure_stable(evidence, 'library')
                changed = copy.deepcopy(evidence)
                changed['runner_sha256'] = 'changed'
                with self.assertRaises(RuntimeError):
                    coverage.ensure_stable(changed, 'library')
                fixture.write_text('second')
                with self.assertRaises(RuntimeError):
                    coverage.ensure_stable(evidence, 'library')
                self.assertFalse(evidence['source_stable'])


if __name__ == '__main__':
    unittest.main()
