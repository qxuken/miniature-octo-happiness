import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('upgrade', Path(__file__).with_name('study_cli_upgrade.py'))
u = importlib.util.module_from_spec(spec)
spec.loader.exec_module(u)

class Controls(unittest.TestCase):
    def test_output_admission_before_external_work(self):
        with tempfile.TemporaryDirectory() as t:
            root=Path(t); existing=root/'existing';existing.mkdir()
            dangling=root/'dangling';dangling.symlink_to(root/'absent')
            for path in [existing,dangling]:
                with patch.object(sys,'argv',['upgrade','--output',str(path)]),patch.object(u,'capture') as cmd:
                    with self.assertRaises(SystemExit):u.main()
                    cmd.assert_not_called()
            self.assertFalse((root/'absent').exists())

    def test_preserve_selector_remove_overrides(self):
        with patch.dict(os.environ,{'RUSTUP_HOME':'/rust','RUSTUP_TOOLCHAIN':'1.99.0','RUSTFLAGS':'bad','CARGO_PROFILE_RELEASE_PANIC':'abort'},clear=True):
            self.assertEqual(u.clean_env(),{'RUSTUP_HOME':'/rust','RUSTUP_TOOLCHAIN':'1.99.0','CARGO_INCREMENTAL':'0'})

    def test_real_failed_child_and_timeout_are_reaped(self):
        with tempfile.TemporaryDirectory() as t:
            with self.assertRaises(RuntimeError):
                u.execute([sys.executable,'-c','raise SystemExit(2)'],Path(t)/'failed')
            pid=Path(t)/'pid'
            with self.assertRaises(subprocess.TimeoutExpired):
                u.execute([sys.executable,'-c',f'import os,time;open({str(pid)!r},"w").write(str(os.getpid()));time.sleep(30)'],Path(t)/'timeout',timeout=0.5)
            with self.assertRaises(ProcessLookupError):os.kill(int(pid.read_text()),0)

    def test_exited_leader_cannot_leave_live_descendant(self):
        import signal
        import time
        with tempfile.TemporaryDirectory() as t:
            root=Path(t);pidfile=root/'descendant'
            script=('import subprocess,sys; '
                    'p=subprocess.Popen([sys.executable,"-c","import time;time.sleep(30)"]); '
                    f'open({str(pidfile)!r},"w").write(str(p.pid)); '
                    'raise SystemExit(2)')
            pid=None
            try:
                with self.assertRaises(RuntimeError):
                    u.execute([sys.executable,'-c',script],root/'log')
                pid=int(pidfile.read_text())
                deadline=time.monotonic()+3
                while True:
                    status=subprocess.run(['ps','-o','stat=','-p',str(pid)],capture_output=True,text=True,timeout=2).stdout.strip()
                    if not status or status.startswith('Z'):break
                    self.assertLess(time.monotonic(),deadline,'descendant remains live')
                    time.sleep(0.02)
            finally:
                if pid is None and pidfile.exists():pid=int(pidfile.read_text())
                if pid is not None:
                    try:os.kill(pid,signal.SIGKILL)
                    except ProcessLookupError:pass

    def test_copied_artifact_and_source_mismatch_reject(self):
        import json
        with tempfile.TemporaryDirectory() as t:
            root=Path(t)
            for label in ['old','new']:(root/(label+'-study-peer')).write_bytes(label.encode())
            sources={label:{'files':{'Cargo.lock':'frozen'}} for label in ['old','new']}
            proof={'verified':True,'artifacts':u.inventory(root),'fixture_sha256':'fixture',
                   'sources':{label:{'files':sources[label]['files'],'binary_sha256':u.digest(root/(label+'-study-peer'))} for label in sources}}
            evidence=root/'evidence.json';evidence.write_text(json.dumps(proof))
            u.verify_copy(root,sources,'fixture')
            (root/'old-study-peer').write_bytes(b'mutated')
            with self.assertRaisesRegex(RuntimeError,'artifact'):u.verify_copy(root,sources,'fixture')
            (root/'old-study-peer').write_bytes(b'old')
            proof['sources']['old']['files']={'Cargo.lock':'wrong'}
            evidence.write_text(json.dumps(proof))
            with self.assertRaisesRegex(RuntimeError,'source'):u.verify_copy(root,sources,'fixture')

    def test_cleanup_attempts_volume_after_container_error(self):
        with patch.object(u.subprocess,'run',side_effect=[OSError('missing'),None]) as run,patch.object(u,'capture',side_effect=['','']):
            u.cleanup('owned','owned-data')
            self.assertEqual(run.call_args_list[1].args[0],['docker','volume','rm','owned-data'])
        with patch.object(u.subprocess,'run'),patch.object(u,'capture',side_effect=['','owned-data\n']):
            with self.assertRaises(RuntimeError):u.cleanup('owned','owned-data')
        with patch.object(u.subprocess,'run'),patch.object(u,'capture',side_effect=OSError('daemon')):
            with self.assertRaises(OSError):u.cleanup('owned','owned-data')

    def test_fixture_uses_real_binary_seam_and_public_oracles(self):
        text=u.fixture(u.ROOT,u.ROOT/'tools/study-cli-upgrade-fixture.rs')
        self.assertIn('Command::new(binary)',text)
        self.assertNotIn('Command::new(BIN)',text)
        self.assertIn('STUDY_OLD_BINARY',text)
        self.assertIn('core(root, "a")',text)
        with tempfile.TemporaryDirectory() as t:
            p=Path(t)/'apps/study-peer/tests';p.mkdir(parents=True)
            (p/'credential_rotation.rs').write_text('changed helper')
            with self.assertRaises(RuntimeError):u.fixture(Path(t),u.ROOT/'tools/study-cli-upgrade-fixture.rs')

if __name__=='__main__':unittest.main()
