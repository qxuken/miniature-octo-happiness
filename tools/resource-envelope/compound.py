#!/usr/bin/env python3
"""Frozen compound resource qualification. Requires fresh output; never publishes."""
import argparse
import functools
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import uuid

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


BASE = load("compound_observer", ROOT / "tools/resource-envelope/run.py")
SCALE = load("compound_source", ROOT / "tools/serialization-scale/run.py")
PROCESSES = load("compound_processes", ROOT / "tools/check_helpers.py")
require = BASE.require
MIB = 1024 * 1024
CORE = 96 * MIB
FIXTURE, FINAL = 180, 212
NATIVE = ["fixture", "drafts_ready", "core_limit_rejected", "drafts_released",
          "contended_commits", "image_limit_rejected", "reopen",
          "backup_budget_rejected", "backup", "restore", "compact"]
FAULTS = ["allocation", "append_drop", "append_persist", "root_drop", "root_persist",
          "witness_drop", "witness_persist", "acknowledged"]
OVERLAYS = ["tools/resource-envelope/" + name for name in
            ["Cargo.toml", "Cargo.lock", "src/compound.rs", "compound.py", "run.py", "test_compound.py"]]


def fnv(chunks):
    value = 14695981039346656037
    for chunk in chunks:
        for byte in chunk:
            value = ((value ^ byte) * 1099511628211) & ((1 << 64) - 1)
    return value


def payload(epoch, length):
    # Independent byte oracle: no Rust/encoder/decoder imports.
    result = bytearray((epoch * 13 + i) % 251 for i in range(length))
    result[:8] = epoch.to_bytes(8, "little")
    return result


@functools.lru_cache(maxsize=1)
def fixed_digests():
    return (fnv(payload(epoch, 896 * 1024) for epoch in range(100, 148)),
            fnv(payload(epoch, 8192) for epoch in [64, *range(1, 65)]),
            fnv([bytes(64 * 8 * 8)]))


@functools.lru_cache(maxsize=8)
def digests(seq, changed):
    bulk, deep, retained = fixed_digests()
    chunks = []
    for writer in range(8):
        actual = seq + (1 if changed and writer == 0 else 0)
        chunks += [actual.to_bytes(8, "little"), payload(1000 + writer * 10 + actual, 65536)]
    return [bulk, deep, fnv(chunks), retained]


def integers(values, length):
    return isinstance(values, list) and len(values) == length and all(type(n) is int and n >= 0 for n in values)


