#!/usr/bin/env python3
"""Run workspace checks, macOS/Linux C consumers, and macOS Odin examples."""
import argparse
import os
import platform
from pathlib import Path
import subprocess
import tempfile
import sys

ROOT = Path(__file__).resolve().parent.parent
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
if not TARGET.is_absolute():
    TARGET = ROOT / TARGET


def run(*args):
    subprocess.run([str(a) for a in args], cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-odin", action="store_true")
    args = parser.parse_args()
    run("cargo", "fmt", "--all", "--check")
    run("cargo", "clippy", "--offline", "--workspace", "--all-features", "--all-targets", "--", "-D", "warnings")
    for mode in ["debug", "release"]:
        flags = ["--release"] if mode == "release" else []
        run("cargo", "test", "--offline", "--workspace", "--all-features", *flags)
        run("cargo", "run", "--offline", "-p", "weekend-tasks", *flags, "--", "demo")
        run("cargo", "run", "--offline", "--manifest-path",
            ROOT / "crates/weekend-db-typed/tests/renamed/Cargo.toml",
            "--target-dir", TARGET / "renamed-consumer", *flags)
        run("cargo", "run", "--offline", "--example", "memory", *flags)
        run("cargo", "run", "--offline", "--example", "raw_tasks", *flags)
        run("cargo", "run", "--offline", "--example", "simulated", *flags)
        run("cargo", "run", "--offline", "--example", "typed", *flags)
        run("cargo", "build", "--offline", "--workspace", *flags)
        dest = TARGET / mode
        filename = ("weekend_db.dll" if sys.platform == "win32" else
                    "libweekend_db.dylib" if sys.platform == "darwin" else "libweekend_db.so")
        run(sys.executable, ROOT / "tools/ffi_smoke.py", dest / filename)
        if sys.platform not in ("darwin", "linux"):
            continue
        library = dest / filename
        run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", ROOT / "include",
            ROOT / "examples/memory.c", library, f"-Wl,-rpath,{dest}", "-o", dest / "c-memory")
        run(dest / "c-memory")
        run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread", "-I", ROOT / "include",
            ROOT / "examples/threads.c", library, f"-Wl,-rpath,{dest}", "-o", dest / "c-threads")
        run(dest / "c-threads")
        if sys.platform == "linux" and platform.machine() not in ("x86_64", "aarch64"):
            continue
        run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", ROOT / "include",
            ROOT / "examples/file.c", library, f"-Wl,-rpath,{dest}", "-o", dest / "c-file")
        with tempfile.TemporaryDirectory(prefix="weekend-db-consumers-") as directory:
            run("cargo","run","--offline","--example","file",*flags,"--",Path(directory)/"rust.db")
            run("cargo","run","--offline","--example","typed",*flags,"--",Path(directory)/"typed.db")
            run(dest / "c-file",Path(directory)/"c.db")
        if sys.platform == "darwin" and not args.skip_odin:
            run("odin", "build", ROOT / "examples/odin", f"-out:{dest / 'odin-memory'}",
                f"-define:ENGINE_LIB={os.path.relpath(library, ROOT / 'examples/odin')}",
                f"-extra-linker-flags:-Wl,-rpath,{dest}")
            run(dest / "odin-memory")
            run("odin","build",ROOT/"examples/odin_file",f"-out:{dest/'odin-file'}",
                f"-define:ENGINE_LIB={os.path.relpath(library,ROOT/'examples/odin_file')}",f"-extra-linker-flags:-Wl,-rpath,{dest}")
            with tempfile.TemporaryDirectory(prefix="weekend-db-odin-") as directory:
                run(dest/"odin-file",Path(directory)/"odin.db")
    if sys.platform == "linux":
        print("Rust/C memory, file, and thread checks passed in debug and release; Odin is checked on macOS.")
    elif sys.platform != "darwin":
        print("Rust and shared-library C ABI checks passed; native file storage is unsupported here.")
    elif args.skip_odin:
        print("Rust/C checks passed; Odin was explicitly skipped.")
    else:
        print("Rust/C/Odin engine checks passed in debug and release.")


if __name__ == "__main__":
    main()
