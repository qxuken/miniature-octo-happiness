#!/usr/bin/env python3
"""Accelerated 30-day two-peer native-file qualification; no calendar waiting."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import socket
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent


def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--directory', type=Path, required=True)
    p.add_argument('--binary', type=Path, default=ROOT / 'tools/monthly-workload/target/release/monthly-workload')
    p.add_argument('--engine-source', default='working-tree', help='Declared engine source used for both binary builds')
    p.add_argument('--server-command', help='JSON argv prefix including remote binary, e.g. docker exec ... /binary')
    p.add_argument('--server-db', help='Existing remote parent directory plus new database filename')
    p.add_argument('--server-listen')
    p.add_argument('--server-address')
    p.add_argument('--client-listen')
    p.add_argument('--client-address')
    args = p.parse_args()
    directory = args.directory.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    commands = [json.loads(args.server_command) if args.server_command else [str(args.binary.resolve())], [str(args.binary.resolve())]]
    paths = [args.server_db or str(directory / 'server.wdb'), str(directory / 'client.wdb')]
    ports = [port(), port()]
    listens = [args.server_listen or f'127.0.0.1:{ports[0]}', args.client_listen or f'127.0.0.1:{ports[1]}']
    addresses = [args.server_address or listens[0], args.client_address or listens[1]]
    rows = []
    start = time.monotonic()
    initial_disk = shutil.disk_usage(directory)
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    def argv(role, phase, day, extra=()):
        return commands[role] + [phase, paths[role], str(role), str(day), *extra]
    def record(output, wall):
        result = json.loads(output.strip().splitlines()[-1])
        if result['phase'] == 'day':
            assert 'transaction_us' in result, 'stale driver binary'
        if result['phase'] == 'backup':
            assert 'backup_us' in result and 'restore_us' in result, 'stale driver binary'
        result['process_wall_ms'] = round(wall * 1000, 3)
        rows.append(result)
        with (directory / 'events.jsonl').open('a') as log:
            log.write(json.dumps(result) + '\n')
        return result
    def run(role, phase, day, extra=(), interrupt=False):
        begin = time.monotonic()
        result = subprocess.run(argv(role, phase, day, extra), capture_output=True, text=True, timeout=60)
        if interrupt:
            assert result.returncode == 70, (result.stdout, result.stderr)
            rows.append({'phase': 'interrupted-after-receipt', 'role': role, 'day': day,
                         'process_wall_ms': round((time.monotonic()-begin)*1000,3)})
            return None
        assert result.returncode == 0, (phase, role, result.stdout, result.stderr)
        return record(result.stdout, time.monotonic() - begin)
    def sync(sender, day):
        receiver = 1-sender
        begin = time.monotonic()
        server = subprocess.Popen(argv(receiver, 'serve', day, [listens[receiver]]), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        # A bounded readiness loop avoids hanging on a crashed or inaccessible server.
        import selectors
        selector = selectors.DefaultSelector()
        selector.register(server.stdout, selectors.EVENT_READ)
        try:
            assert selector.select(30), 'server readiness timeout'
            assert server.stdout.readline().strip() == 'READY', 'server failed to bind'
            result = run(sender, 'sync', day, [addresses[receiver]])
            output, error = server.communicate(timeout=30)
            assert server.returncode == 0, error
            peer = record(output, time.monotonic()-begin)
            return result, peer
        finally:
            selector.close()
            if server.poll() is None:
                server.kill()
                server.communicate()
    for role in [0,1]:
        run(role, 'init', 0)
    for day in range(1,31):
        for role in [0,1]:
            if day % 7 == 0:
                run(role, 'day', day, ['interrupt'], interrupt=True)
            run(role, 'day', day)
        sync(0, day)
        sync(1, day)
        for sender in [0,1]:
            replies = sync(sender, day)
            assert all(reply['commits'] == 0 for reply in replies), 'repeat sync committed'
        for role in [0,1]:
            run(role, 'checkpoint', day)
            run(role, 'verify', day)
        print(f'day {day}/30 verified', flush=True)
    for role in [0,1]:
        run(role, 'backup', 30)
    keys = sorted({key for row in rows for key in row})
    with (directory / 'events.csv').open('w', newline='') as f:
        writer = csv.DictWriter(f, fieldnames=keys)
        writer.writeheader()
        writer.writerows(rows)
    def distribution(values):
        values = sorted(values)
        return {'count':len(values), 'min':min(values), 'median':values[len(values)//2],
                'p95':values[min(len(values)-1,int(len(values)*.95))], 'max':max(values)} if values else None
    def latency(phase, key):
        return distribution(row[key] for row in rows if row['phase'] == phase and key in row)
    summary = {
        'source':source, 'declared_engine_source':args.engine_source,
        'working_tree_dirty':bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()),
        'source_bindings':{name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in ['tools/monthly_workload.rs','tools/monthly_workload.py','tools/monthly-workload/Cargo.toml','tools/monthly-workload/Cargo.lock','examples/support/study.rs']},
        'local_toolchain':subprocess.check_output(['rustc','-Vv'],text=True).strip(), 'calendar_days':30, 'wall_seconds':time.monotonic()-start,
        'kind':'accelerated deterministic calendar, not 30-day wall-clock soak',
        'commands':commands, 'paths':paths, 'addresses':addresses,
        'local_binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        'scheduled':{'task_edits':480,'conflicts':60,'retries':60,'offline_crdt_updates':60,
                     'daily_checkpoints':60,'interrupted_processes':8,'sync_sessions':120},
        'final_oracle':{'counter':1830,'completed_ids':list(range(60)), 'weekly_goal':120,
                        'session_limit':20,'task_score_formula':'3000 + 10*role + id, id=1..8',
                        'retained_roots_per_node':30},
        'latency_us':{phase:latency(phase,'elapsed_us') for phase in ['day','sync','serve','checkpoint','verify','backup']},
        'core_memory_by_phase':[{'role':row['role'],'day':row['day'],'phase':row['phase'],**row['memory_usage']} for row in rows if row.get('memory_usage') is not None],
        'memory_scope':'requested core heap layouts per process domain; excludes backend, typed/app output buffers and RSS overhead',
        'reopen_us':latency('verify','reopen_us'),
        'successful_transaction_us':distribution(value for row in rows for value in row.get('transaction_us',[])),
        'backup_us':latency('backup','backup_us'),'restore_us':latency('backup','restore_us'),
        'file_bytes_by_day':[[row['role'],row['day'],row['file_bytes']] for row in rows if row['phase']=='verify'],
        'local_disk_free_before':initial_disk.free, 'local_disk_free_after':shutil.disk_usage(directory).free,
        'child_max_rss_bytes':resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * (1 if sys.platform=='darwin' else 1024),
        'rss_scope':'local child-process high-water mark, excludes remote/container descendants; not an allocation cap',
        'ack_scope':'CRDT request/reply only; passive follower durable acknowledgment not exercised',
        'passed':True,
    }
    (directory / 'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(directory / 'summary.json')


if __name__ == '__main__':
    main()
