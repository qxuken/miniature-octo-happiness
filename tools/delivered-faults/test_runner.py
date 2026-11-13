import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import run

class Controls(unittest.TestCase):
    def test_admission_preserves_existing_dangling_and_package_namespace(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp);existing=p/'existing';existing.mkdir();(existing/'keep').write_text('keep')
            link=p/'link';link.symlink_to(p/'missing');package=p/'package';package.mkdir()
            for output in [existing,link,p/'absent/child',package/'new']:
                with patch.object(sys,'argv',['run.py','--package',str(package),'--output',str(output)]),patch.object(run.subprocess,'check_output') as command:
                    with self.assertRaises(SystemExit):run.main()
                    command.assert_not_called()
            self.assertEqual((existing/'keep').read_text(),'keep');self.assertFalse((p/'missing').exists());self.assertFalse((package/'new').exists())
    def test_outcomes_reject_false_ack_and_partial_commit(self):
        for mode in range(6):
            row={'mode':mode,'status':0 if mode==0 else 14 if mode in [1,5] else 15,'visible':4 if mode==0 else 3,'reopened':4 if mode in [0,2,3] else 3,'final':4,'retained_revision':2,'hits':1 if mode in [1,2,3,4] else 0,'syncs':[3,1,2,3,1,0][mode],'root_writes':int(mode in [0,2,3,4]),'witness_writes':int(mode in [0,3]),'native_errno':28 if mode==5 else 0,'verified':True}
            run.validate(row,mode)
            for key,value in [('status',99),('reopened',99),('visible',99),('hits',99),('native_errno',99),('verified',1)]:
                broken=copy.copy(row);broken[key]=value
                with self.assertRaises(ValueError):run.validate(broken,mode)
    def test_cleanup_always_attempts_both_then_requires_live_absence(self):
        name='weekend-delivered-faults-'+'a'*32;volume=name+'-data'
        for found,expected in [('',True),(volume+'\n',False)]:
            with patch.object(run.subprocess,'run',side_effect=[subprocess.TimeoutExpired('docker',30),subprocess.CompletedProcess([],0)] ) as call,patch.object(run.subprocess,'check_output',side_effect=['',found]):
                self.assertEqual(run.cleanup(name,volume),expected)
                self.assertEqual(call.call_args_list[1].args[0],['docker','volume','rm',volume])
        with patch.object(run.subprocess,'run') as call:
            with self.assertRaises(ValueError):run.cleanup('unrelated','data')
            call.assert_not_called()
        with patch.object(run.subprocess,'run'),patch.object(run.subprocess,'check_output',side_effect=subprocess.CalledProcessError(1,'docker')):
            with self.assertRaises(subprocess.CalledProcessError):run.cleanup(name,volume)
    def test_phases_cannot_claim_recovery_from_exercise_summary(self):
        exercise={'mode':1,'status':14,'visible':3,'hits':1,'syncs':1,'root_writes':0,'witness_writes':0,'native_errno':0,'verified':True}
        recovery={'mode':1,'reopened':3,'final':4,'retained_revision':2,'verified':True}
        run.combine(exercise,recovery,1)
        for a,b in [(exercise,{}),({**exercise,**recovery},recovery),(exercise,{**recovery,'mode':2}),(exercise,{**recovery,'verified':False}),({**exercise,'mode':True},recovery),(exercise,{**recovery,'mode':1.0})]:
            with self.assertRaises(ValueError):run.combine(a,b,1)
    def test_failed_setup_retains_false_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp)/'out'
            with patch.object(sys,'argv',['run.py','--package',tmp,'--output',str(out)]):
                # The package-namespace check rejects before acquiring output ownership.
                with self.assertRaises(SystemExit):run.main()
                self.assertFalse(out.exists())
            package=Path(tmp)/'package';package.mkdir()
            with patch.object(sys,'argv',['run.py','--package',str(package),'--output',str(out)]),patch.object(run,'verify',side_effect=ValueError('invalid package')):
                with self.assertRaises(ValueError):run.main()
            self.assertIs(json.loads((out/'evidence.json').read_text())['verified'],False)

if __name__=='__main__':unittest.main()
