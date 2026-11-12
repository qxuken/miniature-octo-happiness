#!/usr/bin/env python3
"""Bounded native soak against an archived engine and explicit driver overlay."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tarfile
import time


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    p.add_argument("--seconds", type=int, default=600)
    p.add_argument("--rounds", type=int, default=100000)
    args = p.parse_args()
    if not 1 <= args.seconds <= 86400 or not 1 <= args.rounds <= 100000:
        p.error("seconds must be 1..86400 and rounds 1..100000")
    if platform.system() not in ("Darwin", "Linux"):
        p.error("driver currently qualifies macOS/Linux compaction only")
    root = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    output.mkdir(parents=False, exist_ok=False)
    source = output / "source"
    source.mkdir()
    archive = output / "source.tar"
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    subprocess.run(["git", "archive", "--format=tar", "--output", str(archive), revision], cwd=root, check=True)
    with tarfile.open(archive) as tar:
        tar.extractall(source, filter="data")
    archive.unlink()
    # Driver files may be under review: hash each explicit overlay, never copy
    # untracked workspace files or credentials recursively.
    driver = ["tools/soak/Cargo.toml", "tools/soak/Cargo.lock", "tools/soak/src/main.rs"]
    for name in driver:
        dest = source / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes((root / name).read_bytes())
    hashes = {str(f.relative_to(source)): hashlib.sha256(f.read_bytes()).hexdigest()
              for f in sorted(source.rglob("*")) if f.is_file()}
    evidence = {"engine_commit": revision, "driver_overlay": driver, "source_sha256": hashes,
                "platform": platform.platform(), "machine": platform.machine(),
                "filesystem_df": subprocess.check_output(["df", "-h", str(output)], text=True),
                "filesystem_block_size": os.statvfs(output).f_frsize,
                "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
                "cargo": subprocess.check_output(["cargo", "-V"], text=True).strip(),
                "seconds": args.seconds, "round_limit": args.rounds,
                "rss_scope": "sampled child process RSS from ps, KiB; excludes between-sample peaks; not a cap"}
    env = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_TARGET_DIR=str(output / "target"))
    with (output / "build.log").open("w") as log:
        subprocess.run(["cargo", "build", "--offline", "--locked", "--release", "--manifest-path",
                        str(source / "tools/soak/Cargo.toml")], env=env, stdout=log, stderr=log, check=True)
    binary = output / "target/release/weekend-native-soak"
    evidence["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
    command = [str(binary), str(output / "data"), str(args.seconds), str(args.rounds)]
    evidence["command"] = command
    (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    start = time.monotonic()
    peak = 0
    with (output / "rounds.jsonl").open("w") as log, (output / "stderr.log").open("w") as err:
        child = subprocess.Popen(command, stdout=log, stderr=err)
        try:
            while child.poll() is None:
                if time.monotonic() - start > args.seconds + 120:
                    raise TimeoutError("soak exceeded deadline plus 120-second completion grace")
                sample = subprocess.run(["ps", "-o", "rss=", "-p", str(child.pid)], capture_output=True, text=True)
                if sample.stdout.strip():
                    peak = max(peak, int(sample.stdout.strip()))
                time.sleep(0.2)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait()
            evidence.update(returncode=child.returncode, elapsed_seconds=time.monotonic()-start,
                            sampled_peak_rss_kib=peak)
            rows = [json.loads(line) for line in (output / "rounds.jsonl").read_text().splitlines()]
            evidence["last_record"] = rows[-1] if rows else None
            evidence["verified"] = child.returncode == 0 and bool(rows) and rows[-1].get("complete") is True
            (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    if not evidence["verified"]:
        raise SystemExit("soak failed; inspect evidence and stderr")
    print(json.dumps({k: evidence[k] for k in ("engine_commit", "verified", "elapsed_seconds", "sampled_peak_rss_kib", "last_record")}, indent=2))


if __name__ == "__main__":
    main()
