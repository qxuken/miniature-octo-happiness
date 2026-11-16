//! Bounded release benchmark samples and deterministic CSV aggregation.
#[cfg(unix)]
use crate::{
    assurance::Campaign,
    support::{Run, path, sha},
};
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::{
    delivery::{read_json_hashed, reparse},
    source_inventory::snapshot,
};
use anyhow::{Context, Result, ensure};
#[cfg(unix)]
use clap::Args;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use serde_json::Value;
use serde_json::json;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::path::PathBuf;
use std::{collections::BTreeSet, fs};
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use std::{io::Read, path::Path};

#[cfg(unix)]
#[derive(Args)]
pub struct Options {
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value_t=3, value_parser=clap::value_parser!(u16).range(1..=100))]
    pub runs: u16,
    #[arg(long)]
    pub compact: bool,
}
const SCOPE: &str = "small release benchmark; raw samples and median/range, no service guarantee";
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
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn retained_text(root: &Path, name: &str) -> Result<(String, String)> {
    const LIMIT: u64 = 1024 * 1024;
    let file = root.join(name);
    let metadata = fs::symlink_metadata(&file)?;
    ensure!(
        metadata.is_file() && !reparse(&metadata) && metadata.len() <= LIMIT,
        "nonregular or oversized benchmark file {name}"
    );
    let mut contents = String::new();
    fs::File::open(&file)?
        .take(LIMIT + 1)
        .read_to_string(&mut contents)?;
    ensure!(
        contents.len() as u64 <= LIMIT,
        "benchmark file {name} grew beyond its limit"
    );
    let hash = format!("{:x}", Sha256::digest(contents.as_bytes()));
    Ok((contents, hash))
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn successful_command<'a>(commands: &'a [Value], name: &str) -> Result<&'a [Value]> {
    let mut matching = commands.iter().filter(|row| row["name"] == name);
    let command = matching
        .next()
        .with_context(|| format!("missing benchmark command {name}"))?;
    let supervisor = &command["supervision"];
    ensure!(
        matching.next().is_none()
            && command["stdout"] == format!("{name}.stdout.log")
            && command["passed"] == true
            && command["exit_code"] == 0
            && command["invocation_error"].is_null()
            && supervisor["returncode"] == 0
            && supervisor["cleanup_verified"] == true
            && supervisor["failure"].is_null()
            && supervisor["cleanup_failure"].is_null()
            && supervisor["timed_out"] == false
            && supervisor["cancelled"] == false,
        "unsuccessful or ambiguous benchmark command {name}"
    );
    command["argv"]
        .as_array()
        .map(Vec::as_slice)
        .context("missing benchmark command arguments")
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn verify(root: &Path, source: &str, tree: &Path) -> Result<Value> {
    let metadata = fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir() && !reparse(&metadata),
        "benchmark evidence directory alias"
    );
    let (report, report_hash) = read_json_hashed(&root.join("evidence.json"))?;
    ensure!(
        report["scope"] == SCOPE
            && report["source"] == source
            && report["git_identity_available"] == true
            && report["dirty"] == false
            && report["passed"] == true
            && report["cancelled"] == false,
        "benchmark source, clean checkout or pass status mismatch"
    );
    let expected = json!(snapshot(tree)?);
    ensure!(
        report["source_hashes"] == expected
            && expected["Cargo.lock"].is_string()
            && expected["tools/benchmark.rs"].is_string()
            && report["benchmark_source_sha256"] == expected["tools/benchmark.rs"],
        "benchmark inputs differ from the Git archive"
    );
    let compact = report["compact"]
        .as_bool()
        .context("missing benchmark compaction setting")?;
    let runs = report["requested_runs"]
        .as_u64()
        .filter(|count| (1..=100).contains(count))
        .context("invalid benchmark sample count")? as usize;
    let hashes = report["raw_csv_sha256"]
        .as_array()
        .context("missing benchmark sample hashes")?;
    ensure!(hashes.len() == runs, "benchmark sample count mismatch");
    let commands = report["commands"]
        .as_array()
        .context("missing benchmark commands")?;
    let build = successful_command(commands, "build")?;
    let [prefix @ .., target] = build else {
        anyhow::bail!("missing benchmark build target")
    };
    let build_prefix = [
        "cargo",
        "build",
        "--offline",
        "--locked",
        "--release",
        "--bin",
        "benchmark",
        "--target-dir",
    ];
    ensure!(
        prefix.iter().eq(build_prefix.iter()),
        "benchmark used a different build command"
    );
    let target = target
        .as_str()
        .filter(|path| !path.is_empty())
        .context("invalid benchmark build target")?;
    let binary = format!("{}/release/benchmark", target.trim_end_matches('/'));
    let mut samples = Vec::with_capacity(runs);
    for (index, expected_hash) in hashes.iter().enumerate() {
        let sample = index + 1;
        let name = format!("sample-{sample}");
        let args = successful_command(commands, &name)?;
        let compact_flag = if compact { &["--compact"][..] } else { &[][..] };
        ensure!(
            matches!(args, [program, output, flags @ ..]
                if program == &binary
                    && output.as_str().and_then(|path| path.rsplit('/').next())
                        == Some(&format!("files-{sample}"))
                    && flags.iter().eq(compact_flag.iter())),
            "benchmark sample {sample} used different arguments"
        );
        let (csv, hash) = retained_text(root, &format!("run-{sample}.csv"))?;
        let (stdout, _) = retained_text(root, &format!("{name}.stdout.log"))?;
        ensure!(
            *expected_hash == hash && stdout == csv,
            "benchmark sample {sample} digest or transcript mismatch"
        );
        let rows = parse(&csv)?;
        ensure!(
            rows.iter().any(|row| row.name == "file_compaction") == compact,
            "benchmark sample {sample} compaction case mismatch"
        );
        samples.push(rows);
    }
    let (summary, summary_hash) = retained_text(root, "summary.csv")?;
    ensure!(
        report["summary_sha256"] == summary_hash && summary == summarize(&samples)?,
        "benchmark summary digest or aggregation mismatch"
    );
    Ok(json!({
        "evidence_sha256": report_hash,
        "raw_csv_sha256": hashes,
        "summary_sha256": summary_hash,
        "runs": runs,
        "compact": compact
    }))
}

