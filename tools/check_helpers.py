#!/usr/bin/env python3
"""POSIX Python helper controls, isolated per suite/profile; no native qualification."""
import argparse
import hashlib
import importlib.util
import json
import os
import resource
import signal
from pathlib import Path
import subprocess
import sys
import time
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import compatibility as helpers

OPTIONAL_ENV = ('REPLICA_PROGRESS_BIN', 'REPLICA_PROGRESS_FIXTURE', 'RESOURCE_ENVELOPE_EVIDENCE')
PRUNE = {'target', '__pycache__', '.git'}

def require(condition, message):
    if not condition:
        raise ValueError(message)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inventory(root):
    files = {}
    for parent, directories, names in os.walk(root, followlinks=False):
        directories[:] = sorted(d for d in directories if d not in PRUNE)
        for name in directories + sorted(names):
            path = Path(parent) / name
            require(not path.is_symlink(), 'symlink under helper source: ' + str(path))
        for name in sorted(names):
            path = Path(parent) / name
            if path.is_file():
                files[str(path.relative_to(root))] = digest(path)
    return files

# These are trusted Python controls, not an adversarial process sandbox. Direct
# Popen children are journaled without changing session/stdio flags. A snapshot
# also covers the brief spawn/journal window while the worker is still alive.
RAW_POPEN = subprocess.Popen
MAX_FILE_BYTES = 8 * 1024 * 1024
EXTERNAL_INPUTS = (
    'include/weekend_db.h', 'bindings/odin/weekend_db.odin',
    'src/storage/codec.rs', 'src/storage/mod.rs',
    'apps/study-peer/tests/credential_rotation.rs',
)

def external_inputs(root):
    result = {}
    for name in EXTERNAL_INPUTS:
        path = root / name
        require(path.is_file() and not any(p.is_symlink() for p in [path, *path.parents]),
                'missing/symlink external input: ' + name)
        result[name] = digest(path)
    return result

def bounded_json(path, maximum=MAX_FILE_BYTES):
    require(path.stat().st_size <= maximum, 'worker JSON exceeds limit')
    with path.open('rb') as file:
        data = file.read(maximum + 1)
    require(len(data) <= maximum, 'worker JSON exceeds limit')
    return json.loads(data)

def process_table():
    # Use the captured implementation to avoid recursively journaling this probe
    # or inheriting suite mocks of subprocess.run/Popen. ps output is trusted
    # system process metadata, bounded by the host process table; communicate
    # has a two-second deadline and qualification rejects output over 8 MiB.
    child = RAW_POPEN(['/bin/ps', '-axo', 'pid=,ppid=,pgid=,lstart=,stat='],
                      stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                      env={**os.environ, 'LC_ALL': 'C'})
    try:
        stdout, stderr = child.communicate(timeout=2)
    except BaseException:
        child.kill(); child.wait(timeout=2)
        raise
    require(child.returncode == 0 and len(stdout) <= MAX_FILE_BYTES,
            'cannot inspect process ownership')
    result = {}
    for line in stdout.decode('ascii').splitlines():
        parts = line.split()
        require(len(parts) == 9, 'unrecognized process identity')
        pid, parent, group = map(int, parts[:3])
        result[pid] = {'pid': pid, 'parent': parent, 'group': group,
                       'start': ' '.join(parts[3:8]), 'state': parts[8]}
    return result

def journal_popen(journal):
    def spawn(*args, **kwargs):
        child = RAW_POPEN(*args, **kwargs)
        try:
            identity = process_table().get(child.pid)
            require(identity is not None, 'spawned child identity unavailable')
            # One synchronous append, before returning the Popen handle to tests.
            with journal.open('a') as file:
                file.write(json.dumps(identity) + '\n')
                file.flush()
        except BaseException:
            child.kill(); child.wait(timeout=5)
            raise
        return child
    return spawn

def cancel_worker(signum, frame):
    raise KeyboardInterrupt('helper watchdog cancellation')

