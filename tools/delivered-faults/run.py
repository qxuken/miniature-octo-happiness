#!/usr/bin/env python3
"""Private exact-package Linux C ABI fault drill; no host filesystem exhaustion."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import uuid

HERE=Path(__file__).resolve().parent
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def inventory(path):return {str(p.relative_to(path)):sha(p) for p in sorted(path.rglob('*')) if p.is_file()}
def verify(package):
    d=json.loads((package/'manifest.json').read_text())
    if d.get('verified') is not True or not re.fullmatch('[0-9a-f]{40}',d.get('source','')) or d.get('source_id')!=d['source']:raise ValueError('unverified exact-source package')
    hashes=d.get('sha256',{})
    if not hashes:raise ValueError('empty package inventory')
    for p in package.rglob('*'):
        if p.is_symlink():raise ValueError('package symlink')
    if set(inventory(package))!=set(hashes)|{'manifest.json'}:raise ValueError('package inventory')
    for name,digest in hashes.items():
        if Path(name).is_absolute() or '..' in Path(name).parts or sha(package/name)!=digest:raise ValueError('package hash/path')
    if not (package/'libweekend_db.so').is_file() or d.get('features')!='crdt,replication':raise ValueError('requires combined native Linux package')
    return d

def validate(row,mode):
    expected={'mode':mode,'status':0 if mode==0 else 14 if mode in [1,5] else 15,
              'visible':4 if mode==0 else 3,'reopened':4 if mode in [0,2,3] else 3,'final':4,
              'retained_revision':2,'hits':1 if mode in [1,2,3,4] else 0,
              'syncs':[3,1,2,3,1,0][mode], 'root_writes':1 if mode in [0,2,3,4] else 0,
              'witness_writes':1 if mode in [0,3] else 0,'native_errno':28 if mode==5 else 0,'verified':True}
    if set(row)!=set(expected) or any(type(row[k]) is not type(v) or row[k]!=v for k,v in expected.items()):raise ValueError('unexpected ABI/state/fault outcome')
    return row

def combine(exercise,recovery,mode):
    if set(exercise)!={'mode','status','visible','hits','syncs','root_writes','witness_writes','native_errno','verified'} or set(recovery)!={'mode','reopened','final','retained_revision','verified'}:
        raise ValueError('missing independent exercise/recovery phase')
    if type(exercise['mode']) is not int or type(recovery['mode']) is not int or exercise['mode']!=recovery['mode'] or exercise['verified'] is not True or recovery['verified'] is not True:
        raise ValueError('phase identity or completion mismatch')
    return validate({**exercise,**recovery},mode)

def main():
    if len(sys.argv)>2 and sys.argv[1]=='--oracle':
        action=sys.argv[2]
        if action=='verify': result=verify(Path(sys.argv[3]))
        elif action=='combine':
            value=json.loads(Path(sys.argv[3]).read_text())
            result=combine(value['exercise'],value['recovery'],value['mode'])
        else:raise ValueError('unknown oracle')
        print(json.dumps(result));return
    sys.path.insert(0,str(HERE.parent))
    import supervision
    supervision.command(['faults','delivered',*sys.argv[1:]])

if __name__=='__main__':main()
