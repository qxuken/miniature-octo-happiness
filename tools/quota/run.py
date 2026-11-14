#!/usr/bin/env python3
"""Root-owned isolated Linux ext4 quota drill; no host-filesystem quota changes."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import pwd
import re
import selectors
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import uuid

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 'tools'))
import compatibility as helpers

IMAGE_BYTES = 64 * 1024 * 1024
LOG_BYTES = 1024 * 1024
MIN_FREE = 16 * 1024 * 1024
class Unsupported(RuntimeError): pass

def require(ok, message):
    if not ok: raise ValueError(message)

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def same(actual, expected):
    if type(actual) is not type(expected): return False
    if isinstance(expected, dict):
        return set(actual) == set(expected) and all(same(actual[k], v) for k, v in expected.items())
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(same(a, b) for a, b in zip(actual, expected))
    return actual == expected

def invoke(argv, output, evidence, label, *, timeout=30, allowed=(0,), maximum=LOG_BYTES, env=None):
    record = {'label': label, 'argv': list(map(str, argv)), 'timeout': timeout, 'exit': None}
    evidence['commands'].append(record)
    selector = selectors.DefaultSelector(); child = None
    started = time.monotonic(); deadline = started + timeout
    paths = {name: output / (label + '.' + name) for name in ['stdout', 'stderr']}
    try:
        with paths['stdout'].open('xb') as stdout, paths['stderr'].open('xb') as stderr:
            child = subprocess.Popen(record['argv'], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     start_new_session=True, env=env)
            streams = {child.stdout: [stdout, 0], child.stderr: [stderr, 0]}
            for stream in streams: selector.register(stream, selectors.EVENT_READ)
            while selector.get_map():
                remaining = deadline-time.monotonic()
                if remaining <= 0: raise TimeoutError('command deadline')
                events = selector.select(min(remaining, .2))
                for key, _ in events:
                    stream = key.fileobj; file, count = streams[stream]
                    block = os.read(stream.fileno(), min(65536, maximum-count+1))
                    if not block: selector.unregister(stream); continue
                    require(count+len(block) <= maximum, 'command output limit')
                    file.write(block); streams[stream][1] += len(block)
            remaining = deadline-time.monotonic()
            if remaining <= 0: raise TimeoutError('command deadline')
            child.wait(timeout=remaining)
            require(child.returncode in allowed, 'command failed: ' + label)
    finally:
        if child is not None:
            helpers.stop(child)
            record['exit'] = child.returncode
            child.stdout.close(); child.stderr.close()
        selector.close(); record['elapsed_seconds'] = time.monotonic()-started
    return paths['stdout'].read_text()

def one_json(text):
    require(len(text.encode()) <= LOG_BYTES, 'JSON result too large')
    value = json.loads(text)
    require(isinstance(value, dict), 'result must be one JSON object')
    return value

def loop_rows(text):
    value = one_json(text)
    require(isinstance(value.get('loopdevices'), list), 'invalid loop inventory')
    return value['loopdevices']

def owned_loops(text, image):
    rows = loop_rows(text); result = []
    for row in rows:
        backing = row.get('back-file')
        if backing == str(image):
            require(re.fullmatch(r'/dev/loop[0-9]+', row.get('name', '')) is not None, 'invalid owned loop name')
            result.append(row['name'])
    return result

def exact_mount(text, mountpoint, loop):
    value = one_json(text); rows = value.get('filesystems')
    require(isinstance(rows, list) and len(rows) == 1, 'one exact mount required')
    row = rows[0]
    require(row.get('target') == str(mountpoint) and row.get('source') == loop and row.get('fstype') == 'ext4',
            'mount/backing identity mismatch')
    return row

def validate_probe(row, uid, mode):
    require(row.get('mode') == mode and type(row.get('pid')) is int and row['pid'] > 0, 'wrong probe identity')
    ids = row.get('identity', {})
    require(same(ids, {'uid': uid, 'uids': [uid]*4, 'gids': [uid]*4, 'groups': [],
                    'caps': {key: 0 for key in ['CapInh','CapPrm','CapEff','CapBnd','CapAmb']}, 'no_new_privs': 1}),
            'guest privileges not verified')
    require(type(row.get('global_free_bytes')) is int and row['global_free_bytes'] > MIN_FREE and
            type(row.get('available_bytes')) is int and row['available_bytes'] > MIN_FREE,
            'global filesystem space too low to distinguish quota')
    if mode == 'fill':
        require(type(row.get('errno')) is int and row['errno'] == 122 and
                type(row.get('written')) is int and 0 <= row['written'] <= 8*1024*1024 and
                row.get('ceiling') == 8*1024*1024, 'actual bounded EDQUOT required')
    elif mode == 'remove': require(row.get('removed') is True, 'ballast removal not proven')
    elif mode == 'write-control': require(row.get('fsynced_bytes') == 4096, 'same-UID fsynced success required')
    return row

def rows_for(revision):
    new = revision == 4; length = 65536 if new else 512
    data = bytes((i*(17 if new else 13)+(23 if new else 7)) % 251 for i in range(length))
    return [{'key_hex':'7061796c6f6164','kind':'bytes','bytes_hex':data.hex()},
            {'key_hex':'726f77','kind':'integer','integer':18 if new else 17}]

def validate_fixture(row, source, stage, revision):
    expected = {'stage':stage, 'source':source, 'features':0, 'current_revision':revision,
                'retained_id':1, 'retained_revision':2, 'commit_error':14 if stage=='fail' else 0,
                'native_category':'Quota' if stage=='fail' else 'None',
                'native_raw_code':122 if stage=='fail' else None, 'native_operation':'Write' if stage=='fail' else 'None',
                'terminal_error':11 if stage=='fail' else 0, 'recovery_required':stage=='fail',
                'blocked_write_error':16 if stage=='fail' else 0,
                'current_rows':rows_for(revision), 'retained_rows':rows_for(3)}
    require(set(row) == set(expected)|{'pid'}, 'fixture schema mismatch')
    require(type(row['pid']) is int and row['pid'] > 0, 'invalid fixture process')
    for key,value in expected.items():
        require(same(row[key], value), 'fixture oracle mismatch: '+key)
    return row

def formatter_command(executable, image):
    # mkfs.ext4 commonly resolves to mke2fs: do not depend on argv[0] basename.
    return [executable, '-t', 'ext4', '-q', '-F', '-O', 'quota', '-E', 'quotatype=usrquota', str(image)]

def source_binding(source):
    git = ['git', '-c', 'safe.directory='+str(ROOT), '-C', str(ROOT)]
    resolved=subprocess.check_output([*git,'rev-parse','--verify',source+'^{commit}'],text=True,timeout=30).strip()
    require(resolved==source,'source must be the exact commit object, not a tag or tree')
    archive = subprocess.check_output([*git, 'archive', source], timeout=30)
    files = {}
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar:
            require(member.isdir() or member.isfile(), 'nonregular archived source')
            if member.isfile():
                path = ROOT/member.name
                require(not path.is_symlink() and path.is_file(), 'missing/symlink source')
                digest = hashlib.sha256(tar.extractfile(member).read()).hexdigest()
                require(sha(path) == digest, 'source differs from requested commit: '+member.name)
                files[member.name] = digest
    return {'source':source, 'commit_verified':True, 'archive_sha256':hashlib.sha256(archive).hexdigest(), 'files':files}

def cleanup_owned(scratch, image, mountpoint, output, evidence, image_identity, tool_paths=None):
    require(scratch.is_absolute() and re.fullmatch(r'weekend-quota-[0-9a-f]{32}-.+',scratch.name) is not None and not scratch.is_symlink() and scratch.stat().st_uid==os.geteuid(), 'unowned scratch')
    require(image == scratch/'image.ext4' and mountpoint == scratch/'mount', 'unowned quota paths')
    if image_identity is not None:
        info=image.lstat()
        require(stat.S_ISREG(info.st_mode) and (info.st_dev,info.st_ino)==image_identity, 'image identity changed')
    result = {'errors': [], 'mount_absent':False, 'loops_absent':False}; counter = 0
    def call(argv, allowed=(0,)):
        nonlocal counter
        counter += 1
        logical_command=argv[0]
        if tool_paths:argv=[tool_paths[argv[0]],*argv[1:]]
        text=invoke(argv, output, evidence, 'cleanup-'+str(counter), timeout=10, allowed=allowed)
        if logical_command=='findmnt':
            require((not text.strip()) == (evidence['commands'][-1]['exit']==1), 'ambiguous mount absence')
            if not text.strip():require(not (output/('cleanup-'+str(counter)+'.stderr')).read_text().strip(), 'mount absence query errored')
        return text
    identity_drift = False
    # Query before each destructive command. Never act on an unrelated mount/device.
    try:
        loops = owned_loops(call(['losetup','--json','--list','--output','NAME,BACK-FILE']), image)
        text = call(['findmnt','--json','--mountpoint',str(mountpoint),'--output','SOURCE,FSTYPE,TARGET,OPTIONS'], (0,1))
        if text.strip():
            require(len(loops)==1, 'cannot bind mounted loop')
            try:exact_mount(text,mountpoint,loops[0])
            except ValueError:
                identity_drift=True;raise
            call(['umount',str(mountpoint)])
    except (OSError, subprocess.SubprocessError, ValueError, TimeoutError) as error:
        result['errors'].append(type(error).__name__)
    try:
        loops = owned_loops(call(['losetup','--json','--list','--output','NAME,BACK-FILE']), image)
        require(not identity_drift, 'mount identity drift prevents detach')
        for loop in loops:
            call(['losetup','--detach',loop])
    except (OSError, subprocess.SubprocessError, ValueError, TimeoutError) as error:
        result['errors'].append(type(error).__name__)
    # Detach may be lazy; independently poll actual mount and backing associations.
    for _ in range(10):
        try:
            text = call(['findmnt','--json','--mountpoint',str(mountpoint),'--output','SOURCE,FSTYPE,TARGET,OPTIONS'], (0,1))
            result['mount_absent'] = not text.strip()
            result['loops_absent'] = not owned_loops(call(['losetup','--json','--list','--output','NAME,BACK-FILE']),image)
            if result['mount_absent'] and result['loops_absent']:break
        except (OSError, subprocess.SubprocessError, ValueError, TimeoutError) as error:
            result['errors'].append(type(error).__name__);break
        time.sleep(.1)
    result['verified'] = result['mount_absent'] and result['loops_absent']
    if result['verified']:shutil.rmtree(scratch)
    return result

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture',type=Path,required=True);parser.add_argument('--source',required=True)
    parser.add_argument('--output',type=Path,required=True);parser.add_argument('--probe-only',action='store_true')
    args = parser.parse_args(argv)
    require(sys.platform=='linux' and os.geteuid()==0,'requires Linux root controller; elevate only this entrypoint')
    require(re.fullmatch('[0-9a-f]{40}',args.source) is not None,'full source SHA required')
    output = helpers.fresh_output(args.output);logs=output/'logs';logs.mkdir()
    evidence={'verified':False,'status':'running','commands':[],'python':sys.version,'source':args.source,
              'controller_python':{'path':str(Path(sys.executable).resolve()),'sha256':sha(Path(sys.executable).resolve())}}
    scratch=None;image_identity=None;tool_paths={}
    try:
        require(args.fixture.is_absolute() and args.fixture.is_file() and not args.fixture.is_symlink() and args.fixture.stat().st_size<=64*1024*1024,'absolute bounded regular fixture required')
        evidence['source_binding']=source_binding(args.source)
        evidence['fixture_sha256']=sha(args.fixture)
        tools={name:sha(HERE/name) for name in ['run.py','probe.py','test_runner.py','fixture.rs']}
        tools['compatibility.py']=sha(ROOT/'tools/compatibility.py');evidence['tools_sha256']=tools
        tool_hashes={}
        for name in ['losetup','mkfs.ext4','mount','findmnt','setquota','setpriv','umount','uname']:
            found=shutil.which(name)
            if found is None:raise Unsupported('missing quota tool: '+name)
            tool_paths[name]=str(Path(found).resolve())
            tool_hashes[name]={'path':tool_paths[name],'sha256':sha(Path(tool_paths[name]))}
        evidence['executables']=tool_hashes
        scratch=Path(tempfile.mkdtemp(prefix='weekend-quota-'+uuid.uuid4().hex+'-'));scratch.chmod(0o711)
        image=scratch/'image.ext4';mountpoint=scratch/'mount';mountpoint.mkdir()
        fd=os.open(image,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
        with os.fdopen(fd,'wb') as file:file.truncate(IMAGE_BYTES)
        info=image.stat();image_identity=(info.st_dev,info.st_ino);evidence['image_identity']=list(image_identity)
        fixture=scratch/'fixture';shutil.copyfile(args.fixture,fixture);fixture.chmod(0o555)
        probe=scratch/'probe.py';shutil.copyfile(HERE/'probe.py',probe);probe.chmod(0o444)
        evidence.update(scratch=str(scratch),image_bytes=IMAGE_BYTES)
        sequence=0
        def call(argv, timeout=30, allowed=(0,), env=None):
            nonlocal sequence
            sequence+=1
            argv=list(argv)
            if str(argv[0]) in tool_paths:argv[0]=tool_paths[str(argv[0])]
            return invoke(argv,logs,evidence,'command-'+str(sequence),timeout=timeout,allowed=allowed,env=env)
        evidence['kernel']=call(['uname','-a'])
        try:
            call(formatter_command(tool_paths['mkfs.ext4'], image))
            loop=call(['losetup','--find','--show','--nooverlap',str(image)]).strip()
            require(owned_loops(call(['losetup','--json','--list','--output','NAME,BACK-FILE']),image)==[loop],'attached loop identity mismatch')
            call(['mount','-t','ext4','-o','rw,usrquota,nodev,nosuid,noexec',loop,str(mountpoint)])
            exact_mount(call(['findmnt','--json','--mountpoint',str(mountpoint),'--output','SOURCE,FSTYPE,TARGET,OPTIONS']),mountpoint,loop)
        except (OSError, subprocess.SubprocessError, ValueError, TimeoutError) as error:
            raise Unsupported('private quota filesystem unavailable') from error
        uid=60000
        while uid<65000:
            try:pwd.getpwuid(uid)
            except KeyError:break
            uid+=1
        require(uid<65000,'no unused private numeric UID')
        data=mountpoint/'data';data.mkdir(mode=0o700);os.chown(data,uid,uid)
        evidence['quota']={'uid':uid,'gid':uid,'type':'user','soft_kib':4096,'hard_kib':4096,'inode_soft':0,'inode_hard':0,'image_bytes':IMAGE_BYTES}
        # Identity check immediately before configuring quotas on the private mount.
        require(owned_loops(call(['losetup','--json','--list','--output','NAME,BACK-FILE']),image)==[loop],'loop changed before quota')
        exact_mount(call(['findmnt','--json','--mountpoint',str(mountpoint),'--output','SOURCE,FSTYPE,TARGET,OPTIONS']),mountpoint,loop)
        try:call(['setquota','-u',str(uid),'4096','4096','0','0',str(mountpoint)])
        except (OSError,subprocess.SubprocessError,ValueError,TimeoutError) as error:
            raise Unsupported('private quota configuration unavailable') from error
        drop=['setpriv','--reuid',str(uid),'--regid',str(uid),'--clear-groups','--inh-caps=-all','--ambient-caps=-all','--bounding-set=-all','--no-new-privs']
        env=helpers.child_env()
        for name in ['LD_PRELOAD','LD_LIBRARY_PATH','PYTHONPATH','PYTHONHOME']:env.pop(name,None)
        env['WEEKEND_DB_SOURCE_ID']=args.source
        # The pinned controller Python may live below a private home directory.
        # Use the distro Python for the guest; bind its identity and exact hash.
        guest_python=Path('/usr/bin/python3').resolve();require(guest_python.is_file(),'guest Python unavailable')
        evidence['guest_python']={'path':str(guest_python),'sha256':sha(guest_python),
                                  'version':call([*drop,guest_python,'-I','--version'],env=env).strip()}
        def guest_probe(mode):
            return validate_probe(one_json(call([*drop,guest_python,'-I','-B',probe,mode,data,str(uid)],env=env)),uid,mode)
        evidence['guest_identity']=guest_probe('identity')
        evidence['calibration_full']=guest_probe('fill');guest_probe('remove')
        evidence['calibration_control']=guest_probe('write-control')
        if args.probe_only:
            evidence['status']='capability-only';return 0
        results=[];db=data/'database.wdb'
        def phase(stage, revision):
            argv=[*drop,fixture,stage,db,*([str(revision)] if stage=='inspect' else [])]
            row=validate_fixture(one_json(call(argv,env=env)),args.source,stage,revision);results.append(row)
        phase('seed',3);evidence['engine_full']=guest_probe('fill')
        phase('fail',3);phase('inspect',3)
        guest_probe('remove');phase('retry',4);phase('inspect',4)
        require(len({r['pid'] for r in results})==len(results),'fixture phases must be independent processes')
        require(sha(args.fixture)==evidence['fixture_sha256'] and sha(fixture)==evidence['fixture_sha256'],'fixture changed')
        require(source_binding(args.source)==evidence['source_binding'],'source changed')
        require(all(sha(Path(row['path']))==row['sha256'] for row in tool_hashes.values()),'privileged executable changed')
        require(sha(Path(evidence['controller_python']['path']))==evidence['controller_python']['sha256'],'controller Python changed')
        require(sha(guest_python)==evidence['guest_python']['sha256'],'guest Python changed')
        current={name:sha(HERE/name) for name in ['run.py','probe.py','test_runner.py','fixture.rs']}
        current['compatibility.py']=sha(ROOT/'tools/compatibility.py');require(current==tools,'qualification tools changed')
        evidence.update(results=results,status='qualified',verified=True)
    except Unsupported as error:
        evidence.update(status='unsupported',failure=str(error))
        if args.probe_only:return 0
        raise
    except BaseException as error:
        evidence.update(status='failed',failure_type=type(error).__name__);raise
    finally:
        if scratch is not None:
            try:evidence['cleanup']=cleanup_owned(scratch,scratch/'image.ext4',scratch/'mount',logs,evidence,image_identity,tool_paths)
            except BaseException as error:evidence['cleanup']={'verified':False,'failure_type':type(error).__name__}
            if not evidence['cleanup']['verified']:evidence['verified']=False;evidence['status']='cleanup-failed'
        evidence['artifact_sha256']={str(p.relative_to(output)):sha(p) for p in output.rglob('*') if p.is_file()}
        (output/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
        require(scratch is None or evidence['cleanup']['verified'],'owned quota resources remain')
    return 0

if __name__=='__main__':raise SystemExit(main())
