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
from unittest.mock import patch
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
        lines=model.expected(model.decode(model.encode(model.TRACE_V1,version=1))).splitlines()
        self.assertEqual(len(lines),59)
        self.assertEqual([int(line.split()[0]) for line in lines if line.split()[2]=='10'],[22,33,42])
        self.assertEqual([int(line.split()[0]) for line in lines if line.split()[2]=='11'],[13,23,24])
        self.assertIn(' H=2[00=m{6e:u1,76:l[b0001ff,u2]};ff=m{6e:u2,76:l[b0002ff,u3]}] R=- ',lines[55])
        self.assertEqual(lines[-1],'58 11 3 10 [00=m{6e:u9,76:l[b0009ff,u10]};07=m{6e:u7,76:l[b0007ff,u8]}] H=- R=- X=-')

    def test_v1_bytes_oracle_and_records_semantics_preserved(self):
        data=model.encode(model.TRACE_V1,version=1)
        self.assertEqual(model.encode(model.decode(data)),data)
        self.assertEqual(hashlib.sha256(data).hexdigest(),'78b916121e57e2cb0ce3d010dd126e46ef4d8047ec7ec9beb330f6dd3df33101')
        self.assertEqual(hashlib.sha256(model.expected(model.decode(data)).encode()).hexdigest(),'5997dbaa08eef5bc3c98e6bdcea55f991da09d8d8d2693e5b73f5c5045e295ff')
        # V1 accepted but ignored BEGIN's fourth field; no reinterpretation.
        cells=[(1,0,0,1),(1,1,0,0),(5,1,0,0),(5,0,0,0),(7,0,0,0),(7,1,0,0)]
        old=model.expected(model.decode(model.encode(cells,version=1))).splitlines()
        new=model.expected(model.decode(model.encode(cells))).splitlines()
        self.assertEqual(old[3],'3 5 0 3 [] H=- R=- X=3')
        self.assertEqual(new[3],'3 5 10 2 [] H=- R=- X=-')

    def test_hand_checked_whole_database_empty_publication_and_terminal_states(self):
        cells=[(1,0,0,1),(1,1,0,0),(5,1,0,0),(5,0,0,0),(3,0,0,0),(5,0,0,0),(7,0,0,0),(7,1,0,0)]
        self.assertEqual(model.expected(cells),
            '0 1 0 1 [] H=- R=- X=-\n'
            '1 1 0 1 [] H=- R=- X=-\n'
            '2 5 0 2 [] H=- R=- X=2\n'
            '3 5 10 2 [] H=- R=- X=-\n'
            '4 3 11 2 [] H=- R=- X=-\n'
            '5 5 11 2 [] H=- R=- X=-\n'
            '6 7 0 2 [] H=- R=- X=-\n'
            '7 7 0 2 [] H=- R=- X=-\n')
        self.assertEqual(model.expected([(1,0,0,1),(5,0,0,0),(7,0,0,0)]),
            '0 1 0 1 [] H=- R=- X=-\n1 5 0 2 [] H=- R=- X=2\n2 7 0 2 [] H=- R=- X=-\n')

    def test_hand_checked_retained_publication_validation_and_reopen(self):
        cells=[(1,0,0,0),(2,0,0,1),(5,0,0,0),(7,0,0,0),(8,0,0,0),
               (1,0,0,1),(1,1,0,0),(3,1,0,0),(10,0,0,0),(5,0,0,0),
               (3,0,0,0),(5,0,0,0),(5,1,0,0),(7,0,0,0),(7,1,0,0),
               (9,0,0,0),(12,0,0,0),(11,0,1,0),(13,0,1,0),
               (9,0,0,0),(12,0,0,0),(11,0,1,0)]
        row='[00=m{6e:u1,76:l[b0001ff,u2]}]';val='m{6e:u1,76:l[b0001ff,u2]}'
        # status, publication revision, current rows, held view, durable IDs, result
        controls=[(0,1,'[]','-','-','-'),(0,1,'[]','-','-','-'),
                  (0,2,row,'-','-','2'),(0,2,row,'-','-','-'),
                  (0,2,row,'2'+row,'-','-'),(0,2,row,'2'+row,'-','-'),
                  (0,2,row,'2'+row,'-','-'),(0,2,row,'2'+row,'-',val),
                  (0,3,row,'2'+row,'1','1'),(10,3,row,'2'+row,'1','-'),
                  (11,3,row,'2'+row,'1','-'),(11,3,row,'2'+row,'1','-'),
                  (0,4,row,'2'+row,'1','4'),(0,4,row,'2'+row,'1','-'),
                  (0,4,row,'2'+row,'1','-'),(0,4,row,'-','1','-'),
                  (0,4,row,'-','1','-'),(0,4,row,'2'+row,'1','-'),
                  (0,5,row,'2'+row,'-','-'),(0,5,row,'-','-','-'),
                  (0,5,row,'-','-','-'),(3,5,row,'-','-','-')]
        expected=''.join(f'{i} {cells[i][0]} {status} {revision} {rows} H={held} R={ids} X={result}\n'
                         for i,(status,revision,rows,held,ids,result) in enumerate(controls))
        self.assertEqual(model.expected(cells),expected)

    def test_invalid_v2_validation_mode_and_unknown_version_rejected(self):
        cells=[(1,0,0,2)]
        with self.assertRaisesRegex(ValueError,'validation mode'): model.decode(model.encode(cells))
        with self.assertRaisesRegex(ValueError,'validation mode'): model.expected(cells)
        self.assertEqual(model.decode(model.encode(cells,version=1)),cells)
        with self.assertRaises(ValueError): model.decode(b'WDBTRC3\n'+struct.pack('<I',0))

    def test_v2_finite_validation_matrix_and_unchanged_v1_prefix(self):
        lines=model.expected().splitlines()
        self.assertEqual(len(lines),134)
        self.assertEqual('\n'.join(lines[:59])+'\n',model.expected(model.decode(model.encode(model.TRACE_V1,version=1))))
        self.assertEqual([i for i,line in enumerate(lines) if line.split()[2]=='10'],[22,33,42,75,92,114,127])
        self.assertEqual(lines[-1],'133 11 3 22 [00=m{6e:u14,76:l[b000eff,u15]};07=m{6e:u16,76:l[b0010ff,u17]}] H=- R=- X=-')

class PackageProvenance(unittest.TestCase):
    def test_environment_preserves_toolchain_selection_but_removes_build_overrides(self):
        with patch.dict(os.environ, {'RUSTUP_HOME':'/selected/toolchains',
                                     'RUSTUP_TOOLCHAIN':'1.99.0',
                                     'RUSTFLAGS':'bad', 'RUSTC_WRAPPER':'bad',
                                     'CARGO_BUILD_RUSTFLAGS':'bad',
                                     'CARGO_PROFILE_RELEASE_PANIC':'abort',
                                     'LD_PRELOAD':'bad'}, clear=True):
            env=run.clean_env('a'*40)
        self.assertEqual(env, {'RUSTUP_HOME':'/selected/toolchains',
                              'RUSTUP_TOOLCHAIN':'1.99.0',
                              'CARGO_INCREMENTAL':'0','CARGO_NET_OFFLINE':'true',
                              'WEEKEND_DB_SOURCE_ID':'a'*40})

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
