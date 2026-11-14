#!/usr/bin/env python3
"""Native x64 MSVC C/C++ trace against a relocated, manifest-pinned DLL package."""
import argparse
import ctypes
import hashlib
import json
import ntpath
import os
from pathlib import Path, PureWindowsPath
import platform
import re
import shutil
import signal
import struct
import subprocess
import sys
import threading
import time

import model

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
TOOLS = ROOT / 'tools'
TRACE_SHA256 = '4ad2fe8534dc73bd3032f1dcc41220f5bf919482c1f7df0a0a8716c4c40812c8'
EXPECTED_SHA256 = '0075c8251dae1a0b2f1fd6d4ef152da3c581a947abd8edebad7f4f33aa74cef3'
SOURCE_FILES = [HERE/'model.py', HERE/'consumer.c', HERE/'windows_consumer.c', Path(__file__).resolve(), TOOLS/'windows_consumers.py']


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def unique_object(pairs):
    value = {}
    for key, entry in pairs:
        require(key not in value, 'duplicate JSON key: '+key)
        value[key] = entry
    return value


def read_json(path):
    require(path.stat().st_size <= 2*1024*1024, 'oversized JSON')
    return json.loads(path.read_bytes(), object_pairs_hook=unique_object)


def relative_name(name):
    require(isinstance(name, str) and name and '\x00' not in name, 'invalid payload name')
    path = PureWindowsPath(name)
    require(not path.drive and not path.root and all(p not in ('', '.', '..') for p in name.replace('\\', '/').split('/')), 'absolute/escaping payload')
    devices = {'CON','PRN','AUX','NUL', *('COM'+str(i) for i in range(1,10)), *('LPT'+str(i) for i in range(1,10))}
    for part in path.parts:
        require(':' not in part and part[-1:] not in (' ', '.') and part.split('.')[0].upper() not in devices, 'ambiguous Windows payload')
    return path.as_posix()


def inventory(root):
    result = {}
    for parent, directories, names in os.walk(root, followlinks=False):
        for name in sorted(directories+names):
            p = Path(parent)/name
            require(not p.is_symlink() and not getattr(p, 'is_junction', lambda: False)() and
                    not (getattr(p.lstat(), 'st_file_attributes', 0) & 0x400), 'reparse/symlink input rejected')
            if p.is_file():
                require(p.stat().st_size <= 256*1024*1024, 'oversized input file')
                result[p.relative_to(root).as_posix()] = sha(p)
        require(len(result) <= 4096, 'package file limit')
    require(len({p.casefold() for p in result}) == len(result), 'case-aliased inventory')
    return result


def verify_package(package, manifest_hash):
    require(re.fullmatch('[0-9a-f]{64}', manifest_hash) is not None, 'pinned manifest SHA-256 required')
    require(not package.is_symlink() and not getattr(package, 'is_junction', lambda: False)(), 'package alias rejected')
    manifest = package/'manifest.json'
    require(sha(manifest) == manifest_hash, 'manifest pin mismatch')
    info = read_json(manifest)
    require(info.get('verified') is True and info.get('candidate_eligible') is True and info.get('dirty_status') == '', 'clean verified package required')
    require(re.fullmatch('[0-9a-f]{40}', info.get('source', '')) is not None and info.get('source_id') == info['source'], 'exact source identity required')
    require('host: x86_64-pc-windows-msvc' in info.get('toolchain', '').splitlines(), 'native x64 MSVC package required')
    require(isinstance(info.get('sha256'), dict) and info['sha256'], 'empty package inventory')
    hashes = {}
    for name, digest in info['sha256'].items():
        name = relative_name(name)
        require(re.fullmatch('[0-9a-f]{64}', digest) is not None, 'invalid payload hash')
        require(name.casefold() not in {p.casefold() for p in hashes} and name.casefold() != 'manifest.json', 'duplicate/self-hashed payload')
        hashes[name] = digest
    actual = inventory(package)
    require(set(actual) == set(hashes)|{'manifest.json'}, 'package inventory mismatch')
    require(all(actual[name] == digest for name, digest in hashes.items()), 'package payload mismatch')
    require({'weekend_db.dll','weekend_db.dll.lib','weekend_db.h'} <= set(hashes), 'DLL/import library/header missing')
    features = info.get('features')
    require(isinstance(features, str), 'feature profile missing')
    names = features.split(',') if features else []
    require(len(set(names)) == len(names) and set(names) <= {'crdt','replication'}, 'unsupported feature profile')
    bits = (1 if 'replication' in names else 0) | (2 if 'crdt' in names else 0)
    return info, bits


