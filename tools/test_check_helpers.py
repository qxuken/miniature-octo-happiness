"""Small disposable worker/control fixtures; never recursively run the registry."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import check_helpers as runner

class HelperRunnerTests(unittest.TestCase):
    def record(self):
        return {'complete': True, 'suite': '/fixture', 'optimize': 0,
                'discovered': ['helper_suite.Control.test_ok'], 'executed': ['helper_suite.Control.test_ok'],
                'tests_run': 1, 'failures': [], 'errors': [], 'skips': [],
                'expected_failures': [], 'unexpected_successes': []}

    def test_machine_validation_rejects_false_green_and_skip_masking(self):
        good = self.record(); row = {'allowed_skips': []}
        self.assertEqual(runner.validate(good, row, Path('/fixture'), 0)['passed'], 1)
        changes = [{'tests_run': 0, 'discovered': [], 'executed': []}, {'executed': []},
                   {'failures': [{'id': 'failed'}]}, {'errors': [{'id': 'error'}]},
                   {'expected_failures': ['expected']}, {'unexpected_successes': ['unexpected']},
                   {'skips': [{'id': good['discovered'][0], 'reason': 'hidden'}]}, {'complete': False}]
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                runner.validate({**good, **change}, row, Path('/fixture'), 0)
        skipped = {**good, 'skips': [{'id': good['discovered'][0], 'reason': 'registered'}]}
        with self.assertRaisesRegex(ValueError, 'all tests skipped'):
            runner.validate(skipped, {'allowed_skips': good['discovered']}, Path('/fixture'), 0)

    def test_registry_requires_all_discovered_suites_and_rejects_symlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); (root/'test_one.py').write_text('')
            path = root/'registry.json'; data = {'version': 1, 'suites': [{'path':'test_one.py','allowed_skips':[]}]}
            path.write_text(json.dumps(data)); runner.registry(root,path)
            (root/'test_omitted.py').write_text('')
            with self.assertRaisesRegex(ValueError, 'exactly cover'): runner.registry(root,path)
            (root/'test_omitted.py').unlink(); (root/'test_link.py').symlink_to(root/'test_one.py')
            with self.assertRaisesRegex(ValueError, 'symlink'): runner.registry(root,path)

    def test_output_admission_before_registry_or_children(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); existing=root/'existing';existing.mkdir();(existing/'keep').write_text('keep')
            link=root/'link';link.symlink_to(root/'absent')
            for output in [existing,link]:
                with patch.object(runner,'registry') as registry,patch.object(runner,'execute') as execute:
                    with self.assertRaises(ValueError):runner.main(['--output',str(output)])
                    registry.assert_not_called();execute.assert_not_called()
            self.assertEqual((existing/'keep').read_text(),'keep');self.assertFalse((root/'absent').exists())

    def test_machine_worker_result_has_actual_ids_and_assertions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();suite=root/'fixture.py';result=root/'result.json'
            suite.write_text('import unittest\nclass Control(unittest.TestCase):\n def test_ok(self): self.assertEqual(1,1)\n')
            runner.execute([sys.executable,'-I','-B',str(Path(runner.__file__).resolve()),'--worker',str(suite),'--output',str(result)],root,[])
            record=json.loads(result.read_text())
            self.assertEqual(runner.validate(record,{'allowed_skips':[]},suite,0),{'tests':1,'passed':1,'skipped':0})
            self.assertEqual(record['executed'],['helper_suite.Control.test_ok'])

    def test_real_zero_and_skipped_workers_cannot_be_false_green(self):
        programs = [
            '',
            'import unittest\nclass Control(unittest.TestCase):\n def test_ok(self): pass\n @unittest.skip("hidden")\n def test_skip(self): pass\n',
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            for index, program in enumerate(programs):
                directory = root / str(index); directory.mkdir()
                suite = directory / 'fixture.py'; suite.write_text(program)
                result = directory / 'result.json'
                runner.execute([sys.executable, '-I', '-B', str(Path(runner.__file__).resolve()),
                                '--worker', str(suite), '--output', str(result)], directory, [])
                record = json.loads(result.read_text())
                self.assertTrue(record['complete'])
                with self.assertRaises(ValueError):
                    runner.validate(record, {'allowed_skips': []}, suite, 0)

    def test_standard_dataclass_worker_module_registration(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();suite=root/'fixture.py';result=root/'result.json'
            suite.write_text('from __future__ import annotations\nimport unittest\nfrom dataclasses import dataclass\n@dataclass\nclass Data:\n n: int = 7\nclass Control(unittest.TestCase):\n def test_data(self): self.assertEqual(Data().n,7)\n')
            runner.execute([sys.executable,'-I','-B',str(Path(runner.__file__).resolve()),'--worker',str(suite),'--output',str(result)],root,[])
            self.assertEqual(runner.validate(json.loads(result.read_text()),{'allowed_skips':[]},suite,0)['passed'],1)

    def test_external_inputs_reject_missing_symlink_and_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();tools=root/'tools';tools.mkdir();source=root/'external';source.write_text('first')
            with patch.object(runner,'EXTERNAL_INPUTS',('external',)):
                before=runner.external_inputs(root)
                source.unlink()
                with self.assertRaises(ValueError):runner.external_inputs(root)
                target=root/'target-file';target.write_text('first');source.symlink_to(target)
                with self.assertRaises(ValueError):runner.external_inputs(root)
                source.unlink();source.write_text('first')
                def change(_):
                    source.write_text('changed');return {}
                with patch.object(runner,'HERE',tools),patch.object(runner,'registry',return_value=([],{})),patch.object(runner,'inventory',side_effect=change):
                    with self.assertRaisesRegex(ValueError,'external input changed'):
                        runner.main(['--output',str(root/'proof')])
                self.assertNotEqual(before,runner.external_inputs(root))

    def test_output_file_and_result_json_bounds(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);evidence=[]
            with self.assertRaises(ValueError):
                runner.execute([sys.executable,'-c','import os;os.write(1,b"x"*65536);os.write(1,b"x"*65536)'],root,evidence,file_limit=4096)
            self.assertLessEqual((root/'stdout.log').stat().st_size,4096)
            self.assertLessEqual((root/'stderr.log').stat().st_size,4096)
            self.assertIsNotNone(evidence[0]['exit'])
            with self.assertRaisesRegex(ValueError,'JSON exceeds'):
                runner.bounded_json(root/'stdout.log',maximum=100)

    def test_whole_run_deadline_prevents_next_worker(self):
        with tempfile.TemporaryDirectory() as temporary:
            row={'path':'fixture.py','allowed_skips':[]}
            with patch.object(runner,'registry',return_value=([row],{})),patch.object(runner,'external_inputs',return_value={}),patch.object(runner.time,'monotonic',side_effect=[100,701]),patch.object(runner,'execute') as execute:
                with self.assertRaisesRegex(ValueError,'whole-run deadline'):
                    runner.main(['--output',str(Path(temporary)/'proof')])
                execute.assert_not_called()

    def test_detached_nested_session_is_stopped_on_timeout_and_early_exit(self):
        for early in [False,True]:
            with self.subTest(early=early),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary).resolve();suite=root/'fixture.py';marker=root/'pid';result=root/'result.json';pid=None
                suite.write_text('import subprocess,sys,os,time\nfrom pathlib import Path\nchild=subprocess.Popen([sys.executable,"-c","import time;time.sleep(30)"],start_new_session=True)\nPath('+repr(str(marker))+').write_text(str(child.pid))\n'+('os._exit(2)\n' if early else 'time.sleep(30)\n'))
                evidence=[]
                try:
                    with self.assertRaises((ValueError,subprocess.TimeoutExpired)):
                        runner.execute([sys.executable,'-I','-B',str(Path(runner.__file__).resolve()),'--worker',str(suite),'--output',str(result)],root,evidence,timeout=.5)
                    pid=int(marker.read_text());self.assertTrue(evidence[0]['cleanup']['verified'])
                    state=subprocess.run(['ps','-o','stat=','-p',str(pid)],capture_output=True,text=True,timeout=2).stdout.strip()
                    self.assertTrue(not state or state.startswith('Z'),state)
                finally:
                    if pid is None and marker.exists():pid=int(marker.read_text())
                    if pid is not None:
                        try:os.kill(pid,signal.SIGKILL)
                        except ProcessLookupError:pass

    def test_exited_detached_helper_group_cleans_reparented_grandchild(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();suite=root/'fixture.py';marker=root/'pid';result=root/'result.json';pid=None
            helper="import subprocess,sys;from pathlib import Path;c=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']);Path(sys.argv[1]).write_text(str(c.pid))"
            suite.write_text('import subprocess,sys,unittest\nclass Control(unittest.TestCase):\n def test_helper(self): subprocess.run([sys.executable,"-c",'+repr(helper)+','+repr(str(marker))+'],start_new_session=True,check=True)\n')
            evidence=[]
            try:
                runner.execute([sys.executable,'-I','-B',str(Path(runner.__file__).resolve()),'--worker',str(suite),'--output',str(result)],root,evidence)
                pid=int(marker.read_text());self.assertTrue(evidence[0]['cleanup']['verified'])
                state=subprocess.run(['ps','-o','stat=','-p',str(pid)],capture_output=True,text=True,timeout=2).stdout.strip()
                self.assertTrue(not state or state.startswith('Z'),state)
            finally:
                if pid is None and marker.exists():pid=int(marker.read_text())
                if pid is not None:
                    try:os.kill(pid,signal.SIGKILL)
                    except ProcessLookupError:pass

    def test_inspection_failure_still_reaps_actual_worker(self):
        child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'],start_new_session=True)
        try:
            with patch.object(runner,'process_table',side_effect=RuntimeError('inspection failed')):
                with self.assertRaises(RuntimeError):runner.cleanup_worker(child,Path('/unused-journal'))
            self.assertIsNotNone(child.returncode)
            with self.assertRaises(ChildProcessError):os.waitpid(child.pid,os.WNOHANG)
        finally:
            runner.helpers.stop(child)

    def test_malformed_journal_still_cleans_known_detached_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();suite=root/'fixture.py';marker=root/'pid';pid=None
            suite.write_text('import subprocess,sys,os,time\nfrom pathlib import Path\nchild=subprocess.Popen([sys.executable,"-c","import time;time.sleep(30)"],start_new_session=True)\nPath('+repr(str(marker))+').write_text(str(child.pid))\nwith open(os.environ["HELPER_PROCESS_JOURNAL"],"a") as f: f.write("{bad\\n")\ntime.sleep(30)\n')
            evidence=[]
            try:
                with self.assertRaises(ValueError):
                    runner.execute([sys.executable,'-I','-B',str(Path(runner.__file__).resolve()),'--worker',str(suite),'--output',str(root/'result.json')],root,evidence,timeout=.5)
                pid=int(marker.read_text());self.assertFalse(evidence[0]['cleanup']['verified'])
                state=subprocess.run(['ps','-o','stat=','-p',str(pid)],capture_output=True,text=True,timeout=2).stdout.strip()
                self.assertTrue(not state or state.startswith('Z'),state)
            finally:
                if pid is None and marker.exists():pid=int(marker.read_text())
                if pid is not None:
                    try:os.kill(pid,signal.SIGKILL)
                    except ProcessLookupError:pass

    def test_nonzero_worker_cannot_be_accepted(self):
        with tempfile.TemporaryDirectory() as temporary:
            evidence=[]
            with self.assertRaisesRegex(ValueError,'nonzero'):
                runner.execute([sys.executable,'-c','raise SystemExit(2)'],Path(temporary),evidence)
            self.assertEqual(evidence[0]['exit'],2)

    def test_real_timeout_records_terminal_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            evidence=[]
            with self.assertRaises(subprocess.TimeoutExpired):
                runner.execute([sys.executable,'-c','import time;time.sleep(30)'],Path(temporary),evidence,timeout=.1)
            self.assertTrue(evidence[0]['timed_out']);self.assertIsNotNone(evidence[0]['exit'])
            with self.assertRaises(ChildProcessError):os.waitpid(evidence[0]['pid'],os.WNOHANG)

    def test_exited_leader_descendant_is_stopped(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);marker=root/'pid';pid=None
            code="import subprocess,sys;from pathlib import Path;c=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']);Path(sys.argv[1]).write_text(str(c.pid));sys.exit(2)"
            try:
                with self.assertRaises(ValueError):runner.execute([sys.executable,'-c',code,str(marker)],root,[])
                pid=int(marker.read_text());deadline=time.monotonic()+3
                while True:
                    state=subprocess.run(['ps','-o','stat=','-p',str(pid)],capture_output=True,text=True,timeout=2).stdout.strip()
                    if not state or state.startswith('Z'):break
                    self.assertLess(time.monotonic(),deadline,'descendant still running');time.sleep(.02)
            finally:
                if pid is None and marker.exists():pid=int(marker.read_text())
                if pid is not None:
                    try:os.kill(pid,signal.SIGKILL)
                    except ProcessLookupError:pass

    def test_failure_retains_false_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)/'new/child'
            with patch.object(runner,'registry',side_effect=ValueError('omitted suite')):
                with self.assertRaises(ValueError):runner.main(['--output',str(output)])
            record=json.loads((output/'evidence.json').read_text())
            self.assertIs(record['verified'],False);self.assertEqual(record['failure_type'],'ValueError')

if __name__ == '__main__':unittest.main()
