//! Bounded, alternating-order comparison of separately built engine fixtures.
#[path = "../../tools/sqlite-baseline/src/workload.rs"]
mod workload;

use crate::{
    artifact_trace::inventory,
    assurance::{self, Campaign},
    delivery, serialization_scale,
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
use workload::{Expected, Work};

const HARNESS: &str = "tools/sqlite-baseline";
const TIMINGS: [&str; 5] = [
    "commit_ns",
    "transaction_ns",
    "pin_ns",
    "reopen_ns",
    "reopen_verify_ns",
];

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    revision: String,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 64)]
    commits: u64,
    #[arg(long, default_value_t = 32)]
    keys: u64,
    #[arg(long, default_value_t = 4)]
    batch: u64,
    #[arg(long, default_value_t = 256)]
    payload: u64,
    #[arg(long, default_value_t = 16)]
    retain_every: u64,
    #[arg(long, default_value_t = 32)]
    reopens: u64,
    #[arg(long, default_value_t = 3)]
    rounds: usize,
    /// Sample the resident memory of each workload process during its round.
    #[arg(long)]
    sample_rss: bool,
}

fn admit(run: &Run, opts: &Options) -> Result<(PathBuf, Work)> {
    let work = Work {
        commits: opts.commits,
        keys: opts.keys,
        batch: opts.batch,
        payload: opts.payload,
        retain_every: opts.retain_every,
        reopens: opts.reopens,
    };
    work.validate()?;
    ensure!(
        (1..=8).contains(&opts.rounds),
        "workload exceeds the bounded supported range"
    );
    Ok((assurance::fresh_output(&run.root, &opts.output)?, work))
}

fn check_round(row: &Value, work: Work, expected: &Expected, engine: &str) -> Result<()> {
    ensure!(
        row["oracle_digest"].as_u64() == Some(expected.digest)
            && row["history_count"].as_u64() == Some(expected.retained.len() as u64),
        "{engine} state digest or retained count mismatch"
    );
    for (name, count) in TIMINGS.into_iter().zip([
        work.commits,
        work.commits,
        expected.retained.len() as u64,
        work.reopens,
        work.reopens,
    ]) {
        ensure!(
            row[name].as_array().is_some_and(|samples| {
                samples.len() as u64 == count && samples.iter().all(|x| x.is_u64())
            }),
            "{engine} incomplete or malformed {name}"
        );
    }
    for name in ["backup_ns", "restore_ns", "compact_ns"] {
        ensure!(row[name].is_u64(), "{engine} missing {name}");
    }
    if engine == "sqlite" {
        ensure!(
            row["checkpoint_result"] == json!([0, 0, 0])
                && row["settings"]
                    == json!({"journal_mode":"wal","synchronous":2,"fullfsync":1,
                      "checkpoint_fullfsync":1,"wal_autocheckpoint":1000,"cache_size":-2000,
                      "mmap_size":0,"busy_timeout":5000,"page_size":4096})
                && row["compile_options"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty() && v.iter().all(Value::is_string))
                && ["sqlite_source_id", "sqlite_version"]
                    .iter()
                    .all(|name| row[*name].as_str().is_some_and(|s| !s.is_empty())),
            "SQLite durability settings or runtime identity missing"
        );
    }
    Ok(())
}

fn summarize(rounds: &[Value]) -> Result<Value> {
    let mut summary = json!({});
    for engine in ["weekend", "sqlite"] {
        for name in TIMINGS {
            let mut samples = Vec::new();
            for round in rounds {
                for sample in round[engine][name]
                    .as_array()
                    .context("missing baseline samples")?
                {
                    samples.push(sample.as_u64().context("malformed baseline sample")?);
                }
            }
            ensure!(!samples.is_empty(), "missing baseline sample");
            samples.sort_unstable();
            summary[engine][name] = json!({
                "samples":samples.len(),"p50_ns":samples[(50 * samples.len()).div_ceil(100) - 1],
                "p95_ns":samples[(95 * samples.len()).div_ceil(100) - 1],
                "p99_ns":samples[(99 * samples.len()).div_ceil(100) - 1],
            });
        }
    }
    Ok(summary)
}

