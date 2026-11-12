"""Contract and provenance rejection checks, without building or invoking the engine."""
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
import model
import run

class TraceContract(unittest.TestCase):
    def test_binary_bounds(self):
        self.assertEqual(model.decode(model.encode()),model.TRACE)
        for data in [b'',b'BADMAGIC'+struct.pack('<I',0),model.MAGIC+struct.pack('<I',257),model.encode()+b'x',model.encode([(14,0,0,0)])]:
            with self.assertRaises(ValueError): model.decode(data)

    def test_hand_checked_publications_and_missing_delete(self):
        cells=[(1,0,0,0),(6,0,99,0),(3,0,99,0),(5,0,0,0),(5,0,0,0),(7,0,0,0)]
        self.assertEqual(model.expected(cells),
            '0 1 0 1 [] H=- R=- X=-\n'
            '1 6 0 1 [] H=- R=- X=-\n'
            '2 3 3 1 [] H=- R=- X=-\n'
            '3 5 0 2 [] H=- R=- X=2\n'
            '4 5 11 2 [] H=- R=- X=-\n'
            '5 7 0 2 [] H=- R=- X=-\n')

    def test_finite_trace_conflicts_and_durable_history(self):
        lines=model.expected().splitlines()
        self.assertEqual(len(lines),59)
        self.assertEqual([int(line.split()[0]) for line in lines if line.split()[2]=='10'],[22,33,42])
        self.assertEqual([int(line.split()[0]) for line in lines if line.split()[2]=='11'],[13,23,24])
        self.assertIn(' H=2[00=m{6e:u1,76:l[b0001ff,u2]};ff=m{6e:u2,76:l[b0002ff,u3]}] R=- ',lines[55])
        self.assertEqual(lines[-1],'58 11 3 10 [00=m{6e:u9,76:l[b0009ff,u10]};07=m{6e:u7,76:l[b0007ff,u8]}] H=- R=- X=-')

class PackageProvenance(unittest.TestCase):
    def test_cleanup_argv_is_limited_to_matching_uuid_pair(self):
        nonce='a'*32;container='weekend-trace-'+nonce;volume='weekend-trace-data-'+nonce
        self.assertEqual(run.cleanup_commands(container,volume),[
            ['docker','rm','-f',container],['docker','volume','rm',volume]])
        for bad_container,bad_volume in [('unrelated',volume),(container,'unrelated'),(container,volume+'x'),(container,'weekend-trace-data-'+'b'*32),('--all',volume)]:
            with self.assertRaises(ValueError): run.cleanup_commands(bad_container,bad_volume)

    def package(self,root):
        (root/'payload').write_bytes(b'synthetic')
        data={'verified':True,'source':'a'*40,'source_id':'a'*40,'sha256':{'payload':hashlib.sha256(b'synthetic').hexdigest()}}
        (root/'manifest.json').write_text(json.dumps(data));return data

    def test_corruption_extra_file_symlink_and_dirty_identity_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);data=self.package(root);run.verify(root)
            (root/'payload').write_bytes(b'changed')
            with self.assertRaises(ValueError): run.verify(root)
            (root/'payload').write_bytes(b'synthetic');(root/'extra').write_bytes(b'x')
            with self.assertRaises(ValueError): run.verify(root)
            (root/'extra').unlink();(root/'alias').symlink_to('payload')
            with self.assertRaises(ValueError): run.verify(root)
            (root/'alias').unlink();data['source_id']+='-dirty';(root/'manifest.json').write_text(json.dumps(data))
            with self.assertRaises(ValueError): run.verify(root)

    def test_manifest_path_escape_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);data=self.package(root)
            data['sha256']['../escaped']='0'*64;(root/'manifest.json').write_text(json.dumps(data))
            with self.assertRaises(ValueError): run.verify(root)

    def test_existing_output_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as d:
            parent=Path(d);package=parent/'package';package.mkdir();self.package(package)
            (package/'rust-source').mkdir()
            output=parent/'existing';output.mkdir();(output/'keep').write_text('preserve')
            result=subprocess.run([sys.executable,str(Path(run.__file__)), '--package',str(package),'--output',str(output)],capture_output=True,timeout=10)
            self.assertNotEqual(result.returncode,0)
            self.assertIn(b'FileExistsError',result.stderr)
            self.assertEqual(list(output.iterdir()),[output/'keep'])
            self.assertEqual((output/'keep').read_text(),'preserve')

    def test_dangling_final_symlink_rejected_before_any_input_or_work(self):
        with tempfile.TemporaryDirectory() as d:
            parent=Path(d);target=parent/'redirected';output=parent/'requested'
            output.symlink_to(target,target_is_directory=True)
            before=set(parent.iterdir())
            result=subprocess.run([sys.executable,str(Path(run.__file__)), '--package',str(parent/'nonexistent-package'),'--output',str(output)],capture_output=True,timeout=10)
            self.assertNotEqual(result.returncode,0)
            self.assertIn(b'FileExistsError',result.stderr)
            self.assertEqual(result.stdout,b'')
            self.assertTrue(output.is_symlink())
            self.assertFalse(os.path.lexists(target))
            self.assertEqual(set(parent.iterdir()),before)

if __name__=='__main__': unittest.main()
