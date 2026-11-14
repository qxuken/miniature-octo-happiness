#!/usr/bin/env python3
"""Finite native resource envelope against a frozen engine; no overwrite or remote work."""
import argparse
import functools
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tarfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "tools/sqlite-baseline/run.py"
SPEC = importlib.util.spec_from_file_location("private_baseline", HELPER)
HELPERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPERS)
require = HELPERS.require


def command(argv, **kwargs):
    # Bound setup probes/builds without changing the shared baseline helper.
    kwargs.setdefault("timeout", 120)
    return HELPERS.command(argv, **kwargs)


@functools.lru_cache(maxsize=8)
def oracle_hash(epochs, length):
    # Separate Python byte oracle, independent of the Rust generator/checker.
    value = 14695981039346656037
    for epoch in epochs:
        prefix = epoch.to_bytes(8, "little")
        for index in range(length):
            byte = prefix[index] if index < 8 else (epoch * 13 + index) % 251
            value = ((value ^ byte) * 1099511628211) & ((1 << 64) - 1)
    return value


def check_rows(rows):
    require(len(rows) == 21 and rows[-1] == {"complete": True, "cases": 4}, "incomplete phase set")
    phases = [row["phase"] for row in rows[:-1]]
    wanted = ["nested_first_root", "nested_64_roots", *(["nested_reopen"] * 3),
              "draft_pressure", "core_limit_rejected", "drafts_released_admission_recovers",
              "pins_released_admission_recovers", "pins_released_reopen_reclaims_cache",
              "synthetic_exact_image_boundary", "synthetic_compact_reopens_admission",
              "reachable_natural_image_limit", *(["reachable_reopen"] * 3),
              "reachable_backup", "reachable_restore", "reachable_compact", "bounded_stack_depth64"]
    require(phases == wanted, "phase ordering/count mismatch")
    natural = next(row for row in rows[:-1] if row["phase"] == "reachable_natural_image_limit")
    count = natural["detail"]["accepted_keys"]
    require(8 <= count < 80, "missing natural image-limit rejection")
    require(len(natural["detail"]["commit_ns"]) == count, "commit sample count mismatch")
    for row in rows[:-1]:
        require(0 <= row["live_bytes"] <= row["peak_bytes"] <= row["cap_bytes"], "core accounting invalid")
        require(row["rss_kib"] > 0 and row["file_bytes"] <= 64 * 1024 * 1024, "resource sample invalid")
        if row["oracle_kind"] == "large":
            require(row["current_count"] == count and row["retained_count"] == 8, "large root set mismatch")
            epochs = [*range(1, count + 1), *(key for root in range(1, 9) for key in range(1, root + 1))]
            expected = oracle_hash(tuple(epochs), 896 * 1024)
        else:
            require(row["current_count"] == 1, "nested row count mismatch")
            if row["phase"] == "nested_first_root" or row["oracle_kind"] == "stack":
                require(row["retained_count"] == 1, "one-root fixture mismatch")
                epochs = [1, 1]
            else:
                require(row["retained_count"] in (0, 64), "nested root count mismatch")
                epochs = [64, *range(1, row["retained_count"] + 1)]
            expected = oracle_hash(tuple(epochs), 8192)
        require(row["oracle_digest"] == expected, f"independent byte oracle mismatch: {row['phase']}")
    full = next(row for row in rows[:-1] if row["phase"] == "nested_64_roots")
    require(len(full["detail"]["commit_ns"]) == len(full["detail"]["pin_ns"]) == 64, "nested samples mismatch")
    pressure = next(row for row in rows[:-1] if row["phase"] == "core_limit_rejected")
    require(pressure["live_bytes"] > pressure["cap_bytes"] * 0.95, "not near core cap")
    require(natural["file_bytes"] > 64 * 1024 * 1024 * 0.9, "reachable image below 90%")


