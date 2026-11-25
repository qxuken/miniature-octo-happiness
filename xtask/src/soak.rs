//! Bounded native lifecycle workload backed by an independent round oracle.
use crate::{
    artifact_trace::inventory,
    assurance::{self, Campaign},
    delivery,
    resource_oracle::latency_samples,
    serialization_scale,
    supervise::{self, Report, Request},
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const CAP: u64 = 16 * 1024 * 1024;
const IMAGE_CAP: u64 = 32 * 1024 * 1024;
const MAX_ROUNDS: u64 = 100_000;
const MAX_ROW: u64 = 4096;
const HARNESS: &str = "tools/soak";
const TIMING_FIELDS: [&str; 6] = [
    "round",
    "concurrent",
    "compact",
    "reopen",
    "backup",
    "restore",
];

#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = "HEAD")]
    revision: String,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u64).range(1..=86400))]
    seconds: u64,
    #[arg(long, default_value_t = MAX_ROUNDS, value_parser = clap::value_parser!(u64).range(1..=100000))]
    rounds: u64,
}

fn integer(row: &Value, name: &str) -> Result<u64> {
    row[name]
        .as_u64()
        .with_context(|| format!("invalid soak record {name}"))
}

fn summary(mut reader: impl BufRead, round_limit: u64) -> Result<(Value, Value)> {
    let (mut rounds, mut reads, mut elapsed) = (0_u64, 0, 0);
    let mut cumulative_ns = 0_u128;
    let mut timing_samples: [Vec<u64>; TIMING_FIELDS.len()] = Default::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        let size = (&mut reader)
            .take(MAX_ROW + 1)
            .read_until(b'\n', &mut line)?;
        ensure!(size > 0, "missing soak completion");
        ensure!(size as u64 <= MAX_ROW, "oversized soak record");
        let row = delivery::json(&line)?;
        if row["complete"] == true {
            ensure!(
                rounds > 0 && integer(&row, "rounds")? == rounds,
                "invalid completion count"
            );
            ensure!(
                integer(&row, "reads")? == reads && integer(&row, "elapsed_ms")? >= elapsed,
                "completion differs from observed rounds"
            );
            ensure!(
                reader.fill_buf()?.is_empty(),
                "extra data after soak completion"
            );
            let timings = TIMING_FIELDS
                .into_iter()
                .zip(timing_samples.map(latency_samples))
                .collect::<BTreeMap<_, _>>();
            return Ok((row, json!(timings)));
        }
        rounds += 1;
        ensure!(
            rounds <= round_limit && row["complete"].is_null() && integer(&row, "round")? == rounds,
            "nonconsecutive round records or exceeded round limit"
        );
        ensure!(
            integer(&row, "revision")? == 2 + 65 * rounds + rounds.saturating_sub(8),
            "unexpected round revision"
        );
        ensure!(
            integer(&row, "retained")? == rounds.min(8),
            "unexpected retained count"
        );
        let (new_reads, new_elapsed) = (integer(&row, "reads")?, integer(&row, "elapsed_ms")?);
        ensure!(
            new_reads > reads && new_elapsed >= elapsed,
            "nonmonotonic progress"
        );
        (reads, elapsed) = (new_reads, new_elapsed);
        let timing = &row["timing_ns"];
        let round_ns = integer(timing, "round")?;
        cumulative_ns += u128::from(round_ns);
        timing_samples[0].push(round_ns);
        let mut phase_ns = 0_u128;
        for (samples, field) in timing_samples.iter_mut().zip(TIMING_FIELDS).skip(1) {
            let measured = integer(timing, field)?;
            samples.push(measured);
            phase_ns += u128::from(measured);
        }
        ensure!(
            round_ns > 0
                && phase_ns <= u128::from(round_ns)
                && cumulative_ns < (u128::from(elapsed) + 1) * 1_000_000,
            "inconsistent soak stage timings"
        );
        let (live, peak) = (integer(&row, "live_bytes")?, integer(&row, "peak_bytes")?);
        ensure!(
            live > 0 && live <= peak && peak <= CAP,
            "core accounting outside declared bound"
        );
        let (before, after) = (
            integer(&row, "append_bytes")?,
            integer(&row, "compacted_bytes")?,
        );
        ensure!(
            after > 0 && after <= before && before < IMAGE_CAP,
            "invalid maintenance file lengths"
        );
    }
}