def _cleanup_worker(child, journal):
    owned = {}
    table = process_table()
    frontier = {child.pid}
    seen = set(frontier)
    while frontier:
        descendants = {pid for pid, row in table.items() if row['parent'] in frontier} - seen
        for pid in descendants:
            owned[pid] = table[pid]
        seen.update(descendants)
        frontier = descendants
    journal_errors = []
    def read_journal():
        try:
            if not journal.exists():
                return
            if journal.stat().st_size > MAX_FILE_BYTES:
                journal_errors.append('oversized journal')
            with journal.open('rb') as file:
                data = file.read(MAX_FILE_BYTES)
            for line in data.splitlines():
                try:
                    row = json.loads(line)
                    require(set(row) == {'pid', 'parent', 'group', 'start', 'state'} and
                            type(row['pid']) is int and isinstance(row['start'], str), 'invalid process journal')
                    owned[row['pid']] = row
                except (ValueError, TypeError):
                    journal_errors.append('malformed journal entry')
        except OSError:
            journal_errors.append('unreadable journal')
    read_journal()
    # Signal the worker first to permit its own finally blocks to run.
    if child.poll() is None:
        child.send_signal(signal.SIGTERM)
        try:
            child.wait(timeout=.3)
        except subprocess.TimeoutExpired:
            pass
    helpers.stop(child)
    read_journal()
    deadline = time.monotonic() + 4
    while True:
        table = process_table()
        # An explicitly created session owns its group even after its leader
        # exits. Existing group members reserve that PGID against reuse.
        groups = [row for row in owned.values() if row['pid'] == row['group']]
        for leader in groups:
            current = table.get(leader['pid'])
            if current is not None and current['start'] != leader['start']:
                continue  # recycled PID now identifies a different session
            for pid, row in table.items():
                if row['group'] == leader['group']:
                    owned[pid] = row
        live = [row for pid, row in table.items() if pid in owned and
                row['start'] == owned[pid]['start'] and not row['state'].startswith('Z')]
        if not live:
            require(not journal_errors, 'unverifiable journal after best-effort cleanup')
            return {'verified': True, 'owned_descendants': list(owned.values()),
                    'identity': 'PID plus ps lstart; bounded trusted-suite process lifetime'}
        for row in live:
            try:
                os.kill(row['pid'], signal.SIGKILL)
            except ProcessLookupError:
                pass
        require(time.monotonic() < deadline, 'owned descendant remains running')
        time.sleep(.02)

def cleanup_worker(child, journal):
    try:
        return _cleanup_worker(child, journal)
    finally:
        # Inspection/journal errors must never bypass direct worker cleanup.
        helpers.stop(child)

def registry(root, path):
    data = json.loads(path.read_text())
    require(set(data) == {'version', 'suites'} and type(data['version']) is int and data['version'] == 1,
            'invalid registry schema')
    rows = data['suites']
    require(isinstance(rows, list) and rows, 'empty registry')
    paths = []
    for row in rows:
        require(set(row) == {'path', 'allowed_skips'}, 'invalid suite row')
        name = row['path']
        require(isinstance(name, str) and not Path(name).is_absolute() and '..' not in Path(name).parts,
                'invalid suite path')
        require(isinstance(row['allowed_skips'], list) and
                all(isinstance(s, str) for s in row['allowed_skips']) and
                len(set(row['allowed_skips'])) == len(row['allowed_skips']), 'invalid skip registry')
        paths.append(name)
    require(len(paths) == len(set(paths)), 'duplicate suite')
    source = inventory(root)
    discovered = {p for p in source if Path(p).name.startswith('test') and p.endswith('.py')}
    require(set(paths) == discovered, 'registry must exactly cover discovered test*.py suites')
    return rows, source

def test_ids(suite):
    for entry in suite:
        if isinstance(entry, unittest.TestSuite):
            yield from test_ids(entry)
        else:
            yield entry.id()