def observe(binary, data, report, deadline_seconds=180, *, checker=check_rows, arguments=()):
    data, report = Path(data), Path(report)
    if os.path.lexists(data) or os.path.lexists(report):
        raise ValueError("observer outputs must be new")
    out = report.with_suffix(".jsonl")
    err = report.with_suffix(".stderr")
    if os.path.lexists(out) or os.path.lexists(err):
        raise ValueError("observer logs must be new")
    evidence = {"passed": False, "rss_scope": "main child sampled every ~50ms; stack child has its own phase RSS; not a process cap"}
    with report.open("x") as stream:
        json.dump(evidence, stream)
    started = time.monotonic()
    samples = []
    with out.open("x") as stdout, err.open("x") as stderr:
        child = subprocess.Popen([str(binary), str(data), *map(str, arguments)], stdout=stdout, stderr=stderr, start_new_session=True)
        failure = None
        try:
            # Observe exit without reaping: reserve the leader PID until its
            # group is retired, so PID reuse cannot target an unrelated group.
            while os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
                require(out.stat().st_size < 8 * 1024 * 1024 and err.stat().st_size < 8 * 1024 * 1024, "resource driver output limit")
                if time.monotonic() - started > deadline_seconds:
                    raise TimeoutError("resource driver exceeded 180-second deadline")
                if sys.platform == "linux":
                    try:
                        status = Path(f"/proc/{child.pid}/status").read_text()
                        sample = next(line for line in status.splitlines() if line.startswith("VmRSS:"))
                        samples.append(int(sample.split()[1]))
                    except (FileNotFoundError, StopIteration):
                        pass
                else:
                    output = subprocess.run(["ps", "-o", "rss=", "-p", str(child.pid)],
                                            capture_output=True, text=True, timeout=5)
                    if output.stdout.strip():
                        samples.append(int(output.stdout.strip()))
                time.sleep(0.05)
        except Exception as error:
            failure = f"{type(error).__name__}: {error}"
        finally:
            group_killed = False
            # A completed leader may leave descendants behind. Always retire
            # the owned process group, including on a nominally successful exit.
            try:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                    group_killed = True
                except ProcessLookupError:
                    pass
                except PermissionError:
                    # This macOS kernel reports EPERM for a zombie-only group.
                    # Ignore it only after proving the owned leader has exited
                    # and a bounded process-table query finds no live members.
                    require(os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is not None, "cannot retire live leader")
                    groups = subprocess.run(["ps", "-axo", "pgid=,stat="], capture_output=True, text=True, timeout=5)
                    require(groups.returncode == 0 and len(groups.stdout) <= 8 * 1024 * 1024, "cannot verify retired group")
                    members = [line.split() for line in groups.stdout.splitlines()]
                    require(all(len(row) == 2 for row in members), "invalid process group inventory")
                    require(not any(int(group) == child.pid and not state.startswith("Z") for group, state in members), "owned group remains live")
            finally:
                child.wait(timeout=5)
            evidence.update(returncode=child.returncode, elapsed_seconds=time.monotonic() - started,
                            rss_samples_kib=samples, rss_sample_count=len(samples),
                            sampled_peak_rss_kib=max(samples, default=0),
                            process_group=child.pid, process_group_killed=group_killed)
            try:
                require(out.stat().st_size < 8 * 1024 * 1024 and err.stat().st_size < 8 * 1024 * 1024, "resource driver output limit")
                rows = [json.loads(line) for line in out.read_text().splitlines()]
                evidence["rows"] = rows
                if child.returncode == 0 and failure is None:
                    checker(rows)
                    require(bool(samples), "no RSS samples")
                    evidence["passed"] = True
                elif failure is None:
                    failure = f"driver exit {child.returncode}"
            except Exception as error:
                failure = f"{type(error).__name__}: {error}"
            evidence["failure"] = failure
            report.write_text(json.dumps(evidence, indent=2) + "\n")
    require(evidence["passed"], "resource driver failed; inspect logs")
    return evidence


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--observe":
        require(len(sys.argv) == 5, "invalid internal observer arguments")
        observe(*sys.argv[2:])
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    parser.add_argument("--revision", required=True)
    parser.add_argument("--docker-image", help="local installed image, resolved to immutable ID; network disabled")
    args = parser.parse_args()
    require(sys.platform in ("darwin", "linux"), "macOS/Linux only")
    revision = command(["git", "rev-parse", "--verify", args.revision + "^{commit}"], cwd=ROOT, text=True).stdout.strip()
    output = HELPERS.fresh_output(args.output)
    source = output / "source"
    source.mkdir()
    archive = command(["git", "archive", "--format=tar", revision], cwd=ROOT).stdout
    with tarfile.open(fileobj=io.BytesIO(archive)) as packed:
        packed.extractall(source, filter="data")
    overlay = ["tools/resource-envelope/Cargo.toml", "tools/resource-envelope/Cargo.lock",
               "tools/resource-envelope/src/main.rs", "tools/resource-envelope/run.py",
               "tools/resource-envelope/test_runner.py", "tools/resource-envelope/README.md"]
    for name in overlay:
        path = source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes((ROOT / name).read_bytes())
    require((source / "tools/sqlite-baseline/run.py").read_bytes() == HELPER.read_bytes(), "shared helper differs from explicit engine revision")
    hashes = {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in source.rglob("*") if p.is_file()}
    evidence = {"passed": False, "engine_revision": revision, "archive_sha256": hashlib.sha256(archive).hexdigest(),
                "source_sha256": hashes, "driver_overlay": overlay, "host_platform": platform.platform(),
                "filesystem_df": command(["df", "-h", output], text=True).stdout}
    env = os.environ.copy()
    for name in list(env):
        if (name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "WEEKEND_RESOURCE_STACK_CHILD")
                or name.startswith("CARGO_PROFILE_")
                or (name.startswith("CARGO_TARGET_") and name.endswith("_RUSTFLAGS"))):
            env.pop(name, None)
    env.update(CARGO_INCREMENTAL="0", CARGO_TARGET_DIR=str(output / "build"), WEEKEND_DB_SOURCE_ID=revision)
    if args.docker_image:
        image = command(["docker", "image", "inspect", "--format", "{{.Id}}", args.docker_image], text=True).stdout.strip()
        name = "weekend-resource-" + uuid.uuid4().hex[:12]
        script = """mkdir /work
rustc -vV > /work/rustc.txt
stat -f -c '%T' /work > /work/filesystem.txt
cargo build --offline --release --locked --manifest-path /src/tools/resource-envelope/Cargo.toml > /work/build.log 2>&1
python3 /src/tools/resource-envelope/run.py --observe /work/build/release/weekend-resource-envelope /work/data /work/run.json
"""
        argv = ["docker", "run", "--name", name, "--network", "none", "--memory", "2g", "--pids-limit", "256",
                "--mount", f"type=bind,src={source},dst=/src,readonly", "--workdir", "/src",
                "--env", "CARGO_INCREMENTAL=0", "--env", "CARGO_TARGET_DIR=/work/build",
                "--env", f"WEEKEND_DB_SOURCE_ID={revision}", image, "sh", "-ec", script]
        evidence.update(docker_image_id=image, docker_command=argv, runtime_filesystem="container writable layer; measured in filesystem.txt")
        try:
            result = subprocess.run(argv, capture_output=True, text=True, timeout=300)
            (output / "docker.log").write_text(result.stdout + result.stderr)
            evidence["docker_exit_code"] = result.returncode
            copies = {}
            for item in ("run.json", "run.jsonl", "run.stderr", "rustc.txt", "filesystem.txt", "build.log", "data", "build/release/weekend-resource-envelope"):
                target = output / ("driver" if item.startswith("build/") else item)
                copied = subprocess.run(["docker", "cp", f"{name}:/work/{item}", str(target)], capture_output=True, text=True, timeout=60)
                copies[item] = {"returncode": copied.returncode, "stderr": copied.stderr}
            evidence["docker_copies"] = copies
            require(result.returncode == 0, "Docker qualification failed")
            require(all(item["returncode"] == 0 for item in copies.values()), "Docker evidence copy incomplete")
            binary = output / "driver"
        except Exception as error:
            evidence["failure"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            cleanup = {"container": name, "absent": False}
            try:
                removed = subprocess.run(["docker", "rm", "-f", name], capture_output=True, text=True, timeout=30)
                cleanup["remove_returncode"] = removed.returncode
                cleanup["stderr"] = removed.stderr
                remaining = command(["docker", "ps", "-aq", "--filter", f"name=^/{name}$"], text=True, timeout=10).stdout.strip()
                cleanup["absent"] = not remaining
            except Exception as error:
                cleanup["failure"] = f"{type(error).__name__}: {error}"
            evidence["docker_cleanup"] = cleanup
            (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
            require(cleanup["absent"], "Docker container removal not verified")
        evidence["rustc"] = (output / "rustc.txt").read_text()
        evidence["filesystem_type"] = (output / "filesystem.txt").read_text().strip()
    else:
        prefix = ["mise", "x", "rust@1.99.1", "--"]
        argv = prefix + ["cargo", "build", "--offline", "--release", "--locked", "--manifest-path", source / "tools/resource-envelope/Cargo.toml"]
        result = command(argv, cwd=source, env=env, text=True)
        (output / "build.log").write_text(result.stdout + result.stderr)
        binary = output / "build/release/weekend-resource-envelope"
        evidence.update(build_command=[str(a) for a in argv], rustc=command(prefix + ["rustc", "-vV"], text=True).stdout)
        old_env = os.environ.copy()
        try:
            os.environ.clear()
            os.environ.update(env)
            observe(binary, output / "data", output / "run.json")
        finally:
            os.environ.clear()
            os.environ.update(old_env)
    evidence["run"] = json.loads((output / "run.json").read_text())
    require(evidence["run"]["passed"], "unverified run")
    check_rows(evidence["run"]["rows"])
    evidence["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
    require(hashes == {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest()
                       for p in source.rglob("*") if p.is_file()}, "source changed during qualification")
    evidence["passed"] = True
    (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps({"passed": True, "evidence": str(output / "evidence.json"),
                      "sampled_peak_rss_kib": evidence["run"]["sampled_peak_rss_kib"]}, indent=2))


if __name__ == "__main__":
    main()
