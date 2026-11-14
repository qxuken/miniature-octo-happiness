"""Portable handoff controls; no engine build or native-Windows qualification."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import model
import windows as trace


class WindowsTraceControls(unittest.TestCase):
    def package(self, root):
        for name, data in [('weekend_db.dll',b'dll'),('weekend_db.dll.lib',b'imports'),('weekend_db.h',b'header')]:
            (root/name).write_bytes(data)
        info={'verified':True,'candidate_eligible':True,'dirty_status':'','source':'a'*40,'source_id':'a'*40,
              'toolchain':'host: x86_64-pc-windows-msvc','features':'crdt,replication',
              'sha256':{p.name:trace.sha(p) for p in root.iterdir()}}
        (root/'manifest.json').write_text(json.dumps(info))
        return info,trace.sha(root/'manifest.json')

    def test_qualified_oracle_is_reused_byte_for_byte(self):
        entries=trace.cases()
        self.assertEqual(len(model.TRACE),134)
        self.assertEqual(entries[0][1],model.encode())
        self.assertEqual(hashlib.sha256(entries[0][1]).hexdigest(),trace.TRACE_SHA256)
        self.assertEqual(hashlib.sha256(entries[0][2]).hexdigest(),trace.EXPECTED_SHA256)
        for name,data,expected in entries:
            if expected is not None:
                self.assertEqual(model.expected(model.decode(data)).encode(),expected,name)
            else:
                with self.assertRaises(ValueError):model.decode(data)

    def test_both_optimized_languages_use_delivered_import_library(self):
        for cpp,language,standard in [(False,'/TC','/std:c11'),(True,'/TP','/std:c++17')]:
            command=trace.compiler_command('cl.exe',cpp,Path('/inputs'),Path('/package'),Path('/out/trace.exe'))
            for flag in ['/O2','/UNDEBUG','/W4','/WX','/MD',language,standard]:self.assertIn(flag,command)
            split=command.index('/link')
            imports=str(Path('/package')/'weekend_db.dll.lib')
            self.assertIn(imports,command[split+1:])
            self.assertNotIn(imports,command[:split])
            self.assertIn('bcrypt.lib',command[split+1:])
            self.assertIn('/I'+str(Path('/package')),command[:split])

    def test_manifest_pin_payload_and_complete_inventory_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);_,pin=self.package(root)
            self.assertEqual(trace.verify_package(root,pin)[1],3)
            with self.assertRaises(ValueError):trace.verify_package(root,'0'*64)
            (root/'weekend_db.dll').write_bytes(b'wrong DLL')
            with self.assertRaises(ValueError):trace.verify_package(root,pin)
            (root/'weekend_db.dll').write_bytes(b'dll');(root/'extra').write_bytes(b'unknown')
            with self.assertRaises(ValueError):trace.verify_package(root,pin)

    def test_manifest_rejects_dirty_wrong_target_and_unknown_features(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);original,_=self.package(root)
            for change in [{'verified':False},{'candidate_eligible':False},{'dirty_status':'M file'},
                           {'source_id':'a'*40+'-dirty'},{'toolchain':'host: x86_64-apple-darwin'},
                           {'features':'crdt,unknown'},{'features':'crdt,crdt'}]:
                (root/'manifest.json').write_text(json.dumps({**original,**change}))
                with self.assertRaises(ValueError):trace.verify_package(root,trace.sha(root/'manifest.json'))

    def test_windows_path_escapes_devices_and_aliases_rejected(self):
        self.assertEqual(trace.relative_name('folder'+chr(92)+'payload.bin'),'folder/payload.bin')
        for value in [r'C:\payload',r'\payload',r'\\host\share\payload','../payload','a/../payload',
                      'a//payload','payload:stream','CON','aux.txt','folder/NUL','trailing.','trailing ','.']:
            with self.assertRaises(ValueError,msg=value):trace.relative_name(value)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);info,_=self.package(root)
            info['sha256']['WEEKEND_DB.DLL']=info['sha256']['weekend_db.dll']
            (root/'manifest.json').write_text(json.dumps(info))
            with self.assertRaises(ValueError):trace.verify_package(root,trace.sha(root/'manifest.json'))

    def test_backslash_payload_keys_and_reparse_rejection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);info,_=self.package(root)
            (root/'folder').mkdir();(root/'folder/payload').write_bytes(b'extra')
            info['sha256']['folder'+chr(92)+'payload']=trace.sha(root/'folder/payload')
            (root/'manifest.json').write_text(json.dumps(info));pin=trace.sha(root/'manifest.json')
            trace.verify_package(root,pin)
            with patch.object(Path,'is_symlink',return_value=True):
                with self.assertRaises(ValueError):trace.verify_package(root,pin)

    def proof(self):
        return {'schema':1,'phase':'before','pid':123,'loaded_dll':r'C:\proof\weekend_db.dll',
                'sha256':'b'*64,'source_id':'a'*40,'abi':1,'format':2,'features':3}

    def check(self,record):
        trace.check_attestation(record,'before',r'C:\proof\weekend_db.dll','b'*64,'a'*40,3,123)

    def test_actual_loaded_path_hash_source_features_and_pid_required(self):
        row=self.proof();self.check(row)
        self.check({**row,'loaded_dll':r'c:\PROOF\weekend_db.dll'})
        for key,value in [('loaded_dll',r'C:\other\weekend_db.dll'),('loaded_dll','weekend_db.dll'),
                          ('sha256','c'*64),('source_id','c'*40),('features',0),('features',True),
                          ('pid',124),('schema',True),('phase','after'),('abi',2),('format',3)]:
            with self.assertRaises(ValueError):self.check({**row,key:value})
        with self.assertRaises(ValueError):self.check({**row,'extra':'untrusted'})
        broken=copy.deepcopy(row);del broken['sha256']
        with self.assertRaises(ValueError):self.check(broken)

    def test_duplicate_and_oversized_json_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'proof.json';path.write_text('{"schema":1,"schema":1}')
            with self.assertRaises(ValueError):trace.read_json(path)
            path.write_bytes(b' '*(2*1024*1024+1))
            with self.assertRaises(ValueError):trace.read_json(path)

    def test_existing_output_is_rejected_before_inputs_or_processes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);output=root/'existing';output.mkdir();(output/'keep').write_text('preserve')
            with patch.object(trace,'verify_package') as verify,patch.object(trace,'execute') as execute:
                with self.assertRaises(ValueError):trace.main(['--package',str(root/'missing'),'--manifest-sha256','a'*64,'--output',str(output)])
                verify.assert_not_called();execute.assert_not_called()
            self.assertEqual((output/'keep').read_text(),'preserve')

    def test_compile_environment_overrides_removed_case_insensitively(self):
        env=trace.clean_env({'CL':'/DNDEBUG','_cl_':'/DNDEBUG','LINK':'inject','_LINK_':'inject',
                             'PYTHONPATH':'injected','PATH':'tools','INCLUDE':'sdk'})
        self.assertEqual(env,{'PATH':'tools','INCLUDE':'sdk'})

    def test_real_child_output_limit_and_timeout_are_not_false_green(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);commands=[]
            with self.assertRaisesRegex(ValueError,'output limit'):
                trace.execute([sys.executable,'-c','import os,time;os.write(1,b"x"*4096);time.sleep(30)'],root,'overflow',commands,maximum=128,timeout=10)
            self.assertLessEqual((root/'overflow.stdout').stat().st_size,128)
            self.assertIsNotNone(commands[-1]['exit'])
            with self.assertRaises(TimeoutError):
                trace.execute([sys.executable,'-c','import time;time.sleep(30)'],root,'timeout',commands,timeout=.2)
            self.assertIsNotNone(commands[-1]['exit'])

    def test_real_success_and_nonzero_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);commands=[]
            result=trace.execute([sys.executable,'-c','print("ok")'],root,'ok',commands,timeout=10)
            self.assertEqual(result['exit'],0);self.assertTrue(result['cleanup_verified'])
            self.assertEqual((root/'ok.stdout').read_bytes().splitlines(),[b'ok'])
            with self.assertRaisesRegex(ValueError,'command failed'):
                trace.execute([sys.executable,'-c','raise SystemExit(2)'],root,'bad',commands)
            self.assertEqual(commands[-1]['exit'],2)


if __name__=='__main__':unittest.main()
