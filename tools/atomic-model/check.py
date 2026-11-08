#!/usr/bin/env python3
"""Run bounded ownership/budget models and two fail-required budget controls."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
OUTPUT = ROOT.parent.parent / "target/atomic-model-evidence"


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    files = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", *sorted((ROOT / "src").glob("*.rs"))]
    hashes = lambda: {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    production = ROOT.parent.parent / "src/memory.rs"
    production_hash = lambda: hashlib.sha256(production.read_bytes()).hexdigest()
    evidence = {"source_hashes": hashes(), "production_memory_sha256": production_hash(),
                "runs": [], "passed": False}
    env = {**os.environ, "CARGO_INCREMENTAL": "0", "CARGO_TARGET_DIR": str(OUTPUT / "build")}
    toolchain = ["+" + os.environ["MODEL_TOOLCHAIN"]] if os.environ.get("MODEL_TOOLCHAIN") else []
    try:
        evidence["rustc"] = subprocess.check_output(["rustc", *toolchain, "-vV"], text=True)
        for binary, mode, expected in [
            ("weekend-atomic-model", "", None), ("budget", "", None),
            ("budget", "--broken-admission", "no released reservation may be forgotten"),
            ("budget", "--broken-sampling", "observed peak must not decrease"),
        ]:
            command = ["cargo", *toolchain, "run", "--locked", "--manifest-path", str(ROOT / "Cargo.toml"),
                       "--bin", binary, "--", *([mode] if mode else [])]
            process = subprocess.run(command, env=env, capture_output=True, text=True, timeout=600)
            log = OUTPUT / (binary + (mode or "-baseline") + ".log")
            log.write_text(process.stdout + process.stderr)
            passed = ((process.returncode == 0) if expected is None else
                      (process.returncode != 0 and expected in process.stderr))
            evidence["runs"].append({"command": command, "exit_code": process.returncode,
                                      "log": log.name, "expected_assertion": expected, "passed": passed})
            if not passed:
                raise RuntimeError(f"Model/control failed: {log}")
        if evidence["source_hashes"] != hashes() or evidence["production_memory_sha256"] != production_hash():
            raise RuntimeError("Model source changed during execution")
        evidence["passed"] = True
        print("Both bounded models passed and both budget mutants were detected")
    finally:
        (OUTPUT / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
