//! Frozen resource workloads and independent observation using the POSIX supervisor.
use crate::{
    artifact_trace::{docker_retire, inventory, nonce},
    assurance::{self, Campaign},
    delivery, faults, resource_oracle, serialization_scale,
    supervise::{self, GroupRssSample, Report, Request},
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
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
const DRIVER: &str = "tools/resource-envelope";
const BASE: &[&str] = &["Cargo.toml", "Cargo.lock", "src/main.rs"];
const COMPOUND: &[&str] = &[
    "src/compound.rs",
    "src/breadth.rs",
    "src/breadth_sustained.rs",
    "src/resilience.rs",
    "src/space.rs",
    "namespace_fault.c",
];
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
    CompoundIsolation,
    CompoundBreadth,
    CompoundBreadthSustained,
    CompoundBreadthArrivals,
    Resilience,
    NamespaceFailures,
}
impl Profile {
    fn name(self) -> &'static str {
        match self {
            Self::Envelope => "envelope",
            Self::Compound => "compound",
            Self::CompoundCycles => "compound-cycles",
            Self::CompoundIsolation => "compound-isolation",
            Self::CompoundBreadth => "compound-breadth",
            Self::CompoundBreadthSustained => "compound-breadth-sustained",
            Self::CompoundBreadthArrivals => "compound-breadth-arrivals",
            Self::Resilience => "resilience",
            Self::NamespaceFailures => "namespace-failures",
        }
    }
    fn binary(self) -> &'static str {
        match self {
            Self::Envelope => "weekend-resource-envelope",
            Self::Compound
            | Self::CompoundCycles
            | Self::CompoundIsolation
            | Self::CompoundBreadth
            | Self::CompoundBreadthSustained
            | Self::CompoundBreadthArrivals
            | Self::Resilience
            | Self::NamespaceFailures => "weekend-compound-resource",
        }
    }
    fn cycles(self) -> &'static [&'static str] {
        match self {
            Self::CompoundCycles => &["cycles"],
            Self::CompoundIsolation => &["cycles", "cycles-isolated"],
            _ => &[],
        }
    }
    fn breadth_mode(self) -> Option<&'static str> {
        match self {
            Self::CompoundBreadth => Some("breadth"),
            Self::CompoundBreadthSustained => Some("breadth-sustained"),
            Self::CompoundBreadthArrivals => Some("breadth-arrivals"),
            _ => None,
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
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_STREAM)
        .read_to_end(&mut bytes)?;
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
    if !result.group_rss_samples.is_empty() {
        value["rss_scope"] = json!(
            "leader and sum of RSS in its owned process group, sampled together about every 50ms"
        );
        value["group_rss"] = group_rss_summary(&result.group_rss_samples, &result.rss_samples_kib)?;
    }
    Ok(())
}

