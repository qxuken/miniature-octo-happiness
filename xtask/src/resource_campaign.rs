//! Frozen resource workloads and independent observation using the POSIX supervisor.
use crate::{
    artifact_trace::{docker_retire, inventory, nonce},
    assurance::{self, Campaign},
    delivery, faults, resource_oracle, serialization_scale,
    supervise::{self, Report, Request},
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};
const DRIVER: &str = "tools/resource-envelope";
const BASE: &[&str] = &["Cargo.toml", "Cargo.lock", "src/main.rs"];
const COMPOUND: &[&str] = &["src/compound.rs", "src/resilience.rs", "src/space.rs"];
const MAX_STREAM: u64 = 8 * 1024 * 1024;
const RESILIENCE_PROFILES: [&str; 8] = [
    "maintenance-copy",
    "maintenance-publish",
    "recovery-0",
    "recovery-1",
    "recovery-2",
    "recovery-3",
    "recovery-4",
    "recovery-5",
];
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum Profile {
    Envelope,
    Compound,
    CompoundCycles,
    Resilience,
}
impl Profile {
    fn name(self) -> &'static str {
        match self {
            Self::Envelope => "envelope",
            Self::Compound => "compound",
            Self::CompoundCycles => "compound-cycles",
            Self::Resilience => "resilience",
        }
    }
    fn binary(self) -> &'static str {
        match self {
            Self::Envelope => "weekend-resource-envelope",
            Self::Compound | Self::CompoundCycles | Self::Resilience => "weekend-compound-resource",
        }
    }
}
#[derive(Args)]
pub struct Options {
    pub profile: Profile,
    #[arg(long)]
    pub revision: String,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long)]
    pub docker_image: Option<String>,
    #[arg(long, hide = true)]
    pub worker: Option<PathBuf>,
}
fn write_json(output: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(output, bytes)?;
    Ok(())
}
fn rows(path: &Path) -> Result<Vec<Value>> {
    let bytes = fs::read(path)?;
    ensure!(bytes.len() < MAX_STREAM as usize, "resource output limit");
    std::str::from_utf8(&bytes)?
        .lines()
        .map(|line| delivery::json(line.as_bytes()).context("invalid resource JSON row"))
        .collect()
}
fn process_evidence(value: &mut Value, result: &Report) -> Result<()> {
    let fields = serde_json::to_value(result)?;
    for (key, entry) in fields.as_object().unwrap() {
        value[key] = entry.clone();
    }
    value["process_group"] = json!(result.pid);
    value["process_group_killed"] = json!(result.cleanup_verified);
    value["rss_sample_count"] = json!(result.rss_samples_kib.len());
    value["sampled_peak_rss_kib"] =
        json!(result.rss_samples_kib.iter().copied().max().unwrap_or(0));
    Ok(())
}
struct Observation<'a> {
    binary: &'a Path,
    data: &'a Path,
    report: &'a Path,
    args: &'a [OsString],
    context: &'a Run,
    deadline: f64,
    inspect_group: bool,
}
fn observe(request: Observation<'_>, check: impl FnOnce(&[Value]) -> Result<()>) -> Result<Value> {
    observe_with(request, supervise::run, check)
}
fn observe_with(
    request: Observation<'_>,
    execute: impl FnOnce(&Request) -> Result<Report>,
    check: impl FnOnce(&[Value]) -> Result<()>,
) -> Result<Value> {
    let stdout = request.report.with_extension("jsonl");
    let stderr = request.report.with_extension("stderr");
    for p in [request.data, request.report, &stdout, &stderr] {
        ensure!(
            fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "observer outputs must be new"
        );
    }
    let mut report = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(request.report)?;
    let mut evidence = json!({"passed":false,"failure":null,"cleanup_failure":null,"returncode":null,"rss_samples_kib":[],"rss_sample_count":0,"sampled_peak_rss_kib":0,"rss_scope":"main child sampled about every 50ms, with separate stack-child telemetry"});
    serde_json::to_writer(&mut report, &evidence)?;
    report.write_all(b"\n")?;
    drop(report);
    let start = Instant::now();
    let result = (|| -> Result<()> {
        let argv = [request.binary.as_os_str(), request.data.as_os_str()]
            .into_iter()
            .chain(request.args.iter().map(OsString::as_os_str))
            .map(|s| Ok(s.to_str().context("UTF-8 resource argument")?.to_owned()))
            .collect::<Result<Vec<_>>>()?;
        let env = request
            .context
            .env
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.to_str()
                        .context("UTF-8 resource environment key")?
                        .to_owned(),
                    v.to_str()
                        .context("UTF-8 resource environment value")?
                        .to_owned(),
                ))
            })
            .collect::<Result<_>>()?;
        let result = execute(&Request {
            argv,
            cwd: Some(request.context.root.clone()),
            env: Some(env),
            stdout: stdout.clone(),
            stderr,
            report: None,
            timeout_seconds: request.deadline,
            maximum_bytes: MAX_STREAM,
            exclusive_limit: true,
            file_limit: None,
            journal: None,
            sample_rss: true,
        })?;
        process_evidence(&mut evidence, &result)?;
        ensure!(
            result.success(),
            "resource execution failed: {}; cleanup {:?}",
            result
                .failure
                .as_deref()
                .unwrap_or("unverified process retirement"),
            result.cleanup_failure
        );
        let rows = rows(&stdout)?;
        check(&rows)?;
        ensure!(!result.rss_samples_kib.is_empty(), "no RSS samples");
        evidence["rows"] = json!(rows);
        if request.inspect_group {
            let pid = i32::try_from(result.pid.context("missing resource PID")?)?;
            let live = supervise::process_table()?
                .values()
                .any(|p| p.group == pid && !p.state.starts_with('Z'));
            evidence["owned_live_processes_absent"] = json!(!live);
            ensure!(!live, "owned process group still live");
        }
        Ok(())
    })();
    evidence["elapsed_seconds"] = json!(start.elapsed().as_secs_f64());
    if let Err(e) = &result
        && evidence["failure"].is_null()
    {
        evidence["failure"] = json!(format!("{e:#}"));
    }
    evidence["passed"] = json!(result.is_ok());
    write_json(request.report, &evidence)?;
    result?;
    Ok(evidence)
}
fn clean_env(
    inherited: &BTreeMap<OsString, OsString>,
    source: &str,
) -> BTreeMap<OsString, OsString> {
    let mut env = serialization_scale::clean_env(inherited);
    env.remove(OsStr::new("WEEKEND_RESOURCE_STACK_CHILD"));
    env.insert("WEEKEND_DB_SOURCE_ID".into(), source.into());
    env
}
pub(crate) fn worker(
    run: &Run,
    source: &Path,
    output: &Path,
    revision: &str,
    profile: Profile,
) -> Result<()> {
    let source = dunce::canonicalize(source)?;
    let output = assurance::fresh_output(&run.root, output)?;
    ensure!(
        !output.starts_with(&source) && !source.starts_with(&output),
        "resource source/output overlap"
    );
    let mut run = Run::new(run.root.clone());
    run.env = clean_env(&run.env, revision);
    let mut c = Campaign::detached(
        &run,
        &output,
        "resource driver with independent byte and state oracle",
    )?;
    c.evidence["verified"] = json!(false);
    c.evidence["revision"] = json!(revision);
    c.evidence["runs"] = json!({});
    c.evidence["profile"] = json!(profile.name());
    c.save()?;
    let result = (|| -> Result<()> {
        let source_hashes = inventory(&source, &[])?;
        let version = c.command(cmd(["rustc", "-vV"]), "rustc", None, 30.)?;
        c.evidence["source_hashes"] = json!(source_hashes);
        c.evidence["rustc"] = json!(version);
        c.evidence["cargo_configuration"] = toolchain::configuration(&source, &run.env)?;
        let filesystem = if cfg!(target_os = "linux") {
            vec![
                "findmnt".into(),
                "-T".into(),
                path(&output),
                "-n".into(),
                "-o".into(),
                "FSTYPE,SOURCE,TARGET".into(),
            ]
        } else {
            vec!["df".into(), "-P".into(), path(&output)]
        };
        c.evidence["filesystem"] = json!(c.command(filesystem, "filesystem", None, 30.)?);
        let target = output.join("cargo-target");
        let mut build = vec![
            "cargo".into(),
            "build".into(),
            "--offline".into(),
            "--locked".into(),
            "--release".into(),
            "--manifest-path".into(),
            path(source.join(DRIVER).join("Cargo.toml")),
            "--bin".into(),
            profile.binary().into(),
            "--target-dir".into(),
            path(&target),
        ];
        if profile != Profile::Envelope {
            build.extend(cmd(["--features", "compound"]));
        }
        c.command_at(build, "fixture-build", None, 180., &source)?;
        let binary = output.join("driver");
        fs::copy(target.join("release").join(profile.binary()), &binary)?;
        let binary_hash = sha(&binary)?;
        c.evidence["binary_sha256"] = json!(binary_hash);
        if profile == Profile::Envelope {
            let report = output.join("run.json");
            let data = output.join("data");
            let value = observe(
                Observation {
                    binary: &binary,
                    data: &data,
                    report: &report,
                    args: &[],
                    context: &run,
                    deadline: 180.,
                    inspect_group: false,
                },
                resource_oracle::envelope,
            )?;
            c.evidence["run"] = value;
        } else {
            let source_image = output.join("native-data/source.wdb");
            for (label, args) in [
                ("native", vec![]),
                ("inspect", vec![path(&source_image), "inspect".into()]),
                ("faults", vec![path(&source_image)]),
            ] {
                let report = output.join(format!("{label}.json"));
                let data = output.join(format!("{label}-data"));
                let value = observe(
                    Observation {
                        binary: &binary,
                        data: &data,
                        report: &report,
                        args: &args,
                        context: &run,
                        deadline: 180.,
                        inspect_group: true,
                    },
                    |rows| resource_oracle::compound(rows, label, revision),
                )?;
                c.evidence["runs"][label] = value;
                c.save()?;
            }
            if profile == Profile::CompoundCycles {
                let seed = output.join("native-data/compact.wdb");
                let seed_hash = sha(&seed)?;
                let report = output.join("cycles.json");
                let data = output.join("cycles-data");
                let value = observe(
                    Observation {
                        binary: &binary,
                        data: &data,
                        report: &report,
                        args: &[path(&seed), "cycles".into()],
                        context: &run,
                        deadline: 480.,
                        inspect_group: true,
                    },
                    |rows| resource_oracle::compound_cycles(rows, revision),
                )?;
                ensure!(sha(&seed)? == seed_hash, "cycle seed changed");
                ensure!(
                    fs::read_dir(&data)?.count() == 1 && data.join("cycles.wdb").is_file(),
                    "cycle maintenance artifacts remain"
                );
                c.evidence["runs"]["cycles"] = value;
                c.evidence["cycle_seed_sha256"] = json!(seed_hash);
                c.save()?;
            }
            if profile == Profile::Resilience {
                let image_hash = sha(&source_image)?;
                c.evidence["resilience_runs"] = json!({});
                for label in RESILIENCE_PROFILES {
                    let mut args = vec![path(&source_image)];
                    match label.strip_prefix("recovery-") {
                        Some(mode) => args.extend(cmd(["resilience-recovery", mode])),
                        None => args.push(label.into()),
                    }
                    let report = output.join(format!("{label}.json"));
                    let data = output.join(format!("{label}-data"));
                    let value = observe(
                        Observation {
                            binary: &binary,
                            data: &data,
                            report: &report,
                            args: &args,
                            context: &run,
                            deadline: 180.,
                            inspect_group: true,
                        },
                        |rows| resource_oracle::resilience(rows, label, revision),
                    )?;
                    c.evidence["resilience_runs"][label] = value;
                    c.save()?;
                }
                ensure!(
                    sha(&source_image)? == image_hash,
                    "resilience source image changed"
                );
                c.evidence["image_sha256"] = json!(image_hash);
            }
        }
        ensure!(
            sha(&binary)? == binary_hash && inventory(&source, &[])? == source_hashes,
            "resource source or binary changed"
        );
        Ok(())
    })();
    c.evidence["artifacts"] = json!(inventory(&output, &["evidence.json", "cargo-target"])?);
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}
fn archive(c: &mut Campaign<'_>, reference: &str) -> Result<(String, PathBuf)> {
    let (revision, source) = assurance::archive_source(c, reference)?;
    let mut overlay = BTreeMap::new();
    for file in BASE.iter().chain(COMPOUND) {
        let name = PathBuf::from(DRIVER).join(file);
        let dest = source.join(&name);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::copy(c.run.root.join(&name), &dest)?;
        overlay.insert(name, sha(&dest)?);
    }
    c.evidence["driver_overlay"] = json!(overlay);
    Ok((revision, source))
}
fn docker(
    c: &mut Campaign<'_>,
    source: &Path,
    revision: &str,
    image: &str,
    profile: Profile,
) -> Result<()> {
    let image = c.command(
        cmd(["docker", "image", "inspect", "--format={{.Id}}", image]),
        "image",
        None,
        30.,
    )?;
    ensure!(
        image.len() == 71
            && image.starts_with("sha256:")
            && image[7..].bytes().all(|b| b.is_ascii_hexdigit()),
        "immutable local Docker image required"
    );
    let runner = c.output.join("runner");
    faults::stage_runner(&c.run.root, &runner)?;
    let hashes = inventory(&runner, &[])?;
    let registry = toolchain::cargo_home(&c.run.root, &c.env)?.join("registry");
    let container = format!("weekend-resource-{}", nonce()?);
    let volume = format!("{container}-data");
    c.evidence["docker"] = json!({"image":image,"container":container,"volume":volume,"cleanup_verified":false,"runner_source_sha256":hashes});
    c.save()?;
    let result = (|| -> Result<()> {
        c.command(
            cmd(["docker", "volume", "create", &volume]),
            "volume",
            None,
            30.,
        )?;
        let worker_output = if profile == Profile::Envelope {
            "/work/result"
        } else {
            "/owned/result"
        };
        let script = "set -eu; mkdir -p \"$(dirname \"$3\")\"; cargo build --offline --locked --manifest-path /runner/Cargo.toml -p xtask --target-dir /owned/runner-target; exec /owned/runner-target/debug/xtask resource \"$1\" --revision \"$2\" --worker /source --output \"$3\"";
        let executed = c.command(
            vec![
                "docker".into(),
                "run".into(),
                "--pull=never".into(),
                "--network=none".into(),
                "--memory=2g".into(),
                "--cpus=2".into(),
                "--pids-limit=256".into(),
                "--ulimit=core=0".into(),
                "--name".into(),
                container.clone().into(),
                "--mount".into(),
                format!("type=volume,source={volume},target=/owned").into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/runner,readonly",
                    runner.display()
                )
                .into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/source,readonly",
                    source.display()
                )
                .into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    registry.display()
                )
                .into(),
                image.into(),
                "sh".into(),
                "-c".into(),
                script.into(),
                "resource".into(),
                profile.name().into(),
                revision.into(),
                worker_output.into(),
            ],
            "docker-run",
            None,
            match profile {
                Profile::Envelope => 300.,
                Profile::Compound => 600.,
                Profile::CompoundCycles => 900.,
                Profile::Resilience => 1200.,
            },
        );
        let copied = c.command(
            vec![
                "docker".into(),
                "cp".into(),
                format!("{container}:{worker_output}").into(),
                path(c.output.join("run")),
            ],
            "copyback",
            None,
            180.,
        );
        executed?;
        copied?;
        ensure!(
            inventory(&runner, &[])? == hashes,
            "frozen worker sources changed"
        );
        Ok(())
    })();
    let retired = docker_retire(
        c,
        &container,
        &volume,
        [
            cmd(["docker", "rm", "-f", &container]),
            cmd(["docker", "volume", "rm", &volume]),
        ],
    );
    c.evidence["docker"]["cleanup_verified"] = json!(retired.is_ok());
    retired?;
    result
}
fn check_proof(
    output: &Path,
    source: &BTreeMap<String, String>,
    revision: &str,
    profile: Profile,
) -> Result<Value> {
    let proof = delivery::read_json(&output.join("evidence.json"))?;
    ensure!(
        proof["verified"] == true
            && proof["passed"] == true
            && proof["revision"] == revision
            && proof["profile"] == profile.name(),
        "resource worker did not qualify"
    );
    ensure!(
        proof["source_hashes"] == json!(source),
        "resource worker used unexpected source"
    );
    ensure!(
        proof["artifacts"] == json!(inventory(output, &["evidence.json", "cargo-target"])?),
        "resource worker copyback mismatch"
    );
    ensure!(
        proof["binary_sha256"] == sha(&output.join("driver"))?,
        "resource worker binary mismatch"
    );
    if profile == Profile::Envelope {
        ensure!(proof["run"]["passed"] == true, "unverified resource run");
        resource_oracle::envelope(
            proof["run"]["rows"]
                .as_array()
                .context("resource rows missing")?,
        )?;
    } else {
        ensure!(
            proof["runs"].as_object().is_some_and(|m| {
                m.len() == 3 + usize::from(profile == Profile::CompoundCycles)
            }),
            "incomplete compound profiles"
        );
        for label in ["native", "inspect", "faults"] {
            let run = &proof["runs"][label];
            ensure!(
                run["passed"] == true && run["owned_live_processes_absent"] == true,
                "unverified compound run: {label}"
            );
            resource_oracle::compound(
                run["rows"].as_array().context("compound rows missing")?,
                label,
                revision,
            )?;
        }
        if profile == Profile::CompoundCycles {
            let run = &proof["runs"]["cycles"];
            ensure!(
                run["passed"] == true && run["owned_live_processes_absent"] == true,
                "unverified compound cycle run"
            );
            let rows = run["rows"]
                .as_array()
                .context("compound cycle rows missing")?;
            resource_oracle::compound_cycles(rows, revision)?;
            let data = output.join("cycles-data");
            ensure!(
                proof["cycle_seed_sha256"] == sha(&output.join("native-data/compact.wdb"))?
                    && fs::read_dir(&data)?.count() == 1
                    && fs::metadata(data.join("cycles.wdb"))?.len()
                        == resource_oracle::number(&rows[rows.len() - 2], "file_bytes")?,
                "compound cycle seed or compacted file changed"
            );
        }
        if profile == Profile::Resilience {
            ensure!(
                proof["resilience_runs"]
                    .as_object()
                    .is_some_and(|m| m.len() == RESILIENCE_PROFILES.len()),
                "incomplete resilience profiles"
            );
            for label in RESILIENCE_PROFILES {
                let run = &proof["resilience_runs"][label];
                ensure!(
                    run["passed"] == true && run["owned_live_processes_absent"] == true,
                    "unverified resilience run: {label}"
                );
                resource_oracle::resilience(
                    run["rows"].as_array().context("resilience rows missing")?,
                    label,
                    revision,
                )?;
            }
            ensure!(
                proof["image_sha256"] == sha(&output.join("native-data/source.wdb"))?,
                "resilience image changed after copyback"
            );
        }
    }
    Ok(proof)
}
pub fn run(run: &Run, options: Options) -> Result<()> {
    if let Some(source) = options.worker {
        return worker(
            run,
            &source,
            &options.output,
            &options.revision,
            options.profile,
        );
    }
    let output = assurance::fresh_output(&run.root, &options.output)?;
    let mut c = Campaign::new(run, &output, "frozen resource qualification")?;
    c.evidence["verified"] = json!(false);
    c.save()?;
    let result = (|| -> Result<()> {
        let (revision, source) = archive(&mut c, &options.revision)?;
        let hashes = inventory(&source, &[])?;
        c.evidence["source_sha256"] = json!(hashes);
        let run_path = output.join("run");
        if let Some(image) = options.docker_image {
            docker(&mut c, &source, &revision, &image, options.profile)?;
        } else {
            worker(run, &source, &run_path, &revision, options.profile)?;
        }
        let proof = check_proof(&run_path, &hashes, &revision, options.profile)?;
        ensure!(
            inventory(&source, &[])? == hashes,
            "archived resource source changed"
        );
        c.evidence["worker_sha256"] = json!(sha(&run_path.join("evidence.json"))?);
        c.evidence["filesystem"] = proof["filesystem"].clone();
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
    struct Fixture {
        dir: Scratch,
        run: Run,
        path: PathBuf,
        report: PathBuf,
        data: PathBuf,
    }
    impl Fixture {
        fn new(body: &str) -> Self {
            let dir = Scratch::new(&std::env::temp_dir(), "resource-observer").unwrap();
            let root = dir.path();
            let path = root.join("child.sh");
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                run: Run::new(root.to_owned()),
                report: root.join("report.json"),
                data: root.join("data"),
                dir,
                path,
            }
        }
        fn request<'a>(&'a self, seconds: f64, args: &'a [OsString]) -> Observation<'a> {
            Observation {
                binary: &self.path,
                data: &self.data,
                report: &self.report,
                args,
                context: &self.run,
                deadline: seconds,
                inspect_group: true,
            }
        }
        fn evidence(&self) -> Value {
            delivery::read_json(&self.report).unwrap()
        }
    }
    #[test]
    fn existing_and_dangling_outputs_reject_before_creating_logs() {
        let fixture = Fixture::new("exit 42");
        fs::create_dir(&fixture.data).unwrap();
        assert!(observe(fixture.request(3., &[]), |_| panic!("unexpected oracle")).is_err());
        assert!(!fixture.report.exists());
        fs::remove_dir(&fixture.data).unwrap();
        fs::write(&fixture.report, b"preserve").unwrap();
        assert!(observe(fixture.request(3., &[]), |_| panic!("unexpected oracle")).is_err());
        assert_eq!(fs::read(&fixture.report).unwrap(), b"preserve");
        fs::remove_file(&fixture.report).unwrap();
        let target = fixture.dir.path().join("absent");
        symlink(&target, &fixture.report).unwrap();
        assert!(assurance::fresh_output(&fixture.run.root, &fixture.report).is_err());
        assert!(observe(fixture.request(3., &[]), |_| panic!("unexpected oracle")).is_err());
        assert!(!target.exists());
        assert!(!fixture.report.with_extension("jsonl").exists());
    }
    #[test]
    fn malformed_json_and_spawn_errors_save_terminal_evidence() {
        let malformed = Fixture::new("printf 'broken\\n'");
        assert!(observe(malformed.request(4., &[]), |_| panic!("unexpected oracle")).is_err());
        let evidence = malformed.evidence();
        assert_eq!(evidence["passed"], false);
        assert_eq!(evidence["returncode"], 0);
        assert!(
            evidence["failure"]
                .as_str()
                .unwrap()
                .contains("invalid resource JSON")
        );
        let absent = Fixture::new("exit 0");
        fs::remove_file(&absent.path).unwrap();
        assert!(observe(absent.request(4., &[]), |_| panic!("unexpected oracle")).is_err());
        let evidence = absent.evidence();
        assert_eq!(evidence["passed"], false);
        assert_eq!(evidence["returncode"], Value::Null);
        assert!(
            evidence["failure"]
                .as_str()
                .unwrap()
                .contains("spawn child")
        );
    }
    #[test]
    fn observer_preserves_observation_cleanup_and_cancellation_failures() {
        let errors = [
            (Some("sampler timeout"), None, false),
            (Some("supervisor cancelled"), None, true),
            (None, Some("wait failed after reap"), false),
            (
                Some("observation failed"),
                Some("cleanup probe failed"),
                false,
            ),
            (None, Some("process probe timeout"), false),
            (None, Some("process probe malformed"), false),
            (None, Some("process probe sees live child"), false),
        ];
        for (failure, cleanup, cancelled) in errors {
            let fixture = Fixture::new("exit 0");
            assert!(
                observe_with(
                    fixture.request(4., &[]),
                    |_| Ok(Report {
                        pid: Some(123),
                        returncode: Some(if cancelled { -9 } else { 0 }),
                        failure: failure.map(str::to_owned),
                        cleanup_failure: cleanup.map(str::to_owned),
                        cleanup_verified: cleanup.is_none(),
                        cancelled,
                        ..Default::default()
                    }),
                    |_| panic!("failed process reached oracle")
                )
                .is_err()
            );
            let evidence = fixture.evidence();
            assert_eq!(evidence["passed"], false);
            assert_eq!(evidence["cancelled"], cancelled);
            assert_eq!(evidence["cleanup_failure"], json!(cleanup));
            if let Some(failure) = failure {
                assert_eq!(evidence["failure"], failure);
            } else {
                assert!(!evidence["failure"].is_null());
            }
            assert_eq!(evidence["process_group_killed"], cleanup.is_none());
        }
        let fixture = Fixture::new("exit 0");
        assert!(
            observe_with(
                fixture.request(4., &[]),
                |_| anyhow::bail!("supervisor unavailable"),
                |_| panic!("failed launch reached oracle")
            )
            .is_err()
        );
        let evidence = fixture.evidence();
        assert_eq!(evidence["failure"], "supervisor unavailable");
        assert_eq!(evidence["returncode"], Value::Null);
        assert_eq!(evidence["rss_sample_count"], 0);
    }
    #[test]
    fn real_output_cap_and_deadline_retire_child_groups() {
        use crate::support::cmd;
        for size in [8, 9] {
            let body = format!("exec /bin/dd if=/dev/zero bs=1048576 count={size}");
            let fixture = Fixture::new(&body);
            assert!(
                observe(fixture.request(10., &[]), |_| panic!(
                    "overflow reached parser"
                ))
                .is_err()
            );
            let evidence = fixture.evidence();
            assert!(
                evidence["failure"]
                    .as_str()
                    .unwrap()
                    .contains("output limit")
            );
            assert_eq!(evidence["cleanup_failure"], Value::Null);
        }
        let fixture = Fixture::new("sleep 30 &\nprintf '%s\\n' \"$!\" > \"$2\"\nwait");
        let pid_file = fixture.dir.path().join("spawned");
        assert!(
            observe(
                fixture.request(0.6, &cmd([pid_file.as_os_str()])),
                |_| panic!("timeout reached parser")
            )
            .is_err()
        );
        let evidence = fixture.evidence();
        assert!(pid_file.is_file(), "fixture actually spawned a child");
        assert_eq!(evidence["timed_out"], true);
        assert_eq!(evidence["process_group_killed"], true);
        let pid = fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        assert!(
            supervise::process_table()
                .unwrap()
                .get(&pid)
                .is_none_or(|row| row.state.starts_with('Z'))
        );
    }
    #[test]
    fn observer_preserves_arguments_and_retires_descendants_after_leader_exit() {
        let fixture = Fixture::new("sleep 30 &\nsleep 1\nprintf '{\"argument\":\"%s\"}\\n' \"$2\"");
        let evidence = observe(fixture.request(8., &cmd(["sentinel"])), |rows| {
            assert_eq!(rows, &[json!({"argument":"sentinel"})]);
            Ok(())
        })
        .unwrap();
        assert_eq!(evidence["process_group_killed"], true);
        assert_eq!(evidence["owned_live_processes_absent"], true);
    }
}