fn observe(
    c: &mut Campaign<'_>,
    request: &Request,
    round_limit: u64,
    seconds: u64,
    execute: impl FnOnce(&Request) -> Result<Report>,
) -> Result<()> {
    ensure!(!c.cancelled(), "campaign cancelled before soak");
    c.evidence["command"] = json!(request.argv);
    c.evidence["timeout_seconds"] = json!(request.timeout_seconds);
    c.evidence["maximum_bytes"] = json!(request.maximum_bytes);
    c.save()?;
    let result = execute(request)?;
    c.evidence["supervision"] = serde_json::to_value(&result)?;
    c.evidence["elapsed_seconds"] = json!(result.elapsed_seconds);
    c.evidence["sampled_peak_rss_kib"] = json!(result.sampled_peak_rss_kib);
    c.evidence["rss_sample_count"] = json!(result.rss_sample_count);
    c.save()?;
    let hash = |file: &Path| -> Result<(String, String)> {
        Ok((
            file.file_name()
                .context("soak log name")?
                .to_string_lossy()
                .into_owned(),
            sha(file)?,
        ))
    };
    c.evidence["logs_sha256"] = json!(BTreeMap::from([
        hash(&request.stdout)?,
        hash(&request.stderr)?
    ]));
    ensure!(
        result.success(),
        "soak child or process cleanup failed: {:?} / {:?}",
        result.failure,
        result.cleanup_failure
    );
    let (last, timings) = summary(BufReader::new(File::open(&request.stdout)?), round_limit)?;
    if integer(&last, "rounds")? < round_limit {
        ensure!(
            integer(&last, "elapsed_ms")? >= seconds * 1000
                && result.elapsed_seconds >= seconds as f64,
            "soak ended before reaching its round or duration limit"
        );
    }
    c.evidence["last_record"] = last;
    c.evidence["round_timing_ns"] = timings;
    Ok(())
}

fn block_size(output: &Path) -> Result<u128> {
    let pathname = CString::new(output.as_os_str().as_bytes())?;
    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // Pass a valid C path and writable struct. Read only on success.
    let status = unsafe { libc::statvfs(pathname.as_ptr(), info.as_mut_ptr()) };
    if status != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(u128::from(unsafe { info.assume_init() }.f_frsize))
}

