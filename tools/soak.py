#!/usr/bin/env python3
"""Bounded native soak against an archived engine and explicit driver overlay."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tarfile
import time

import compatibility as helpers

ROOT = Path(__file__).resolve().parent.parent
CAP = 16 * 1024 * 1024
OVERLAYS = ['tools/soak/Cargo.toml', 'tools/soak/Cargo.lock',
            'tools/soak/src/main.rs', 'tools/soak.py', 'tools/compatibility.py']


def summary(rows, round_limit):
    """Check independently predictable lifecycle counters, not just a done flag."""
    helpers.require(len(rows) >= 2, 'missing round records or completion')
    previous_reads = previous_elapsed = 0
    for number, row in enumerate(rows[:-1], 1):
        def integer(name):
            value = row.get(name)
            helpers.require(type(value) is int and value >= 0, 'invalid round ' + name)
            return value
        helpers.require(row.get('complete') is None and integer('round') == number,
                        'nonconsecutive round records')
        # Initial revision 2; 64 data commits and one retain per round,
        # plus one release after the eighth retained root.
        helpers.require(integer('revision') == 2 + 65 * number + max(0, number - 8),
                        'unexpected round revision')
        helpers.require(integer('retained') == min(number, 8), 'unexpected retained count')
        reads, elapsed = integer('reads'), integer('elapsed_ms')
        helpers.require(reads > previous_reads and elapsed >= previous_elapsed,
                        'nonmonotonic progress')
        live, peak = integer('live_bytes'), integer('peak_bytes')
        helpers.require(0 < live <= peak <= CAP, 'core accounting outside declared bound')
        before, after = integer('append_bytes'), integer('compacted_bytes')
        helpers.require(0 < after <= before < 32 * 1024 * 1024,
                        'invalid maintenance file lengths')
        previous_reads, previous_elapsed = reads, elapsed
    last = rows[-1]
    rounds = last.get('rounds')
    helpers.require(last.get('complete') is True and type(rounds) is int
                    and rounds == len(rows) - 1 and 1 <= rounds <= round_limit,
                    'invalid completion count')
    helpers.require(type(last.get('reads')) is int and last['reads'] == previous_reads
                    and type(last.get('elapsed_ms')) is int
                    and last['elapsed_ms'] >= previous_elapsed,
                    'completion differs from observed rounds')
    return last


def execute(command, output, evidence, seconds, rounds, env):
    started = time.monotonic()
    peak = 0
    with (output / 'rounds.jsonl').open('x') as log, (output / 'stderr.log').open('x') as err:
        child = subprocess.Popen(command, env=env, start_new_session=True, stdout=log, stderr=err)
        evidence.update(command=command, pid=child.pid)
        try:
            while child.poll() is None:
                if time.monotonic() - started > seconds + 120:
                    raise TimeoutError('soak exceeded deadline plus 120-second completion grace')
                sample = subprocess.run(['ps', '-o', 'rss=', '-p', str(child.pid)],
                                        capture_output=True, text=True, timeout=5)
                if sample.returncode == 0 and sample.stdout.strip():
                    rss = int(sample.stdout.strip())
                    helpers.require(rss >= 0, 'negative RSS sample')
                    peak = max(peak, rss)
                elif child.poll() is None:
                    raise RuntimeError('RSS probe failed while child remained live')
                time.sleep(0.2)
        finally:
            helpers.stop(child)
            evidence.update(returncode=child.returncode, elapsed_seconds=time.monotonic() - started,
                            sampled_peak_rss_kib=peak)
    helpers.require(child.returncode == 0, 'soak child failed')
    rows = [json.loads(line) for line in (output / 'rounds.jsonl').read_text().splitlines()]
    evidence['last_record'] = summary(rows, rounds)
    for name in ['rounds.jsonl', 'stderr.log']:
        evidence.setdefault('logs_sha256', {})[name] = helpers.digest(output / name)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('output', type=Path)
    p.add_argument('--seconds', type=int, default=600)
    p.add_argument('--rounds', type=int, default=100000)
    args = p.parse_args(argv)
    if not 1 <= args.seconds <= 86400 or not 1 <= args.rounds <= 100000:
        p.error('seconds must be 1..86400 and rounds 1..100000')
    if platform.system() not in ('Darwin', 'Linux'):
        p.error('driver currently qualifies macOS/Linux compaction only')
    # Reject the final component before resolving it, including dangling symlinks.
    # Preserve the documented existing-parent requirement.
    if not Path(os.path.abspath(args.output)).parent.is_dir():
        p.error('output parent must exist')
    output = helpers.fresh_output(args.output)
    evidence = {'verified': False, 'commands': [], 'python': sys.version,
                'platform': platform.platform(), 'machine': platform.machine(),
                'seconds': args.seconds, 'round_limit': args.rounds,
                'driver_overlay': OVERLAYS,
                'rss_scope': 'sampled child process RSS from ps, KiB; not a cap'}
    try:
        tool_inputs = {name: helpers.digest(ROOT / name) for name in OVERLAYS}
        evidence['overlay_sha256'] = tool_inputs
        revision = helpers.run(['git', 'rev-parse', 'HEAD'], output, evidence,
                               'revision', cwd=ROOT).strip()
        evidence['engine_commit'] = revision
        archive = output / 'source.tar'
        helpers.run(['git', 'archive', '--format=tar', '--output', archive, revision],
                    output, evidence, 'archive', cwd=ROOT)
        evidence['archive_sha256'] = helpers.digest(archive)
        source = output / 'source'
        source.mkdir()
        with tarfile.open(archive) as tar:
            tar.extractall(source, filter='data')
        for name in OVERLAYS:
            dest = source / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes((ROOT / name).read_bytes())
        hashes = helpers.source_hashes(source)
        evidence['source_sha256'] = hashes
        evidence['filesystem_df'] = helpers.run(['df', '-h', output], output, evidence,
                                               'filesystem', cwd=source)
        evidence['filesystem_block_size'] = os.statvfs(output).f_frsize
        evidence['rustc'] = helpers.run(['rustc', '-Vv'], output, evidence, 'rustc', cwd=source)
        evidence['cargo'] = helpers.run(['cargo', '-V'], output, evidence, 'cargo', cwd=source).strip()
        env = helpers.child_env()
        env.update(CARGO_TARGET_DIR=str(output / 'target'), CARGO_NET_OFFLINE='true')
        helpers.run(['cargo', 'build', '--offline', '--locked', '--release', '--manifest-path',
                     source / 'tools/soak/Cargo.toml'], output, evidence, 'build',
                    cwd=source, env=env, timeout=300)
        helpers.require(helpers.source_hashes(source) == hashes, 'source changed during build')
        binary = output / 'target/release/weekend-native-soak'
        evidence['binary_sha256'] = helpers.digest(binary)
        command = [str(binary), str(output / 'data'), str(args.seconds), str(args.rounds)]
        execute(command, output, evidence, args.seconds, args.rounds, env)
        helpers.require(helpers.digest(binary) == evidence['binary_sha256'], 'binary changed during run')
        helpers.require(helpers.source_hashes(source) == hashes, 'archived source changed during run')
        helpers.require({name: helpers.digest(ROOT / name) for name in OVERLAYS} == tool_inputs,
                        'qualification tools changed during run')
        evidence.update(source_stable=True, tools_stable=True, verified=True)
    except Exception as error:
        evidence['failure'] = f'{type(error).__name__}: {error}'
        raise
    finally:
        (output / 'evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(json.dumps({key: evidence[key] for key in ['engine_commit', 'verified', 'elapsed_seconds',
                                                    'sampled_peak_rss_kib', 'last_record']}, indent=2))


if __name__ == '__main__':
    main()
