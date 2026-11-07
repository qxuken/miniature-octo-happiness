#!/usr/bin/env python3
"""Run release benchmark samples and save raw CSV plus median/range summaries."""
import argparse
import csv
import io
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="New output directory")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--compact", action="store_true")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("runs must be positive")
    dest = args.output.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    subprocess.run(["cargo", "build", "--offline", "--release", "--bin", "benchmark"], cwd=ROOT, check=True)
    samples = {}
    for run in range(1, args.runs + 1):
        command = [str(ROOT / "target/release/benchmark"), str(dest / f"files-{run}")]
        if args.compact:
            command.append("--compact")
        result = subprocess.check_output(command, cwd=ROOT, text=True)
        (dest / f"run-{run}.csv").write_text(result)
        for row in csv.DictReader(io.StringIO(result)):
            samples.setdefault(row["case"], []).append(row)
    with (dest / "summary.csv").open("w", newline="") as file:
        writer = csv.writer(file)
        writer.writerow(["case", "operations_per_run", "median_ns_per_operation", "min_ns_per_operation", "max_ns_per_operation", "file_bytes"])
        for case, rows in samples.items():
            count = int(rows[0]["operations"])
            times = [int(row["elapsed_ns"]) / count if count else 0 for row in rows]
            writer.writerow([case, count, round(statistics.median(times), 2), round(min(times), 2), round(max(times), 2), rows[0]["file_bytes"]])
    print((dest / "summary.csv").read_text())


if __name__ == "__main__":
    main()
