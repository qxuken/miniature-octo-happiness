"""Shared near-cap maintenance oracle for real kernel ENOSPC and EDQUOT."""
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('space_resilience', ROOT/'tools/resource-envelope/resilience.py')
R = importlib.util.module_from_spec(spec); spec.loader.exec_module(R)
C = R.C
MIB = 1024*1024
CEILING = 384*MIB


def validate(rows, mode, label, source, errno):
    C.require(isinstance(rows,list) and len(rows)==2, 'incomplete space fixture')
    C.require(type(rows[-1].get('complete'))is bool and all(type(rows[-1].get(k))is int for k in ['features','abi','format']), 'invalid completion types')
    C.require(rows[-1]==R.completion(mode,source), 'wrong fixture build identity')
    row=rows[0]
    if mode in ['space-check','space-advance']:
        changed=label in ['advanced','advanced-source']
        R.state(row,'space_'+label,213 if changed else 212,changed)
        C.require(row['detail']=={'fresh_process':True} and type(row['detail']['fresh_process'])is bool,'missing fresh checker')
    else:
        C.require(type(row.get('pid'))is int and row['pid']>0, 'missing fixture pid')
        C.require(row.get('operation')==label and row.get('stage')==mode.removeprefix('space-'),'wrong maintenance operation')
        if mode=='space-fail':
            C.require(set(row)=={'stage','operation','pid','error','category','raw','native_operation','partial_bytes','source_unchanged','backup_unchanged'},'wrong failure schema')
            C.require(type(row['error'])is int and row['error']==14 and type(row['raw'])is int and row['raw']==errno and row['category']==('Quota'if errno==122 else'DiskFull'),'actual classified kernel error required')
            C.require(row['native_operation']in ['Write','Sync'] and type(row['partial_bytes'])is int and MIB<=row['partial_bytes']<63074304,'not a partial-copy exhaustion')
            C.require(row['source_unchanged']is True and row['backup_unchanged']is True,'source changed on failure')
        elif mode=='space-retry':
            C.require(set(row)=={'stage','operation','pid','partial_rejected','existing_rejected','partial_preserved','matches_backup'},'wrong retry schema')
            C.require(row['partial_rejected']is True and row['partial_preserved']is True and row['matches_backup']is True and type(row['existing_rejected'])is bool and row['existing_rejected']==(label!='compact'),'invalid retry/partial disposition')
        elif mode=='space-retire':
            C.require(set(row)=={'stage','operation','pid','owned_outputs_removed'} and row['owned_outputs_removed']is True,'unretired owned outputs')
        else:raise AssertionError('unknown fixture mode')
    return row


def exercise(call, drop, fixture, data, seed, probe, python, uid, env, source, evidence, errno, validate_probe):
    from hashlib import sha256
    digest=lambda p:sha256(p.read_bytes()).hexdigest()
    records=[];pids=[];env={**env,'WEEKEND_PRIVATE_MAINTENANCE':'quota'if errno==122 else'enospc'}
    def guest(mode):
        from_probe=json.loads(call([*drop,python,'-I','-B',probe,mode,data,str(uid),'--ceiling',str(CEILING),*(['--seed',seed]if mode=='install-seed'else[])],timeout=120,env=env))
        # Use the existing strict privilege/global-space checks, with the larger
        # independent ceiling and explicit distinction between quota and ENOSPC.
        validate_probe(from_probe,uid,mode,ceiling=CEILING,expected_errno=errno)
        return from_probe
    def phase(mode,label,image=None,changed=False):
        args=[*drop,fixture,data,image or data/'source.wdb',mode,label,*(['changed']if changed else[])]
        rows=[json.loads(line)for line in call(args,timeout=120,env=env).splitlines()]
        row=validate(rows,mode,label,source,errno)
        pid=evidence['commands'][-1]['pid'];pids.append(pid)
        if 'pid'in row:C.require(row['pid']==pid,'fixture PID substitution')
        records.append({'mode':mode,'label':label,'pid':pid,'rows':rows})
        return row
    seed_rows=[json.loads(line)for line in call([*drop,fixture,seed],timeout=180,env=env).splitlines()]
    C.check_rows(seed_rows,'native',source);pids.append(evidence['commands'][-1]['pid'])
    expected={n:{'bytes':(seed/n).stat().st_size,'sha256':digest(seed/n)}for n in ['source.wdb','backup.wdb']}
    installed=guest('install-seed');C.require(installed.get('installed')==expected,'installed seed differs')
    evidence['maintenance_seed']={'rows':seed_rows,'installed':expected}
    phase('space-check','initial-source');phase('space-check','initial-backup',data/'backup.wdb')
    # A deliberately non-full copy must make the required-failure fixture fail,
    # then its actual successful output must pass an independent process oracle.
    call([*drop,fixture,data,data/'source.wdb','space-fail','backup'],timeout=120,allowed=(101,),env=env)
    command=evidence['commands'][-1];pids.append(command['pid'])
    stderr=(Path(evidence['_logs'])/(command['label']+'.stderr')).read_text()
    C.require('Result::unwrap_err()'in stderr and '`Ok`'in stderr and 'CompactionStats'in stderr,'negative control failed at wrong assertion')
    negative=data/'failed-backup.wdb'
    C.require(negative.is_file()and not negative.is_symlink()and negative.stat().st_uid==uid and negative.stat().st_size==63074304 and digest(negative)==digest(data/'backup.wdb'),'non-full negative copy not actually successful')
    phase('space-check','negative-output',negative)
    evidence['maintenance_negative']={'exit':101,'pid':command['pid'],'required_failure_rejected':True,'successful_copy_verified':True}
    negative.unlink()
    cases=[]
    for op in ['backup','restore','compact']:
        original=digest(data/'source.wdb');backup=digest(data/'backup.wdb')
        full=guest('fill');headroom=guest('headroom')
        failed=phase('space-fail',op)
        C.require(digest(data/'source.wdb')==original and digest(data/'backup.wdb')==backup,'independent failure source hash')
        phase('space-check','full-source-'+op);phase('space-check','full-backup-'+op,data/'backup.wdb')
        removed=guest('remove');control=guest('write-control')
        phase('space-retry',op)
        phase('space-check','retried-source-'+op)
        phase('space-check','retried-output-'+op,data/'source.wdb'if op=='compact'else data/('retry-'+op+'.wdb'))
        phase('space-check','unchanged-backup-'+op,data/'backup.wdb')
        phase('space-retire',op)
        cases.append({'operation':op,'full':full,'headroom':headroom,'failure':failed,'ballast_removed':removed,'write_control':control})
    phase('space-advance','advanced');phase('space-check','advanced-source',changed=True);phase('space-check','final-backup',data/'backup.wdb')
    C.require(len(set(pids))==len(pids),'fixture phases reused a process')
    C.require({p.name for p in data.iterdir()}=={'source.wdb','backup.wdb'},'unretired outputs or ballast')
    evidence.update(maintenance_cases=cases,maintenance_records=records,maintenance_pids=pids)
    # Synthetic final images are bounded and retained separately from the mount.
    return [data/'source.wdb',data/'backup.wdb']
