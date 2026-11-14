#!/usr/bin/env python3
"""Calibrate ASan on both sides of the ABI, then run compiled C consumers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source_hashes():
    paths = [ROOT / name for name in ["Cargo.toml", "Cargo.lock", "build.rs",
                                     "include/weekend_db.h", "tools/c_sanitizers.py"]]
    for directory in ["src", "crates"]:
        paths.extend((ROOT / directory).rglob("*.rs"))
        paths.extend((ROOT / directory).rglob("Cargo.toml"))
    paths.extend((ROOT / "examples").glob("*.c"))
    return {str(p.relative_to(ROOT)): digest(p) for p in sorted(set(paths)) if p.is_file()}


def environment(target_dir, flags):
    env = dict(os.environ)
    for key in ["RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTC", "RUSTDOC",
                "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "LSAN_OPTIONS"]:
        env.pop(key, None)
    env.update(CARGO_TARGET_DIR=str(target_dir), CARGO_INCREMENTAL="0",
               CARGO_ENCODED_RUSTFLAGS="\x1f".join(flags),
               ASAN_OPTIONS="halt_on_error=1:abort_on_error=0:exitcode=86:detect_leaks="
                            + ("1" if sys.platform == "linux" else "0"))
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/c-sanitizer-evidence")
    parser.add_argument("--cc", default="clang")
    args = parser.parse_args()
    if sys.platform not in ("darwin", "linux"):
        parser.error("This profile currently supports native macOS and Linux only")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before = source_hashes()
    evidence = {"passed": False, "source_hashes": before, "commands": [],
                "platform": platform.platform(), "calibration": [], "consumers": [],
                "scope": "ASan diagnostic build, C memory/thread/file consumers; not a release artifact"}
    env = os.environ.copy()

    def run(command, name, expected_error=None):
        command = [str(part) for part in command]
        process = subprocess.run(command, cwd=ROOT, env=env, capture_output=True,
                                 text=True, errors="replace", timeout=600)
        (output / (name + ".log")).write_text(process.stdout + process.stderr)
        evidence["commands"].append({"argv": command, "exit_code": process.returncode,
                                     "log": name + ".log"})
        if expected_error is not None:
            if process.returncode == 0 or expected_error not in process.stderr:
                raise RuntimeError(f"{name}: sanitizer failed to detect the deliberate control")
        elif process.returncode != 0:
            raise RuntimeError(f"{name} failed; see {output / (name + '.log')}")
        return process.stdout.strip()

    try:
        cc = shutil.which(args.cc)
        if cc is None:
            raise RuntimeError(f"C compiler unavailable: {args.cc}")
        evidence["clang"] = run([cc, "--version"], "clang-version")
        if "clang" not in evidence["clang"].lower():
            raise RuntimeError("This profile requires Clang's shared ASan runtime")
        evidence["rustc"] = run(["rustc", "+nightly", "-vV"], "rustc-version")
        target = next(line.split(": ", 1)[1] for line in evidence["rustc"].splitlines()
                      if line.startswith("host:"))
        architecture = target.split("-", 1)[0]
        runtime_name = ("libclang_rt.asan_osx_dynamic.dylib" if sys.platform == "darwin"
                        else f"libclang_rt.asan-{architecture}.so")
        runtime = Path(run([cc, "-print-file-name=" + runtime_name], "asan-runtime")).resolve()
        if not runtime.is_file():
            raise RuntimeError(f"Shared ASan runtime unavailable: {runtime}")
        evidence["runtime"] = {"path": str(runtime), "sha256": digest(runtime)}
        # rustc suppresses the linker's default runtimes. Link the exact shared
        # runtime also selected by Clang, avoiding two independent ASan runtimes.
        flags = ["-Zsanitizer=address", "-Zexternal-clangrt", "-C", "link-arg=" + str(runtime)]
        env = environment(output / "build", flags)
        evidence["rust_flags"] = flags
        evidence["asan_options"] = env["ASAN_OPTIONS"]
        cflags = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-g", "-O1",
                  "-fsanitize=address", "-fno-omit-frame-pointer"]
        if sys.platform == "linux":
            cflags += ["-shared-libasan"]
        cflags += ["-Wl,-rpath," + str(runtime.parent)]
        extension = "dylib" if sys.platform == "darwin" else "so"
        calibration_rs = output / "calibration.rs"
        calibration_rs.write_text('''#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn calibrated_read(p: *const u8, index: usize) -> u8 {
    unsafe { p.add(index).read_volatile() }
}
''')
        calibration_lib = output / ("libcalibration." + extension)
        run(["rustc", "+nightly", "--edition=2024", "--crate-type=cdylib", *flags,
             calibration_rs, "-o", calibration_lib], "calibration-rust-build")
        calibration_c = output / "calibration.c"
        calibration_c.write_text('''#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
extern uint8_t calibrated_read(const uint8_t *, size_t);
int main(int argc, char **argv) {
    assert(argc == 2);
    uint8_t *p = malloc(8);
    assert(p != NULL);
    for (size_t i = 0; i < 8; ++i) p[i] = 0;
    volatile size_t index = strstr(argv[1], "invalid") == NULL ? 7 : 8;
    volatile uint8_t result = argv[1][0] == 'c' ? p[index] : calibrated_read(p, index);
    free(p);
    return result;
}
''')
        calibration = output / "calibration"
        run([cc, *cflags, calibration_c, calibration_lib, "-Wl,-rpath," + str(output),
             "-o", calibration], "calibration-c-build")
        for mode in ["c-valid", "rust-valid", "c-invalid", "rust-invalid"]:
            expected = "AddressSanitizer: heap-buffer-overflow" if "invalid" in mode else None
            run([calibration, mode], "calibration-" + mode, expected)
            evidence["calibration"].append({"mode": mode, "passed": True})
        run(["cargo", "+nightly", "build", "--locked", "-Zbuild-std", "--lib",
             "--features", "crdt,replication", "--target", target], "rust-library-build")
        library = output / "build" / target / "debug" / ("libweekend_db." + extension)
        evidence["library"] = {"path": str(library), "sha256": digest(library)}
        for name in ["memory", "threads", "file"]:
            binary = output / ("c-" + name)
            run([cc, *cflags, "-pthread", "-I", ROOT / "include", ROOT / f"examples/{name}.c",
                 library, "-Wl,-rpath," + str(library.parent), "-o", binary], name + "-build")
            command = [binary] + ([output / "file.wdb"] if name == "file" else [])
            run(command, name + "-run")
            evidence["consumers"].append({"name": name, "sha256": digest(binary), "passed": True})
        if source_hashes() != before:
            raise RuntimeError("Source changed during the run; evidence rejected")
        evidence["passed"] = True
        print(f"C/Rust ASan calibration and three consumers passed: {output}")
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