def canonical_windows(path):
    require(isinstance(path, str) and PureWindowsPath(path).is_absolute(), 'absolute loaded DLL path required')
    if path.startswith('\\\\?\\UNC\\'):
        path = '\\\\'+path[8:]
    elif path.startswith('\\\\?\\'):
        path = path[4:]
    return ntpath.normcase(ntpath.normpath(path))


def check_attestation(record, phase, dll, digest, source, features, pid):
    expected = {'schema':1,'phase':phase,'pid':pid,'loaded_dll':str(dll),'sha256':digest,
                'source_id':source,'abi':1,'format':2,'features':features}
    require(isinstance(record, dict) and set(record) == set(expected), 'loader proof schema mismatch')
    for key, value in expected.items():
        require(type(record[key]) is type(value), 'loader proof type mismatch: '+key)
        if key == 'loaded_dll':
            require(canonical_windows(record[key]) == canonical_windows(value), 'wrong DLL actually loaded')
        else:
            require(record[key] == value, 'loader proof mismatch: '+key)


def windows_volume(directory):
    from ctypes import wintypes as w
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    kernel.GetCurrentProcess.argtypes=[];kernel.GetCurrentProcess.restype=w.HANDLE
    kernel.IsWow64Process2.argtypes=[w.HANDLE,ctypes.POINTER(w.USHORT),ctypes.POINTER(w.USHORT)]
    kernel.IsWow64Process2.restype=w.BOOL
    process_machine=w.USHORT();native_machine=w.USHORT()
    require(kernel.IsWow64Process2(kernel.GetCurrentProcess(),ctypes.byref(process_machine),ctypes.byref(native_machine)), 'native architecture query failed')
    require(process_machine.value==0 and native_machine.value==0x8664,'native x64 Python/Windows required; emulation is a separate profile')
    kernel.GetVolumePathNameW.argtypes=[w.LPCWSTR,w.LPWSTR,w.DWORD]
    kernel.GetVolumePathNameW.restype=w.BOOL
    kernel.GetVolumeInformationW.argtypes=[w.LPCWSTR,w.LPWSTR,w.DWORD,ctypes.POINTER(w.DWORD),ctypes.POINTER(w.DWORD),ctypes.POINTER(w.DWORD),w.LPWSTR,w.DWORD]
    kernel.GetVolumeInformationW.restype=w.BOOL
    kernel.GetDriveTypeW.argtypes=[w.LPCWSTR];kernel.GetDriveTypeW.restype=w.UINT
    root=ctypes.create_unicode_buffer(32768);filesystem=ctypes.create_unicode_buffer(64)
    serial=w.DWORD();component=w.DWORD();flags=w.DWORD()
    require(kernel.GetVolumePathNameW(str(directory),root,len(root)), 'data volume lookup failed')
    require(kernel.GetVolumeInformationW(root,None,0,ctypes.byref(serial),ctypes.byref(component),ctypes.byref(flags),filesystem,len(filesystem)), 'data filesystem query failed')
    drive_type=kernel.GetDriveTypeW(root)
    require(filesystem.value.upper()=='NTFS' and drive_type==3,'local fixed NTFS data volume required')
    return {'root':root.value,'filesystem':filesystem.value,'drive_type':drive_type,'serial':serial.value,'flags':flags.value,
            'process_machine':process_machine.value,'native_machine':native_machine.value}