def check_rows(rows, profile, source):
    phases = NATIVE if profile == "native" else FAULTS if profile == "faults" else ["fresh_process_reopen"]
    require(len(rows) == len(phases) + 1, "incomplete/extra phase set")
    require(type(rows[-1].get("complete")) is bool and
            all(type(rows[-1].get(key)) is int for key in ["features", "abi", "format"]), "invalid completion types")
    require(rows[-1] == {"complete": True, "profile": profile, "source_id": source,
                         "features": 4, "abi": 1, "format": 2}, "wrong build identity/completion")
    require([r.get("phase") for r in rows[:-1]] == phases, "wrong phase order")
    for row in rows[:-1]:
        phase = row["phase"]
        changed = profile == "faults"
        seq = 0 if profile == "native" and phase in NATIVE[:4] else 4
        survives = phase in FAULTS[4:]
        revision = (FINAL + 1 + int(survives)) if changed else FIXTURE if seq == 0 else FINAL
        require(type(row.get("changed")) is bool and row["changed"] == changed, "wrong logical state")
        require(type(row.get("seq")) is int and row["seq"] == seq and
                type(row.get("revision")) is int and row["revision"] == revision, "wrong reopened revision")
        require(type(row.get("roots")) is int and row["roots"] == 64 and row.get("digests") == digests(seq, changed), "independent byte/root oracle")
        for field in ["live_bytes", "peak_bytes", "cap_bytes", "file_bytes", "elapsed_ns", "rss_kib"]:
            require(type(row.get(field)) is int and row[field] >= 0, "invalid telemetry: " + field)
        require(row["rss_kib"] > 0, "missing process RSS")
        require(0 < row["live_bytes"] <= row["peak_bytes"] <= row["cap_bytes"] == CORE, "core budget exceeded")
        require(64 * MIB * 9 // 10 < row["file_bytes"] <= 64 * MIB, "not a reachable near-cap image")
        detail = row["detail"]
        if phase == "drafts_ready":
            require(detail.get("drafts") == 8 and integers(detail.get("prepare_ns"), 8), "missing overlapping drafts")
        if phase == "core_limit_rejected":
            require(row["live_bytes"] > CORE * 99 // 100 and detail == {"error": "Limit", "drafts": 8}, "missing saturated rejection")
        if phase == "drafts_released":
            require(detail.get("baseline_live") == row["live_bytes"], "draft charges not reclaimed")
        if phase == "contended_commits":
            require(detail.get("writers") == 8 and detail.get("rounds") == 4 and
                    integers(detail.get("commit_ns"), 32) and integers(detail.get("reader_ns"), 256), "incomplete concurrent samples")
        if phase == "image_limit_rejected":
            require(detail == {"error": "Limit", "new_payload_bytes": 4 * MIB}, "missing image-limit rejection")
        if phase == "backup_budget_rejected":
            require(detail == {"copy_budget": CORE, "error": "Limit"}, "copy domain boundary")
        if phase in ["backup", "restore", "compact"]:
            require(detail == {"copy_budget": 256 * MIB}, "missing copy budget")
        if changed:
            error = "OutOfMemory" if phase == "allocation" else "Io" if phase in FAULTS[1:3] else "Ok" if phase == "acknowledged" else "OutcomeUnknown"
            require(detail.get("error") == error and detail.get("reopened_revision") == FINAL + int(survives), "wrong durability outcome")
            require(type(detail.get("pressure_live_bytes")) is int and CORE * 89 // 100 < detail["pressure_live_bytes"] < CORE and
                    detail.get("pending_drafts") == 2, "missing compound failure pressure")
    if profile == "native":
        require(len({row["file_bytes"] for row in rows[:4]}) == 1, "pressure changed physical image")
        require(rows[0]["live_bytes"] == rows[3]["live_bytes"], "baseline charge not restored")
        require(len({row["file_bytes"] for row in rows[4:8]}) == 1, "rejection/reopen changed image length")
    return True


def observe(binary, data, report, profile, source, arguments=()):
    result = BASE.observe(binary, data, report, checker=lambda rows: check_rows(rows, profile, source), arguments=arguments)
    # The observer retires the entire process group even after leader exit.
    # Independently reject surviving live group members (zombies are not live).
    try:
        live = [p for p in PROCESSES.process_table().values() if p["group"] == result["process_group"] and not p["state"].startswith("Z")]
    except Exception as error:
        result.update(passed=False, failure="process inventory: " + type(error).__name__)
        Path(report).write_text(json.dumps(result, indent=2) + "\n")
        raise
    result["owned_live_processes_absent"] = not live
    result["rss_scope"] = f"{profile} worker only, sampled ~50ms; includes caller/allocator/backend/copy domains, excludes coordinator"
    result["passed"] = result["passed"] and not live
    Path(report).write_text(json.dumps(result, indent=2) + "\n")
    require(not live, "owned process group still live")
    return result


def worker(source, output, revision):
    source, output = Path(source).resolve(), Path(output).absolute()
    require(not os.path.lexists(output), "worker output must be fresh")
    require(not output.is_relative_to(source) and not source.is_relative_to(output), "source/output overlap")
    output.mkdir()
    env = SCALE.clean_env()
    env.update(CARGO_TARGET_DIR=str(output / "build"), WEEKEND_DB_SOURCE_ID=revision)
    evidence = {"verified": False, "revision": revision, "source_hashes": SCALE.inventory(source), "runs": {}}
    old_env = dict(os.environ)
    try:
        evidence["rustc"] = SCALE.command(["rustc", "-vV"], text=True, env=env).stdout
        configs = set()
        for start in [source, Path.cwd(), Path(env.get("CARGO_HOME", str(Path.home() / ".cargo")))]:
            for parent in [start, *start.parents]:
                for candidate in [parent / ".cargo/config", parent / ".cargo/config.toml", parent / "config", parent / "config.toml"]:
                    if candidate.is_file(): configs.add(candidate)
        evidence["trusted_cargo_configuration_sha256"] = {str(p): SCALE.sha(p) for p in sorted(configs)}
        evidence["removed_environment_names"] = sorted(set(os.environ) - set(env))
        evidence["filesystem"] = SCALE.command(["findmnt", "-T", output, "-n", "-o", "FSTYPE,SOURCE,TARGET"] if sys.platform == "linux" else ["df", "-P", output], text=True).stdout
        try:
            result = SCALE.command(["cargo", "build", "--offline", "--locked", "--release", "--manifest-path", source / "tools/resource-envelope/Cargo.toml", "--features", "compound", "--bin", "weekend-compound-resource"], env=env, cwd=source, text=True)
        except subprocess.CalledProcessError as error:
            (output / "build.log").write_text((error.stdout or "") + (error.stderr or ""))
            raise
        (output / "build.log").write_text(result.stdout + result.stderr)
        binary = output / "build/release/weekend-compound-resource"
        evidence["binary_sha256"] = SCALE.sha(binary)
        os.environ.clear(); os.environ.update(env)
        source_file = output / "native-data/source.wdb"
        for profile, arguments in [("native", ()), ("inspect", (source_file, "inspect")), ("faults", (source_file,))]:
            evidence["runs"][profile] = observe(binary, output / (profile + "-data"), output / (profile + ".json"), profile, revision, arguments)
        require(SCALE.sha(binary) == evidence["binary_sha256"], "binary changed")
        require(SCALE.inventory(source) == evidence["source_hashes"], "source changed")
        evidence["verified"] = True
    finally:
        os.environ.clear(); os.environ.update(old_env)
        evidence["artifacts"] = {str(p.relative_to(output)): SCALE.sha(p) for p in output.rglob("*") if p.is_file() and "build" not in p.relative_to(output).parts}
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--revision")
    parser.add_argument("--docker-image")
    parser.add_argument("--worker", nargs=3)
    args = parser.parse_args()
    if args.worker:
        worker(*args.worker); return
    require(args.output is not None and args.revision is not None, "output and revision required")
    output = BASE.HELPERS.fresh_output(args.output)
    evidence = {"verified": False}
    try:
        revision = SCALE.command(["git", "rev-parse", "--verify", args.revision + "^{commit}"], cwd=ROOT, text=True).stdout.strip()
        archive = SCALE.command(["git", "archive", revision], cwd=ROOT).stdout
        (output / "source.tar").write_bytes(archive)
        source = output / "source"; source.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as packed: packed.extractall(source, filter="data")
        for name in OVERLAYS:
            path = source / name; path.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(ROOT / name, path)
        for name in ["tools/check_helpers.py", "tools/serialization-scale/run.py", "tools/sqlite-baseline/run.py"]:
            require((source / name).read_bytes() == (ROOT / name).read_bytes(), "archived shared helper mismatch")
        evidence.update(revision=revision, archive_sha256=SCALE.sha(output / "source.tar"), source_hashes=SCALE.inventory(source), overlays={name: SCALE.sha(ROOT / name) for name in OVERLAYS})
        if args.docker_image:
            image = SCALE.command(["docker", "image", "inspect", "--format", "{{.Id}}", args.docker_image], text=True).stdout.strip()
            name = "compound-resource-" + uuid.uuid4().hex; volume = name + "-data"
            evidence.update(image=image, container=name, volume=volume)
            try:
                SCALE.command(["docker", "volume", "create", volume])
                argv = ["docker", "run", "--name", name, "--network", "none", "--memory", "2g", "--cpus", "2", "--pids-limit", "256",
                        "--mount", f"type=volume,src={volume},dst=/data", "--mount", f"type=bind,src={source},dst=/src,readonly",
                        image, "python3", "/src/tools/resource-envelope/compound.py", "--worker", "/src", "/data/run", revision]
                with (output / "docker.log").open("x") as log:
                    result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT, timeout=600)
            finally:
                # Copy partial logs before retiring a failed/timed-out worker.
                try:
                    copied = subprocess.run(["docker", "cp", name + ":/data/run", str(output / "run")], capture_output=True, text=True, timeout=120)
                    evidence["copy"] = {"exit": copied.returncode, "stderr": copied.stderr}
                finally:
                    evidence["cleanup"] = SCALE.cleanup_docker(name, volume)
                    require(evidence["cleanup"]["container_absent"] and evidence["cleanup"]["volume_absent"], "Docker cleanup not verified")
            result.check_returncode()
            require(copied.returncode == 0, "Docker evidence copy incomplete")
        else:
            worker(source, output / "run", revision)
        run = json.loads((output / "run/evidence.json").read_text())
        require(run["verified"], "unverified worker")
        for profile, report in run["runs"].items(): check_rows(report["rows"], profile, revision)
        for name, digest in run["artifacts"].items(): require(SCALE.sha(output / "run" / name) == digest, "copied artifact changed")
        require(SCALE.sha(output / "run/build/release/weekend-compound-resource") == run["binary_sha256"], "copied binary mismatch")
        require(SCALE.inventory(source) == evidence["source_hashes"], "archived source changed")
        evidence["run_evidence_sha256"] = SCALE.sha(output / "run/evidence.json")
        evidence["verified"] = True
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
