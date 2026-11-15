//! Bounded release benchmark samples and deterministic CSV aggregation.
use crate::{
    assurance::Campaign,
    support::{Run, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::json;
use std::{collections::BTreeSet, fs, path::PathBuf};

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value_t=3, value_parser=clap::value_parser!(u16).range(1..=100))]
    pub runs: u16,
    #[arg(long)]
    pub compact: bool,
}
const HEADER: &str = "case,operations,elapsed_ns,file_bytes";
const SUMMARY: &str = "case,operations_per_run,median_ns_per_operation,min_ns_per_operation,max_ns_per_operation,file_bytes\n";
#[derive(Debug, PartialEq, Eq)]
struct Row {
    name: String,
    operations: u64,
    elapsed: u128,
    bytes: u64,
}
fn parse(raw: &str) -> Result<Vec<Row>> {
    let mut lines = raw.lines();
    ensure!(lines.next() == Some(HEADER), "invalid benchmark CSV header");
    let mut rows = vec![];
    let mut names = BTreeSet::new();
    for line in lines {
        ensure!(rows.len() < 100, "too many benchmark rows");
        let fields = line.split(',').collect::<Vec<_>>();
        let [name, operations, elapsed, bytes] = fields.as_slice() else {
            anyhow::bail!("malformed benchmark row")
        };
        ensure!(
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && names.insert(*name),
            "invalid or duplicate benchmark case"
        );
        let row = Row {
            name: (*name).into(),
            operations: operations.parse()?,
            elapsed: elapsed.parse()?,
            bytes: bytes.parse()?,
        };
        ensure!(
            row.operations != 0 || row.elapsed == 0,
            "elapsed time for non-timed benchmark case"
        );
        rows.push(row);
    }
    ensure!(!rows.is_empty(), "no benchmark cases");
    Ok(rows)
}
fn number(n: f64) -> String {
    // Match the historical human-readable CSV: two-place rounding, trim
    // redundant fractional zeros but keep .0 for whole nonzero measurements.
    let s = format!("{n:.2}");
    let s = s.trim_end_matches('0');
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.into()
    }
}
fn summarize(runs: &[Vec<Row>]) -> Result<String> {
    let first = runs.first().context("missing benchmark samples")?;
    ensure!(
        runs.iter().all(|r| r.len() == first.len()),
        "benchmark case count changed between runs"
    );
    let mut summary = String::from(SUMMARY);
    use std::fmt::Write;
    for (i, base) in first.iter().enumerate() {
        let mut samples = vec![];
        for run in runs {
            let row = &run[i];
            ensure!(
                row.name == base.name
                    && row.operations == base.operations
                    && row.bytes == base.bytes,
                "benchmark case/count/size drift: {}",
                base.name
            );
            if row.operations != 0 {
                samples.push(row.elapsed as f64 / row.operations as f64);
            }
        }
        let values = if samples.is_empty() {
            ["0".into(), "0".into(), "0".into()]
        } else {
            samples.sort_by(f64::total_cmp);
            let middle = samples.len() / 2;
            let median = if samples.len().is_multiple_of(2) {
                (samples[middle - 1] + samples[middle]) / 2.
            } else {
                samples[middle]
            };
            [
                number(median),
                number(samples[0]),
                number(*samples.last().unwrap()),
            ]
        };
        writeln!(
            summary,
            "{},{},{},{},{},{}",
            base.name, base.operations, values[0], values[1], values[2], base.bytes
        )?;
    }
    Ok(summary)
}
pub fn run(run: &Run, opts: Options) -> Result<()> {
    let mut c = Campaign::new(
        run,
        &opts.output,
        "small release benchmark; raw samples and median/range, no service guarantee",
    )?;
    c.evidence["requested_runs"] = json!(opts.runs);
    c.evidence["compact"] = json!(opts.compact);
    let target = run.target();
    let result = (|| -> Result<()> {
        c.command(
            vec![
                "cargo".into(),
                "build".into(),
                "--offline".into(),
                "--locked".into(),
                "--release".into(),
                "--bin".into(),
                "benchmark".into(),
                "--target-dir".into(),
                path(&target),
            ],
            "build",
            None,
            300.,
        )?;
        let binary = target.join("release/benchmark");
        let digest = sha(&binary)?;
        let source = run.root.join("tools/benchmark.rs");
        let source_hash = sha(&source)?;
        let mut samples = vec![];
        let mut raw_hashes = vec![];
        for i in 1..=opts.runs {
            let label = format!("sample-{i}");
            let mut args = vec![path(&binary), path(c.output.join(format!("files-{i}")))];
            if opts.compact {
                args.push("--compact".into());
            }
            c.command(args, &label, None, 180.)?;
            let raw = c.output.join(format!("run-{i}.csv"));
            fs::copy(c.output.join(format!("{label}.stdout.log")), &raw)?;
            let rows = parse(&fs::read_to_string(&raw)?)?;
            ensure!(
                rows.iter().any(|r| r.name == "file_compaction") == opts.compact,
                "requested compaction case missing or unexpected"
            );
            samples.push(rows);
            raw_hashes.push(sha(&raw)?);
        }
        let summary = summarize(&samples)?;
        let file = c.output.join("summary.csv");
        fs::write(&file, &summary)?;
        ensure!(
            sha(&binary)? == digest && sha(&source)? == source_hash,
            "benchmark input changed"
        );
        c.evidence["benchmark_binary_sha256"] = json!(digest);
        c.evidence["benchmark_source_sha256"] = json!(source_hash);
        c.evidence["raw_csv_sha256"] = json!(raw_hashes);
        c.evidence["summary_sha256"] = json!(sha(&file)?);
        print!("{summary}");
        Ok(())
    })();
    c.finish(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggregation_preserves_raw_case_order_median_range_and_image_size() {
        let runs = [10, 28, 4, 22].map(|elapsed| {
            parse(&format!("{HEADER}\npoint,2,{elapsed},0\nimage,0,0,128\n")).unwrap()
        });
        assert_eq!(
            summarize(&runs).unwrap(),
            format!("{SUMMARY}point,2,8.0,2.0,14.0,0\nimage,0,0,0,0,128\n")
        );
        assert_eq!(
            summarize(&runs[..3]).unwrap(),
            format!("{SUMMARY}point,2,5.0,2.0,14.0,0\nimage,0,0,0,0,128\n")
        );
    }
    #[test]
    fn malformed_incomplete_and_inconsistent_samples_reject() {
        for body in [
            "",
            "case,1,2",
            "case,0,9,1",
            "case,1,2,0\ncase,1,3,0",
            "bad-case,1,2,0",
        ] {
            assert!(parse(&format!("{HEADER}\n{body}\n")).is_err());
        }
        let base = parse(&format!("{HEADER}\ncase,2,8,0\n")).unwrap();
        for body in [
            "other,2,4,0",
            "case,3,4,0",
            "case,2,4,1",
            "case,2,4,0\nextra,2,4,0",
        ] {
            let changed = parse(&format!("{HEADER}\n{body}\n")).unwrap();
            assert!(
                summarize(&[parse(&format!("{HEADER}\ncase,2,8,0\n")).unwrap(), changed]).is_err()
            );
        }
        assert!(summarize(&[base]).unwrap().contains("case,2,4.0,4.0,4.0,0"));
    }
}
