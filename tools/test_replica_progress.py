import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('progress',Path(__file__).with_name('replica_progress.py'))
progress=importlib.util.module_from_spec(spec);spec.loader.exec_module(progress)
STREAM='11'*16

def sample():
    return {'schema':'weekend-replica-progress','version':1,'observed_at':'2026-11-13T00:00:00.000000Z','action':'poll','cli_sha256':'12'*32,
        'enrollment':{'stream':STREAM,'file_identity':'22'*16},'local':{'state':'verified','revision':4},
        'capture':{'state':'verified','revision':3,'digest':'33'*32},'follower':{'state':'durable','revision':3,'digest':'33'*32},
        'relation':'exact_capture','prior':{'state':'none','observed_at':None},'error':None}

class Contracts(unittest.TestCase):
    def test_relations_are_not_false_exact_acknowledgments(self):
        r=sample();progress.reconcile(r,None);self.assertEqual(r['relation'],'exact_capture')
        r=sample();r['follower']['revision']=4;progress.reconcile(r,None);self.assertEqual(r['relation'],'newer_than_capture')
        r=sample();r['follower']['revision']=2;progress.reconcile(r,None);self.assertEqual(r['relation'],'behind_capture')
        r=sample();r['follower']['digest']='44'*32
        with self.assertRaisesRegex(progress.Rejected,'digest_conflict'):progress.reconcile(r,None)
        r=sample();r['follower']['revision']=5
        with self.assertRaises(progress.Rejected):progress.reconcile(r,None)

    def test_previous_never_substitutes_for_unknown_receipt(self):
        prior=sample();r=sample();r['follower']={'state':'unknown','revision':None,'digest':None};r['relation']='unknown'
        progress.reconcile(r,prior);self.assertEqual(r['relation'],'unknown');self.assertEqual(r['prior']['state'],'historical_only')
        r=sample();r['follower']['revision']=2
        with self.assertRaisesRegex(progress.Rejected,'follower_regression'):progress.reconcile(r,prior)

    def test_strict_saved_schema_and_parser(self):
        progress.validate(sample())
        for changes in [{'version':True},{'extra':1},{'observed_at':'today'},{'relation':'newer_than_capture'}]:
            r=sample();r.update(changes)
            with self.assertRaises(progress.Rejected):progress.validate(r)
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp)/'prior';p.write_text('{"version":1,"version":2}')
            with self.assertRaises(progress.Rejected):progress.bounded_json(p)
        for line in ['follower durable_revision=01 stream='+STREAM+' digest='+'33'*32,'follower empty garbage']:
            with self.assertRaises(progress.Rejected):progress.receipt(line,STREAM)

    def test_oversized_revision_in_actual_observe_parse_is_sanitized(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary=Path(tmp)/'bin';binary.write_bytes(b'fixture')
            args=SimpleNamespace(binary=binary,ship=None,poll=True,stream=STREAM,source=Path(tmp)/'db',peer='127.0.0.1:1',credentials=Path(tmp)/'secret',pin=Path(tmp)/'pin',capture=None,previous=None,timeout=3,attempts=1)
            lines=['local verified_revision='+('1'*5000)+' stream='+STREAM+' file_identity='+('22'*16), 'weekend-replicate follower-init EMPTY_DIR '+STREAM+' '+('22'*16)]
            with patch.object(progress,'invoke',return_value=(0,lines)):
                result=progress.observe(args)
            self.assertEqual(result['error'],'parse_error')
            self.assertEqual(result['local'],{'state':'unknown','revision':None})
            self.assertIsNone(result['follower']['revision'])
            self.assertNotIn('1'*100,json.dumps(result))
            self.assertEqual(progress.revision(str(progress.U64)),progress.U64)
            with self.assertRaises(progress.Rejected):progress.revision(str(progress.U64+1))

    def test_deep_json_is_a_sanitized_invalid_prior(self):
        with tempfile.TemporaryDirectory() as tmp:
            prior=Path(tmp)/'prior';prior.write_text('['*1200+'0'+']'*1200)
            self.assertLess(prior.stat().st_size,16384)
            with self.assertRaisesRegex(progress.Rejected,'prior_invalid'):
                progress.bounded_json(prior)

    def test_selector_creation_failure_spawns_no_child(self):
        with patch.object(progress.selectors,'DefaultSelector',side_effect=OSError('fixture')),patch.object(progress.subprocess,'Popen') as spawn:
            with self.assertRaises(OSError):progress.invoke(Path(sys.executable),[],time.monotonic()+5)
            spawn.assert_not_called()

    def test_selector_registration_failure_kills_and_reaps_actual_child(self):
        actual=subprocess.Popen
        children=[]
        def spawn(*args,**kwargs):
            child=actual(*args,**kwargs);children.append(child);return child
        with patch.object(progress.selectors,'DefaultSelector') as factory,patch.object(progress.subprocess,'Popen',side_effect=spawn):
            factory.return_value.register.side_effect=OSError('fixture')
            with self.assertRaises(OSError):
                progress.invoke(Path(sys.executable),['-c','import time; time.sleep(30)'],time.monotonic()+5)
            factory.return_value.close.assert_called_once()
        self.assertEqual(len(children),1)
        self.assertIsNotNone(children[0].returncode)
        self.assertTrue(children[0].stdout.closed)
        with self.assertRaises(ChildProcessError):os.waitpid(children[0].pid,os.WNOHANG)

    def test_excess_output_rejected_before_sleep_deadline(self):
        with tempfile.TemporaryDirectory() as tmp:
            child=Path(tmp)/'child'
            child.write_text('#!'+sys.executable+'\nimport os,time\nos.write(1,b"x"*9000)\ntime.sleep(30)\n')
            child.chmod(0o700)
            started=time.monotonic()
            with self.assertRaisesRegex(progress.Rejected,'parse_error'):
                progress.invoke(child,[],started+10)
            self.assertLess(time.monotonic()-started,5)

    def test_observe_timeout_preserves_valid_prior_as_historical(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary=Path(tmp)/'bin';binary.write_bytes(b'fixture')
            prior=Path(tmp)/'prior';prior.write_text(json.dumps(sample()))
            args=SimpleNamespace(binary=binary,ship=None,poll=True,stream=STREAM,source=Path(tmp)/'db',peer='127.0.0.1:1',credentials=Path(tmp)/'secret',pin=Path(tmp)/'pin',capture=None,previous=prior,timeout=3,attempts=1)
            with patch.object(progress,'describe',return_value=(4,'22'*16)),patch.object(progress,'invoke',side_effect=progress.Rejected('timeout')):
                result=progress.observe(args)
            self.assertEqual(result['error'],'timeout')
            self.assertEqual(result['prior'],{'state':'historical_only','observed_at':sample()['observed_at']})
            self.assertEqual(result['follower'],{'state':'unavailable','revision':None,'digest':None})

    def test_existing_observation_rejected_before_replication(self):
        with tempfile.TemporaryDirectory() as tmp:
            saved=Path(tmp)/'saved';saved.write_text('keep')
            args=['progress','--binary','bin','--source','db','--stream',STREAM,'--peer','127.0.0.1:1','--credentials','private','--pin','pin','--poll','--save',str(saved)]
            with patch.object(sys,'argv',args),patch.object(progress,'observe') as observe:
                with self.assertRaises(SystemExit):progress.main()
                observe.assert_not_called()
            self.assertEqual(saved.read_text(),'keep')

    def test_lost_reply_keeps_verified_capture_but_no_remote_ack(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary=Path(tmp)/'bin';binary.write_bytes(b'trusted fixture')
            args=SimpleNamespace(binary=binary,ship=Path(tmp)/'export',poll=False,stream=STREAM,source=Path(tmp)/'db',peer='127.0.0.1:1',credentials=Path(tmp)/'SECRET',pin=Path(tmp)/'PRIVATE',capture=None,previous=None,timeout=3,attempts=1)
            with patch.object(progress,'describe',return_value=(3,'22'*16)),patch.object(progress,'capture',return_value=sample()['capture']),patch.object(progress,'invoke',return_value=(1,[f'local captured_revision=3 stream={STREAM} file_identity='+ '22'*16])):
                r=progress.observe(args)
            self.assertEqual(r['capture']['state'],'verified');self.assertEqual(r['follower']['state'],'unknown');self.assertEqual(r['error'],'cli_failure')
            self.assertNotIn('SECRET',json.dumps(r));self.assertNotIn('PRIVATE',json.dumps(r))

class DropResponses:
    def __init__(self,address):
        self.listener=socket.socket();self.listener.bind(('127.0.0.1',0));self.listener.listen(1);self.listener.settimeout(8)
        self.address=f'127.0.0.1:{self.listener.getsockname()[1]}';self.upstream=address;self.sockets=[];self.forwarded=0;self.discarded=0
        self.thread=threading.Thread(target=self.run,daemon=True);self.thread.start()
    def run(self):
        try:
            client,_=self.listener.accept();server=socket.create_connection(tuple(self.upstream.rsplit(':',1)[:1])+ (int(self.upstream.rsplit(':',1)[1]),),timeout=8)
            self.sockets=[client,server]
            def relay(a,b,up):
                try:
                    a.settimeout(8)
                    while True:
                        data=a.recv(65536)
                        if not data:break
                        if up:
                            b.sendall(data);self.forwarded+=len(data)
                        elif self.forwarded>8192:self.discarded+=len(data)
                        else:b.sendall(data)
                except OSError:pass
            reader=threading.Thread(target=relay,args=(server,client,False),daemon=True);reader.start();relay(client,server,True)
            for sock in self.sockets:
                try:sock.shutdown(socket.SHUT_RDWR)
                except OSError:pass
                sock.close()
            reader.join(9)
        except OSError:pass
    def close(self):
        self.listener.close()
        for sock in self.sockets:
            try:sock.shutdown(socket.SHUT_RDWR)
            except OSError:pass
            sock.close()
        self.thread.join(10)
        if self.thread.is_alive():raise RuntimeError('proxy did not stop')

@unittest.skipUnless(os.environ.get('REPLICA_PROGRESS_BIN') and os.environ.get('REPLICA_PROGRESS_FIXTURE'),'requires native fixture binaries')
class NativeTLS(unittest.TestCase):
    def test_authenticated_observations_restarts_dropped_reply_and_stale_capture(self):
        binary=Path(os.environ['REPLICA_PROGRESS_BIN']).resolve();fixture=Path(os.environ['REPLICA_PROGRESS_FIXTURE']).resolve()
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);db=root/'source.wdb';client=root/'client';server=root/'server';follower=root/'follower';follower.mkdir()
            def run(argv):return subprocess.check_output([str(a) for a in argv],stderr=subprocess.PIPE,text=True,timeout=45)
            run([fixture,'create',db]);run([binary,'keygen',client]);run([binary,'keygen',server])
            described=run([binary,'describe',db,STREAM]);identity=re.search('file_identity=([a-f0-9]+)',described)[1]
            run([binary,'follower-init',follower,STREAM,identity]);servers=[]
            def start():
                log=(root/f'server-{len(servers)}.log').open('w+')
                process=subprocess.Popen([str(binary),'serve',str(follower),STREAM,identity,'127.0.0.1:0',str(server),str(client/'cert.der'),'--max-exchanges','20'],stdout=log,stderr=subprocess.DEVNULL)
                servers.append((process,log));deadline=time.monotonic()+10
                while time.monotonic()<deadline:
                    log.seek(0);line=log.readline()
                    if line.startswith('ready '):return line.split()[1]
                    if process.poll() is not None:raise RuntimeError('server exited')
                    time.sleep(.02)
                raise RuntimeError('server startup timeout')
            def stop():
                process,log=servers[-1];process.kill();process.wait(timeout=10);log.close()
            def observe(address,extra,expected=0,timeout=10):
                args=[sys.executable,str(Path(progress.__file__)), '--binary',str(binary),'--source',str(db),'--stream',STREAM,'--peer',address,'--credentials',str(client),'--pin',str(server/'cert.der'),'--timeout',str(timeout),*map(str,extra)]
                done=subprocess.run(args,capture_output=True,text=True,timeout=timeout+15)
                self.assertEqual(done.returncode,expected,done.stderr);return json.loads(done.stdout)
            try:
                address=start();empty=observe(address,['--poll']);self.assertEqual(empty['follower']['state'],'empty')
                export=root/'first.export';proxy=DropResponses(address)
                try:uncertain=observe(proxy.address,['--ship',export],expected=1,timeout=3)
                finally:proxy.close()
                self.assertGreater(proxy.discarded,0);self.assertEqual(uncertain['follower']['state'],'unknown');self.assertIsNone(uncertain['follower']['revision'])
                stop();address=start()
                saved=root/'saved.json';confirmed=observe(address,['--poll','--capture',export,'--save',saved])
                self.assertEqual(confirmed['relation'],'exact_capture');self.assertEqual(confirmed['follower']['revision'],3)
                self.assertEqual(saved.stat().st_mode & 0o777,0o600)
                duplicate=observe(address,['--poll','--capture',export,'--previous',saved]);self.assertEqual(duplicate['prior']['state'],'consistent')
                run([fixture,'advance',db]);newexport=root/'second.export';new=observe(address,['--ship',newexport,'--previous',saved]);self.assertEqual(new['follower']['revision'],4);self.assertEqual(new['prior']['state'],'superseded')
                newer=observe(address,['--poll','--capture',export,'--previous',saved]);self.assertEqual(newer['relation'],'newer_than_capture');self.assertNotEqual(newer['follower']['digest'],newer['capture']['digest'])
                changed=root/'same-revision-different.export';shutil.copyfile(newexport,changed)
                with changed.open('ab') as f:f.write(bytes(4096))
                conflict=observe(address,['--poll','--capture',changed],expected=1);self.assertEqual(conflict['error'],'digest_conflict')
                foreign=copy.deepcopy(confirmed);foreign['enrollment']['file_identity']='ff'*16;other=root/'foreign.json';other.write_text(json.dumps(foreign))
                rejected=observe(address,['--poll','--previous',other],expected=1);self.assertEqual(rejected['error'],'identity_mismatch')
                stop()
                unavailable=observe(address,['--poll','--previous',saved],expected=1)
                self.assertEqual(unavailable['prior']['state'],'historical_only')
                self.assertEqual(unavailable['prior']['observed_at'],confirmed['observed_at'])
                self.assertEqual(unavailable['follower']['state'],'unavailable')
                self.assertIsNone(unavailable['follower']['revision'])
                status=run([binary,'status',follower,STREAM,identity]);self.assertIn('durable_revision=4',status)
                slot=re.search(r'selected_slot=Some\((\d)\)',status)[1]
                run([fixture,'check',follower/f'replica-{slot}.wdb',4]);run([fixture,'check',db,4])
            finally:
                for process,log in servers:
                    if process.poll() is None:process.kill();process.wait(timeout=10)
                    if not log.closed:log.close()

if __name__=='__main__':unittest.main()
