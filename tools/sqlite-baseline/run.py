#!/usr/bin/env python3
"""Private, bounded durable-history baseline. Requires an explicit engine revision."""
import argparse
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import shutil
import sqlite3
import subprocess
import sys
import tarfile
import time

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def require(condition, message="baseline invariant failed"):
    # Assurance checks and PRAGMA/checkpoint calls must survive Python -O.
    if not condition:
        raise AssertionError(message)


def command(argv, **kwargs):
    return subprocess.run([str(a) for a in argv], check=True, capture_output=True, **kwargs)


def fresh_output(value):
    # Reject before resolving: an existing symlink must never redirect output.
    candidate = Path(os.path.abspath(value))
    if os.path.lexists(candidate):
        raise ValueError("output already exists (including dangling symlinks)")
    if not candidate.parent.is_dir():
        raise ValueError("output parent must already be a directory")
    candidate = candidate.parent.resolve() / candidate.name
    candidate.mkdir(mode=0o700)  # Atomic no-overwrite reservation.
    return candidate


def payload(key, epoch, length):
    data = bytearray((epoch * 17 + key * 31 + i) % 251 for i in range(length))
    data[:8] = epoch.to_bytes(8, "little")
    data[8:16] = key.to_bytes(8, "little")
    return bytes(data)


def oracle(work):
    # Forward replay is independent of Rust's backwards last-write lookup.
    state = {k: payload(k, 0, work["payload"]) for k in range(work["keys"])}
    roots = {}
    for epoch in range(1, work["commits"] + 1):
        for j in range(work["batch"]):
            key = (epoch * work["batch"] + j) % work["keys"]
            state[key] = payload(key, epoch, work["payload"])
        if epoch % work["retain_every"] == 0 or epoch == work["commits"]:
            roots[epoch] = state.copy()
    return state, roots


def digest(current, roots, commits):
    value = 14695981039346656037
    for epoch, state in [(commits, current), *roots.items()]:
        pieces = [epoch.to_bytes(8, "little")]
        for key in sorted(state):
            pieces.extend((key.to_bytes(8, "little"), state[key]))
        for piece in pieces:
            for byte in piece:
                value = ((value ^ byte) * 1099511628211) & ((1 << 64) - 1)
    return value


def connect(path, create=False):
    if not create and not path.is_file():
        raise ValueError("source does not exist")
    db = sqlite3.connect(path.as_uri() + ("?mode=rwc" if create else "?mode=rw"),
                         uri=True, isolation_level=None, timeout=5)
    wanted = {"synchronous": 2, "fullfsync": 1, "checkpoint_fullfsync": 1,
              "wal_autocheckpoint": 1000, "cache_size": -2000,
              "mmap_size": 0, "busy_timeout": 5000}
    if create:
        db.execute("PRAGMA page_size=4096")
    require(db.execute("PRAGMA journal_mode=WAL").fetchone()[0] == "wal", "WAL mode rejected")
    for name, value in wanted.items():
        db.execute(f"PRAGMA {name}={value}")
        require(db.execute(f"PRAGMA {name}").fetchone()[0] == value, name)
    require(db.execute("PRAGMA page_size").fetchone()[0] == 4096, "page size mismatch")
    return db


def sizes(path):
    return {suffix or "main": Path(str(path) + suffix).stat().st_size
            if Path(str(path) + suffix).exists() else 0 for suffix in ("", "-wal", "-shm")}


def read_state(db, epoch):
    rows = db.execute("""SELECT v.k,v.payload FROM versions v JOIN
        (SELECT k,MAX(epoch) AS epoch FROM versions WHERE epoch<=? GROUP BY k) x
        USING(k,epoch)""", (epoch,)).fetchall()
    state = {}
    for key, data in rows:
        require(isinstance(key, bytes) and len(key) == 8, "noncanonical binary key")
        number = int.from_bytes(key, "little")
        require(number not in state, "duplicate logical key")
        state[number] = bytes(data)
    return state


