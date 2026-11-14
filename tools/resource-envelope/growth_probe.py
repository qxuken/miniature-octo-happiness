#!/usr/bin/env python3
"""Disposable allocation-work/Memory-domain instrumentation, not a latency probe."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shutil

SPEC = importlib.util.spec_from_file_location("growth_compound", Path(__file__).with_name("compound.py"))
COMPOUND = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPOUND)
NEEDLE = "self.charge = Some(charge); // release old charge only after successful realloc"
REPLACEMENT = """VEC_GROWS.fetch_add(1, Ordering::Relaxed);
        VEC_OLD_BYTES.fetch_add((capacity * std::mem::size_of::<T>()) as u64, Ordering::Relaxed);
        VEC_NEW_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        """ + NEEDLE
SUFFIX = '''
// Disposable measurement only: fields/layout/allocation policy are unchanged.
static VEC_GROWS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static VEC_OLD_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static VEC_NEW_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
impl Drop for MemoryState {
    fn drop(&mut self) {
        eprintln!("GROWTH_DOMAIN cap={} peak={} used={} grows={} old={} new={}",
            self.limits.max_live_bytes, self.peak.load(Ordering::Relaxed), self.used.load(Ordering::Relaxed),
            VEC_GROWS.load(Ordering::Relaxed), VEC_OLD_BYTES.load(Ordering::Relaxed), VEC_NEW_BYTES.load(Ordering::Relaxed));
    }
}
'''


def instrument(source):
    path = Path(source) / "src/memory.rs"
    text = path.read_text()
    COMPOUND.require("GROWTH_DOMAIN" not in text and text.count(NEEDLE) == 1, "unique uninstrumented allocator required")
    path.write_text(text.replace(NEEDLE, REPLACEMENT) + SUFFIX)


def parse(text):
    pattern = r"GROWTH_DOMAIN cap=(\d+) peak=(\d+) used=(\d+) grows=(\d+) old=(\d+) new=(\d+)"
    lines = [line for line in text.splitlines() if line.startswith("GROWTH_DOMAIN")]
    COMPOUND.require(all(re.fullmatch(pattern, line) for line in lines), "malformed allocation instrumentation")
    records = [dict(zip(["cap", "peak", "used", "grows", "old", "new"], map(int, row)))
               for row in re.findall("^" + pattern + "$", text, re.M)]
    COMPOUND.require(bool(records), "missing allocation instrumentation")
    COMPOUND.require(all(r["used"] == 0 and r["peak"] <= r["cap"] for r in records), "domain ownership/admission invalid")
    for key in ["grows", "old", "new"]:
        COMPOUND.require([r[key] for r in records] == sorted(r[key] for r in records), "nonmonotonic allocation totals")
    return records


def validate_reference(reference, revision, source_hashes):
    COMPOUND.require(reference["verified"] is True, "unverified clean reference")
    COMPOUND.require(reference["revision"] == revision, "different clean source ID")
    COMPOUND.require(reference["source_hashes"] == source_hashes, "different clean source files")
    COMPOUND.require(set(reference["runs"]) == {"native", "inspect", "faults"}, "incomplete clean profiles")
    for profile, run in reference["runs"].items():
        COMPOUND.require(run["passed"] is True and run["owned_live_processes_absent"] is True, "unclean reference worker")
        COMPOUND.check_rows(run["rows"], profile, revision)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--output", required=True)
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    reference = json.loads(args.reference.read_text())
    validate_reference(reference, args.revision, COMPOUND.SCALE.inventory(args.source))
    output = COMPOUND.BASE.HELPERS.fresh_output(args.output)
    original = args.source.resolve()
    COMPOUND.require(not output.is_relative_to(original) and not original.is_relative_to(output), "source/output overlap")
    source = output / "source"; shutil.copytree(original, source)
    instrument(source)
    COMPOUND.worker(source, output / "run", args.revision + "+growth-probe")
    measured = json.loads((output / "run/evidence.json").read_text())
    summary = {"verified": False, "scope": "instrumented allocation work only; do not use timings/RSS", "profiles": {}}
    for profile, run in measured["runs"].items():
        other = reference["runs"][profile]
        for actual, expected in zip(run["rows"][:-1], other["rows"][:-1]):
            for key in ["phase", "revision", "seq", "changed", "roots", "digests", "live_bytes", "file_bytes"]:
                COMPOUND.require(actual[key] == expected[key], "instrumentation changed state/layout: " + key)
        records = parse((output / "run" / (profile + ".stderr")).read_text())
        summary["profiles"][profile] = {"domains": records, "total_grows": records[-1]["grows"],
            "old_layout_bytes": records[-1]["old"], "new_layout_bytes": records[-1]["new"],
            "copy_domain_peaks": [r["peak"] for r in records if r["cap"] == 256 * 1024 * 1024]}
    summary.update(verified=True, source_hashes=measured["source_hashes"], binary_sha256=measured["binary_sha256"],
                   run_sha256=COMPOUND.SCALE.sha(output / "run/evidence.json"), reference_sha256=COMPOUND.SCALE.sha(args.reference))
    (output / "growth.json").write_text(json.dumps(summary, indent=2) + "\n")


if __name__ == "__main__": main()
