//! Paired clean/instrumented encoder workloads on frozen source archives.
use crate::{
    artifact_trace::{copy_tree, docker_retire, inventory, nonce},
    assurance::{self, Campaign},
    delivery, faults,
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};
const DRIVER: &str = "tools/serialization-scale";
const OVERLAY: [&str; 3] = ["Cargo.toml", "Cargo.lock", "src/main.rs"];
#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum Axis {
    Breadth,
    History,
}
impl Axis {
    fn name(self) -> &'static str {
        match self {
            Self::Breadth => "breadth",
            Self::History => "history",
        }
    }
}
#[derive(Args)]
pub struct Options {
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value = "HEAD")]
    pub revision: String,
    #[arg(long)]
    pub docker_image: Option<String>,
    #[arg(long, value_enum, default_value = "breadth")]
    pub axis: Axis,
    #[arg(long, hide = true)]
    pub worker: Option<PathBuf>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Case {
    width: u64,
    shared: bool,
    roots: Option<u64>,
}
impl Case {
    fn shape(self) -> &'static str {
        if self.shared { "shared" } else { "distinct" }
    }
    fn limited(self) -> bool {
        self.roots.is_none() && !self.shared && self.width == 2048
    }
    fn epochs(self) -> u64 {
        self.roots.unwrap_or(if self.limited() { 7 } else { 8 })
    }
    fn label(self) -> String {
        format!(
            "{}-{}{}",
            self.shape(),
            self.width,
            self.roots.map_or(String::new(), |r| format!("-roots-{r}"))
        )
    }
}
fn cases(axis: Axis) -> Vec<Case> {
    let widths: &[u64] = if axis == Axis::Breadth {
        &[128, 512, 2048]
    } else {
        &[1, 8, 32, 64]
    };
    widths
        .iter()
        .flat_map(|w| {
            [false, true].map(|shared| Case {
                width: if axis == Axis::Breadth { *w } else { 8 },
                shared,
                roots: (axis == Axis::History).then_some(*w),
            })
        })
        .collect()
}
fn phases(case: Case) -> Vec<&'static str> {
    let mut phases = vec!["create"];
    for _ in 0..case.epochs() {
        phases.extend(["commit", "retain", "verify"]);
    }
    if case.limited() {
        phases.extend(["commit", "verify-limit"]);
    }
    phases.extend([
        "reopen",
        "verify-reopen",
        "backup",
        "restore",
        "reopen-copy",
        "verify-copy",
        "reopen-copy",
        "verify-copy",
        "compact",
        "reopen-compact",
        "verify-compact",
    ]);
    phases
}
fn validate_events(events: &[Value], case: Case) -> Result<&Value> {
    let phases = phases(case);
    ensure!(
        events.len() == 2 * phases.len() + 1,
        "missing or extra workload phases"
    );
    for (phase, pair) in phases.iter().zip(events.as_chunks::<2>().0.iter()) {
        ensure!(
            pair[0] == json!({"event":"start","phase":phase})
                && pair[1].as_object().is_some_and(|e| e.len() == 3)
                && pair[1]["event"] == "end"
                && pair[1]["phase"] == *phase
                && pair[1]["ns"].as_u64().is_some(),
            "invalid workload phase sequence: {phase}"
        );
    }
    let final_row = events.last().unwrap();
    for (k, v) in [
        ("event", json!("complete")),
        ("verified", json!(true)),
        ("width", json!(case.width)),
        ("epochs", json!(case.epochs())),
        ("shared", json!(case.shared)),
        ("limited_at", json!(if case.limited() { 8 } else { 0 })),
    ] {
        ensure!(
            final_row[k] == v,
            "unexpected completed workload outcome: {k}"
        );
    }
    if case.limited() {
        ensure!(
            final_row["limit_headroom"]
                .as_u64()
                .is_some_and(|v| v < case.width * 4096),
            "missing independent image exhaustion evidence"
        );
    }
    Ok(final_row)
}
fn paired(a: &Value, b: &Value) -> Result<()> {
    for key in [
        "width",
        "epochs",
        "limited_at",
        "shared",
        "before_bytes",
        "after_bytes",
        "verified",
    ] {
        ensure!(
            a[key] == b[key] && !a[key].is_null(),
            "paired oracle differs: {key}"
        );
    }
    for key in ["before_bytes", "after_bytes"] {
        ensure!(
            a[key].as_u64().is_some_and(|n| n > 0),
            "non-integral image size"
        );
    }
    Ok(())
}
const VALUE: &str = "old.value.same_node(value)";
const LINEAR: &str = ".find(|r| r.revision == head.data_revision)";
const INDEXED: &str =
    "fn compare(a: u64, b: u64) -> std::cmp::Ordering {\n        a.cmp(&b)\n    }";