class WindowsJob:
    """Own a suspended child's entire tree before allowing it to execute."""
    def __init__(self, child):
        require(ctypes.sizeof(ctypes.c_void_p)==8,'x64 Python required for native job ownership')
        from ctypes import wintypes as w
        self.k = ctypes.WinDLL('kernel32', use_last_error=True)
        k = self.k
        class Limits(ctypes.Structure):
            _fields_ = [('user',ctypes.c_int64),('job_user',ctypes.c_int64),('flags',w.DWORD),
                        ('min_ws',ctypes.c_size_t),('max_ws',ctypes.c_size_t),('active',w.DWORD),
                        ('affinity',ctypes.c_size_t),('priority',w.DWORD),('scheduling',w.DWORD)]
        class IO(ctypes.Structure):
            _fields_ = [(n,ctypes.c_uint64) for n in ['read_ops','write_ops','other_ops','read_bytes','write_bytes','other_bytes']]
        class Extended(ctypes.Structure):
            _fields_ = [('basic',Limits),('io',IO),*[(n,ctypes.c_size_t) for n in ['process_memory','job_memory','peak_process','peak_job']]]
        class Accounting(ctypes.Structure):
            _fields_ = [(n,ctypes.c_int64) for n in ['user','kernel','period_user','period_kernel']]+[(n,w.DWORD) for n in ['faults','total','active','terminated']]
        class Thread(ctypes.Structure):
            _fields_ = [(n,w.DWORD) for n in ['size','usage','id','owner']]+[('base',w.LONG),('delta',w.LONG),('flags',w.DWORD)]
        self.Accounting = Accounting
        signatures = {
            'CreateJobObjectW':([ctypes.c_void_p,w.LPCWSTR],w.HANDLE),
            'SetInformationJobObject':([w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD],w.BOOL),
            'QueryInformationJobObject':([w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD,ctypes.c_void_p],w.BOOL),
            'AssignProcessToJobObject':([w.HANDLE,w.HANDLE],w.BOOL),
            'TerminateJobObject':([w.HANDLE,w.UINT],w.BOOL),
            'OpenProcess':([w.DWORD,w.BOOL,w.DWORD],w.HANDLE),
            'CreateToolhelp32Snapshot':([w.DWORD,w.DWORD],w.HANDLE),
            'Thread32First':([w.HANDLE,ctypes.POINTER(Thread)],w.BOOL),
            'Thread32Next':([w.HANDLE,ctypes.POINTER(Thread)],w.BOOL),
            'OpenThread':([w.DWORD,w.BOOL,w.DWORD],w.HANDLE),
            'ResumeThread':([w.HANDLE],w.DWORD),
            'CloseHandle':([w.HANDLE],w.BOOL),
        }
        for name,(arguments,result) in signatures.items():
            function = getattr(k,name); function.argtypes=arguments; function.restype=result
        self.handle = k.CreateJobObjectW(None,None)
        if not self.handle: raise ctypes.WinError(ctypes.get_last_error())
        try:
            limits = Extended(); limits.basic.flags = 0x2000 # KILL_ON_JOB_CLOSE
            self.ok(k.SetInformationJobObject(self.handle,9,ctypes.byref(limits),ctypes.sizeof(limits)))
            process = k.OpenProcess(0x0101,False,child.pid) # SET_QUOTA | TERMINATE
            self.ok(process)
            try: self.ok(k.AssignProcessToJobObject(self.handle,process))
            finally: k.CloseHandle(process)
            snapshot = k.CreateToolhelp32Snapshot(0x4,0)
            require(snapshot not in (None,ctypes.c_void_p(-1).value), 'thread snapshot failed')
            try:
                entry = Thread(); entry.size=ctypes.sizeof(entry)
                found = []; valid=k.Thread32First(snapshot,ctypes.byref(entry))
                while valid:
                    if entry.owner == child.pid: found.append(entry.id)
                    entry.size=ctypes.sizeof(entry); valid=k.Thread32Next(snapshot,ctypes.byref(entry))
                require(len(found)==1, 'one suspended primary thread required')
                thread = k.OpenThread(0x2,False,found[0])
                self.ok(thread)
                try: require(k.ResumeThread(thread)==1, 'cannot resume owned primary thread')
                finally: k.CloseHandle(thread)
            finally: k.CloseHandle(snapshot)
        except BaseException:
            self.close(); raise

    @staticmethod
    def ok(result):
        if not result: raise ctypes.WinError(ctypes.get_last_error())

    def close(self):
        if not self.handle: return
        try:
            self.ok(self.k.TerminateJobObject(self.handle,0xEE))
            deadline=time.monotonic()+5
            while True:
                info=self.Accounting()
                self.ok(self.k.QueryInformationJobObject(self.handle,1,ctypes.byref(info),ctypes.sizeof(info),None))
                if info.active==0: break
                require(time.monotonic()<deadline,'job children remain active')
                time.sleep(.01)
        finally:
            self.k.CloseHandle(self.handle); self.handle=None


