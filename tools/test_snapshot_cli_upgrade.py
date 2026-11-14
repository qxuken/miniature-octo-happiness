import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import snapshot_cli_upgrade as tool

class Contract(unittest.TestCase):
    def test_docker_cleanup_attempts_both_and_requires_both_absence(self):
        name='snapshot-cli-upgrade-'+'a'*32;volume=name+'-data'
        with patch.object(tool.subprocess,'run',side_effect=[subprocess.TimeoutExpired('docker',30),subprocess.CompletedProcess([],0)]) as remove,patch.object(tool.subprocess,'check_output',side_effect=['','']):
            result=tool.cleanup_docker(name,volume)
        self.assertEqual(remove.call_args_list[1].args[0],['docker','volume','rm',volume])
        self.assertTrue(result['cleanup_verified'])
        self.assertEqual(result['cleanup_errors'],['TimeoutExpired'])
        with patch.object(tool.subprocess,'run'),patch.object(tool.subprocess,'check_output',side_effect=[OSError('daemon unavailable'),volume+'\n']) as inspect:
            result=tool.cleanup_docker(name,volume)
        self.assertEqual(inspect.call_count,2)
        self.assertFalse(result['cleanup_verified'])
        with patch.object(tool.subprocess,'run') as remove:
            with self.assertRaises(RuntimeError):tool.cleanup_docker('unowned',volume)
            remove.assert_not_called()

    def test_exact_receipt_grammar(self):
        line='follower durable_revision=3 stream='+tool.STREAM+' digest='+'ab'*32
        self.assertEqual(tool.ack(line),{'revision':3,'digest':'ab'*32})
        for invalid in [line+'\n'+line,line.replace('=3 ','=03 '),line.replace(tool.STREAM,'22'*16),line.replace('=3 ','='+str(1<<64)+' '),line.replace('=3 ','='+'1'*5000+' '),'follower empty']:
            with self.assertRaises(RuntimeError):tool.ack(invalid)
    def test_fresh_output_preserves_existing_and_dangling(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'exists';path.mkdir();(path/'keep').write_text('keep')
            with self.assertRaises(ValueError):tool.helpers.fresh_output(path)
            self.assertEqual((path/'keep').read_text(),'keep')
            link=Path(tmp)/'link';link.symlink_to(Path(tmp)/'missing')
            with self.assertRaises(ValueError):tool.helpers.fresh_output(link)
            self.assertTrue(link.is_symlink());self.assertFalse((Path(tmp)/'missing').exists())
    def test_environment_preserves_selected_toolchain(self):
        with patch.dict(os.environ,{'RUSTUP_TOOLCHAIN':'1.99.1','RUSTFLAGS':'bad','CARGO_BUILD_RUSTFLAGS':'bad'},clear=True):
            env=tool.helpers.child_env()
        self.assertEqual(env['RUSTUP_TOOLCHAIN'],'1.99.1');self.assertNotIn('RUSTFLAGS',env);self.assertNotIn('CARGO_BUILD_RUSTFLAGS',env)
    def test_timeout_reaps_real_child_and_records_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp);e={'commands':[]}
            with self.assertRaises(subprocess.TimeoutExpired):
                tool.invoke([sys.executable,'-c','import time;time.sleep(30)'],out,e,'timeout',timeout=.1)
            self.assertIsNotNone(e['commands'][0]['exit']);self.assertNotEqual(e['commands'][0]['exit'],0)
    def test_failed_setup_records_failure_and_scrubs_keys(self):
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp)/'out'
            def fail(*args):
                private=out/'work/client';private.mkdir(parents=True);(private/'key.der').write_bytes(b'private')
                raise RuntimeError('fixture')
            with patch.object(sys,'argv',['tool','--output',str(out)]),patch.object(tool,'prepare',side_effect=fail):
                with self.assertRaises(RuntimeError):tool.main()
            e=json.loads((out/'evidence.json').read_text());self.assertFalse(e['verified']);self.assertEqual(e['failure_type'],'RuntimeError');self.assertFalse((out/'work/client/key.der').exists())
    def test_expected_failure_does_not_accept_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(RuntimeError):tool.invoke([sys.executable,'-c','pass'],Path(tmp),{'commands':[]},'wrong-status',success=False)

if __name__=='__main__':unittest.main()