class Results(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.executed = []

    def startTest(self, test):
        self.executed.append(test.id())
        super().startTest(test)

def worker(suite_path, output):
    journal_name = os.environ.get("HELPER_PROCESS_JOURNAL")
    previous_popen = subprocess.Popen
    previous_signal = signal.signal(signal.SIGTERM, cancel_worker)
    if journal_name:
        subprocess.Popen = journal_popen(Path(journal_name))
    record = {'complete': False, 'suite': str(suite_path), 'optimize': sys.flags.optimize}
    try:
        require(suite_path.is_file() and not suite_path.is_symlink(), 'invalid worker suite')
        sys.path.insert(0, str(suite_path.parent))
        spec = importlib.util.spec_from_file_location('helper_suite', suite_path)
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        suite = unittest.defaultTestLoader.loadTestsFromModule(module)
        discovered = list(test_ids(suite))
        result = unittest.TextTestRunner(stream=sys.stderr, verbosity=2, resultclass=Results).run(suite)
        record.update(complete=True, discovered=discovered, executed=result.executed,
                      tests_run=result.testsRun,
                      failures=[{'id': t.id(), 'traceback': message} for t, message in result.failures],
                      errors=[{'id': t.id(), 'traceback': message} for t, message in result.errors],
                      skips=[{'id': t.id(), 'reason': reason} for t, reason in result.skipped],
                      expected_failures=[t.id() for t, _ in result.expectedFailures],
                      unexpected_successes=[t.id() for t in result.unexpectedSuccesses])
        return int(not result.wasSuccessful())
    except BaseException as error:
        record['failure_type'] = type(error).__name__
        raise
    finally:
        subprocess.Popen = previous_popen
        signal.signal(signal.SIGTERM, previous_signal)
        with output.open('x') as file:
            json.dump(record, file, indent=2)
            file.write('\n')

def validate(record, row, suite_path, optimize):
    require(record.get('complete') is True and record.get('suite') == str(suite_path) and
            type(record.get('optimize')) is int and record['optimize'] == optimize, 'incomplete/wrong worker identity')
    ids = record.get('discovered')
    require(isinstance(ids, list) and ids and all(isinstance(s, str) for s in ids) and len(ids) == len(set(ids)),
            'zero/invalid/duplicate discovered tests')
    require(record.get('executed') == ids and type(record.get('tests_run')) is int and
            record['tests_run'] == len(ids), 'test execution incomplete')
    for field in ['failures', 'errors', 'expected_failures', 'unexpected_successes']:
        require(record.get(field) == [], 'test failures or expected-failure masking: ' + field)
    skips = record.get('skips')
    require(isinstance(skips, list) and all(isinstance(s, dict) and set(s) == {'id', 'reason'} and
            isinstance(s['reason'], str) for s in skips), 'invalid skip evidence')
    skip_ids = [s['id'] for s in skips]
    require(len(skip_ids) == len(set(skip_ids)) and set(skip_ids) == set(row['allowed_skips']) and
            set(skip_ids) <= set(ids), 'unexpected/missing registered skips')
    require(len(ids) > len(skip_ids), 'all tests skipped')
    return {'tests': len(ids), 'passed': len(ids) - len(skip_ids), 'skipped': len(skip_ids)}

def execute(command, directory, evidence, *, env=None, timeout=90, file_limit=MAX_FILE_BYTES):
    record = {'command': list(map(str, command)), 'timeout': timeout, 'exit': None}
    evidence.append(record)
    journal = directory / 'processes.jsonl'
    child_env = dict(os.environ if env is None else env, HELPER_PROCESS_JOURNAL=str(journal))
    def limit_files():
        resource.setrlimit(resource.RLIMIT_FSIZE, (file_limit, file_limit))
    with (directory / 'stdout.log').open('x') as stdout, (directory / 'stderr.log').open('x') as stderr:
        child = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=child_env, start_new_session=True, preexec_fn=limit_files)
        record['pid'] = child.pid
        started = time.monotonic()
        try:
            child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            record['timed_out'] = True
            raise
        finally:
            try:
                record["cleanup"] = cleanup_worker(child, journal)
            except BaseException as error:
                record["cleanup"] = {"verified": False, "failure_type": type(error).__name__}
                helpers.stop(child)
                raise
            finally:
                record.update(exit=child.returncode, elapsed_seconds=time.monotonic() - started)
    require(child.returncode == 0, 'worker nonzero exit')

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--worker', type=Path)
    args = parser.parse_args(argv)
    if args.worker:
        return worker(args.worker.resolve(), args.output)
    require(os.name == 'posix', 'POSIX controls only; Windows has separate portable checks')
    output = helpers.fresh_output(args.output)
    evidence = {'verified': False, 'scope': 'Python helper controls only; not native qualification or MC/DC',
                'python': sys.version, 'python_executable': str(Path(sys.executable).resolve()),
                'python_sha256': digest(Path(sys.executable).resolve()), 'commands': [], 'suites': []}
    try:
        rows, before = registry(HERE, HERE / 'helper_suites.json')
        evidence['source_before'] = before
        external = external_inputs(HERE.parent)
        evidence['external_before'] = external
        deadline = time.monotonic() + 600
        env = helpers.child_env()
        for name in (*OPTIONAL_ENV, 'PYTHONOPTIMIZE', 'PYTHONPATH', 'PYTHONHOME'):
            env.pop(name, None)
        env['PYTHONDONTWRITEBYTECODE'] = '1'
        evidence['disabled_optional_env'] = list(OPTIONAL_ENV)
        for row in rows:
            for optimize in [0, 1]:
                directory = output / (row['path'].replace('/', '__') + ('-O' if optimize else '-normal'))
                directory.mkdir()
                result_path = directory / 'result.json'
                suite_path = HERE / row['path']
                command = [sys.executable, '-I', '-B', *(['-O'] if optimize else []), str(Path(__file__).resolve()),
                           '--worker', str(suite_path), '--output', str(result_path)]
                outcome = {'path': row['path'], 'optimize': optimize, 'verified': False}
                evidence['suites'].append(outcome)
                remaining = deadline - time.monotonic()
                require(remaining > 0, 'whole-run deadline exceeded')
                execute(command, directory, evidence['commands'], env=env, timeout=min(90, remaining))
                record = bounded_json(result_path)
                outcome.update(validate(record, row, suite_path, optimize), verified=True,
                               result_sha256=digest(result_path), allowed_skips=row['allowed_skips'])
        after = inventory(HERE)
        evidence['source_after'] = after
        evidence['external_after'] = external_inputs(HERE.parent)
        require(external == evidence['external_after'], 'external input changed during run')
        require(before == after, 'helper source/registry changed during run')
        evidence['verified'] = True
    except BaseException as error:
        evidence['failure_type'] = type(error).__name__
        raise
    finally:
        evidence['artifact_sha256'] = inventory(output)
        with (output / 'evidence.json').open('x') as file:
            json.dump(evidence, file, indent=2)
            file.write('\n')
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