def execute(argv, directory, label, commands, *, env=None, timeout=120, any_exit=False, maximum=4*1024*1024):
    record={'command':list(map(str,argv)), 'timeout':timeout, 'exit':None, 'cleanup_verified':False}
    commands.append(record); started=time.monotonic(); child=None; job=None
    failures=[]; threads=[]; overflow=threading.Event()
    def drain(pipe,path):
        try:
            with path.open('xb') as output:
                total=0
                while True:
                    block=pipe.read1(65536)
                    if not block:break
                    remaining=max(0,maximum-total);output.write(block[:remaining]);total+=len(block)
                    if total>maximum:overflow.set()
        except BaseException as error:
            failures.append(type(error).__name__);overflow.set()
        finally:pipe.close()
    try:
        kwargs={'creationflags':0x4} if os.name=='nt' else {'start_new_session':True}
        child=subprocess.Popen(record['command'],cwd=directory,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,**kwargs)
        record['pid']=child.pid
        if os.name=='nt':job=WindowsJob(child)
        for stream,name in [(child.stdout,'stdout'),(child.stderr,'stderr')]:
            thread=threading.Thread(target=drain,args=(stream,directory/(label+'.'+name)),daemon=True)
            threads.append(thread);thread.start()
        deadline=started+timeout
        while child.poll() is None:
            require(not overflow.is_set(),'command output limit')
            if time.monotonic()>=deadline:raise TimeoutError('command deadline')
            time.sleep(.01)
    finally:
        try:
            if child is not None:
                try:
                    if job is not None:job.close()
                    elif os.name!='nt':
                        try:os.killpg(child.pid,signal.SIGKILL)
                        except ProcessLookupError:pass
                finally:
                    if child.poll() is None:child.kill() # Includes suspended setup failure.
                    child.wait(timeout=5);record['exit']=child.returncode
            for thread in threads:thread.join(timeout=5)
            require(not any(t.is_alive() for t in threads),'command pipes remain open')
            record['cleanup_verified']=True
        finally:record['seconds']=time.monotonic()-started
    require(not overflow.is_set() and not failures,'command output limit/read failure')
    require(any_exit or record['exit']==0,'command failed: '+label)
    return record


def compiler_command(compiler, cpp, inputs, package, executable):
    return [str(compiler),'/nologo','/W4','/WX','/O2','/UNDEBUG','/MD','/utf-8','/showIncludes',
            '/TP' if cpp else '/TC','/std:c++17' if cpp else '/std:c11',
            '/I'+str(package),str(inputs/'windows_consumer.c'),'/Fo'+str(executable.with_suffix('.obj')),
            '/Fe'+str(executable),'/link','/INCREMENTAL:NO',str(package/'weekend_db.dll.lib'),'bcrypt.lib']