fn group_rss_summary(samples: &[GroupRssSample], leaders: &[u64]) -> Result<Value> {
    ensure!(
        !samples.is_empty() && samples.len() == leaders.len(),
        "missing paired process group RSS samples"
    );
    let mut peak = 0;
    let mut overlap = 0;
    let mut largest_non_leader = 0;
    for (sample, &leader) in samples.iter().zip(leaders) {
        ensure!(
            leader > 0
                && sample.leader_kib == leader
                && sample.group_kib >= leader
                && sample.members > 0
                && (sample.members > 1 || sample.group_kib == leader),
            "invalid process group RSS sample"
        );
        peak = peak.max(sample.group_kib);
        overlap += usize::from(sample.group_kib > leader);
        largest_non_leader = largest_non_leader.max(sample.group_kib - leader);
    }
    Ok(
        json!({"samples":samples.len(),"sampled_peak_kib":peak,"overlap_samples":overlap,
        "sampled_peak_non_leader_kib":largest_non_leader}),
    )
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
        } else if let Some(mode) = profile.breadth_mode() {
            let sustained = mode != "breadth";
            let arrivals = mode == "breadth-arrivals";
            let mut value = observe(
                Observation {
                    binary: &binary,
                    data: &output.join("data"),
                    report: &output.join("run.json"),
                    args: &["--".into(), mode.into()],
                    context: &run,
                    deadline: 240.,
                    inspect_group: true,
                },
                |rows| {
                    if sustained {
                        resource_oracle::sustained_breadth(rows, revision, arrivals).map(drop)
                    } else {
                        resource_oracle::compound_breadth(rows, revision)
                    }
                },
            )?;
            if sustained {
                value.as_object_mut().unwrap().remove("rows");
                value["sample_log_sha256"] = json!(sha(&output.join("run.jsonl"))?);
                c.evidence["summary"] =
                    check_sustained_breadth(&value, &output, revision, arrivals)?;
                write_json(&output.join("run.json"), &value)?;
            } else {
                check_breadth(&value, &output.join("data"), revision)?;
            }
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
            for &mode in profile.cycles() {
                let seed = output.join("native-data/compact.wdb");
                let seed_hash = sha(&seed)?;
                let report = output.join(format!("{mode}.json"));
                let data = output.join(format!("{mode}-data"));
                let execute: fn(&Request) -> Result<Report> =
                    if profile == Profile::CompoundIsolation {
                        supervise::run_group_rss
                    } else {
                        supervise::run
                    };
                let value = observe_with(
                    Observation {
                        binary: &binary,
                        data: &data,
                        report: &report,
                        args: &[path(&seed), mode.into()],
                        context: &run,
                        deadline: 480.,
                        inspect_group: true,
                    },
                    execute,
                    |rows| {
                        resource_oracle::compound_cycles(rows, revision, mode == "cycles-isolated")
                    },
                )?;
                ensure!(sha(&seed)? == seed_hash, "cycle seed changed");
                ensure!(
                    fs::read_dir(&data)?.count() == 1 && data.join("cycles.wdb").is_file(),
                    "cycle maintenance artifacts remain"
                );
                c.evidence["runs"][mode] = value;
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
            if profile == Profile::NamespaceFailures {
                let shim = output.join("namespace-shim");
                let mut build = cmd(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-O2"]);
                if cfg!(target_os = "macos") {
                    build.push("-dynamiclib".into());
                } else {
                    build.extend(cmd(["-shared", "-fPIC"]));
                }
                build.extend([
                    path(source.join(DRIVER).join("namespace_fault.c")),
                    "-o".into(),
                    path(&shim),
                ]);
                if cfg!(target_os = "linux") {
                    build.push("-ldl".into());
                }
                c.command_at(build, "namespace-build", None, 30., &source)?;
                let shim_hash = sha(&shim)?;
                let image_hash = sha(&source_image)?;
                let value = observe(
                    Observation {
                        binary: &binary,
                        data: &output.join("namespace-data"),
                        report: &output.join("namespace.json"),
                        args: &[path(&source_image), profile.name().into(), path(&shim)],
                        context: &run,
                        deadline: 240.,
                        inspect_group: true,
                    },
                    |rows| {
                        resource_oracle::namespace_failures(
                            rows,
                            revision,
                            cfg!(target_os = "macos"),
                        )
                    },
                )?;
                ensure!(
                    sha(&shim)? == shim_hash && sha(&source_image)? == image_hash,
                    "namespace shim or source image changed"
                );
                c.evidence["namespace_run"] = value;
                c.evidence["namespace_shim_sha256"] = json!(shim_hash);
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
        let mut args: Vec<OsString> = vec![
            "docker".into(),
            "run".into(),
            "--pull=never".into(),
            "--network=none".into(),
            "--memory=2g".into(),
            "--cpus=2".into(),
            "--pids-limit=256".into(),
            "--ulimit=core=0".into(),
        ];
        if profile == Profile::NamespaceFailures {
            // The worker owns the case directories. Capabilities would allow
            // root to bypass the mode change under test.
            args.push("--cap-drop=ALL".into());
        }
        args.extend([
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
        ]);
        let executed = c.command(
            args,
            "docker-run",
            None,
            match profile {
                Profile::Envelope => 300.,
                Profile::Compound | Profile::NamespaceFailures => 600.,
                Profile::CompoundCycles => 900.,
                Profile::CompoundIsolation => 1400.,
                Profile::CompoundBreadth
                | Profile::CompoundBreadthSustained
                | Profile::CompoundBreadthArrivals => 600.,
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
fn check_breadth(run: &Value, data: &Path, revision: &str) -> Result<()> {
    ensure!(
        run["passed"] == true && run["owned_live_processes_absent"] == true,
        "unverified breadth run"
    );
    let rows = run["rows"].as_array().context("breadth rows missing")?;
    resource_oracle::compound_breadth(rows, revision)?;
    ensure!(
        fs::read_dir(data)?.count() == 5,
        "unexpected breadth outputs"
    );
    let source = data.join("source.wdb");
    ensure!(
        fs::metadata(source)?.len() == resource_oracle::number(&rows[0], "file_bytes")?,
        "breadth source size differs from report"
    );
    let backup_hash = sha(&data.join("backup.wdb"))?;
    for (name, row) in [
        ("backup.wdb", &rows[3]),
        ("restored.wdb", &rows[4]),
        ("compact.wdb", &rows[5]),
    ] {
        let path = data.join(name);
        ensure!(
            sha(&path)? == backup_hash
                && fs::metadata(&path)?.len() == resource_oracle::number(row, "file_bytes")?,
            "breadth copy bytes differ from report or backup: {name}"
        );
    }
    let reopened = rows
        .iter()
        .find(|row| row["phase"] == "concurrent_reopen")
        .context("missing concurrent reopen evidence")?;
    ensure!(
        fs::metadata(data.join("concurrent.wdb"))?.len()
            == resource_oracle::number(reopened, "file_bytes")?,
        "breadth concurrent image size differs from report"
    );
    Ok(())
}

fn check_sustained_breadth(
    run: &Value,
    output: &Path,
    revision: &str,
    arrivals: bool,
) -> Result<Value> {
    ensure!(
        run["passed"] == true && run["owned_live_processes_absent"] == true,
        "unverified sustained breadth run"
    );
    let raw = output.join("run.jsonl");
    ensure!(
        run["sample_log_sha256"] == sha(&raw)?,
        "sustained sample log changed"
    );
    let rows = rows(&raw)?;
    let summary = resource_oracle::sustained_breadth(&rows, revision, arrivals)?;
    let data = output.join("data");
    ensure!(
        fs::read_dir(&data)?.count() == 2
            && fs::metadata(data.join("seed.wdb"))?.len()
                == resource_oracle::number(&rows[0], "file_bytes")?
            && fs::metadata(data.join("sustained.wdb"))?.len()
                == resource_oracle::number(&rows[8], "compact_bytes")?,
        "unexpected sustained breadth images"
    );
    Ok(summary)
}

fn check_proof(
    output: &Path,
    source: &BTreeMap<String, String>,
    revision: &str,
    profile: Profile,
    macos: bool,
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
    } else if profile == Profile::CompoundBreadth {
        check_breadth(&proof["run"], &output.join("data"), revision)?;
    } else if let Some(mode) = profile.breadth_mode() {
        ensure!(
            proof["summary"]
                == check_sustained_breadth(
                    &proof["run"],
                    output,
                    revision,
                    mode == "breadth-arrivals"
                )?,
            "sustained breadth summary differs from raw samples"
        );
    } else {
        ensure!(
            proof["runs"]
                .as_object()
                .is_some_and(|m| { m.len() == 3 + profile.cycles().len() }),
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
        for &mode in profile.cycles() {
            let run = &proof["runs"][mode];
            ensure!(
                run["passed"] == true && run["owned_live_processes_absent"] == true,
                "unverified compound cycle run"
            );
            let rows = run["rows"]
                .as_array()
                .context("compound cycle rows missing")?;
            resource_oracle::compound_cycles(rows, revision, mode == "cycles-isolated")?;
            if profile == Profile::CompoundIsolation {
                let samples: Vec<GroupRssSample> =
                    serde_json::from_value(run["group_rss_samples"].clone())?;
                let leaders: Vec<u64> = serde_json::from_value(run["rss_samples_kib"].clone())?;
                let summary = group_rss_summary(&samples, &leaders)?;
                ensure!(
                    run["group_rss"] == summary
                        && (mode != "cycles-isolated"
                            || summary["overlap_samples"].as_u64().is_some_and(|n| n > 0)),
                    "missing or inconsistent process group RSS evidence"
                );
            }
            let data = output.join(format!("{mode}-data"));
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
        if profile == Profile::NamespaceFailures {
            let run = &proof["namespace_run"];
            ensure!(
                run["passed"] == true && run["owned_live_processes_absent"] == true,
                "unverified namespace failure run"
            );
            resource_oracle::namespace_failures(
                run["rows"].as_array().context("namespace rows missing")?,
                revision,
                macos,
            )?;
            let data = output.join("namespace-data");
            let control = data.join("control");
            ensure!(
                proof["namespace_shim_sha256"] == sha(&output.join("namespace-shim"))?
                    && proof["image_sha256"] == sha(&output.join("native-data/source.wdb"))?
                    && fs::read_dir(&data)?.count() == 1
                    && fs::read_dir(&control)?.count() == 1
                    && fs::metadata(control.join("store.wdb"))?.len() == 63_074_304,
                "namespace shim, seed or compacted control changed"
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
        if let Some(image) = options.docker_image.as_deref() {
            docker(&mut c, &source, &revision, image, options.profile)?;
        } else {
            worker(run, &source, &run_path, &revision, options.profile)?;
        }
        let proof = check_proof(
            &run_path,
            &hashes,
            &revision,
            options.profile,
            options.docker_image.is_none() && cfg!(target_os = "macos"),
        )?;
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

    fn script_request(request: &Request) -> Request {
        // A parallel test can fork while fs::write holds the script open. Its
        // inherited writable descriptor can cause ETXTBSY after our close.
        // Execute the stable interpreter and read the fixture as data, keeping
        // the script's arguments and every supervision limit unchanged.
        let mut request = request.clone();
        request.argv.insert(0, "/bin/sh".to_owned());
        request
    }
    fn observe_script(
        request: Observation<'_>,
        check: impl FnOnce(&[Value]) -> Result<()>,
    ) -> Result<Value> {
        observe_with(request, |r| supervise::run(&script_request(r)), check)
    }
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
    #[cfg(target_os = "linux")]
    #[test]
    fn observer_runs_script_while_another_process_inherits_its_writer() {
        use std::os::fd::AsRawFd;
        const WORKER: &str = "WEEKEND_TEST_RESOURCE_INHERITED_WRITER";
        if std::env::var_os(WORKER).is_none() {
            // Exec into one selected test before retaining descriptors: a raw
            // fork in the parallel suite could retain unrelated fixture writers.
            let scratch = Scratch::new(&std::env::temp_dir(), "inherited-writer-test").unwrap();
            let mut run = Run::new(scratch.path().to_owned());
            run.env.insert(WORKER.into(), "1".into());
            let output = run
                .bounded_capture(
                    &cmd([
                        std::env::current_exe().unwrap().into_os_string(),
                        "--exact".into(),
                        "resource_campaign::tests::observer_runs_script_while_another_process_inherits_its_writer".into(),
                        "--nocapture".into(),
                    ]),
                    15.,
                )
                .unwrap();
            assert!(output.contains("test result: ok. 1 passed; 0 failed;"));
            return;
        }
        struct InheritedWriter(libc::pid_t);
        impl Drop for InheritedWriter {
            fn drop(&mut self) {
                // SAFETY: the child remains blocked on our pipe until killed;
                // reap it even if an assertion or the observer fails.
                unsafe {
                    libc::kill(self.0, libc::SIGKILL);
                    while libc::waitpid(self.0, std::ptr::null_mut(), 0) < 0
                        && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
                    {
                    }
                }
            }
        }
        let fixture = Fixture::new("printf '{\"argument\":\"%s\"}\\n' \"$2\"; sleep 0.2");
        let writer = OpenOptions::new().write(true).open(&fixture.path).unwrap();
        let (gate, _release) = std::io::pipe().unwrap();
        // Reproduce a sibling fork between fs::write's open and close. CLOEXEC
        // does not close its inherited writable descriptor until exec occurs.
        // SAFETY: the child only calls async-signal-safe read and _exit; no Rust
        // allocation, locks or destructors run in the forked test process.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork: {}", std::io::Error::last_os_error());
        if pid == 0 {
            unsafe {
                let mut byte = 0_u8;
                libc::read(gate.as_raw_fd(), (&mut byte as *mut u8).cast(), 1);
                libc::_exit(0);
            }
        }
        let _inherited = InheritedWriter(pid);
        drop(writer);
        let error = std::process::Command::new(&fixture.path)
            .output()
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::ETXTBSY));
        let evidence = observe_script(fixture.request(8., &cmd(["sentinel"])), |rows| {
            assert_eq!(rows, &[json!({"argument":"sentinel"})]);
            Ok(())
        })
        .unwrap();
        assert_eq!(evidence["owned_live_processes_absent"], true);
    }
    #[test]
    fn group_rss_accounts_for_a_live_child_and_rejects_impossible_samples() {
        let fixture = Fixture::new("sleep 0.8 & wait; printf '{}\\n'");
        let evidence = observe_with(
            fixture.request(4., &[]),
            |r| supervise::run_group_rss(&script_request(r)),
            |rows| {
                ensure!(rows == [json!({})], "missing worker completion");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(evidence["owned_live_processes_absent"], true);
        assert!(evidence["group_rss"]["overlap_samples"].as_u64().unwrap() > 0);
        let mut sample = GroupRssSample {
            leader_kib: 8,
            group_kib: 19,
            members: 2,
        };
        let summary = group_rss_summary(&[sample], &[8]).unwrap();
        assert_eq!(summary["sampled_peak_kib"], 19);
        assert_eq!(summary["sampled_peak_non_leader_kib"], 11);
        assert!(group_rss_summary(&[sample], &[]).is_err());
        assert!(group_rss_summary(&[sample], &[9]).is_err());
        sample.members = 1;
        assert!(group_rss_summary(&[sample], &[8]).is_err());
    }

    #[test]
    fn existing_and_dangling_outputs_reject_before_creating_logs() {
        let fixture = Fixture::new("exit 42");
        fs::create_dir(&fixture.data).unwrap();
        assert!(observe_script(fixture.request(3., &[]), |_| panic!("unexpected oracle")).is_err());
        assert!(!fixture.report.exists());
        fs::remove_dir(&fixture.data).unwrap();
        fs::write(&fixture.report, b"preserve").unwrap();
        assert!(observe_script(fixture.request(3., &[]), |_| panic!("unexpected oracle")).is_err());
        assert_eq!(fs::read(&fixture.report).unwrap(), b"preserve");
        fs::remove_file(&fixture.report).unwrap();
        let target = fixture.dir.path().join("absent");
        symlink(&target, &fixture.report).unwrap();
        assert!(assurance::fresh_output(&fixture.run.root, &fixture.report).is_err());
        assert!(observe_script(fixture.request(3., &[]), |_| panic!("unexpected oracle")).is_err());
        assert!(!target.exists());
        assert!(!fixture.report.with_extension("jsonl").exists());
    }
    #[test]
    fn malformed_json_and_spawn_errors_save_terminal_evidence() {
        let malformed = Fixture::new("printf 'broken\\n'");
        assert!(
            observe_script(malformed.request(4., &[]), |_| panic!("unexpected oracle")).is_err()
        );
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
                observe_script(fixture.request(10., &[]), |_| panic!(
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
            observe_script(
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
        let evidence = observe_script(fixture.request(8., &cmd(["sentinel"])), |rows| {
            assert_eq!(rows, &[json!({"argument":"sentinel"})]);
            Ok(())
        })
        .unwrap();
        assert_eq!(evidence["process_group_killed"], true);
        assert_eq!(evidence["owned_live_processes_absent"], true);
    }
}