fn instrument(root: &Path) -> Result<()> {
    let codec = root.join("src/storage/codec.rs");
    let module = root.join("src/storage/mod.rs");
    let mut data = fs::read_to_string(&codec)?;
    let mut api = fs::read_to_string(&module)?;
    ensure!(
        !data.contains("SERIAL_VALUE.fetch_add(")
            && !data.contains("SERIAL_DATA.fetch_add(")
            && !api.contains("fn serialization_probe_counts("),
        "instrumentation already applied"
    );
    ensure!(
        data.matches(VALUE).count() == 1,
        "unique VALUE comparison site required"
    );
    let (site, substitute) = match (data.matches(LINEAR).count(), data.matches(INDEXED).count()) {
        (1, 0) => (
            LINEAR,
            ".find(|r| { SERIAL_DATA.fetch_add(1, std::sync::atomic::Ordering::Relaxed); r.revision == head.data_revision })",
        ),
        (0, 1) => (
            INDEXED,
            "fn compare(a: u64, b: u64) -> std::cmp::Ordering {\n        SERIAL_DATA.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n        a.cmp(&b)\n    }",
        ),
        _ => anyhow::bail!("unique linear or indexed DATA comparison site required"),
    };
    data=data.replace(VALUE,"{ SERIAL_VALUE.fetch_add(1, std::sync::atomic::Ordering::Relaxed); old.value.same_node(value) }").replace(site,substitute);
    data.push_str("\nstatic SERIAL_VALUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);\nstatic SERIAL_DATA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);\npub(super) fn serialization_probe_counts() -> (u64,u64) { (SERIAL_VALUE.load(std::sync::atomic::Ordering::Relaxed), SERIAL_DATA.load(std::sync::atomic::Ordering::Relaxed)) }\n");
    api.push_str("\n/// Disposable experiment only; not a shipped API.\npub fn serialization_probe_counts() -> (u64,u64) { codec::serialization_probe_counts() }\n");
    fs::write(codec, data)?;
    fs::write(module, api)?;
    Ok(())
}
fn clean_env(original: &BTreeMap<OsString, OsString>) -> BTreeMap<OsString, OsString> {
    let mut env = original.clone();
    env.retain(|k, _| {
        let k = k.to_string_lossy();
        matches!(k.as_ref(), "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN")
            || !(k.starts_with("RUST")
                || k.starts_with("CARGO_PROFILE_")
                || k.starts_with("CARGO_TARGET_")
                || k.starts_with("CARGO_BUILD_")
                || matches!(
                    k.as_ref(),
                    "CARGO_ENCODED_RUSTFLAGS"
                        | "CARGO_ENCODED_RUSTDOCFLAGS"
                        | "LLVM_PROFILE_FILE"
                        | "LD_PRELOAD"
                        | "DYLD_INSERT_LIBRARIES"
                ))
    });
    env.insert("CARGO_INCREMENTAL".into(), "0".into());
    env
}
fn observe(
    c: &mut Campaign<'_>,
    binary: &Path,
    case: Case,
    work: &Path,
    label: &str,
) -> Result<Value> {
    let mut args = vec![
        path(binary),
        case.shape().into(),
        case.width.to_string().into(),
        path(work),
    ];
    if let Some(n) = case.roots {
        args.push(n.to_string().into());
    }
    c.sample_rss = true;
    let execution = c.command(args, label, None, 180.);
    c.sample_rss = false;
    let output = execution?;
    let events = output
        .lines()
        .map(|line| delivery::json(line.as_bytes()))
        .collect::<Result<Vec<_>>>()?;
    validate_events(&events, case)?;
    let report: crate::supervise::Report = serde_json::from_value(
        c.evidence["commands"].as_array().unwrap().last().unwrap()["supervision"].clone(),
    )?;
    let peak = report
        .rss_samples_kib
        .iter()
        .max()
        .context("missing process RSS samples")?;
    Ok(
        json!({"shape":case.shape(),"width":case.width,"requested_roots":case.roots,"exit":report.returncode,"elapsed_seconds":report.elapsed_seconds,"events":events,"rss_samples_kib":report.rss_samples_kib,"sampled_peak_rss_bytes":peak.checked_mul(1024).context("RSS overflow")?,"log_sha256":sha(&c.output.join(format!("{label}.stdout.log")))?}),
    )
}
fn workload(c: &mut Campaign<'_>, source: &Path, axis: Axis) -> Result<()> {
    let before = inventory(source, &[])?;
    let work = c.output.clone();
    let config_env = c.env.clone();
    c.env = clean_env(&c.env);
    c.evidence["axis"] = json!(axis.name());
    c.evidence["source_hashes"] = json!(before);
    c.evidence["runs"] = json!([]);
    c.evidence["removed_environment_names"] = json!(
        config_env
            .keys()
            .filter(|k| !c.env.contains_key(*k))
            .map(|k| k.to_string_lossy())
            .collect::<Vec<_>>()
    );
    let rustc = c.command(cmd(["rustc", "-vV"]), "rustc", None, 30.)?;
    let cargo = c.command(cmd(["cargo", "-V"]), "cargo", None, 30.)?;
    c.evidence["rustc"] = json!(rustc);
    c.evidence["cargo"] = json!(cargo);
    let fs = if cfg!(target_os = "linux") {
        vec![
            "findmnt".into(),
            "-T".into(),
            path(&work),
            "-n".into(),
            "-o".into(),
            "FSTYPE,SOURCE,TARGET".into(),
        ]
    } else {
        cmd([OsStr::new("df"), OsStr::new("-P"), work.as_os_str()])
    };
    let filesystems = c.command(fs, "filesystem", None, 30.)?;
    c.evidence["filesystem"] = json!(filesystems);
    let mut config = BTreeMap::new();
    let cargo_home = crate::toolchain::cargo_home(source, &c.env)?;
    for start in [source, &c.run.root, &cargo_home] {
        for p in start.ancestors() {
            for name in [
                ".cargo/config",
                ".cargo/config.toml",
                "config",
                "config.toml",
            ] {
                let f = p.join(name);
                if f.is_file() {
                    config.insert(f.clone(), sha(&f)?);
                }
            }
        }
    }
    c.evidence["cargo_configuration_sha256"] = json!(config);
    let requested = cases(axis);
    let mut clean = vec![];
    for variant in ["clean", "instrumented"] {
        let dir = work.join(variant);
        fs::create_dir(&dir)?;
        let copy = dir.join("source");
        copy_tree(source, &copy)?;
        if variant == "instrumented" {
            instrument(&copy)?;
        }
        let inventory_before = inventory(&copy, &[])?;
        c.evidence[format!("{variant}_source_hashes")] = json!(inventory_before);
        let target = dir.join("cargo-target");
        let mut args = vec![
            "cargo".into(),
            "build".into(),
            "--offline".into(),
            "--locked".into(),
            "--release".into(),
            "--manifest-path".into(),
            path(copy.join(DRIVER).join("Cargo.toml")),
            "--target-dir".into(),
            path(&target),
        ];
        if variant == "instrumented" {
            args.extend(cmd(["--features", "probe"]));
        }
        c.command_at(args, &format!("build-{variant}"), None, 180., &copy)?;
        let binary = target.join("release/weekend-serialization-scale");
        let digest = sha(&binary)?;
        fs::copy(&binary, dir.join("weekend-serialization-scale"))?;
        c.evidence[format!("{variant}_binary_sha256")] = json!(digest);
        for (index, case) in requested.iter().enumerate() {
            let label = case.label();
            let mut row = observe(
                c,
                &binary,
                *case,
                &dir.join(&label),
                &format!("{variant}-{label}"),
            )?;
            let final_row = row["events"].as_array().unwrap().last().unwrap().clone();
            let value = final_row["value_comparisons"]
                .as_u64()
                .context("VALUE comparison count missing")?;
            let data = final_row["data_comparisons"]
                .as_u64()
                .context("DATA comparison count missing")?;
            if variant == "clean" {
                ensure!(value == 0 && data == 0, "clean workload is instrumented");
                clean.push(final_row);
            } else {
                ensure!(
                    value > 0,
                    "instrumented workload did not observe VALUE comparisons"
                );
                paired(&clean[index], &final_row)?;
            }
            row["variant"] = json!(variant);
            c.evidence["runs"].as_array_mut().unwrap().push(row);
            c.save()?;
        }
        ensure!(
            sha(&binary)? == digest && inventory(&copy, &[])? == inventory_before,
            "binary or copied source changed"
        );
    }
    ensure!(inventory(source, &[])? == before, "base source changed");
    ensure!(
        c.evidence["runs"].as_array().unwrap().len() == requested.len() * 2,
        "incomplete pairs"
    );
    Ok(())
}
fn output_path(root: &Path, output: &Path) -> Result<PathBuf> {
    let output = assurance::absolute(root, output);
    ensure!(
        fs::symlink_metadata(&output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            && output.parent().is_some_and(Path::is_dir),
        "output must be fresh with existing parent"
    );
    crate::notices::resolve(&output)
}
fn cleanup_commands(name: &str, volume: &str) -> Result<[Vec<OsString>; 2]> {
    let id = name
        .strip_prefix("serialization-scale-")
        .context("unowned serialization container")?;
    ensure!(
        id.len() == 32
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && volume == format!("{name}-data"),
        "unowned serialization volume"
    );
    Ok([
        cmd(["docker", "rm", "-f", name]),
        cmd(["docker", "volume", "rm", volume]),
    ])
}
fn docker(
    c: &mut Campaign<'_>,
    source: &Path,
    output: &Path,
    image: &str,
    axis: Axis,
) -> Result<Value> {
    let image = c.command(
        cmd(["docker", "image", "inspect", "--format={{.Id}}", image]),
        "image",
        None,
        30.,
    )?;
    ensure!(
        image.len() == 71
            && image.starts_with("sha256:")
            && image[7..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "immutable local Docker image required"
    );
    let runner = c.output.join("runner");
    faults::stage_runner(&c.run.root, &runner)?;
    let frozen = inventory(&runner, &[])?;
    let registry = crate::toolchain::cargo_home(&c.run.root, &c.env)?.join("registry");
    let name = format!("serialization-scale-{}", nonce()?);
    let volume = format!("{name}-data");
    c.evidence["docker"] = json!({"image":image,"container":name,"volume":volume,"cleanup_verified":false,"runner_frozen_sha256":frozen});
    c.save()?;
    let result = (|| -> Result<Value> {
        c.command(
            cmd(["docker", "volume", "create", &volume]),
            "create-volume",
            None,
            30.,
        )?;
        let script = "set -eu; cargo build --offline --locked --manifest-path /runner/Cargo.toml -p xtask --target-dir /data/runner-target; exec /data/runner-target/debug/xtask serialization-scale --worker /src --output /data/results --axis \"$1\"";
        let execute = c.command(
            vec![
                "docker".into(),
                "run".into(),
                "--pull=never".into(),
                "--network=none".into(),
                "--memory=2g".into(),
                "--cpus=2".into(),
                "--pids-limit=128".into(),
                "--name".into(),
                name.clone().into(),
                "--mount".into(),
                format!("type=volume,source={volume},target=/data").into(),
                "--mount".into(),
                format!("type=bind,source={},target=/src,readonly", source.display()).into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/runner,readonly",
                    runner.display()
                )
                .into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    registry.display()
                )
                .into(),
                image.clone().into(),
                "sh".into(),
                "-c".into(),
                script.into(),
                "serialization-scale".into(),
                axis.name().into(),
            ],
            "docker-run",
            None,
            900.,
        );
        let copied = c.command(
            vec![
                "docker".into(),
                "cp".into(),
                format!("{name}:/data/results").into(),
                path(output),
            ],
            "copyback",
            None,
            180.,
        );
        execute?;
        copied?;
        let proof = delivery::read_json(&output.join("evidence.json"))?;
        ensure!(
            proof["passed"] == true && proof["verified"] == true,
            "serialization worker did not qualify"
        );
        ensure!(
            proof["workload"]["source_hashes"] == json!(inventory(source, &[])?)
                && inventory(&runner, &[])? == frozen,
            "frozen source/runner changed"
        );
        let copied = inventory(output, &["evidence.json", "cargo-target"])?;
        ensure!(
            proof["artifact_sha256"] == json!(copied),
            "copied serialization evidence mismatch"
        );
        Ok(proof["workload"].clone())
    })();
    let cleanup = docker_retire(c, &name, &volume, cleanup_commands(&name, &volume)?);
    c.evidence["docker"]["cleanup_verified"] = json!(cleanup.is_ok());
    c.save()?;
    cleanup?;
    result
}
fn worker(run: &Run, source: &Path, output: &Path, axis: Axis) -> Result<()> {
    let mut c = Campaign::detached(run, output, "detached serialization workload worker")?;
    let result = workload(&mut c, source, axis);
    c.evidence["verified"] = json!(result.is_ok());
    if result.is_ok() {
        let fields = [
            "axis",
            "source_hashes",
            "runs",
            "rustc",
            "cargo",
            "filesystem",
            "clean_binary_sha256",
            "instrumented_binary_sha256",
        ];
        c.evidence["workload"] = Value::Object(
            fields
                .into_iter()
                .map(|key| (key.into(), c.evidence[key].clone()))
                .collect(),
        );
        c.evidence["artifact_sha256"] =
            json!(inventory(&c.output, &["evidence.json", "cargo-target"])?);
    }
    c.finish(result)
}
pub fn run(run: &Run, opts: Options) -> Result<()> {
    let output = output_path(&run.root, &opts.output)?;
    if let Some(source) = opts.worker {
        return worker(run, &source, &output, opts.axis);
    }
    let mut c = Campaign::new(
        run,
        &output,
        "paired fixed breadth/revisions; sampled RSS, no maximum bound",
    )?;
    c.evidence["verified"] = json!(false);
    c.evidence["axis"] = json!(opts.axis.name());
    c.save()?;
    let result = (|| -> Result<()> {
        let revision = c.command(
            vec![
                "git".into(),
                "rev-parse".into(),
                "--verify".into(),
                "--end-of-options".into(),
                format!("{}^{{commit}}", opts.revision).into(),
            ],
            "engine-revision",
            None,
            30.,
        )?;
        ensure!(
            revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid engine revision"
        );
        let entries = c.command(
            cmd(["git", "ls-tree", "-r", &revision]),
            "archive-inventory",
            None,
            30.,
        )?;
        ensure!(
            entries
                .lines()
                .all(|s| s.starts_with("100644 blob ") || s.starts_with("100755 blob ")),
            "nonregular archive input"
        );
        let archive = output.join("source.tar");
        let source = output.join("source");
        fs::create_dir(&source)?;
        c.command(
            vec![
                "git".into(),
                "archive".into(),
                "--format=tar".into(),
                "--output".into(),
                path(&archive),
                revision.clone().into(),
            ],
            "archive",
            None,
            30.,
        )?;
        c.command(
            vec![
                "tar".into(),
                "-xf".into(),
                path(&archive),
                "-C".into(),
                path(&source),
            ],
            "extract",
            None,
            30.,
        )?;
        let mut overlay = BTreeMap::new();
        for name in OVERLAY {
            let src = run.root.join(DRIVER).join(name);
            let dest = source.join(DRIVER).join(name);
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::copy(&src, &dest)?;
            overlay.insert(name, sha(&dest)?);
        }
        let frozen = inventory(&source, &[])?;
        c.evidence["engine_source"] = json!(revision);
        c.evidence["archive_sha256"] = json!(sha(&archive)?);
        c.evidence["overlay_hashes"] = json!(overlay);
        c.evidence["engine_source_hashes"] = json!(frozen);
        let proof = output.join("run");
        let summary = if let Some(image) = opts.docker_image {
            docker(&mut c, &source, &proof, &image, opts.axis)?
        } else {
            worker(run, &source, &proof, opts.axis)?;
            let v = delivery::read_json(&proof.join("evidence.json"))?;
            ensure!(
                v["passed"] == true && v["verified"] == true,
                "serialization workload did not qualify"
            );
            v["workload"].clone()
        };
        ensure!(
            inventory(&source, &[])? == frozen,
            "archived source overlay changed"
        );
        c.evidence["run_evidence_sha256"] = json!(sha(&proof.join("evidence.json"))?);
        c.evidence["workload"] = summary;
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    fn example(case: Case) -> Vec<Value> {
        let mut v: Vec<_> = phases(case)
            .into_iter()
            .flat_map(|phase| {
                [
                    json!({"event":"start","phase":phase}),
                    json!({"event":"end","phase":phase,"ns":1}),
                ]
            })
            .collect();
        v.push(json!({"event":"complete","verified":true,"width":case.width,"epochs":case.epochs(),"shared":case.shared,"limited_at":if case.limited(){8}else{0},"limit_headroom":32,"before_bytes":65536,"after_bytes":61440}));
        v
    }
    fn mutate(row: &[Value], key: &str, value: Value) -> Vec<Value> {
        let mut v = row.to_vec();
        v.last_mut().unwrap()[key] = value;
        v
    }
    fn omit(row: &[Value], phase: &str) -> Vec<Value> {
        let mut v = row.to_vec();
        let i = v.iter().position(|v| v["phase"] == phase).unwrap();
        v.drain(i..i + 2);
        v
    }
    #[test]
    fn fresh_outputs_reject_existing_and_dangling_before_any_command() {
        let t = Scratch::new(&std::env::temp_dir(), "scale-admission").unwrap();
        let existing = t.path().join("existing");
        let dangling = t.path().join("dangling");
        fs::create_dir(&existing).unwrap();
        std::os::unix::fs::symlink(t.path().join("missing"), &dangling).unwrap();
        let mut run = Run::new(t.path().to_owned());
        run.env.insert("PATH".into(), "/no-programs".into());
        for output in [existing, dangling, t.path().join("missing/child")] {
            let error = super::run(
                &run,
                Options {
                    output,
                    axis: Axis::Breadth,
                    revision: "unknown-revision".into(),
                    docker_image: None,
                    worker: None,
                },
            )
            .unwrap_err();
            assert!(error.to_string().contains("output must be fresh"));
        }
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 2);
    }
    #[test]
    fn build_overrides_removed_but_rustup_selection_preserved() {
        let env = [
            ("RUSTFLAGS", "bad"),
            ("RUSTC_WRAPPER", "bad"),
            ("CARGO_PROFILE_RELEASE_PANIC", "abort"),
            ("CARGO_BUILD_RUSTFLAGS", "bad"),
            ("RUSTUP_HOME", "/toolchain"),
            ("RUSTUP_TOOLCHAIN", "1.99.0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        assert_eq!(
            clean_env(&env),
            [
                ("RUSTUP_HOME".into(), "/toolchain".into()),
                ("RUSTUP_TOOLCHAIN".into(), "1.99.0".into()),
                ("CARGO_INCREMENTAL".into(), "0".into())
            ]
            .into()
        );
    }
    #[test]
    fn breadth_rejects_early_limit_skipped_maintenance_and_wrong_result_types() {
        let case = cases(Axis::Breadth)[0];
        let valid = example(case);
        validate_events(&valid, case).unwrap();
        let mut extra = valid.clone();
        extra.push(valid.last().unwrap().clone());
        let mut early = mutate(&valid, "epochs", json!(7));
        early.last_mut().unwrap()["limited_at"] = json!(8);
        for bad in [
            early,
            mutate(&valid, "width", json!(512)),
            mutate(&valid, "shared", json!(true)),
            mutate(&valid, "verified", json!(1)),
            omit(&valid, "backup"),
            extra,
        ] {
            assert!(validate_events(&bad, case).is_err());
        }
        let limited = cases(Axis::Breadth)[4];
        let result = example(limited);
        validate_events(&result, limited).unwrap();
        for invalid in [json!(-1), json!(false), json!(2048 * 4096), Value::Null] {
            assert!(validate_events(&mutate(&result, "limit_headroom", invalid), limited).is_err());
        }
        let first = valid.last().unwrap();
        let changed = mutate(&valid, "after_bytes", json!(4096));
        assert!(paired(first, changed.last().unwrap()).is_err());
        paired(first, first).unwrap();
    }
    #[test]
    fn history_requires_all_roots_and_every_maintenance_phase() {
        for case in cases(Axis::History) {
            let valid = example(case);
            validate_events(&valid, case).unwrap();
            for bad in [
                mutate(&valid, "epochs", json!(case.epochs() - 1)),
                mutate(&valid, "limited_at", json!(case.epochs())),
                omit(&valid, "restore"),
                omit(&valid, "commit"),
            ] {
                assert!(validate_events(&bad, case).is_err());
            }
        }
    }
    #[test]
    fn cleanup_targets_only_matching_owned_resources() {
        let id = "a".repeat(32);
        let name = format!("serialization-scale-{id}");
        let volume = format!("{name}-data");
        assert_eq!(
            cleanup_commands(&name, &volume).unwrap(),
            [
                cmd(["docker", "rm", "-f", &name]),
                cmd(["docker", "volume", "rm", &volume])
            ]
        );
        for (c, v) in [
            ("unrelated", volume.as_str()),
            (name.as_str(), "unrelated"),
            ("--all", volume.as_str()),
        ] {
            assert!(cleanup_commands(c, v).is_err());
        }
    }
    #[test]
    fn instrumentation_targets_current_unique_sites_and_rejects_reapplication() {
        let t = Scratch::new(&std::env::temp_dir(), "scale-instrument").unwrap();
        fs::create_dir_all(t.path().join("src/storage")).unwrap();
        for (name, bytes) in [
            (
                "codec.rs",
                include_bytes!("../../src/storage/codec.rs").as_slice(),
            ),
            (
                "mod.rs",
                include_bytes!("../../src/storage/mod.rs").as_slice(),
            ),
        ] {
            fs::write(t.path().join("src/storage").join(name), bytes).unwrap();
        }
        instrument(t.path()).unwrap();
        let codec = fs::read_to_string(t.path().join("src/storage/codec.rs")).unwrap();
        assert_eq!(codec.matches("SERIAL_VALUE.fetch_add(").count(), 1);
        assert_eq!(codec.matches("SERIAL_DATA.fetch_add(").count(), 1);
        assert!(codec.contains("Ordering::Relaxed); old.value.same_node(value)"));
        assert!(instrument(t.path()).is_err());
    }
    #[test]
    fn instrumentation_accepts_linear_or_indexed_and_rejects_missing_duplicate_sites() {
        for (site, ok) in [
            (LINEAR.to_owned(), true),
            (INDEXED.to_owned(), true),
            (String::new(), false),
            (format!("{LINEAR}\n{INDEXED}"), false),
            (format!("{INDEXED}\n{INDEXED}"), false),
        ] {
            let t = Scratch::new(&std::env::temp_dir(), "scale-site").unwrap();
            let storage = t.path().join("src/storage");
            fs::create_dir_all(&storage).unwrap();
            let before = format!("{VALUE}\n{site}");
            let codec = storage.join("codec.rs");
            let module = storage.join("mod.rs");
            fs::write(&codec, &before).unwrap();
            fs::write(&module, "").unwrap();
            assert_eq!(instrument(t.path()).is_ok(), ok);
            if ok {
                let changed = fs::read_to_string(&codec).unwrap();
                assert_eq!(changed.matches("SERIAL_DATA.fetch_add(").count(), 1);
                assert_eq!(changed.matches("SERIAL_VALUE.fetch_add(").count(), 1);
                assert!(instrument(t.path()).is_err());
            } else {
                assert_eq!(fs::read_to_string(&codec).unwrap(), before);
                assert!(fs::read_to_string(&module).unwrap().is_empty());
            }
        }
    }
}