def cases():
    v1_begin=[(1,0,0,1),(1,1,0,0),(5,1,0,0),(5,0,0,0),(7,0,0,0),(7,1,0,0)]
    conflict=[(1,0,0,1),(1,1,0,0),(5,1,0,0),(5,0,0,0),(3,0,0,0),(5,0,0,0),(7,0,0,0),(7,1,0,0)]
    entries=[('full-v2',model.encode(),model.expected().encode()),
             ('legacy-v1',model.encode(model.TRACE_V1,version=1),model.expected(model.decode(model.encode(model.TRACE_V1,version=1))).encode()),
             ('legacy-begin',model.encode(v1_begin,version=1),b'0 1 0 1 [] H=- R=- X=-\n1 1 0 1 [] H=- R=- X=-\n2 5 0 2 [] H=- R=- X=2\n3 5 0 3 [] H=- R=- X=3\n4 7 0 3 [] H=- R=- X=-\n5 7 0 3 [] H=- R=- X=-\n'),
             ('whole-conflict',model.encode(conflict),b'0 1 0 1 [] H=- R=- X=-\n1 1 0 1 [] H=- R=- X=-\n2 5 0 2 [] H=- R=- X=2\n3 5 10 2 [] H=- R=- X=-\n4 3 11 2 [] H=- R=- X=-\n5 5 11 2 [] H=- R=- X=-\n6 7 0 2 [] H=- R=- X=-\n7 7 0 2 [] H=- R=- X=-\n'),
             ('whole-success',model.encode([(1,0,0,1),(5,0,0,0),(7,0,0,0)]),b'0 1 0 1 [] H=- R=- X=-\n1 5 0 2 [] H=- R=- X=2\n2 7 0 2 [] H=- R=- X=-\n'),
             ('invalid-mode',model.encode([(1,0,0,2)]),None),
             ('unknown-version',b'WDBTRC3\n'+struct.pack('<I',0),None)]
    require(hashlib.sha256(entries[0][1]).hexdigest()==TRACE_SHA256 and hashlib.sha256(entries[0][2]).hexdigest()==EXPECTED_SHA256,'qualified oracle changed')
    return entries


