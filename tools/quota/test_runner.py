"""No privileged provisioning: bounded children, temporary records and mocked tools."""
import copy
from contextlib import contextmanager
import json
import os
from pathlib import Path
from types import SimpleNamespace
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch, MagicMock
import run

class QuotaControls(unittest.TestCase):
    def fixture_row(self, stage='inspect', revision=3):
        return {'stage':stage,'source':'a'*40,'features':0,'pid':123,'current_revision':revision,
                'retained_id':1,'retained_revision':2,'commit_error':0,'native_category':'None',
                'native_raw_code':None,'native_operation':'None','terminal_error':0,
                'recovery_required':False,'blocked_write_error':0,
                'current_rows':run.rows_for(revision),'retained_rows':run.rows_for(3)}

    def probe_row(self):
        return {'mode':'fill','pid':123,'identity':{'uid':60000,'uids':[60000]*4,'gids':[60000]*4,
                'groups':[],'caps':{k:0 for k in ['CapInh','CapPrm','CapEff','CapBnd','CapAmb']},
                'no_new_privs':1},'global_free_bytes':32*1024*1024,'available_bytes':32*1024*1024,'errno':122,
                'written':4*1024*1024,'ceiling':8*1024*1024}

    def test_source_must_resolve_to_exact_commit_before_archiving(self):
        with patch.object(run.subprocess,'check_output',return_value='b'*40+'\n') as command:
            with self.assertRaisesRegex(ValueError,'exact commit'):
                run.source_binding('a'*40)
        self.assertEqual(command.call_count,1)
        self.assertEqual(command.call_args.args[0][-3:],['rev-parse','--verify','a'*40+'^{commit}'])
        self.assertIn('safe.directory='+str(run.ROOT),command.call_args.args[0])

    def test_resolved_mke2fs_explicitly_selects_ext4(self):
        command=run.formatter_command('/usr/sbin/mke2fs',Path('/owned/image.ext4'))
        self.assertEqual(command,['/usr/sbin/mke2fs','-t','ext4','-q','-F','-O','quota','-E','quotatype=usrquota','/owned/image.ext4'])

    def test_exact_mount_and_backing_identity(self):
        image=Path('/private/image.ext4');point=Path('/private/mount')
        loops=json.dumps({'loopdevices':[{'name':'/dev/loop7','back-file':str(image)}]})
        self.assertEqual(run.owned_loops(loops,image),['/dev/loop7'])
        row={'target':str(point),'source':'/dev/loop7','fstype':'ext4'}
        run.exact_mount(json.dumps({'filesystems':[row]}),point,'/dev/loop7')
        for bad in [{'target':'/'},{'source':'/dev/root'},{'fstype':'tmpfs'}]:
            with self.assertRaises(ValueError):run.exact_mount(json.dumps({'filesystems':[{**row,**bad}]}),point,'/dev/loop7')
        self.assertEqual(run.owned_loops(loops,Path('/other/image.ext4')),[])

    def test_real_quota_category_cannot_be_substituted(self):
        row=self.probe_row();run.validate_probe(row,60000,'fill')
        for change in [{'errno':28},{'errno':None},{'global_free_bytes':1},{'available_bytes':1},{'written':9*1024*1024}]:
            with self.assertRaises(ValueError):run.validate_probe({**row,**change},60000,'fill')
        row=self.probe_row();row['identity']['caps']['CapEff']=1
        with self.assertRaises(ValueError):run.validate_probe(row,60000,'fill')

    def test_full_current_and_retained_oracles_reject_damage_and_stale_state(self):
        for revision in [3,4]:
            row=self.fixture_row(revision=revision);run.validate_fixture(row,'a'*40,'inspect',revision)
            for key,value in [('source','b'*40),('features',1),('current_revision',4 if revision==3 else 3),('retained_revision',3),('pid',True)]:
                with self.assertRaises(ValueError):run.validate_fixture({**row,key:value},'a'*40,'inspect',revision)
            for field in ['current_rows','retained_rows']:
                broken=copy.deepcopy(row);broken[field][0]['bytes_hex']='00'+broken[field][0]['bytes_hex'][2:]
                with self.assertRaises(ValueError):run.validate_fixture(broken,'a'*40,'inspect',revision)
                broken=copy.deepcopy(row);broken[field][1]['integer']=float(broken[field][1]['integer'])
                with self.assertRaises(ValueError):run.validate_fixture(broken,'a'*40,'inspect',revision)

    def test_failure_diagnostics_and_terminal_state_are_required(self):
        row=self.fixture_row(stage='fail');row.update(commit_error=14,native_category='Quota',native_raw_code=122,native_operation='Write',terminal_error=11,recovery_required=True,blocked_write_error=16)
        run.validate_fixture(row,'a'*40,'fail',3)
        for key,value in [('commit_error',0),('native_raw_code',28),('terminal_error',0),('recovery_required',False),('blocked_write_error',0)]:
            with self.assertRaises(ValueError):run.validate_fixture({**row,key:value},'a'*40,'fail',3)

    def test_output_and_timeout_bounds_kill_reap_actual_children(self):
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary);e={'commands':[]}
            with self.assertRaisesRegex(ValueError,'output limit'):
                run.invoke([sys.executable,'-c','import os,time;os.write(1,b"x"*4096);time.sleep(30)'],output,e,'bound',maximum=128)
            self.assertIsNotNone(e['commands'][-1]['exit']);self.assertLessEqual((output/'bound.stdout').stat().st_size,128)
            with self.assertRaises((TimeoutError,subprocess.TimeoutExpired)):
                run.invoke([sys.executable,'-c','import time;time.sleep(30)'],output,e,'timeout',timeout=.1)
            self.assertIsNotNone(e['commands'][-1]['exit'])

    def test_output_admission_precedes_external_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);existing=root/'existing';existing.mkdir();(existing/'keep').write_text('keep');link=root/'link';link.symlink_to(root/'missing')
            for path in [existing,link]:
                with patch.object(run.sys,'platform','linux'),patch.object(run.os,'geteuid',return_value=0),patch.object(run,'source_binding') as external:
                    with self.assertRaises(ValueError):run.main(['--fixture','/unused','--source','a'*40,'--output',str(path)])
                    external.assert_not_called()
            self.assertEqual((existing/'keep').read_text(),'keep');self.assertFalse((root/'missing').exists())

    def test_unsupported_probe_is_not_engine_qualification(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture=root/'fixture';fixture.write_bytes(b'fixture')
            for optional in [False,True]:
                out=root/str(optional)
                with patch.object(run.sys,'platform','linux'),patch.object(run.os,'geteuid',return_value=0),patch.object(run,'source_binding',return_value={}),patch.object(run.shutil,'which',return_value=None):
                    argv=['--fixture',str(fixture.resolve()),'--source','a'*40,'--output',str(out)]+(['--probe-only'] if optional else [])
                    if optional:self.assertEqual(run.main(argv),0)
                    else:
                        with self.assertRaises(run.Unsupported):run.main(argv)
                evidence=json.loads((out/'evidence.json').read_text());self.assertFalse(evidence['verified']);self.assertEqual(evidence['status'],'unsupported')

    def test_completed_quota_paths_recheck_the_same_tool_inventory(self):
        # Exercise main through its final admission gate without provisioning a
        # filesystem. Actual OS effects are replaced at invoke/cleanup boundaries;
        # tool hashes, fixture validation, final evidence and both branches are real.
        for maintenance,changed in [(False,False),(True,False),(False,True),(True,True)]:
            with self.subTest(maintenance=maintenance,changed=changed),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary).resolve();here=root/'tools/quota';here.mkdir(parents=True)
                for name in ['run.py','probe.py','test_runner.py','fixture.rs','maintenance.py']:
                    (here/name).write_text('stable '+name)
                for name in ['compatibility.py','supervision.py']:(root/'tools'/name).write_text('stable '+name)
                fixture=root/'fixture';fixture.write_bytes(b'fixture')
                executable=root/'executable';executable.write_bytes(b'trusted OS tool')
                scratch=root/('weekend-quota-'+'a'*32+'-owned');scratch.mkdir()
                output=root/'out';counter=0
                def mutate():
                    if changed:(root/'tools/supervision.py').write_text('changed during qualification')
                def invoke(argv,logs,evidence,label,**kwargs):
                    nonlocal counter
                    counter+=1;args=list(map(str,argv));evidence['commands'].append({'exit':0})
                    if '--version'in args:return 'Python 3.14.8'
                    if '--find'in args:return '/dev/loop7'
                    if '--list'in args:return json.dumps({'loopdevices':[{'name':'/dev/loop7','back-file':str(scratch/'image.ext4')}]})
                    if '--mountpoint'in args:return json.dumps({'filesystems':[{'target':str(scratch/'mount'),'source':'/dev/loop7','fstype':'ext4'}]})
                    if str(scratch/'probe.py')in args:
                        mode=args[args.index(str(scratch/'probe.py'))+1]
                        row=self.probe_row();row.update(mode=mode,removed=True,fsynced_bytes=4096)
                        return json.dumps(row)
                    if str(scratch/'fixture')in args:
                        index=args.index(str(scratch/'fixture'));stage=args[index+1]
                        revision=int(args[index+3])if stage=='inspect'else 4 if stage=='retry'else 3
                        row=self.fixture_row(stage,revision);row['pid']=counter
                        if stage=='inspect'and revision==4:mutate()
                        if stage=='fail':row.update(commit_error=14,native_category='Quota',native_raw_code=122,native_operation='Write',terminal_error=11,recovery_required=True,blocked_write_error=16)
                        return json.dumps(row)
                    return ''
                def exercise(*args):mutate();return []
                module=SimpleNamespace(exercise=exercise)
                fdopen=run.os.fdopen
                @contextmanager
                def small_image(fd,mode):
                    # The helper worker's inherited file ceiling stays intact;
                    # provisioning is mocked and needs only an inode to bind.
                    with fdopen(fd,mode) as file:
                        yield SimpleNamespace(truncate=lambda size:file.truncate(4096))
                spec=SimpleNamespace(loader=MagicMock())
                with patch.object(run,'ROOT',root),patch.object(run,'HERE',here), \
                     patch.object(run.sys,'platform','linux'),patch.object(run.os,'geteuid',return_value=0), \
                     patch.object(run.os,'chown'),patch.object(run.os,'fdopen',side_effect=small_image), \
                     patch.object(run.pwd,'getpwuid',side_effect=KeyError), \
                     patch.object(run,'source_binding',return_value={}),patch.object(run.shutil,'which',return_value=str(executable)), \
                     patch.object(run.tempfile,'mkdtemp',return_value=str(scratch)),patch.object(run,'invoke',side_effect=invoke), \
                     patch.object(run,'cleanup_owned',return_value={'verified':True}) as cleanup, \
                     patch('importlib.util.spec_from_file_location',return_value=spec),patch('importlib.util.module_from_spec',return_value=module):
                    args=['--fixture',str(fixture),'--source','a'*40,'--output',str(output)]+(['--maintenance']if maintenance else[])
                    if changed:
                        with self.assertRaisesRegex(ValueError,'qualification tools changed'):run.main(args)
                    else:self.assertEqual(run.main(args),0)
                evidence=json.loads((output/'evidence.json').read_text())
                self.assertEqual(evidence['verified'],not changed)
                self.assertEqual(evidence['status'],'failed'if changed else'qualified')
                self.assertIn('supervision.py',evidence['tools_sha256']);self.assertTrue(evidence['cleanup']['verified'])
                cleanup.assert_called_once()

    def cleanup_fixture(self, root):
        scratch=root/('weekend-quota-'+'a'*32+'-owned');scratch.mkdir();image=scratch/'image.ext4';image.write_bytes(b'');point=scratch/'mount';point.mkdir();info=image.stat()
        return scratch,image,point,(info.st_dev,info.st_ino)

    def test_cleanup_attempts_detach_after_unmount_timeout_and_proves_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();scratch,image,point,identity=self.cleanup_fixture(root);calls=[];detached=False;mounted=True;e={'commands':[]}
            def invoke(argv,output,evidence,label,**kwargs):
                nonlocal detached,mounted
                calls.append(argv);evidence['commands'].append({'exit':0});(output/(label+'.stderr')).write_text('')
                if argv[0]=='losetup' and '--detach' in argv:detached=True;return ''
                if argv[0]=='losetup':return json.dumps({'loopdevices':[] if detached else [{'name':'/dev/loop7','back-file':str(image)}]})
                if argv[0]=='umount':mounted=False;raise TimeoutError('command may have completed')
                if not mounted:evidence['commands'][-1]['exit']=1;return ''
                return json.dumps({'filesystems':[{'target':str(point),'source':'/dev/loop7','fstype':'ext4'}]})
            with patch.object(run,'invoke',side_effect=invoke):result=run.cleanup_owned(scratch,image,point,root,e,identity)
            self.assertTrue(result['verified']);self.assertIn(['losetup','--detach','/dev/loop7'],calls);self.assertFalse(scratch.exists())

    def test_cleanup_identity_drift_never_unmounts_or_detaches(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();scratch,image,point,identity=self.cleanup_fixture(root);calls=[];e={'commands':[]}
            def invoke(argv,output,evidence,label,**kwargs):
                calls.append(argv);evidence['commands'].append({'exit':0});(output/(label+'.stderr')).write_text('')
                if argv[0]=='losetup':return json.dumps({'loopdevices':[{'name':'/dev/loop7','back-file':str(image)}]})
                return json.dumps({'filesystems':[{'target':str(point),'source':'/dev/other','fstype':'ext4'}]})
            with patch.object(run,'invoke',side_effect=invoke),patch.object(run.time,'sleep'):
                result=run.cleanup_owned(scratch,image,point,root,e,identity)
            self.assertFalse(result['verified']);self.assertFalse(any(a[0]=='umount' or '--detach' in a for a in calls));self.assertTrue(scratch.exists())

    def test_findmnt_query_error_cannot_prove_absence_or_remove_scratch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();scratch,image,point,identity=self.cleanup_fixture(root);e={'commands':[]}
            def invoke(argv,output,evidence,label,**kwargs):
                evidence['commands'].append({'exit':1 if argv[0]=='findmnt' else 0})
                (output/(label+'.stderr')).write_text('findmnt query error' if argv[0]=='findmnt' else '')
                return '' if argv[0]=='findmnt' else json.dumps({'loopdevices':[]})
            with patch.object(run,'invoke',side_effect=invoke):
                result=run.cleanup_owned(scratch,image,point,root,e,identity)
            self.assertFalse(result['verified']);self.assertTrue(scratch.exists())

if __name__=='__main__':unittest.main()