pub fn run(run: &Run, opts: Options) -> Result<()> {
    ensure!(
        (1..=86_400).contains(&opts.seconds) && (1..=MAX_ROUNDS).contains(&opts.rounds),
        "invalid soak bounds"
    );
    let output = assurance::fresh_output(&run.root, &opts.output)?;
    let mut c = Campaign::new(run, &output, "bounded tiny-record native lifecycle soak")?;
    c.evidence["verified"] = json!(false);
    c.evidence["seconds"] = json!(opts.seconds);
    c.evidence["round_limit"] = json!(opts.rounds);
    c.evidence["rss_scope"] = json!("main child sampled about every 200 ms; workload observation");
    c.limits(8 * 1024 * 1024, None)?;
    c.save()?;
    let result = (|| -> Result<()> {
        let (revision, source) = assurance::archive_source(&mut c, &opts.revision)?;
        let mut overlay = BTreeMap::new();
        for file in ["Cargo.toml", "Cargo.lock", "src/main.rs"] {
            let name = Path::new(HARNESS).join(file);
            let dest = source.join(&name);
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::copy(run.root.join(&name), &dest)?;
            overlay.insert(name, sha(&dest)?);
        }
        c.evidence["driver_overlay"] = json!(overlay);
        let source_hashes = inventory(&source, &[])?;
        c.evidence["engine_source_sha256"] = json!(source_hashes);
        c.env = serialization_scale::clean_env(&c.env);
        c.env.insert("WEEKEND_DB_SOURCE_ID".into(), revision.into());
        c.evidence["rustc"] = json!(c.command(cmd(["rustc", "-Vv"]), "rustc", None, 30.)?);
        c.evidence["cargo"] = json!(c.command(cmd(["cargo", "-V"]), "cargo", None, 30.)?);
        c.evidence["platform"] = json!(c.command(cmd(["uname", "-a"]), "platform", None, 30.)?);
        c.evidence["filesystem_df"] = json!(c.command(
            vec!["df".into(), "-P".into(), path(&output)],
            "filesystem",
            None,
            30.
        )?);
        c.evidence["filesystem_block_size"] = json!(block_size(&output)?);
        let target = output.join("build");
        let mut build = cmd([
            "cargo",
            "build",
            "--offline",
            "--locked",
            "--release",
            "--manifest-path",
        ]);
        build.extend([
            path(source.join(HARNESS).join("Cargo.toml")),
            "--target-dir".into(),
            path(&target),
        ]);
        c.command_at(build, "build", None, 300., &source)?;
        ensure!(
            inventory(&source, &[])? == source_hashes,
            "frozen soak source changed during build"
        );
        let binary = target.join("release/weekend-native-soak");
        let binary_hash = sha(&binary)?;
        c.evidence["binary_sha256"] = json!(binary_hash);
        c.limits(64 * 1024 * 1024, None)?;
        c.sample_rss = true;
        let mut request = c.request(
            vec![
                path(&binary),
                path(output.join("data")),
                opts.seconds.to_string().into(),
                opts.rounds.to_string().into(),
            ],
            "observe",
            (opts.seconds + 120) as f64,
            &source,
        )?;
        request.stdout = output.join("rounds.jsonl");
        request.stderr = output.join("stderr.log");
        observe(
            &mut c,
            &request,
            opts.rounds,
            opts.seconds,
            supervise::run_peak_rss,
        )?;
        ensure!(
            sha(&binary)? == binary_hash && inventory(&source, &[])? == source_hashes,
            "frozen soak source or binary changed during execution"
        );
        c.evidence["artifacts"] = json!(inventory(&output, &["evidence.json", "source", "build"])?);
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn records(count: u64) -> Vec<Value> {
        let mut rows = (1..=count)
            .map(|n| {
                json!({
                    "round":n,"elapsed_ms":n*10,"revision":2+65*n+n.saturating_sub(8),
                    "reads":n*2,"retained":n.min(8),"live_bytes":1,"peak_bytes":2,
                    "append_bytes":16384,"compacted_bytes":8192,
                    "timing_ns":{"round":1000,"concurrent":10,"compact":20,"reopen":n,"backup":15,"restore":25},
                })
            })
            .collect::<Vec<_>>();
        rows.push(json!({"complete":true,"rounds":count,"elapsed_ms":count*10,"reads":count*2}));
        rows
    }
    fn jsonl(rows: &[Value]) -> String {
        rows.iter().map(|row| format!("{row}\n")).collect()
    }
    #[test]
    fn round_oracle_accepts_short_and_release_transition() {
        for count in [1, 8, 9] {
            let rows = records(count);
            assert_eq!(
                summary(jsonl(&rows).as_bytes(), count).unwrap().0,
                *rows.last().unwrap()
            );
        }
    }

    #[test]
    fn timing_summary_requires_measured_phases_and_checks_the_round_clock() {
        let rows = records(100);
        let (_, timing) = summary(jsonl(&rows).as_bytes(), 100).unwrap();
        assert_eq!(
            timing["reopen"],
            json!({"count":100,"zero_ticks":0,"p50_ns":50,"p95_ns":95,"p99_ns":99,"max_ns":100})
        );
        for (stage, value) in [
            ("backup", Value::Null),
            ("restore", json!(-1)),
            ("compact", json!(1001)),
            ("round", json!(0)),
            ("round", json!(11_000_000)),
        ] {
            let mut broken = rows.clone();
            broken[0]["timing_ns"][stage] = value;
            assert!(summary(jsonl(&broken).as_bytes(), 100).is_err(), "{stage}");
        }
    }

    #[test]
    fn incomplete_inconsistent_and_forged_round_records_reject() {
        let complete = records(9);
        for slice in [&[][..], &complete[..9], &complete[1..]] {
            assert!(summary(jsonl(slice).as_bytes(), 9).is_err());
        }
        assert!(summary(jsonl(&complete).as_bytes(), 8).is_err());
        let mut extra = complete.clone();
        extra.push(complete[0].clone());
        assert!(summary(jsonl(&extra).as_bytes(), 9).is_err());
        assert!(summary("a".repeat(MAX_ROW as usize + 1).as_bytes(), 9).is_err());
        for (name, value) in [
            ("round", json!(3)),
            ("elapsed_ms", json!(-1)),
            ("revision", json!(1)),
            ("reads", json!(0)),
            ("retained", json!(0)),
            ("live_bytes", json!(0)),
            ("peak_bytes", json!(CAP + 1)),
            ("append_bytes", json!(IMAGE_CAP)),
            ("compacted_bytes", json!(16_385)),
        ] {
            for replacement in [value, json!(true), json!("1")] {
                let mut bad = complete.clone();
                bad[0][name] = replacement;
                assert!(summary(jsonl(&bad).as_bytes(), 9).is_err(), "{name}");
            }
        }
        for (name, value) in [
            ("rounds", json!(1)),
            ("reads", json!(1)),
            ("elapsed_ms", json!(-1)),
            ("rounds", json!(true)),
            ("complete", json!(1)),
        ] {
            let mut bad = complete.clone();
            bad.last_mut().unwrap()[name] = value;
            assert!(summary(jsonl(&bad).as_bytes(), 9).is_err(), "{name}");
        }
    }

    fn options(output: PathBuf) -> Options {
        Options {
            revision: "unused".into(),
            output,
            seconds: 1,
            rounds: 9,
        }
    }

    #[test]
    fn existing_and_dangling_output_reject_before_external_work() {
        let tmp = Scratch::new(&std::env::temp_dir(), "soak-control").unwrap();
        let dir = tmp.path();
        let file = dir.join("existing");
        let dangling = dir.join("dangling");
        let missing = dir.join("missing");
        fs::write(&file, b"preserve").unwrap();
        symlink(&missing, &dangling).unwrap();
        let run = Run::new(dir.to_owned());
        for candidate in [file.as_path(), dir, dangling.as_path()] {
            assert!(
                super::run(&run, options(candidate.to_owned()))
                    .unwrap_err()
                    .to_string()
                    .contains("fresh output")
            );
        }
        assert_eq!(fs::read(file).unwrap(), b"preserve");
        assert!(!missing.exists());
    }

    #[test]
    fn failed_source_setup_keeps_terminal_evidence() {
        let tmp = Scratch::new(&std::env::temp_dir(), "soak-control").unwrap();
        let git = tmp.path().join("git");
        fs::write(&git, "#!/bin/sh\ncase \"$1 $2\" in\n'rev-parse HEAD') echo fake-source ;;\n'status --porcelain') exit 0 ;;\n*) exit 9 ;;\nesac\n").unwrap();
        fs::set_permissions(&git, fs::Permissions::from_mode(0o700)).unwrap();
        let mut run = Run::new(tmp.path().to_owned());
        run.env.insert("PATH".into(), path(tmp.path()));
        let output = tmp.path().join("output");
        assert!(super::run(&run, options(output.clone())).is_err());
        let proof = delivery::read_json(&output.join("evidence.json")).unwrap();
        assert_eq!(proof["verified"], false);
        assert!(
            proof["error"]
                .as_str()
                .unwrap()
                .contains("engine-revision failed")
        );
        assert_eq!(
            proof["commands"].as_array().unwrap().last().unwrap()["exit_code"],
            9
        );
    }

    #[test]
    fn real_children_need_complete_rounds_and_successful_exit() {
        let tmp = Scratch::new(&std::env::temp_dir(), "soak-control").unwrap();
        let full = jsonl(&records(9));
        let early = jsonl(&records(1));
        let mut claimed = records(1);
        claimed.last_mut().unwrap()["elapsed_ms"] = json!(60_000);
        let claimed = jsonl(&claimed);
        let run = Run::new(tmp.path().to_owned());
        for (name, contents, code, pass) in [
            ("full", full.as_str(), "0", true),
            ("early", early.as_str(), "0", false),
            ("claimed-duration", claimed.as_str(), "0", false),
            ("incomplete", "{\"complete\":true}", "0", false),
            ("malformed", "invalid-json", "0", false),
            ("nonzero", full.as_str(), "1", false),
        ] {
            let output = tmp.path().join(name);
            let mut c = Campaign::detached(&run, &output, "soak observer control").unwrap();
            let process = c
                .request(
                    cmd([
                        "sh",
                        "-c",
                        "printf '%s' \"$1\"; exit \"$2\"",
                        "sh",
                        contents,
                        code,
                    ]),
                    "observer",
                    5.,
                    tmp.path(),
                )
                .unwrap();
            let result = observe(&mut c, &process, 9, 60, supervise::run_peak_rss);
            assert_eq!(c.finish(result).is_ok(), pass);
            let proof = delivery::read_json(&output.join("evidence.json")).unwrap();
            assert_eq!(proof["passed"], pass);
            assert_eq!(proof["supervision"]["cleanup_verified"], true);
            assert_eq!(
                proof["supervision"]["returncode"],
                code.parse::<u64>().unwrap()
            );
            assert_eq!(proof["logs_sha256"].as_object().unwrap().len(), 2);
            if pass {
                assert_eq!(proof["last_record"]["rounds"], 9);
            }
        }
    }
}