def clean_env(env):
    return {k:v for k,v in env.items() if k.upper() not in {'CL','_CL_','LINK','_LINK_','PYTHONPATH','PYTHONHOME','PYTHONOPTIMIZE'}}


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package',type=Path,required=True)
    parser.add_argument('--manifest-sha256',required=True,help='Independently recorded SHA-256 of package manifest.json')
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args(argv)
    # Admission precedes input loading or any child process, even on another OS.
    output=Path(os.path.abspath(args.output))
    require(not os.path.lexists(output),'output already exists')
    package=args.package.resolve()
    output=output.parent.resolve()/output.name
    require(not output.is_relative_to(package) and not package.is_relative_to(output),'output overlaps package')
    info,features=verify_package(package,args.manifest_sha256)
    require(sys.platform=='win32' and os.name=='nt','run on native x64 Windows with MSVC')
    output.parent.mkdir(parents=True,exist_ok=True);output.mkdir(mode=0o700)
    evidence={'verified':False,'native_windows_verified':False,'scope':'Delivered Windows x64 C/C++ finite state trace; not full API, MC/DC or power loss',
              'source':info['source'],'features':features,'manifest_sha256':args.manifest_sha256,'commands':[],
              'python':sys.version,'python_sha256':sha(Path(sys.executable).resolve()),'source_before':{str(p.relative_to(ROOT)):sha(p) for p in SOURCE_FILES}}
    try:
        evidence['platform']={'description':platform.platform(),'machine':platform.machine(),'windows_version':list(sys.getwindowsversion())}
        evidence['data_volume']=windows_volume(output)
        inputs=output/'inputs';inputs.mkdir()
        for p in SOURCE_FILES:shutil.copy2(p,inputs/p.name)
        relocated=output/'relocated-package';shutil.copytree(package,relocated)
        verify_package(relocated,args.manifest_sha256)
        # Discover VS in a bounded owned subprocess. Only build-relevant values
        # cross this boundary; inherited tokens/secrets are never written to logs.
        keep=['PATH','INCLUDE','LIB','LIBPATH','SYSTEMROOT','WINDIR','TEMP','TMP','COMSPEC','VCTOOLSVERSION','WINDOWSSDKVERSION','UCRTVERSION','VSCMD_ARG_TGT_ARCH']
        code='import sys,json;from pathlib import Path;sys.path.insert(0,sys.argv[1]);import windows_consumers;e=windows_consumers.compiler_environment(Path(sys.argv[2]));keep='+repr(keep)+';print(json.dumps({k:v for k,v in e.items() if k.upper() in keep}))'
        execute([sys.executable,'-I','-B','-c',code,TOOLS,output],output,'compiler-environment',evidence['commands'],env=clean_env(os.environ))
        selected=read_json(output/'compiler-environment.stdout');env=clean_env(selected)
        lower={k.lower():v for k,v in env.items()}
        require(lower.get('vscmd_arg_tgt_arch','').lower() in ('x64','amd64') and lower.get('vctoolsversion') and lower.get('windowssdkversion'),'identified x64 VS developer environment required')
        compiler=shutil.which('cl.exe',path=lower.get('path',''));require(compiler is not None,'MSVC compiler missing')
        evidence['msvc']={k:lower[k] for k in ['vctoolsversion','windowssdkversion','vscmd_arg_tgt_arch']}
        evidence['compiler_sha256']=sha(Path(compiler))
        runtime={k:v for k,v in env.items() if k.upper() in {'SYSTEMROOT','WINDIR','TEMP','TMP','COMSPEC'}}
        require('systemroot' in {k.lower() for k in runtime},'SystemRoot missing')
        root=next(v for k,v in runtime.items() if k.lower()=='systemroot')
        runtime['PATH']=str(Path(root)/'System32')+os.pathsep+root
        outcomes=[]
        for cpp,language in [(False,'c'),(True,'cpp')]:
            build=output/language;build.mkdir();exe=build/('consumer-'+language+'.exe')
            dll=build/'weekend_db.dll';shutil.copy2(relocated/'weekend_db.dll',dll)
            dll_hash=sha(dll)
            execute(compiler_command(compiler,cpp,inputs,relocated,exe),build,'compile',evidence['commands'],env=env)
            inputs_cases=cases()+[(name,model.encode(),None) for name in ['wrong-hash','wrong-path','wrong-source']]
            for name,data,expected in inputs_cases:
                run_dir=build/name;run_dir.mkdir();trace=run_dir/'trace.bin';trace.write_bytes(data)
                db=run_dir/'database.wdb';before=run_dir/'before.json';after=run_dir/'after.json'
                expected_dll=package/'weekend_db.dll' if name=='wrong-path' else dll
                digest=('0' if dll_hash[0]!='0' else '1')+dll_hash[1:] if name=='wrong-hash' else dll_hash
                source=('0' if info['source'][0]!='0' else '1')+info['source'][1:] if name=='wrong-source' else info['source']
                command=[exe,trace,db,source,expected_dll,digest,before,after]
                result=execute(command,run_dir,'run',evidence['commands'],env=runtime,any_exit=expected is None)
                if expected is not None:
                    require((run_dir/'run.stdout').read_bytes()==expected,'oracle transcript mismatch: '+language+'/'+name)
                    check_attestation(read_json(before),'before',str(dll),dll_hash,info['source'],features,result['pid'])
                    check_attestation(read_json(after),'after',str(dll),dll_hash,info['source'],features,result['pid'])
                    require(db.is_file(),'successful trace did not create database')
                    (run_dir/'expected.txt').write_bytes(expected)
                else:
                    require(result['exit']!=0 and not db.exists() and not after.exists(),'rejection control accepted/created database')
                    if name in ['invalid-mode','unknown-version']:
                        check_attestation(read_json(before),'before',str(dll),dll_hash,info['source'],features,result['pid'])
                    else:require(not before.exists(),'identity control reached trace')
                require(sha(dll)==dll_hash,'runtime DLL changed')
                outcomes.append({'language':language,'case':name,'exit':result['exit'],'verified':True,'stdout_sha256':sha(run_dir/'run.stdout')})
            evidence[language]={'executable_sha256':sha(exe),'dll_sha256':dll_hash,'profile':'/O2 /UNDEBUG /MD '+('/TP /std:c++17' if cpp else '/TC /std:c11')}
        verify_package(package,args.manifest_sha256);verify_package(relocated,args.manifest_sha256)
        require(evidence['source_before']=={str(p.relative_to(ROOT)):sha(p) for p in SOURCE_FILES},'trace tools changed')
        evidence.update(verified=True,native_windows_verified=True,trace_cells=len(model.TRACE),trace_sha256=TRACE_SHA256,expected_sha256=EXPECTED_SHA256,outcomes=outcomes)
    except BaseException as error:
        evidence['failure_type']=type(error).__name__;raise
    finally:
        evidence['artifact_sha256']=inventory(output)
        (output/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps({'verified':True,'output':str(output),'languages':['c','cpp'],'cells':134}))


if __name__=='__main__':main()