def verify(db, work, expected):
    current, roots = expected
    require(db.execute("SELECT epoch FROM head").fetchall() == [(work["commits"],)], "head mismatch")
    require(db.execute("SELECT id,epoch FROM retained ORDER BY id").fetchall() == list(
        enumerate(roots, 1)), "retained catalog mismatch")
    require(read_state(db, work["commits"]) == current, "current state mismatch")
    for epoch, state in roots.items():
        require(read_state(db, epoch) == state, f"retained state mismatch at {epoch}")
    return digest(current, roots, work["commits"])


def verify_path(path, work, expected):
    db = connect(path)
    try:
        require(db.execute("PRAGMA integrity_check").fetchall() == [("ok",)], "integrity check failed")
        return verify(db, work, expected)
    finally:
        db.close()


def sync_output(path):
    # Explicit destination and directory sync supplements the backup API.
    with path.open("rb") as stream:
        os.fsync(stream.fileno())
        if sys.platform == "darwin":
            import fcntl
            fcntl.fcntl(stream.fileno(), fcntl.F_FULLFSYNC)
    descriptor = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def verified_copy(source, target, work, expected):
    if os.path.lexists(target):
        raise ValueError("copy destination already exists")
    started = time.perf_counter_ns()
    verify_path(source, work, expected)
    # Source is quiescent; use the WAL-aware SQLite backup API, never raw copy.
    src = connect(source)
    dst = sqlite3.connect(target, isolation_level=None)
    try:
        dst.execute("PRAGMA synchronous=FULL")
        dst.execute("PRAGMA fullfsync=ON")
        src.backup(dst)
    finally:
        dst.close()
        src.close()
    verify_path(target, work, expected)
    sync_output(target)
    return time.perf_counter_ns() - started


