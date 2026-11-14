import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parent))
import run

class Controls(unittest.TestCase):
    def test_outcomes_reject_false_ack_and_partial_commit(self):
        for mode in range(6):
            row={'mode':mode,'status':0 if mode==0 else 14 if mode in [1,5] else 15,'visible':4 if mode==0 else 3,'reopened':4 if mode in [0,2,3] else 3,'final':4,'retained_revision':2,'hits':1 if mode in [1,2,3,4] else 0,'syncs':[3,1,2,3,1,0][mode],'root_writes':int(mode in [0,2,3,4]),'witness_writes':int(mode in [0,3]),'native_errno':28 if mode==5 else 0,'verified':True}
            run.validate(row,mode)
            for key,value in [('status',99),('reopened',99),('visible',99),('hits',99),('native_errno',99),('verified',1)]:
                broken=copy.copy(row);broken[key]=value
                with self.assertRaises(ValueError):run.validate(broken,mode)
    def test_phases_cannot_claim_recovery_from_exercise_summary(self):
        exercise={'mode':1,'status':14,'visible':3,'hits':1,'syncs':1,'root_writes':0,'witness_writes':0,'native_errno':0,'verified':True}
        recovery={'mode':1,'reopened':3,'final':4,'retained_revision':2,'verified':True}
        run.combine(exercise,recovery,1)
        for a,b in [(exercise,{}),({**exercise,**recovery},recovery),(exercise,{**recovery,'mode':2}),(exercise,{**recovery,'verified':False}),({**exercise,'mode':True},recovery),(exercise,{**recovery,'mode':1.0})]:
            with self.assertRaises(ValueError):run.combine(a,b,1)

if __name__=='__main__':unittest.main()
