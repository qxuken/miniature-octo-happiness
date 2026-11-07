#!/usr/bin/env python3
"""Run ASan and TSan with nightly, rust-src and an instrumented standard library."""
import argparse
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=["address", "thread"])
    parser.add_argument("--target", help="Defaults to nightly's host target")
    args = parser.parse_args()
    target = args.target or next(line.split(": ", 1)[1] for line in subprocess.check_output(
        ["rustc", "+nightly", "-vV"], text=True).splitlines() if line.startswith("host:"))
    flags = f"-Zsanitizer={args.kind}"
    env = {**os.environ, "RUSTFLAGS": flags, "RUSTDOCFLAGS": flags,
           "CARGO_TARGET_DIR": str(ROOT / "target" / f"sanitizer-{args.kind}")}
    # Rebuilding std instruments its atomics as required by TSan. std's build
    # dependencies may need a first online fetch. The engine has no dependencies;
    # the convenience workspace also builds its host procedural-macro crates.
    subprocess.run(["cargo", "+nightly", "test", "-Zbuild-std", "--workspace", "--all-features",
                    "--target", target], cwd=ROOT, env=env, check=True)


if __name__ == "__main__":
    main()