def sqlite_run(directory, work, expected):
    directory.mkdir()
    path = directory / "main.sqlite"
    db = connect(path, create=True)
    settings = {name: db.execute(f"PRAGMA {name}").fetchone()[0] for name in
                ("journal_mode", "synchronous", "fullfsync", "checkpoint_fullfsync",
                 "page_size", "cache_size", "mmap_size", "wal_autocheckpoint")}
    options = [row[0] for row in db.execute("PRAGMA compile_options")]
    db.executescript("""BEGIN IMMEDIATE;
        CREATE TABLE versions(k BLOB NOT NULL,epoch INTEGER NOT NULL,payload BLOB NOT NULL,
            PRIMARY KEY(k,epoch)) WITHOUT ROWID;
        CREATE TABLE retained(id INTEGER PRIMARY KEY,epoch INTEGER NOT NULL);
        CREATE TABLE head(epoch INTEGER NOT NULL);
        INSERT INTO head VALUES(0); COMMIT;""")
    db.execute("BEGIN IMMEDIATE")
    db.executemany("INSERT INTO versions VALUES(?,?,?)", [(k.to_bytes(8, "little"), 0,
                   payload(k, 0, work["payload"])) for k in range(work["keys"])])
    db.execute("COMMIT")
    result = {name: [] for name in ("commit_ns", "transaction_ns", "pin_ns", "reopen_ns", "reopen_verify_ns")}
    for epoch in range(1, work["commits"] + 1):
        writes = []
        for j in range(work["batch"]):
            key = (epoch * work["batch"] + j) % work["keys"]
            writes.append((key.to_bytes(8, "little"), epoch, payload(key, epoch, work["payload"])))
        started = time.perf_counter_ns()
        db.execute("BEGIN IMMEDIATE")
        db.executemany("INSERT INTO versions VALUES(?,?,?)", writes)
        db.execute("UPDATE head SET epoch=?", (epoch,))
        commit = time.perf_counter_ns()
        db.execute("COMMIT")
        result["commit_ns"].append(time.perf_counter_ns() - commit)
        result["transaction_ns"].append(time.perf_counter_ns() - started)
        if epoch in expected[1]:
            started = time.perf_counter_ns()
            db.execute("BEGIN IMMEDIATE")
            db.execute("INSERT INTO retained(epoch) VALUES(?)", (epoch,))
            db.execute("COMMIT")
            result["pin_ns"].append(time.perf_counter_ns() - started)
    result["oracle_digest"] = verify(db, work, expected)
    result["history_count"] = len(expected[1])
    result["version_rows_before_gc"] = db.execute("SELECT COUNT(*) FROM versions").fetchone()[0]
    require(result["version_rows_before_gc"] == work["keys"] + work["commits"] * work["batch"], "history rows mismatch")
    result["sizes_live"] = sizes(path)
    checkpoint = time.perf_counter_ns()
    result["checkpoint_result"] = db.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone()
    result["checkpoint_ns"] = time.perf_counter_ns() - checkpoint
    require(result["checkpoint_result"] == (0, 0, 0), "checkpoint incomplete")
    result["sizes_checkpointed"] = sizes(path)
    db.close()
    result["sizes_closed"] = sizes(path)
    for _ in range(work["reopens"]):
        started = time.perf_counter_ns()
        db = connect(path)
        require(db.execute("SELECT epoch FROM head").fetchall() == [(work["commits"],)], "reopen head mismatch")
        require(len(db.execute("SELECT id FROM retained").fetchall()) == len(expected[1]), "reopen catalog mismatch")
        result["reopen_ns"].append(time.perf_counter_ns() - started)
        require(verify(db, work, expected) == result["oracle_digest"], "reopen digest mismatch")
        result["reopen_verify_ns"].append(time.perf_counter_ns() - started)
        db.close()
    result["backup_ns"] = verified_copy(path, directory / "backup.sqlite", work, expected)
    result["restore_ns"] = verified_copy(directory / "backup.sqlite", directory / "restored.sqlite", work, expected)
    started = time.perf_counter_ns()
    db = connect(path)
    db.execute("CREATE TEMP TABLE keep(k BLOB,epoch INTEGER,PRIMARY KEY(k,epoch)) WITHOUT ROWID")
    db.execute("BEGIN IMMEDIATE")
    for epoch in sorted({work["commits"], *expected[1]}):
        db.execute("INSERT OR IGNORE INTO keep SELECT k,MAX(epoch) FROM versions WHERE epoch<=? GROUP BY k", (epoch,))
    db.execute("DELETE FROM versions WHERE NOT EXISTS(SELECT 1 FROM keep WHERE keep.k=versions.k AND keep.epoch=versions.epoch)")
    db.execute("COMMIT")
    result["version_rows_after_gc"] = db.execute("SELECT COUNT(*) FROM versions").fetchone()[0]
    needed_versions = {(key, int.from_bytes(data[:8], "little"))
                       for state in [expected[0], *expected[1].values()]
                       for key, data in state.items()}
    require(result["version_rows_after_gc"] == len(needed_versions), "GC version count mismatch")
    db.execute("VACUUM")
    require(db.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone() == (0, 0, 0), "compact checkpoint incomplete")
    db.close()
    verify_path(path, work, expected)
    sync_output(path)
    result["compact_ns"] = time.perf_counter_ns() - started
    result["sizes_compacted"] = sizes(path)
    result["backup_sizes"] = sizes(directory / "backup.sqlite")
    result["restored_sizes"] = sizes(directory / "restored.sqlite")
    result["settings"] = settings
    result["compile_options"] = options
    return result


def quantiles(values):
    ordered = sorted(values)
    return {"samples": len(values), **{f"p{p}_ns": ordered[math.ceil(p * len(values) / 100) - 1]
                                       for p in (50, 95, 99)}}


