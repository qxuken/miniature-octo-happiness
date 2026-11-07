#!/usr/bin/env python3
"""Measure native Rust test coverage with the active rustc's llvm-tools-preview."""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def output(*args):
    return subprocess.check_output([str(a) for a in args], cwd=ROOT, text=True)


def main():
    host = next(line.split(": ", 1)[1] for line in output("rustc", "-vV").splitlines()
                if line.startswith("host:"))
    llvm = Path(output("rustc", "--print", "sysroot").strip()) / "lib/rustlib" / host / "bin"
    suffix = ".exe" if os.name == "nt" else ""
    cov, profdata = (llvm / (name + suffix) for name in ["llvm-cov", "llvm-profdata"])
    if not cov.exists() or not profdata.exists():
        raise SystemExit("Install matching tools first: rustup component add llvm-tools-preview")
    dest = ROOT / "target/coverage"
    profiles = dest / "profiles"
    profiles.mkdir(parents=True, exist_ok=True)
    # Discard only this script's previous raw counters, never other build output.
    for path in profiles.glob("*.profraw"):
        path.unlink()
    env = {**os.environ, "CARGO_TARGET_DIR": str(dest),
           "RUSTFLAGS": "-C instrument-coverage", "RUSTDOCFLAGS": "-C instrument-coverage",
           "LLVM_PROFILE_FILE": str(profiles / "%p-%m.profraw")}
    command = ["cargo", "test", "--offline", "--workspace", "--all-features"]
    artifacts = subprocess.check_output(command + ["--no-run", "--message-format=json"],
                                        cwd=ROOT, env=env, text=True)
    objects = sorted({item["executable"] for line in artifacts.splitlines()
                      if (item := json.loads(line)).get("reason") == "compiler-artifact"
                      and item.get("executable")})
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    merged = dest / "tests.profdata"
    subprocess.run([str(profdata), "merge", "-sparse", *map(str, profiles.glob("*.profraw")),
                    "-o", str(merged)], check=True)
    args = [str(cov), "report", objects[0], f"-instr-profile={merged}",
            "-ignore-filename-regex=/(tests|examples|tools)/|/\\.rustup/|/\\.cargo/registry/"]
    for obj in objects[1:]:
        args += ["-object", obj]
    report = output(*args)
    (dest / "report.txt").write_text(report)
    print(report)
    subprocess.run([str(cov), "show", *args[2:], "-format=html",
                    f"-output-dir={dest / 'html'}"], cwd=ROOT, check=True)
    print(f"Workspace Rust execution only, including proc-macro expansion; toolchain/registry code and C/Odin consumers are excluded. HTML: {dest / 'html/index.html'}")


if __name__ == "__main__":
    main()
