"""Private runner no-overwrite, optimized-check and frozen-source controls."""
import argparse
import hashlib
import importlib.util
import io
import json
import os
import signal
import time
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('compatibility', Path(__file__).with_name('compatibility.py'))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class CompatibilityTests(unittest.TestCase):
    def consumer(self, directory, fault='none'):
        script = directory / 'consumer.py'
        script.write_text(f'''#!{sys.executable}
from pathlib import Path
import sys
mode,path=sys.argv[1],Path(sys.argv[2])
if mode=='write': path.write_bytes(bytes(64))
elif mode=={fault!r}: path.write_bytes(path.read_bytes()+b'control-mutation')
''')
        script.chmod(0o700)
        return script

    def test_both_profiles_reject_existing_and_dangling_outputs_before_build(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            file = root / 'file'
            file.write_bytes(b'preserve')
            directory = root / 'directory'
            directory.mkdir()
            link = root / 'link'
            link.symlink_to(directory)
            dangling = root / 'dangling'
            dangling.symlink_to(root / 'missing')
            for profile in ('storage', 'snapshot'):
                for path in (file, directory, link, dangling):
                    with self.subTest(profile=profile, path=path), patch.object(
                            runner, 'build_endpoints', side_effect=AssertionError('must not build')):
                        with self.assertRaises(ValueError):
                            runner.main(['--profile', profile, '--output', str(path)])
            self.assertEqual(file.read_bytes(), b'preserve')
            self.assertFalse((root / 'missing').exists())
            self.assertEqual(list(directory.iterdir()), [])

    def test_successful_storage_consumers_check_both_writer_directions(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            consumer = self.consumer(root)
            evidence = {'commands': []}
            runner.storage_cases(root, {'historical': consumer, 'current': consumer}, evidence)
            self.assertEqual(set(evidence['results']), {'historical-writer', 'current-writer'})
            self.assertEqual(len(evidence['commands']), 10)
            self.assertTrue(all(v['read_bytes_unchanged'] and v['unsupported_rejected_unchanged']
                                for v in evidence['results'].values()))

    def test_zero_exit_mutating_reader_fails_and_preserves_failure_evidence(self):
        self.mutating_consumer_control('read', 'changed witnessed image')

    def test_zero_exit_mutating_rejector_fails_and_preserves_failure_evidence(self):
        self.mutating_consumer_control('reject', 'wrote unsupported image')

    def mutating_consumer_control(self, fault, message):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            consumer = self.consumer(root, fault)
            output = root / 'output'
            def build(args, output, evidence):
                evidence['builds'] = {}
                return {'historical': consumer, 'current': consumer}
            with patch.object(runner, 'build_endpoints', side_effect=build):
                with self.assertRaisesRegex(RuntimeError, message):
                    runner.main(['--current', 'explicit-current', '--output', str(output)])
            evidence = json.loads((output / 'evidence.json').read_text())
            self.assertFalse(evidence['verified'])
            self.assertIn(message, evidence['failure'])
            self.assertEqual(evidence['requested_sources']['current'], 'explicit-current')
            self.assertTrue(all(command['returncode'] == 0 for command in evidence['commands']))

    def test_requested_commits_original_locks_and_locked_example_overlays(self):
        revisions = {'old': '1' * 40, 'new': '2' * 40}
        archives = {}
        for revision in revisions.values():
            memory = io.BytesIO()
            with tarfile.open(fileobj=memory, mode='w') as tar:
                entry = tarfile.TarInfo('tools')
                entry.type = tarfile.DIRTYPE
                tar.addfile(entry)
                for name, contents in [('Cargo.toml', '[package]\npublish = false\n'),
                                       ('src/version.rs', '// native identity API marker\n'),
                                       ('Cargo.lock', 'original-lock-' + revision)]:
                    data = contents.encode()
                    entry = tarfile.TarInfo(name)
                    entry.size = len(data)
                    tar.addfile(entry, io.BytesIO(data))
            archives[revision] = memory.getvalue()
        def capture(*args):
            if args[:3] == ('git', 'rev-parse', '--verify'):
                return revisions[args[3].removesuffix('^{commit}')]
            return 'mock toolchain'
        def archive(args, **kwargs):
            self.assertEqual(args[:3], ['git', 'archive', '--format=tar'])
            return archives[args[3]]
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            evidence = {'commands': []}
            calls = []
            def run(command, output, evidence, label, *, cwd=None, env=None, timeout=30):
                calls.append([str(arg) for arg in command])
                if command[0] == 'cargo':
                    self.assertIn('--offline', command)
                    self.assertIn('--locked', command)
                    self.assertNotIn('generate-lockfile', command)
                    self.assertTrue(str(command[-1]).startswith(str(output / 'source')))
                    examples = Path(env['CARGO_TARGET_DIR']) / 'debug/examples'
                    examples.mkdir(parents=True)
                    for name in ('qualification_fixture', 'qualification_identity'):
                        (examples / name).write_bytes(b'mock-binary-' + env['WEEKEND_DB_SOURCE_ID'].encode())
                    return ''
                return json.dumps({'identity_kind': 'engine_version_api', 'source_id': env['WEEKEND_DB_SOURCE_ID'], 'features': 0,
                                   'abi_version': 1, 'format_version': 2})
            args = argparse.Namespace(profile='storage', historical='old', current='new')
            with patch.object(runner, 'capture', side_effect=capture), patch.object(
                    runner.subprocess, 'check_output', side_effect=archive), patch.object(runner, 'run', side_effect=run):
                runner.build_endpoints(args, output, evidence)
            self.assertEqual(evidence['sources'], {'historical': '1' * 40, 'current': '2' * 40})
            for role in ('historical', 'current'):
                revision = evidence['sources'][role]
                contents = ('original-lock-' + revision).encode()
                self.assertEqual((output / (role + '-Cargo.lock')).read_bytes(), contents)
                self.assertEqual(evidence['builds'][role]['original_lock_sha256'], hashlib.sha256(contents).hexdigest())
                self.assertEqual(evidence['builds'][role]['runtime_identity']['source_id'], revision)
            self.assertEqual(len(calls), 4)

    def test_early_exited_leader_does_not_leave_running_descendant(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            marker = output / 'descendant.pid'
            program = (
                "import subprocess,sys; from pathlib import Path; "
                "child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); "
                "Path(sys.argv[1]).write_text(str(child.pid)); sys.exit(2)"
            )
            descendant = None
            try:
                with self.assertRaises(RuntimeError):
                    runner.run([sys.executable, '-c', program, marker], output,
                               {'commands': []}, 'early-exit', timeout=5)
                descendant = int(marker.read_text())
                deadline = time.monotonic() + 3
                while True:
                    state = subprocess.run(['ps', '-o', 'stat=', '-p', str(descendant)],
                                           capture_output=True, text=True, timeout=2)
                    # An orphan zombie has stopped executing; its new parent owns reaping.
                    if not state.stdout.strip() or state.stdout.strip().startswith('Z'):
                        break
                    self.assertLess(time.monotonic(), deadline, 'descendant still running')
                    time.sleep(.02)
            finally:
                if descendant is None and marker.exists():
                    descendant = int(marker.read_text())
                if descendant is not None:
                    try:
                        os.kill(descendant, signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_consumer_timeout_kills_and_reaps_process_group(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            evidence = {'commands': []}
            with self.assertRaises(subprocess.TimeoutExpired):
                runner.run([sys.executable, '-c', 'import time;time.sleep(30)'], root,
                           evidence, 'timeout-control', timeout=0.1)
            record = evidence['commands'][0]
            self.assertTrue(record['timed_out'])
            self.assertIsNotNone(record['returncode'])
            self.assertNotEqual(record['returncode'], 0)


if __name__ == '__main__':
    unittest.main()
