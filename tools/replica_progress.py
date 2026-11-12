#!/usr/bin/env python3
"""Private versioned observations around the existing weekend-replicate text CLI."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import selectors
import stat
import subprocess
import time

HEX16 = r'[0-9a-f]{32}'
HEX32 = r'[0-9a-f]{64}'
U64 = (1 << 64) - 1
TOP = {'schema','version','observed_at','action','cli_sha256','enrollment','local','capture','follower','relation','prior','error'}
ERRORS = {None,'timeout','cli_failure','parse_error','identity_mismatch','capture_invalid','local_rollback','follower_regression','digest_conflict','prior_invalid'}

class Rejected(Exception):
    pass

def timestamp():
    return datetime.now(timezone.utc).isoformat(timespec='microseconds').replace('+00:00','Z')

def revision(text):
    if len(text)>20 or not re.fullmatch(r'0|[1-9][0-9]*', text) or int(text) > U64:
        raise Rejected('parse_error')
    return int(text)

def validate(record):
    if not isinstance(record,dict) or set(record)!=TOP or record['schema']!='weekend-replica-progress' or type(record['version']) is not int or record['version']!=1:
        raise Rejected('prior_invalid')
    if record['action'] not in ['ship','poll'] or record['error'] not in ERRORS or (record['cli_sha256'] is not None and not re.fullmatch(HEX32,str(record['cli_sha256']))):
        raise Rejected('prior_invalid')
    if not isinstance(record['observed_at'],str) or not re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{6}Z',record['observed_at']):
        raise Rejected('prior_invalid')
    try: datetime.fromisoformat(record['observed_at'].replace('Z','+00:00'))
    except ValueError: raise Rejected('prior_invalid') from None
    e=record['enrollment']
    if not isinstance(e,dict) or set(e)!={'stream','file_identity'} or not re.fullmatch(HEX16,str(e['stream'])) or (e['file_identity'] is not None and not re.fullmatch(HEX16,str(e['file_identity']))):
        raise Rejected('prior_invalid')
    for name,states in [('local',{'verified','unknown'}),('capture',{'verified','none','unknown'}),('follower',{'durable','empty','unknown','unavailable','error'})]:
        entry=record[name]; keys={'state','revision'} | (set() if name=='local' else {'digest'})
        if not isinstance(entry,dict) or set(entry)!=keys or entry['state'] not in states:
            raise Rejected('prior_invalid')
        present=entry['state'] in ['verified','durable']
        if present:
            if type(entry['revision']) is not int or not 0<=entry['revision']<=U64: raise Rejected('prior_invalid')
            if name!='local' and not re.fullmatch(HEX32,str(entry['digest'])): raise Rejected('prior_invalid')
        elif entry['revision'] is not None or (name!='local' and entry['digest'] is not None): raise Rejected('prior_invalid')
    if record['relation'] not in ['exact_capture','newer_than_capture','behind_capture','digest_conflict','no_capture','unknown']: raise Rejected('prior_invalid')
    prior=record['prior']
    if not isinstance(prior,dict) or set(prior)!={'state','observed_at'} or prior['state'] not in ['none','consistent','superseded','historical_only','rejected']: raise Rejected('prior_invalid')
    if prior['observed_at'] is not None and not isinstance(prior['observed_at'],str): raise Rejected('prior_invalid')
    local,cap,ack=record['local'],record['capture'],record['follower']
    if any(x['state'] in ['verified','durable'] for x in [local,cap,ack]) and e['file_identity'] is None:
        raise Rejected('prior_invalid')
    expected='unknown'
    if ack['state']=='durable':
        if cap['state']!='verified': expected='no_capture'
        elif ack['revision']==cap['revision']: expected='exact_capture' if ack['digest']==cap['digest'] else 'digest_conflict'
        else: expected='newer_than_capture' if ack['revision']>cap['revision'] else 'behind_capture'
    if record['relation']!=expected and not (record['error'] is not None and record['relation']=='unknown'):
        raise Rejected('prior_invalid')
    if record['error'] is None:
        if record['cli_sha256'] is None or e['file_identity'] is None or local['state']!='verified' or ack['state'] not in ['durable','empty'] or expected=='digest_conflict':
            raise Rejected('prior_invalid')
        if (cap['state']=='verified' and cap['revision']>local['revision']) or (ack['state']=='durable' and ack['revision']>local['revision']):
            raise Rejected('prior_invalid')
    return record

def unique_object(pairs):
    result={}
    for key,value in pairs:
        if key in result: raise Rejected('prior_invalid')
        result[key]=value
    return result

def bounded_json(path):
    if not stat.S_ISREG(path.stat().st_mode) or path.stat().st_size>16384: raise Rejected('prior_invalid')
    try:
        with path.open('rb') as f: data=f.read(16385)
        if len(data)>16384: raise Rejected('prior_invalid')
        return validate(json.loads(data,object_pairs_hook=unique_object))
    except (ValueError,TypeError,KeyError,RecursionError): raise Rejected('prior_invalid') from None

def invoke(binary,args,deadline):
    remaining=deadline-time.monotonic()
    if remaining<=0: raise Rejected('timeout')
    output=bytearray()
    selector=selectors.DefaultSelector()
    process=None
    try:
        process=subprocess.Popen([str(binary),*map(str,args)],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,start_new_session=True)
        selector.register(process.stdout,selectors.EVENT_READ)
        while True:
            remaining=deadline-time.monotonic()
            if remaining<=0: raise Rejected('timeout')
            if not selector.select(remaining): raise Rejected('timeout')
            chunk=os.read(process.stdout.fileno(),8193-len(output))
            if not chunk: break
            output.extend(chunk)
            if len(output)>8192: raise Rejected('parse_error')
        remaining=deadline-time.monotonic()
        if remaining<=0: raise Rejected('timeout')
        try: process.wait(timeout=remaining)
        except subprocess.TimeoutExpired: raise Rejected('timeout') from None
    except BaseException:
        if process is not None:
            try: os.killpg(process.pid,signal.SIGKILL)
            except ProcessLookupError: pass
            process.wait(timeout=10)
        raise
    finally:
        try: selector.close()
        finally:
            if process is not None: process.stdout.close()
    try: text=output.decode('ascii')
    except UnicodeDecodeError: raise Rejected('parse_error') from None
    return process.returncode,text.splitlines()

def describe(binary,path,stream,deadline):
    status,lines=invoke(binary,['describe',path,stream],deadline)
    if status: raise Rejected('cli_failure')
    if len(lines)!=2: raise Rejected('parse_error')
    m=re.fullmatch(r'local verified_revision=(\d+) stream=('+HEX16+r') file_identity=('+HEX16+r')',lines[0])
    if not m or m[2]!=stream or lines[1]!=f'weekend-replicate follower-init EMPTY_DIR {stream} {m[3]}': raise Rejected('parse_error')
    return revision(m[1]),m[3]

def capture(binary,path,stream,file,deadline):
    rev,identity=describe(binary,path,stream,deadline)
    if identity!=file: raise Rejected('identity_mismatch')
    before=path.stat()
    if not stat.S_ISREG(before.st_mode) or before.st_size>64*1024*1024: raise Rejected('capture_invalid')
    digest=hashlib.sha256()
    with path.open('rb') as f:
        opened=os.fstat(f.fileno())
        if (opened.st_dev,opened.st_ino,opened.st_size)!=(before.st_dev,before.st_ino,before.st_size): raise Rejected('capture_invalid')
        for chunk in iter(lambda:f.read(65536),b''):
            if time.monotonic()>deadline: raise Rejected('timeout')
            digest.update(chunk)
        after=os.fstat(f.fileno())
    if (before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns)!=(after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns): raise Rejected('capture_invalid')
    return {'state':'verified','revision':rev,'digest':digest.hexdigest()}

def receipt(line,stream):
    if line=='follower empty': return {'state':'empty','revision':None,'digest':None}
    m=re.fullmatch(r'follower durable_revision=(\d+) stream=('+HEX16+r') digest=('+HEX32+r')',line)
    if not m: raise Rejected('parse_error')
    if m[2]!=stream: raise Rejected('identity_mismatch')
    return {'state':'durable','revision':revision(m[1]),'digest':m[3]}

def reject_prior(record,code):
    record['prior']['state']='rejected'
    raise Rejected(code)

def reconcile(record,prior):
    local,cap,ack=record['local'],record['capture'],record['follower']
    if cap['state']=='verified' and cap['revision']>local['revision']: raise Rejected('local_rollback')
    if ack['state']=='durable':
        if ack['revision']>local['revision']: raise Rejected('follower_regression')
        if cap['state']=='verified':
            if ack['revision']==cap['revision']:
                record['relation']='exact_capture' if ack['digest']==cap['digest'] else 'digest_conflict'
                if record['relation']=='digest_conflict': raise Rejected('digest_conflict')
            else: record['relation']='newer_than_capture' if ack['revision']>cap['revision'] else 'behind_capture'
        else: record['relation']='no_capture'
    if prior:
        record['prior']={'state':'historical_only','observed_at':prior['observed_at']}
        if prior['enrollment']!=record['enrollment']: reject_prior(record,'identity_mismatch')
        old=prior['follower']
        if prior['local']['state']=='verified' and local['revision']<prior['local']['revision']: reject_prior(record,'local_rollback')
        if old['state']=='durable' and ack['state'] in ['durable','empty']:
            if ack['state']=='empty' or ack['revision']<old['revision']: reject_prior(record,'follower_regression')
            if ack['revision']==old['revision'] and ack['digest']!=old['digest']: reject_prior(record,'digest_conflict')
            record['prior']['state']='superseded' if ack['revision']>old['revision'] else 'consistent'

def observe(args):
    action='ship' if args.ship else 'poll'
    record={'schema':'weekend-replica-progress','version':1,'observed_at':timestamp(),'action':action,
        'cli_sha256':None,
        'enrollment':{'stream':args.stream,'file_identity':None},'local':{'state':'unknown','revision':None},
        'capture':{'state':'unknown' if args.ship else 'none','revision':None,'digest':None},
        'follower':{'state':'unknown','revision':None,'digest':None},'relation':'unknown',
        'prior':{'state':'none','observed_at':None},'error':None}
    deadline=time.monotonic()+args.timeout
    try:
        if not stat.S_ISREG(args.binary.stat().st_mode) or args.binary.stat().st_size>256*1024*1024: raise Rejected('cli_failure')
        record['cli_sha256']=hashlib.sha256(args.binary.read_bytes()).hexdigest()
        prior=None
        if args.previous:
            try: prior=bounded_json(args.previous)
            except (Rejected,OSError):
                record['prior']['state']='rejected'
                raise Rejected('prior_invalid') from None
            record['prior']={'state':'historical_only','observed_at':prior['observed_at']}
        rev,file=describe(args.binary,args.source,args.stream,deadline)
        record['enrollment']['file_identity']=file;record['local']={'state':'verified','revision':rev}
        if prior and prior['enrollment']!=record['enrollment']: reject_prior(record,'identity_mismatch')
        if args.capture: record['capture']=capture(args.binary,args.capture,args.stream,file,deadline)
        call=['ship',args.source,args.stream,args.ship,args.peer,args.credentials,args.pin,'--attempts',str(args.attempts)] if args.ship else ['poll',args.source,args.stream,args.peer,args.credentials,args.pin]
        status,lines=invoke(args.binary,call,deadline)
        if args.ship:
            if lines:
                m=re.fullmatch(r'local captured_revision=(\d+) stream=('+HEX16+r') file_identity=('+HEX16+r')',lines[0])
                if not m: raise Rejected('parse_error')
                if m[2]!=args.stream or m[3]!=file: raise Rejected('identity_mismatch')
                record['capture']=capture(args.binary,args.ship,args.stream,file,deadline)
                if record['capture']['revision']!=revision(m[1]): raise Rejected('capture_invalid')
                record['local']['revision']=revision(m[1])
        elif lines:
            m=re.fullmatch(r'local verified_revision=(\d+)',lines[0])
            if not m: raise Rejected('parse_error')
            record['local']['revision']=revision(m[1])
        if status: raise Rejected('cli_failure')
        if len(lines)!=2: raise Rejected('parse_error')
        record['follower']=receipt(lines[1],args.stream)
        # Detect ordinary replaced source paths between separately leased CLI calls.
        latest,identity=describe(args.binary,args.source,args.stream,deadline)
        if identity!=file or latest<record['local']['revision']: raise Rejected('identity_mismatch')
        reconcile(record,prior)
    except (Rejected,OSError) as error:
        record['error']=str(error) if isinstance(error,Rejected) else 'cli_failure'
        if record['follower']['state']!='durable': record['follower']['state']='unknown' if args.ship else 'unavailable'
    record['observed_at']=timestamp()
    return validate(record)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=Path,required=True);p.add_argument('--source',type=Path,required=True)
    p.add_argument('--stream',required=True);p.add_argument('--peer',required=True)
    p.add_argument('--credentials',type=Path,required=True);p.add_argument('--pin',type=Path,required=True)
    actions=p.add_mutually_exclusive_group(required=True);actions.add_argument('--ship',type=Path);actions.add_argument('--poll',action='store_true')
    p.add_argument('--capture',type=Path);p.add_argument('--previous',type=Path);p.add_argument('--save',type=Path)
    p.add_argument('--attempts',type=int,default=1);p.add_argument('--timeout',type=float,default=110)
    args=p.parse_args()
    if os.name!='posix':p.error('POSIX subprocess qualification only')
    if not re.fullmatch(HEX16,args.stream) or int(args.stream,16)==0:p.error('nonzero stream requires 32 lowercase hex digits')
    if not 1<=args.attempts<=3 or not 1<=args.timeout<=600:p.error('attempts 1..3 and timeout 1..600 required')
    if args.ship and args.capture:p.error('--capture belongs to poll')
    if args.save and (args.save.exists() or args.save.is_symlink() or not args.save.parent.is_dir()):p.error('save requires a fresh destination')
    result=observe(args); encoded=json.dumps(result,sort_keys=True,separators=(',',':'))+'\n'
    if args.save:
        fd=os.open(args.save,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
        with os.fdopen(fd,'w') as f:f.write(encoded);f.flush();os.fsync(f.fileno())
    print(encoded,end='')
    return int(result['error'] is not None)
if __name__=='__main__':raise SystemExit(main())
