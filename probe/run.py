#!/usr/bin/env python3
"""Build the packaging experiment on macOS; no third-party Python dependencies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
BUILD = ROOT / "build"
RESULTS = []


def run(args, *, capture=False):
    result = subprocess.run([str(a) for a in args], cwd=ROOT, text=True,
                            stdout=subprocess.PIPE if capture else None,
                            stderr=subprocess.STDOUT if capture else None)
    if result.returncode:
        if capture:
            print(result.stdout, file=sys.stderr)
        raise SystemExit(f"Command failed ({result.returncode}): {args}")
    return result.stdout.strip() if capture else ""


def consumers(label, library, *, rust_static=False):
    dest = BUILD / label
    dest.mkdir(exist_ok=True)
    # Two translation units include the public header independently.
    native = ["-framework", "Security", "-framework", "CoreFoundation", "-liconv"] if rust_static else []
    run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", ROOT / "include",
         ROOT / "consumers/contract.c", ROOT / "consumers/main.c", library,
         *native, "-o", dest / "c-consumer"])
    run([dest / "c-consumer"])
    run(["rustc", "--edition=2024", ROOT / "consumers/rust.rs", "-o", dest / "rust-consumer",
         "-C", f"link-arg={library}", *sum((["-C", f"link-arg={arg}"] for arg in native), [])])
    run([dest / "rust-consumer"])
    RESULTS.append({"case": label, "c": "pass", "rust": "pass"})


def odin_case(label, library, with_wgpu):
    name = label + ("-wgpu" if with_wgpu else "-plain")
    log = BUILD / f"{name}.log"
    binary = BUILD / f"{name}-odin"
    source = BUILD / f"{name}-source"
    source.mkdir(exist_ok=True)
    shutil.copyfile(ROOT / "consumers/odin/main.odin", source / "main.odin")
    shutil.copyfile(ROOT / "consumers/integration" / ("wgpu.odin" if with_wgpu else "plain.odin"),
                    source / "integration.odin")
    command = ["odin", "build", str(source), f"-out:{binary}",
               f"-define:PROBE_LIB={os.path.relpath(library, source)}"]
    result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log.write_text(result.stdout)
    if result.returncode:
        # A real integration result, not a reason to suppress unrelated failures.
        if label.startswith("rust-static") and with_wgpu and "duplicate symbol" in result.stdout:
            print(f"{name}: reproduced duplicate symbols (see {log.relative_to(ROOT)})", flush=True)
            RESULTS.append({"case": name, "odin": "duplicate-symbol failure", "log": log.name})
            return
        print(result.stdout, file=sys.stderr)
        raise SystemExit(f"Unexpected Odin failure: {name}")
    run([binary])
    RESULTS.append({"case": name, "odin": "pass"})


def expected_failure(name, args, marker):
    result = subprocess.run([str(a) for a in args], cwd=ROOT, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (BUILD / f"{name}.log").write_text(result.stdout)
    if result.returncode == 0 or marker not in result.stdout:
        print(result.stdout, file=sys.stderr)
        raise SystemExit(f"Expected failure was not diagnosed: {name}")
    RESULTS.append({"case": name, "diagnostic": "pass", "log": f"{name}.log"})
    print(f"{name}: diagnosed as expected", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-odin", action="store_true", help="Explicitly omit Odin integration")
    parser.add_argument("--skip-wgpu", action="store_true", help="Explicitly omit the prebuilt vendor:wgpu check")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("This first packaging probe currently tests macOS only.")
    for tool in ["cc", "ar", "rustc", "cargo"] + ([] if args.skip_odin else ["odin"]):
        if not shutil.which(tool):
            parser.error(f"Missing {tool}; install it or explicitly select the applicable skip option.")
    BUILD.mkdir(exist_ok=True)
    metadata = {"platform": platform.platform(), "machine": platform.machine(),
                "rustc": run(["rustc", "--version"], capture=True),
                "cargo": run(["cargo", "--version"], capture=True),
                "cc": run(["cc", "--version"], capture=True).splitlines()[0],
                "odin": None if args.skip_odin else run(["odin", "version"], capture=True),
                "skipped": [name for name, skip in [("odin", args.skip_odin), ("wgpu", args.skip_wgpu)] if skip]}
    # Generate from the split sources; never maintain two implementations.
    header = (ROOT / "include/weekend_probe.h").read_text()
    body = (ROOT / "c/weekend_probe.c").read_text().replace('#include "weekend_probe.h"\n', "", 1)
    amalgam = BUILD / "amalgamated"
    amalgam.mkdir(exist_ok=True)
    (amalgam / "weekend_probe.h").write_text(header +
        "\n#if defined(WEEKEND_PROBE_IMPLEMENTATION) && !defined(WEEKEND_PROBE_IMPLEMENTED)\n"
        "#define WEEKEND_PROBE_IMPLEMENTED\n" + body + "\n#endif\n")
    (amalgam / "implementation.c").write_text('#define WEEKEND_PROBE_IMPLEMENTATION\n#include "weekend_probe.h"\n')
    libraries = []
    for mode, flags in [("debug", ["-O0", "-g"]), ("release", ["-O2"])]:
        dest = BUILD / f"c-{mode}"
        dest.mkdir(exist_ok=True)
        common = ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", *flags, "-I", ROOT / "include"]
        run([*common, "-c", ROOT / "c/weekend_probe.c", "-o", dest / "probe.o"])
        run(["ar", "rcs", dest / "libweekend_probe.a", dest / "probe.o"])
        run([*common, "-dynamiclib", ROOT / "c/weekend_probe.c", "-o", dest / "libweekend_probe.dylib"])
        consumers(f"c-static-{mode}", dest / "libweekend_probe.a")
        consumers(f"c-shared-{mode}", dest / "libweekend_probe.dylib")
        libraries.extend([(f"c-static-{mode}", dest / "libweekend_probe.a"),
                          (f"c-shared-{mode}", dest / "libweekend_probe.dylib")])
        cargo_flags = ["--release"] if mode == "release" else []
        run(["cargo", "build", "--offline", "--manifest-path", ROOT / "rust/Cargo.toml", *cargo_flags])
        rust_dest = ROOT / "rust/target" / mode
        consumers(f"rust-static-{mode}", rust_dest / "libweekend_probe.a", rust_static=True)
        consumers(f"rust-shared-{mode}", rust_dest / "libweekend_probe.dylib")
        libraries.extend([(f"rust-static-{mode}", rust_dest / "libweekend_probe.a"),
                          (f"rust-shared-{mode}", rust_dest / "libweekend_probe.dylib")])
    # Compile and run the delivered single-header form with two consumer TUs.
    run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", amalgam,
         amalgam / "implementation.c", ROOT / "consumers/contract.c", ROOT / "consumers/main.c",
         "-o", amalgam / "consumer"])
    run([amalgam / "consumer"])
    RESULTS.append({"case": "c-amalgamated-two-consumers", "c": "pass"})
    # C ownership and every injected allocation failure under ASan + UBSan.
    for name, sources in [("contract", [ROOT / "c/weekend_probe.c", ROOT / "consumers/contract.c", ROOT / "consumers/main.c"]),
                          ("allocation", [ROOT / "consumers/allocation.c"])]:
        binary = BUILD / f"sanitize-{name}"
        run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-g", "-fsanitize=address,undefined",
             "-fno-omit-frame-pointer", "-I", ROOT / "include", *sources, "-o", binary])
        run([binary])
        RESULTS.append({"case": f"c-sanitize-{name}", "c": "pass"})
    binary = BUILD / "c-lifetime-mistake"
    run(["cc", "-std=c11", "-g", "-fsanitize=address", "-I", ROOT / "include",
         ROOT / "c/weekend_probe.c", ROOT / "consumers/lifetime.c", "-o", binary])
    # These programs must diagnose the intentional mistakes, not silently pass.
    expected_failure("c-lifetime-mistake", [binary], "heap-use-after-free")
    expected_failure("rust-lifetime-mistake", ["rustc", "--edition=2024", "--cfg", "lifetime_mistake",
                     ROOT / "consumers/rust.rs", "-o", BUILD / "rust-lifetime-mistake"],
                     "does not live long enough")
    if not args.skip_odin:
        for label, library in libraries:
            odin_case(label, library, False)
            if not args.skip_wgpu:
                odin_case(label, library, True)
    # Identify the actual archive tested, not just the binding version.
    if not args.skip_odin and not args.skip_wgpu:
        odin_root = Path(run(["odin", "root"], capture=True))
        arch = "aarch64" if platform.machine() == "arm64" else "x86_64"
        archive = odin_root / f"vendor/wgpu/lib/wgpu-macos-{arch}-release/lib/libwgpu_native.a"
        metadata["wgpu_archive"] = str(archive)
        metadata["wgpu_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
    (BUILD / "results.json").write_text(json.dumps({"environment": metadata, "results": RESULTS}, indent=2) + "\n")
    print(f"Recorded {len(RESULTS)} cases in {BUILD / 'results.json'}", flush=True)


if __name__ == "__main__":
    main()