def run(args):
    if sys.platform not in ("darwin", "linux"):
        raise ValueError("native comparator currently supports macOS and Linux only")
    work = {name: getattr(args, name) for name in ("commits", "keys", "batch", "payload", "retain_every", "reopens")}
    if not (1 <= args.commits <= 512 and 1 <= args.keys <= 256 and 1 <= args.batch <= args.keys
            and 16 <= args.payload <= 8192 and 1 <= args.reopens <= 128
            and args.retain_every > 0 and math.ceil(args.commits / args.retain_every) <= 64
            and args.keys * args.payload <= 1024 * 1024
            and args.commits * args.batch * args.payload <= 64 * 1024 * 1024
            and 1 <= args.rounds <= 8):
        raise ValueError("workload exceeds the bounded supported range")
    revision = command(["git", "rev-parse", "--verify", args.revision + "^{commit}"], cwd=ROOT,
                       text=True).stdout.strip()
    expected = oracle(work)
    expected_digest = digest(*expected, args.commits)
    output = fresh_output(args.output)
    source = output / "source"
    source.mkdir()
    archive = command(["git", "archive", "--format=tar", revision], cwd=ROOT).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as packed:
        packed.extractall(source, filter="data")
    destination = source / "tools/sqlite-baseline"
    # The explicit revision may predate this new harness. Bind the harness separately.
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(HERE, destination, ignore=shutil.ignore_patterns("target", "__pycache__"))
    hashes = {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in source.rglob("*") if p.is_file()}
    env = os.environ.copy()
    for name in list(env):
        if (name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER",
                     "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTFLAGS")
                or name.startswith("CARGO_PROFILE_")
                or (name.startswith("CARGO_TARGET_") and name.endswith("_RUSTFLAGS"))):
            env.pop(name, None)
    env.update(CARGO_INCREMENTAL="0", CARGO_TARGET_DIR=str(output / "build"),
               WEEKEND_DB_SOURCE_ID=revision)
    prefix = ["mise", "x", "rust@1.99.1", "--"]
    build = prefix + ["cargo", "build", "--offline", "--release", "--locked", "--manifest-path", destination / "Cargo.toml"]
    built = command(build, cwd=source, env=env, text=True)
    (output / "build.log").write_text(built.stdout + built.stderr)
    binary = output / "build/release/weekend-sqlite-baseline"
    report = {"engine_revision": revision, "archive_sha256": hashlib.sha256(archive).hexdigest(),
              "source_sha256": hashes, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "build_command": [str(v) for v in build], "workload": work, "rounds": [],
              "platform": platform.platform(), "machine": platform.machine(),
              "python": sys.version, "python_executable": sys.executable,
              "sqlite_version": sqlite3.sqlite_version,
              "sqlite_source_id": sqlite3.connect(":memory:").execute("SELECT sqlite_source_id()").fetchone()[0],
              "rustc": command(prefix + ["rustc", "-vV"], text=True).stdout,
              "oracle_digest": expected_digest, "quantile_method": "nearest-rank, pooled raw samples",
              "cache_policy": "warm OS cache; no cache eviction; fresh database each round",
              "passed": False}
    for index in range(args.rounds):
        engines = ["weekend", "sqlite"] if index % 2 == 0 else ["sqlite", "weekend"]
        record = {"order": engines}
        for engine in engines:
            directory = output / f"round-{index + 1}-{engine}"
            if engine == "weekend":
                values = [work[k] for k in ("commits", "keys", "batch", "payload", "retain_every", "reopens")]
                result = json.loads(command([binary, directory, *values], env=env, text=True).stdout)
                result["settings"] = {"native_sync": "F_FULLFSYNC" if sys.platform == "darwin" else "fsync", "features": "base"}
            else:
                result = sqlite_run(directory, work, expected)
            require(result["oracle_digest"] == expected_digest, "cross-engine digest mismatch")
            require(result["history_count"] == len(expected[1]), "cross-engine history count mismatch")
            record[engine] = result
        report["rounds"].append(record)
        (output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
    report["summary"] = {engine: {name: quantiles([sample for row in report["rounds"]
                              for sample in row[engine][name]]) for name in
                              ("commit_ns", "transaction_ns", "pin_ns", "reopen_ns", "reopen_verify_ns")}
                         for engine in ("weekend", "sqlite")}
    require(hashes == {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in source.rglob("*") if p.is_file()}, "source changed during run")
    report["passed"] = True
    (output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"evidence": str(output / "evidence.json"), "passed": True, "summary": report["summary"]}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    parser.add_argument("--revision", required=True)
    for name, default in (("commits", 64), ("keys", 32), ("batch", 4), ("payload", 256),
                          ("retain-every", 16), ("reopens", 32), ("rounds", 3)):
        parser.add_argument("--" + name, type=int, default=default)
    args = parser.parse_args()
    try:
        run(args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"baseline rejected/failed: {error}\n")


if __name__ == "__main__":
    main()