#[cfg(unix)]
pub fn run(run: &Run, opts: Options) -> Result<()> {
    let mut c = Campaign::new(run, &opts.output, SCOPE)?;
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

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn delivery_requires_matching_benchmark_inputs_and_exact_sample_aggregation() {
        use crate::support::Scratch;

        const SOURCE: &str = "0123456789012345678901234567890123456789";
        let mut scratch = Scratch::new(&std::env::temp_dir(), "benchmark-proof").unwrap();
        let tree = scratch.path().join("source");
        fs::create_dir_all(tree.join("tools")).unwrap();
        fs::write(tree.join("tools/benchmark.rs"), "benchmark code").unwrap();
        fs::write(tree.join("Cargo.lock"), "workspace lock").unwrap();
        let evidence = scratch.path().join("proof");
        fs::create_dir(&evidence).unwrap();
        let raw = format!("{HEADER}\npoint,2,8,0\n");
        let summary = format!("{SUMMARY}point,2,4.0,4.0,4.0,0\n");
        for (name, contents) in [
            ("run-1.csv", &raw),
            ("sample-1.stdout.log", &raw),
            ("summary.csv", &summary),
        ] {
            fs::write(evidence.join(name), contents).unwrap();
        }
        let hash = |name| retained_text(&evidence, name).unwrap().1;
        let command = |name, args| {
            json!({"name":name,"argv":args,"stdout":format!("{name}.stdout.log"),
            "passed":true,"exit_code":0,"supervision":{
                "returncode":0,"cleanup_verified":true,"timed_out":false,"cancelled":false
            }})
        };
        let inputs = json!(snapshot(&tree).unwrap());
        let mut report = json!({
            "scope":SCOPE,"source":SOURCE,"git_identity_available":true,"passed":true,
            "dirty":false,"cancelled":false,"source_hashes":inputs,
            "benchmark_source_sha256":inputs["tools/benchmark.rs"],
            "requested_runs":1,"compact":false,"raw_csv_sha256":[hash("run-1.csv")],
            "summary_sha256":hash("summary.csv"),"commands":[
                command("build",json!(["cargo","build","--offline","--locked","--release","--bin","benchmark","--target-dir","/build"])),
                command("sample-1",json!(["/build/release/benchmark","/output/files-1"]))
            ]
        });
        let write_report = |record: &Value| {
            fs::write(evidence.join("evidence.json"), record.to_string()).unwrap();
        };
        let check = |source| verify(&evidence, source, &tree);
        write_report(&report);
        assert_eq!(check(SOURCE).unwrap()["runs"], 1);
        assert!(
            check("different source")
                .unwrap_err()
                .to_string()
                .contains("source")
        );

        let mut wrong_inputs = report.clone();
        wrong_inputs["source_hashes"]["Cargo.lock"] = json!("substituted");
        write_report(&wrong_inputs);
        assert!(check(SOURCE).unwrap_err().to_string().contains("inputs"));

        let mut dirty = report.clone();
        dirty["dirty"] = json!(true);
        write_report(&dirty);
        assert!(
            check(SOURCE)
                .unwrap_err()
                .to_string()
                .contains("clean checkout")
        );

        write_report(&report);
        fs::write(evidence.join("sample-1.stdout.log"), &summary).unwrap();
        assert!(
            check(SOURCE)
                .unwrap_err()
                .to_string()
                .contains("transcript")
        );
        fs::write(evidence.join("sample-1.stdout.log"), &raw).unwrap();

        fs::write(
            evidence.join("summary.csv"),
            format!("{SUMMARY}point,2,5.0,4.0,4.0,0\n"),
        )
        .unwrap();
        report["summary_sha256"] = json!(hash("summary.csv"));
        write_report(&report);
        assert!(
            check(SOURCE)
                .unwrap_err()
                .to_string()
                .contains("aggregation")
        );
        scratch.close().unwrap();
    }

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
