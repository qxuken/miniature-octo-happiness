"""Strict maintenance/kernel-error oracles; no privileged provisioning."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('space_maintenance',Path(__file__).with_name('maintenance.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
spec=importlib.util.spec_from_file_location('space_quota',Path(__file__).with_name('run.py'))
q=importlib.util.module_from_spec(spec);spec.loader.exec_module(q)
SOURCE='a'*40


class MaintenanceControls(unittest.TestCase):
    def failure(self,errno=122):
        return [{'stage':'fail','operation':'backup','pid':12,'error':14,'category':'Quota'if errno==122 else'DiskFull','raw':errno,'native_operation':'Write','partial_bytes':8*m.MIB,'source_unchanged':True,'backup_unchanged':True},m.R.completion('space-fail',SOURCE)]
    def test_classified_partial_copy_requires_real_errno_and_unchanged_inputs(self):
        for errno in [28,122]:
            good=self.failure(errno);m.validate(good,'space-fail','backup',SOURCE,errno)
            bads=[[],good[:1],good+good]
            for key,value in [('error',15),('raw',28 if errno==122 else 122),('native_operation','Create'),('partial_bytes',0),('partial_bytes',63074304),('source_unchanged',False),('backup_unchanged',1),('pid',True)]:
                bad=copy.deepcopy(good);bad[0][key]=value;bads.append(bad)
            bad=copy.deepcopy(good);bad[1]['source_id']='wrong';bads.append(bad)
            bad=copy.deepcopy(good);bad[1]['features']=4.0;bads.append(bad)
            for bad in bads:
                with self.assertRaises(AssertionError):m.validate(bad,'space-fail','backup',SOURCE,errno)
    def test_retry_and_oracle_reject_wrong_dispositions_and_bytes(self):
        for op in ['backup','restore','compact']:
            row={'stage':'retry','operation':op,'pid':12,'partial_rejected':True,'existing_rejected':op!='compact','partial_preserved':True,'matches_backup':True}
            good=[row,m.R.completion('space-retry',SOURCE)];m.validate(good,'space-retry',op,SOURCE,122)
            for key in ['partial_rejected','existing_rejected','partial_preserved','matches_backup']:
                bad=copy.deepcopy(good);bad[0][key]=not row[key]
                with self.assertRaises(AssertionError):m.validate(bad,'space-retry',op,SOURCE,122)
        state={'phase':'space_initial-source','revision':212,'seq':4,'changed':False,'roots':64,'digests':m.C.digests(4,False),'live_bytes':m.C.CORE//2,'peak_bytes':m.C.CORE,'cap_bytes':m.C.CORE,'rss_kib':1,'file_bytes':66154496,'elapsed_ns':0,'detail':{'fresh_process':True}}
        good=[state,m.R.completion('space-check',SOURCE)];m.validate(good,'space-check','initial-source',SOURCE,28)
        for key,value in [('revision',213),('roots',63),('digests',[0]*4)]:
            bad=copy.deepcopy(good);bad[0][key]=value
            with self.assertRaises(AssertionError):m.validate(bad,'space-check','initial-source',SOURCE,28)
    def test_larger_ballast_still_requires_privilege_and_distinct_space_failures(self):
        row={'mode':'fill','pid':1,'identity':{'uid':60000,'uids':[60000]*4,'gids':[60000]*4,'groups':[],'caps':{k:0 for k in ['CapInh','CapPrm','CapEff','CapBnd','CapAmb']},'no_new_privs':1},'global_free_bytes':128*m.MIB,'available_bytes':128*m.MIB,'errno':122,'written':128*m.MIB,'ceiling':m.CEILING}
        q.validate_probe(row,60000,'fill',ceiling=m.CEILING,expected_errno=122)
        for change in [{'errno':28},{'written':m.CEILING+1},{'available_bytes':0}]:
            with self.assertRaises(ValueError):q.validate_probe({**row,**change},60000,'fill',ceiling=m.CEILING,expected_errno=122)
        q.validate_probe({**row,'errno':28,'global_free_bytes':0,'available_bytes':0},60000,'fill',ceiling=m.CEILING,expected_errno=28)
        with self.assertRaises(ValueError):q.main(['--fixture','unused','--source',SOURCE,'--output','unused','--maintenance','--probe-only'])


if __name__=='__main__':unittest.main()