pub fn run(run: &Run, opts: Options) -> Result<()> {
    let (output, work) = admit(run, &opts)?;
    let expected = work.expected();
    let mut c = Campaign::new(run, &output, "private durable history comparison")?;
    c.evidence["verified"] = json!(false);
    c.evidence["rounds"] = json!([]);
    c.evidence["workload"] = json!({
        "commits":work.commits, "keys":work.keys, "batch":work.batch, "payload":work.payload,
        "retain_every":work.retain_every, "reopens":work.reopens,
    });
    c.evidence["oracle_digest"] = json!(expected.digest);
    c.evidence["oracle_counts"] =
        json!({"current_keys":expected.current.len(),"retained_roots":expected.retained.len()});
    c.evidence["quantile_method"] = json!("nearest-rank, pooled raw samples");
    c.evidence["cache_policy"] =
        json!("warm OS cache; no cache eviction; fresh database each round");
    c.save()?;

    let result = (|| -> Result<()> {
        let (revision, source) = assurance::archive_source(&mut c, &opts.revision)?;
        let harness = source.join(HARNESS);
        let mut overlay = BTreeMap::new();
        // The driver can predate the engine under test. Bind this explicit
        // overlay independently of the archived commits and their sources.
        for file in [
            "Cargo.toml",
            "Cargo.lock",
            "src/main.rs",
            "src/sqlite.rs",
            "src/workload.rs",
        ] {
            let relative = Path::new(HARNESS).join(file);
            let dest = source.join(&relative);
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::copy(run.root.join(&relative), &dest)?;
            overlay.insert(relative, sha(&dest)?);
        }
        let source_hashes = inventory(&source, &[])?;
        c.evidence["engine_source_sha256"] = json!(source_hashes);
        c.evidence["harness_overlay"] = json!(overlay);
        c.env = serialization_scale::clean_env(&c.env);
        c.env.insert("WEEKEND_DB_SOURCE_ID".into(), revision.into());
        c.evidence["rustc"] = json!(c.command(cmd(["rustc", "-vV"]), "rustc", None, 30.)?);
        c.evidence["platform"] = json!(c.command(cmd(["uname", "-a"]), "platform", None, 30.)?);
        c.evidence["cargo_configuration"] = toolchain::configuration(&source, &c.env)?;
        let target = output.join("build");
        let mut binaries = BTreeMap::new();
        for (name, features, binary) in [
            ("weekend", "weekend-engine", "weekend-sqlite-baseline"),
            ("sqlite", "sqlite-comparator", "sqlite-comparator"),
        ] {
            let mut args = cmd([
                "cargo",
                "build",
                "--offline",
                "--locked",
                "--release",
                "--no-default-features",
                "--features",
                features,
                "--bin",
                binary,
            ]);
            args.extend([
                "--manifest-path".into(),
                path(harness.join("Cargo.toml")),
                "--target-dir".into(),
                path(&target),
            ]);
            c.command_at(args, &format!("build-{name}"), None, 180., &source)?;
            let binary = target.join("release").join(binary);
            let hash = sha(&binary)?;
            c.evidence["binaries"][name] = json!({"sha256":hash,"path":binary});
            binaries.insert(name, (binary, hash));
        }
        let sqlite_binary = &binaries["sqlite"].0;
        let mut inspect = if cfg!(target_os = "macos") {
            cmd(["otool", "-L"])
        } else {
            cmd(["ldd"])
        };
        inspect.push(path(sqlite_binary));
        c.evidence["sqlite_linkage"] = json!(c.command(inspect, "sqlite-linkage", None, 30.)?);
        c.sample_rss = opts.sample_rss;
        let mut identity = None;
        for index in 0..opts.rounds {
            let order = if index % 2 == 0 {
                ["weekend", "sqlite"]
            } else {
                ["sqlite", "weekend"]
            };
            let mut row = json!({"order":order});
            for name in order {
                let label = format!("round-{}-{name}", index + 1);
                let directory = output.join(&label);
                let mut args = vec![path(&binaries[name].0), path(directory)];
                args.extend(
                    [
                        work.commits,
                        work.keys,
                        work.batch,
                        work.payload,
                        work.retain_every,
                        work.reopens,
                    ]
                    .map(|x| OsString::from(x.to_string())),
                );
                let stdout = c.command_at(args, &label, None, 180., &source)?;
                let mut result = delivery::json(stdout.as_bytes())?;
                check_round(&result, work, &expected, name)?;
                if name == "sqlite" {
                    let runtime = json!({"version":result["sqlite_version"],"source_id":result["sqlite_source_id"],"compile_options":result["compile_options"]});
                    ensure!(
                        identity.as_ref().is_none_or(|value| *value == runtime),
                        "SQLite runtime changed between rounds"
                    );
                    identity = Some(runtime);
                } else {
                    let sync = if cfg!(target_os = "macos") {
                        "F_FULLFSYNC"
                    } else {
                        "fsync"
                    };
                    result["settings"] = json!({"native_sync":sync,"features":"base"});
                }
                row[name] = result;
            }
            c.evidence["rounds"].as_array_mut().unwrap().push(row);
            c.save()?;
        }
        c.evidence["sqlite_runtime"] = identity.context("missing SQLite runtime identity")?;
        c.evidence["summary"] = summarize(c.evidence["rounds"].as_array().unwrap())?;
        ensure!(
            inventory(&source, &[])? == source_hashes,
            "archived baseline source changed"
        );
        for (binary, hash) in binaries.into_values() {
            ensure!(sha(&binary)? == hash, "baseline binary changed");
        }
        c.evidence["artifacts"] = json!(inventory(&output, &["evidence.json", "build", "source"])?);
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use std::os::unix::fs::symlink;

    fn options(output: PathBuf) -> Options {
        Options {
            output,
            revision: "unused".into(),
            commits: 8,
            keys: 5,
            batch: 3,
            payload: 32,
            retain_every: 3,
            reopens: 2,
            rounds: 1,
            sample_rss: false,
        }
    }

    #[test]
    fn existing_outputs_and_symlinks_reject_before_external_work() {
        let tmp = Scratch::new(&std::env::temp_dir(), "sqlite-controller").unwrap();
        let dir = tmp.path();
        let existing = dir.join("existing");
        let link = dir.join("link");
        let missing = dir.join("missing");
        fs::write(&existing, b"preserve-existing-output").unwrap();
        symlink(&missing, &link).unwrap();
        let run = Run::new(dir.to_path_buf());
        for candidate in [existing.as_path(), dir, link.as_path()] {
            assert!(
                super::run(&run, options(candidate.to_owned()))
                    .unwrap_err()
                    .to_string()
                    .contains("fresh output")
            );
        }
        assert_eq!(fs::read(existing).unwrap(), b"preserve-existing-output");
        assert!(!missing.exists());
    }

    #[test]
    fn invalid_workload_rejects_before_output_and_replay() {
        let tmp = Scratch::new(&std::env::temp_dir(), "sqlite-controller").unwrap();
        let output = tmp.path().join("new");
        let mut opts = options(output.clone());
        opts.batch = 6;
        assert!(
            super::run(&Run::new(tmp.path().to_owned()), opts)
                .unwrap_err()
                .to_string()
                .contains("bounded supported range")
        );
        assert!(!output.exists());
    }

    #[test]
    fn frozen_forward_digest_and_timing_sample_admission() {
        let tmp = Scratch::new(&std::env::temp_dir(), "sqlite-controller").unwrap();
        let opts = Options {
            commits: 64,
            keys: 32,
            batch: 4,
            payload: 256,
            retain_every: 16,
            reopens: 32,
            rounds: 3,
            ..options(tmp.path().join("unused"))
        };
        let (_, work) = admit(&Run::new(tmp.path().to_owned()), &opts).unwrap();
        let expected = work.expected();
        assert_eq!(expected.digest, 14_964_336_404_429_242_912);
        let mut report = json!({
            "oracle_digest":expected.digest, "history_count":4,
            "commit_ns":vec![1_u64;64],"transaction_ns":vec![2_u64;64],
            "pin_ns":vec![3_u64;4],"reopen_ns":vec![4_u64;32],"reopen_verify_ns":vec![5_u64;32],
            "backup_ns":6,"restore_ns":7,"compact_ns":8,
        });
        check_round(&report, work, &expected, "weekend").unwrap();
        let full = json!({"weekend":report,"sqlite":report});
        assert_eq!(
            summarize(&[full]).unwrap()["weekend"]["commit_ns"],
            json!({"samples":64,"p50_ns":1,"p95_ns":1,"p99_ns":1})
        );
        assert!(check_round(&report, work, &expected, "sqlite").is_err());
        report["commit_ns"][0] = json!(true);
        assert!(check_round(&report, work, &expected, "weekend").is_err());
    }
}
