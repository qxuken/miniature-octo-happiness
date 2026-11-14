"""Run solely on a UUID private Linux volume from a read-only frozen seed."""
import json
from pathlib import Path
import shutil
import sys
import run

def state(root,expected):
    actual={name:run.sha(root/name) for name in expected}
    if actual!=expected: raise ValueError('frozen input state changed')
    return actual

def main():
    assert len(sys.argv)==2
    seed=Path('/seed');work=Path('/private-volume/run')
    Path('/private-volume/tmp').mkdir()
    expected=json.loads((seed/'seed-manifest.json').read_text())
    if {k:v for k,v in run.inventory(seed).items() if k!='seed-manifest.json'}!=expected:
        raise ValueError('frozen seed inventory mismatch')
    shutil.copytree(seed,work)
    before=state(work,expected)
    (work/'frozen-before.json').write_text(json.dumps(before,indent=2)+'\n')
    filesystem=work/'filesystem';filesystem.mkdir()
    records={}
    for label,path in [('build',work/'cargo-target'),('runtime',work)]:
        path.mkdir(exist_ok=True)
        data=json.loads(run.run(['findmnt','--json','--target',path,'--output','TARGET,SOURCE,FSTYPE,OPTIONS'],filesystem,label))
        records[label]=data
        if len(data['filesystems'])!=1 or data['filesystems'][0]['fstype']!='ext4':
            raise ValueError('native Linux profile requires verified ext4')
    for package in ['relocated-package','relocated-source']: run.verify(work/package)
    result=run.local(work,work/'relocated-package',work/'relocated-source',sys.argv[1],None)
    for package in ['relocated-package','relocated-source']: run.verify(work/package)
    after=state(work,expected)
    if before!=after: raise ValueError('frozen inputs changed during native run')
    (work/'frozen-after.json').write_text(json.dumps(after,indent=2)+'\n')
    result.update(filesystem=records,frozen_inputs_verified=True,
                  frozen_state_before_sha256=run.sha(work/'frozen-before.json'),
                  frozen_state_after_sha256=run.sha(work/'frozen-after.json'))
    (work/'native-result.json').write_text(json.dumps(result,indent=2)+'\n')

if __name__=='__main__': main()
