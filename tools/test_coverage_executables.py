"""Qualification boundary tests; real LLVM controls are recorded separately."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('driver', Path(__file__).with_name('coverage-executables.py'))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class Boundaries(unittest.TestCase):
    def test_only_libtest_artifacts_and_package_cwd(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'Cargo.toml').write_text('[package]\nname="pkg"\n')
            paths = [Path(tmp) / str(n) for n in range(4)]
            for path in paths:
                path.write_bytes(b'object')
            messages = [json.dumps({'reason': 'compiler-artifact', 'package_id': 'p',
                'target': {'name': str(n), 'kind': kind}, 'profile': {'test': test},
                'executable': str(paths[n])}) for n, (kind, test) in enumerate([
                    (['bin'], False), (['bench'], True), (['test'], True), (['bin'], True)])]
            result = driver.artifacts('\n'.join(messages), {'p': {'name': 'pkg', 'manifest_path': tmp + '/Cargo.toml'}})
            self.assertEqual([r['eligible'] for r in result], [False, False, True, True])
            self.assertTrue(all(r['cwd'] == tmp for r in result))

    def test_custom_harness_rejected_and_multitype_library_included(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / 'Cargo.toml'
            manifest.write_text('[[test]]\nname="custom"\nharness=false\n')
            package = {'manifest_path': str(manifest)}
            self.assertFalse(driver.libtest_target(package, {'kind': ['test'], 'name': 'custom'}))
            self.assertTrue(driver.libtest_target(package, {'kind': ['rlib', 'cdylib'], 'name': 'core'}))
            self.assertTrue(driver.libtest_target(package, {'kind': ['proc-macro'], 'name': 'derive'}))

    def test_exact_parent_pid_and_children_retained(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            for name in ['12-module.profraw', '123-module.profraw']:
                (directory / name).write_bytes(b'profile')
            parent, children = driver.split_profiles(directory, 12)
            self.assertEqual([p.name for p in parent], ['12-module.profraw'])
            self.assertEqual([p.name for p in children], ['123-module.profraw'])
            self.assertEqual(driver.profiles(children)[0]['pid'], 123)
            with self.assertRaises(RuntimeError):
                driver.split_profiles(directory, 99)
            (directory / 'invalid.profraw').touch()
            with self.assertRaises(RuntimeError):
                driver.split_profiles(directory, 12)

    def test_failed_and_empty_tests_rejected(self):
        good = 'test result: ok. 2 passed; 0 failed; 0 ignored;'
        self.assertEqual(driver.test_count(0, good), 2)
        self.assertEqual(driver.test_count(0, good.replace('2 passed', '99 passed') + '\n' + good), 2)
        for code, text in [(1, good), (0, ''), (0, good.replace('2 passed', '0 passed'))]:
            with self.assertRaises(RuntimeError):
                driver.test_count(code, text)

    def test_source_drift_rejected(self):
        driver.require_stable({'source': 'one'}, {'source': 'one'})
        with self.assertRaises(RuntimeError):
            driver.require_stable({'source': 'one'}, {'source': 'two'})

    def test_object_change_rejected_before_execution(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / 'object'
            binary.write_bytes(b'changed')
            with patch.object(driver.subprocess, 'Popen') as process:
                result = driver.execute({'object': str(binary), 'object_sha256': 'old', 'cwd': tmp},
                                        Path(tmp) / 'run', {}, Path(tmp), 'condition', 1)
            process.assert_not_called()
            self.assertEqual(result['status'], 'rejected')
            self.assertIn('changed before execution', result['error'])

    def test_invalid_profile_retains_rejected_metadata_and_next_target_runs(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / 'invalid-profile-test'
            binary.write_text('#!/bin/sh\ndir=${LLVM_PROFILE_FILE%/*}\nprintf profile > "$dir/not-a-pid.profraw"\nprintf "test result: ok. 1 passed; 0 failed;\\n"\n')
            binary.chmod(0o700)
            item = {'object': str(binary), 'object_sha256': driver.digest(binary), 'cwd': tmp}
            with patch.object(driver.cov, 'checked_llvm') as llvm:
                for name in ['first', 'next']:
                    result = driver.execute(item, Path(tmp) / name, {}, Path(tmp), 'condition', 1)
                    saved = json.loads((Path(tmp) / name / 'evidence.json').read_text())
                    self.assertEqual(saved, result)
                    self.assertEqual(result['status'], 'rejected')
                    self.assertIn('Unexpected raw profile filename', result['error'])
                    self.assertIsNone(result['all_raw_profiles'][0]['pid'])
                    self.assertFalse(result['all_raw_profiles'][0]['name_valid'])
                llvm.assert_not_called()

    def test_actual_failed_process_never_reaches_llvm(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / 'failed-test'
            binary.write_text('#!/bin/sh\nprintf "test result: ok. 2 passed; 0 failed;\\n"\nexit 1\n')
            binary.chmod(0o700)
            with patch.object(driver.cov, 'checked_llvm') as llvm:
                result = driver.execute({'object': str(binary), 'object_sha256': driver.digest(binary), 'cwd': tmp},
                                        Path(tmp) / 'run', {}, Path(tmp), 'condition', 1)
            llvm.assert_not_called()
            self.assertEqual(result['test_exit'], 1)
            self.assertEqual(result['status'], 'rejected')

    def test_llvm_mismatch_warning_rejected_even_success_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            import sys
            evidence = {}
            with self.assertRaises(RuntimeError):
                driver.cov.checked_llvm([sys.executable, '-c',
                    'import sys; print("warning: mismatched data", file=sys.stderr)'],
                    evidence, 'report', Path(tmp))
            self.assertEqual(evidence['status'], 'degraded')


if __name__ == '__main__':
    unittest.main()
