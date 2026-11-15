//! Fault qualification orchestration. The engine archive/package and the runner
//! are separate inputs; a historical engine never needs to contain today's tool.
use crate::{
    supervise::{self, Request},
    support::{Run, hash_tree, sha},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const IMAGE: &str = "sha256:c9f225b68fc1af1c121c0473d91bca23f81a3171fd3ae0a063a1daf8bfabec08";
#[derive(Subcommand)]
pub enum Task {
    Native(Native),
    Delivered(Delivered),
}
#[derive(Args)]
pub struct Native {
    #[arg(long,default_value=IMAGE)]
    image: String,
    #[arg(long, default_value = "HEAD")]
    source: String,
    #[arg(long)]
    output: Option<PathBuf>,
}
#[derive(Args)]
pub struct Delivered {
    #[arg(long)]
    package: Option<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    #[arg(long,default_value=IMAGE)]
    image: String,
    #[arg(long, hide = true)]
    worker: bool,
}
fn nonce() -> String {
    format!(
        "{:032x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            ^ ((std::process::id() as u128) << 80)
    )
}
fn fresh(path: &Path) -> Result<PathBuf> {
    let path = env::current_dir()?.join(path);
    ensure!(
        fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "output already exists"
    );
    let parent = dunce::canonicalize(path.parent().context("output parent")?)?;
    let path = parent.join(path.file_name().context("output name")?);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(&path)?;
    Ok(path)
}
fn inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut hashes = hash_tree(root)?;
    if root.join("manifest.json").is_file() {
        hashes.insert("manifest.json".into(), sha(&root.join("manifest.json"))?);
    }
    Ok(hashes)
}
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        ensure!(!ty.is_symlink(), "symlink in input");
        let target = to.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            ensure!(ty.is_file(), "nonregular input");
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
struct Campaign {
    output: PathBuf,
    root: PathBuf,
    evidence: Value,
    counter: usize,
    cancellation: supervise::Cancellation,
}
impl Campaign {
    fn new(root: &Path, output: PathBuf) -> Result<Self> {
        Ok(Self {
            output,
            root: root.to_owned(),
            evidence: json!({"verified":false,"commands":[]}),
            counter: 0,
            cancellation: supervise::Cancellation::new()?,
        })
    }
    fn call(
        &mut self,
        args: Vec<String>,
        label: &str,
        timeout: f64,
        env: Option<BTreeMap<String, String>>,
        allowed: &[i32],
    ) -> Result<String> {
        ensure!(
            !self.cancellation.requested()
                || matches!(
                    label,
                    "remove-container" | "remove-volume" | "container-absence" | "volume-absence"
                ),
            "campaign cancelled"
        );
        self.counter += 1;
        let log = self.output.join(format!("{:03}-{label}.log", self.counter));
        let request = Request {
            argv: args.clone(),
            cwd: Some(self.root.clone()),
            env,
            stdout: log.clone(),
            stderr: log.clone(),
            report: None,
            timeout_seconds: timeout,
            maximum_bytes: 8 * 1024 * 1024,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        };
        let outcome = supervise::run(&request)?;
        self.evidence["commands"]
            .as_array_mut()
            .unwrap()
            .push(json!({"command":args,"label":label,"log":log,"outcome":outcome}));
        ensure!(
            outcome.cleanup_verified && outcome.cleanup_failure.is_none(),
            "{label}: cleanup unverified: {:?}",
            outcome.cleanup_failure
        );
        ensure!(
            (outcome.success() || allowed.contains(&outcome.returncode.unwrap_or(i32::MIN)))
                && !outcome.timed_out
                && !outcome.cancelled
                && outcome
                    .failure
                    .as_ref()
                    .is_none_or(|f| f.starts_with("child exit ")),
            "{label}: {:?}",
            outcome.failure
        );
        Ok(fs::read_to_string(log)?)
    }
    fn ok(&mut self, args: Vec<String>, label: &str, timeout: f64) -> Result<String> {
        self.call(args, label, timeout, None, &[0])
    }
    fn save(&self) -> Result<()> {
        fs::write(
            self.output.join("evidence.json"),
            format!("{}\n", serde_json::to_string_pretty(&self.evidence)?),
        )?;
        Ok(())
    }
}
fn s(p: impl AsRef<Path>) -> String {
    p.as_ref().to_string_lossy().into_owned()
}
fn v(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}
fn image(c: &mut Campaign, id: &str) -> Result<()> {
    ensure!(
        id.len() == 71
            && id.starts_with("sha256:")
            && id[7..]
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "immutable already-local Docker image required"
    );
    ensure!(
        c.ok(
            v(&["docker", "image", "inspect", "--format={{.Id}}", id]),
            "image",
            30.
        )?
        .trim()
            == id,
        "Docker image identity mismatch"
    );
    Ok(())
}
// Both retirements are attempted even if the first command/probe fails. Absence
// queries must succeed; a timed-out daemon is never proof of absence.
fn cleanup(c: &mut Campaign, name: &str, volume: Option<&str>) -> Result<bool> {
    cleanup_with(name, volume, |args, label, allowed| {
        c.call(args, label, 30., None, allowed)
    })
}
fn cleanup_with(
    name: &str,
    volume: Option<&str>,
    mut call: impl FnMut(Vec<String>, &str, &[i32]) -> Result<String>,
) -> Result<bool> {
    ensure!(
        (name.starts_with("weekend-native-faults-")
            || name.starts_with("weekend-delivered-faults-"))
            && name
                .rsplit('-')
                .next()
                .is_some_and(|x| x.len() == 32 && x.bytes().all(|b| b.is_ascii_hexdigit())),
        "unowned container"
    );
    ensure!(
        volume.is_none_or(|v| v == format!("{name}-data")),
        "unowned volume"
    );
    let _ = call(
        v(&["docker", "rm", "-f", name]),
        "remove-container",
        &[0, 1],
    );
    if let Some(volume) = volume {
        let _ = call(
            v(&["docker", "volume", "rm", volume]),
            "remove-volume",
            &[0, 1],
        );
    }
    let containers = call(
        v(&["docker", "ps", "-a", "--format={{.Names}}"]),
        "container-absence",
        &[0],
    );
    let volumes = if volume.is_some() {
        Some(call(
            v(&["docker", "volume", "ls", "--format={{.Name}}"]),
            "volume-absence",
            &[0],
        ))
    } else {
        None
    };
    Ok(!containers?.lines().any(|n| n == name)
        && match (volume, volumes) {
            (Some(v), Some(names)) => !names?.lines().any(|n| n == v),
            _ => true,
        })
}
pub fn run(run: &Run, task: Task) -> Result<()> {
    ensure!(cfg!(unix), "fault campaigns require a POSIX host");
    match task {
        Task::Native(opts) => native(run, opts),
        Task::Delivered(opts) => {
            if opts.worker {
                worker(run, &opts.output)
            } else {
                delivered(run, opts)
            }
        }
    }
}
fn native(run: &Run, opts: Native) -> Result<()> {
    let output =
        fresh(&opts.output.unwrap_or_else(|| {
            env::temp_dir().join(format!("weekend-native-faults-{}", nonce()))
        }))?;
    let mut c = Campaign::new(&run.root, output.clone())?;
    c.evidence["scope"] =
        json!("native Linux errors on bounded tmpfs; not durable device power loss");
    c.save()?;
    let name = format!("weekend-native-faults-{}", nonce());
    let mut acquired = false;
    let result = (|| -> Result<()> {
        let source = c
            .ok(
                vec![
                    "git".into(),
                    "rev-parse".into(),
                    "--verify".into(),
                    format!("{}^{{commit}}", opts.source),
                ],
                "source",
                30.,
            )?
            .trim()
            .to_owned();
        ensure!(
            source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid source identity"
        );
        image(&mut c, &opts.image)?;
        let archive = output.join("source.tar");
        c.ok(
            vec![
                "git".into(),
                "archive".into(),
                "--output".into(),
                s(&archive),
                source.clone(),
            ],
            "archive",
            30.,
        )?;
        let tree = output.join("source");
        fs::create_dir(&tree)?;
        c.ok(
            vec![
                "tar".into(),
                "-xf".into(),
                s(&archive),
                "-C".into(),
                s(&tree),
            ],
            "extract",
            30.,
        )?;
        let overlay = "examples/native_faults.rs";
        fs::copy(run.root.join(overlay), tree.join(overlay))?;
        let before = inventory(&tree)?;
        c.evidence["engine_source"] = json!(source);
        c.evidence["source_archive_sha256"] = json!(sha(&archive)?);
        c.evidence["driver_overlay"] = json!({overlay:sha(&tree.join(overlay))?});
        c.evidence["source_files_sha256"] = json!(before);
        c.evidence["runner_sha256"] = json!(sha(&env::current_exe()?)?);
        c.evidence["image"] = json!(opts.image);
        let script = include_str!("native-faults.sh");
        let registry = env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap()).join(".cargo"))
            .join("registry");
        acquired = true;
        let text = c.ok(
            vec![
                "docker".into(),
                "run".into(),
                "--pull=never".into(),
                "--name".into(),
                name.clone(),
                "--network=none".into(),
                "--memory=2g".into(),
                "--pids-limit=256".into(),
                "--tmpfs".into(),
                "/faults:rw,size=8m,mode=755".into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/source,readonly",
                    tree.display()
                ),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    registry.display()
                ),
                "-e".into(),
                "CARGO_INCREMENTAL=0".into(),
                "-e".into(),
                format!("WEEKEND_DB_SOURCE_ID={source}"),
                opts.image,
                "sh".into(),
                "-c".into(),
                script.into(),
            ],
            "faults",
            300.,
        )?;
        ensure!(
            text.lines().any(|l| l == "NATIVE_FAULTS_COMPLETED"),
            "missing completion marker"
        );
        ensure!(inventory(&tree)? == before, "source changed");
        c.evidence["completion_marker"] = json!(true);
        c.evidence["source_stable"] = json!(true);
        Ok(())
    })();
    let retired = if acquired {
        cleanup(&mut c, &name, None)
    } else {
        Ok(true)
    };
    finish(c, result, retired)
}
fn finish(mut c: Campaign, result: Result<()>, retired: Result<bool>) -> Result<()> {
    let result = result.and_then(|()| {
        ensure!(!c.cancellation.requested(), "campaign cancelled");
        Ok(())
    });
    c.evidence["owned_resources_absent"] = json!(retired.as_ref().is_ok_and(|v| *v));
    if let Err(e) = &result {
        c.evidence["failure"] = json!(format!("{e:#}"));
    }
    if let Err(e) = &retired {
        c.evidence["cleanup_failure"] = json!(format!("{e:#}"));
    }
    c.evidence["verified"] = json!(result.is_ok() && retired.as_ref().is_ok_and(|v| *v));
    c.save()?;
    result?;
    ensure!(retired?, "owned Docker resources remain");
    println!("{}", c.output.join("evidence.json").display());
    Ok(())
}
// Preserve the workspace graph and original lockfile. This is a separate
// current-tool snapshot; the archived engine/exact package remains independent.
pub(crate) fn stage_runner(root: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    let list = target.join(".runner-files.stdout");
    let request = Request {
        argv: v(&["git", "ls-files", "-z"]),
        cwd: Some(root.to_owned()),
        env: None,
        stdout: list.clone(),
        stderr: target.join(".runner-files.stderr"),
        report: None,
        timeout_seconds: 30.,
        maximum_bytes: 8 * 1024 * 1024,
        exclusive_limit: false,
        file_limit: None,
        journal: None,
        sample_rss: false,
    };
    let report = supervise::run(&request)?;
    ensure!(
        report.success(),
        "runner source inventory failed: {report:?}"
    );
    for name in fs::read(&list)?
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
    {
        let name = std::str::from_utf8(name)?;
        let relative = Path::new(name);
        ensure!(
            !relative.is_absolute()
                && !relative
                    .components()
                    .any(|p| p == std::path::Component::ParentDir),
            "unsafe runner path"
        );
        let from = root.join(relative);
        ensure!(
            fs::symlink_metadata(&from)?.is_file(),
            "nonregular runner input: {name}"
        );
        let to = target.join(relative);
        fs::create_dir_all(to.parent().unwrap())?;
        fs::copy(from, to)?;
    }
    ensure!(
        hash_tree(&root.join("xtask"))? == hash_tree(&target.join("xtask"))?,
        "runner has untracked/missing files; checkpoint them first"
    );
    ensure!(
        fs::read(root.join("Cargo.lock"))? == fs::read(target.join("Cargo.lock"))?,
        "runner lockfile changed"
    );
    Ok(())
}

fn delivered(run: &Run, opts: Delivered) -> Result<()> {
    let package = dunce::canonicalize(opts.package.context("--package required")?)?;
    let candidate = env::current_dir()?.join(&opts.output);
    let candidate = dunce::canonicalize(candidate.parent().context("output parent")?)?
        .join(candidate.file_name().context("output name")?);
    ensure!(
        !candidate.starts_with(&package),
        "output must be outside package"
    );
    let output = fresh(&candidate)?;
    let mut c = Campaign::new(&run.root, output.clone())?;
    c.save()?;
    let name = format!("weekend-delivered-faults-{}", nonce());
    let volume = format!("{name}-data");
    let mut acquired = false;
    let result = (|| -> Result<()> {
        let info = crate::delivered_faults::package(&package)?;
        image(&mut c, &opts.image)?;
        let inputs = output.join("inputs");
        fs::create_dir(&inputs)?;
        for n in ["consumer.c", "faults.c"] {
            fs::copy(
                run.root.join("tools/delivered-faults").join(n),
                inputs.join(n),
            )?;
        }
        copy_tree(&package, &output.join("package"))?;
        let frozen = inventory(&inputs)?;
        let manifest = sha(&package.join("manifest.json"))?;
        let runner = output.join("runner");
        stage_runner(&run.root, &runner)?;
        // Only xtask is built from this current snapshot; the consumer links
        // the independently verified package and its embedded engine source ID.
        c.evidence["source"] = info["source"].clone();
        c.evidence["inputs_sha256"] = json!(frozen);
        let runner_before = inventory(&runner)?;
        c.evidence["runner_inputs_sha256"] = json!(runner_before);
        c.evidence["package_manifest_sha256"] = json!(manifest);
        c.evidence["image"] = json!(opts.image);
        c.evidence["container"] = json!(name);
        c.evidence["volume"] = json!(volume);
        acquired = true;
        c.ok(
            v(&["docker", "volume", "create", &volume]),
            "create-volume",
            30.,
        )?;
        let registry = env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap()).join(".cargo"))
            .join("registry");
        let script = "set -eu; cargo build --offline --locked --manifest-path /seed/runner/Cargo.toml -p xtask --target-dir /work/tool; exec /work/tool/debug/xtask faults delivered --worker --output /work/result";
        let execution = c.ok(
            vec![
                "docker".into(),
                "run".into(),
                "--pull=never".into(),
                "--name".into(),
                name.clone(),
                "--network=none".into(),
                "--memory=2g".into(),
                "--cpus=2".into(),
                "--pids-limit=128".into(),
                "--ulimit=core=0".into(),
                "--tmpfs".into(),
                "/faults:rw,size=8m,mode=700".into(),
                "--mount".into(),
                format!("type=volume,source={volume},target=/work"),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/seed,readonly",
                    output.display()
                ),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    registry.display()
                ),
                opts.image,
                "sh".into(),
                "-c".into(),
                script.into(),
            ],
            "docker",
            300.,
        );
        let copied = c.ok(
            v(&[
                "docker",
                "cp",
                &format!("{name}:/work/result"),
                &s(output.join("result")),
            ]),
            "copy-result",
            60.,
        );
        execution?;
        copied?;
        let result = output.join("result");
        let proof: Value = serde_json::from_slice(&fs::read(result.join("evidence.json"))?)?;
        ensure!(proof["verified"] == true, "worker failed");
        for (name, digest) in proof["artifact_sha256"]
            .as_object()
            .context("artifact hashes")?
        {
            ensure!(
                !Path::new(name).is_absolute()
                    && !Path::new(name)
                        .components()
                        .any(|p| p == std::path::Component::ParentDir),
                "invalid artifact path"
            );
            ensure!(
                sha(&result.join(name))? == digest.as_str().context("artifact digest")?,
                "copied proof mismatch"
            );
        }
        ensure!(
            proof["inputs_sha256"] == json!(frozen)
                && inventory(&inputs)? == frozen
                && sha(&package.join("manifest.json"))? == manifest,
            "seed changed"
        );
        crate::delivered_faults::package(&package)?;
        crate::delivered_faults::package(&result.join("relocated-package"))?;
        ensure!(
            proof["results"].as_array().context("results")?.len() == 6,
            "missing cases"
        );
        ensure!(
            inventory(&runner)? == runner_before,
            "runner source changed"
        );
        c.evidence["result_evidence_sha256"] = json!(sha(&result.join("evidence.json"))?);
        Ok(())
    })();
    let retired = if acquired {
        cleanup(&mut c, &name, Some(&volume))
    } else {
        Ok(true)
    };
    finish(c, result, retired)
}
fn worker(run: &Run, output: &Path) -> Result<()> {
    let output = fresh(output)?;
    let mut c = Campaign::new(&run.root, output.clone())?;
    c.save()?;
    let result = (|| -> Result<()> {
        let package = output.join("relocated-package");
        copy_tree(Path::new("/seed/package"), &package)?;
        let info = crate::delivered_faults::package(&package)?;
        let inputs = output.join("inputs");
        copy_tree(Path::new("/seed/inputs"), &inputs)?;
        let frozen = inventory(&inputs)?;
        let manifest = sha(&package.join("manifest.json"))?;
        let mut env: BTreeMap<String, String> = env::vars()
            .filter(|(k, _)| {
                !matches!(
                    k.as_str(),
                    "LD_PRELOAD"
                        | "LD_LIBRARY_PATH"
                        | "LD_DEBUG"
                        | "QUALIFICATION_PRIVATE_TMPFS"
                        | "RUSTFLAGS"
                        | "CARGO_ENCODED_RUSTFLAGS"
                        | "RUSTC"
                        | "RUSTC_WRAPPER"
                        | "RUSTC_WORKSPACE_WRAPPER"
                        | "CARGO_BUILD_TARGET"
                ) && !k.starts_with("CARGO_PROFILE_")
                    && !(k.starts_with("CARGO_TARGET_") && k.ends_with("_RUSTFLAGS"))
            })
            .collect();
        env.insert("CARGO_INCREMENTAL".into(), "0".into());
        c.root = output.clone(); // Consumer paths and rpath are evaluated after relocation.
        c.evidence["source"] = info["source"].clone();
        c.evidence["inputs_sha256"] = json!(frozen);
        c.evidence["package_manifest_sha256"] = json!(manifest);
        c.evidence["runner_sha256"] = json!(sha(&env::current_exe()?)?);
        for (label, args) in [
            ("kernel", v(&["uname", "-a"])),
            ("cc", v(&["cc", "--version"])),
            (
                "filesystem",
                v(&[
                    "findmnt",
                    "-T",
                    &s(&output),
                    "-n",
                    "-o",
                    "FSTYPE,SOURCE,TARGET",
                ]),
            ),
        ] {
            let text = c.ok(args, label, 30.)?;
            if label == "filesystem" {
                ensure!(
                    text.split_whitespace().next() == Some("ext4"),
                    "private volume must be ext4"
                );
            }
            c.evidence[label] = json!(text);
        }
        crate::delivered_faults::self_check()?;
        c.evidence["oracle_controls"] =
            json!({"implementation":"independent Rust", "passed":true, "modes":6});
        for (suffix, extra) in [
            ("", None),
            ("-disabled", Some("-DQUALIFICATION_BYPASS_INJECTION=1")),
        ] {
            let mut args = v(&[
                "cc", "-std=c11", "-O2", "-UNDEBUG", "-Wall", "-Wextra", "-Werror",
            ]);
            if let Some(extra) = extra {
                args.push(extra.into());
            }
            args.extend(v(&[
                "-shared",
                "-fPIC",
                &s(inputs.join("faults.c")),
                "-ldl",
                "-o",
                &s(output.join(format!("faults{suffix}.so"))),
            ]));
            c.ok(args, "build-shim", 30.)?;
        }
        c.ok(
            v(&[
                "cc",
                "-std=c11",
                "-O2",
                "-UNDEBUG",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-I",
                &s(&package),
                &s(inputs.join("consumer.c")),
                &s(package.join("libweekend_db.so")),
                "-Wl,-rpath,$ORIGIN/relocated-package",
                "-ldl",
                "-o",
                &s(output.join("consumer")),
            ]),
            "build-consumer",
            30.,
        )?;
        let mut binaries = BTreeMap::new();
        for name in ["consumer", "faults.so", "faults-disabled.so"] {
            binaries.insert(name, sha(&output.join(name))?);
        }
        c.evidence["binary_sha256"] = json!(binaries);
        let source = info["source"].as_str().context("source")?;
        let mut rows = vec![];
        for mode in 0..6 {
            let data = if mode == 5 {
                PathBuf::from("/faults/case")
            } else {
                output.join(format!("case-{mode}"))
            };
            fs::create_dir(&data)?;
            let mut injected = env.clone();
            injected.insert("LD_PRELOAD".into(), s(output.join("faults.so")));
            injected.insert("LD_DEBUG".into(), "libs".into());
            injected.insert(
                "QUALIFICATION_PRIVATE_TMPFS".into(),
                if mode == 5 { "1" } else { "0" }.into(),
            );
            let exercise = c.call(
                v(&[
                    &s(output.join("consumer")),
                    &mode.to_string(),
                    &s(&data),
                    source,
                ]),
                "exercise",
                90.,
                Some(injected),
                &[0],
            )?;
            let mut clean = env.clone();
            clean.insert("LD_DEBUG".into(), "libs".into());
            let recovery = c.call(
                v(&[
                    &s(output.join("consumer")),
                    "recover",
                    &s(&data),
                    source,
                    &mode.to_string(),
                ]),
                "recover",
                90.,
                Some(clean),
                &[0],
            )?;
            let loaded = format!(
                "calling init: {}",
                package.join("libweekend_db.so").display()
            );
            ensure!(
                exercise.contains(&loaded)
                    && recovery.contains(&loaded)
                    && !recovery.contains(&format!(
                        "calling init: {}",
                        output.join("faults.so").display()
                    )),
                "loader/injector mismatch"
            );
            let one = |text: &str| -> Result<Value> {
                let rows: Vec<_> = text.lines().filter(|l| l.starts_with('{')).collect();
                ensure!(rows.len() == 1, "missing/duplicate complete phase");
                Ok(serde_json::from_str(rows[0])?)
            };
            let phases = output.join(format!("phases-{mode}.json"));
            fs::write(
                &phases,
                serde_json::to_vec(
                    &json!({"exercise":one(&exercise)?,"recovery":one(&recovery)?,"mode":mode}),
                )?,
            )?;
            let phases = crate::delivery::read_json(&phases)?;
            rows.push(crate::delivered_faults::combine(
                &phases["exercise"],
                &phases["recovery"],
                mode,
            )?);
        }
        let data = output.join("disabled-case");
        fs::create_dir(&data)?;
        let mut disabled = env.clone();
        disabled.insert("LD_PRELOAD".into(), s(output.join("faults-disabled.so")));
        let text = c.call(
            v(&[&s(output.join("consumer")), "1", &s(data), source]),
            "disabled-injector",
            30.,
            Some(disabled),
            &[-6],
        )?;
        ensure!(
            text.contains("status==expected"),
            "disabled injector did not fail at ABI status assertion"
        );
        c.evidence["disabled_injector"] = json!({"returncode":-6,"expected_failure":true});
        ensure!(
            inventory(&inputs)? == frozen && sha(&package.join("manifest.json"))? == manifest,
            "input changed"
        );
        for (name, digest) in binaries {
            ensure!(sha(&output.join(name))? == digest, "binary changed");
        }
        crate::delivered_faults::package(&package)?;
        c.evidence["results"] = json!(rows);
        c.evidence["source_stable"] = json!(true);
        Ok(())
    })();
    let mut artifacts = inventory(&output)?;
    artifacts.remove("evidence.json");
    c.evidence["artifact_sha256"] = json!(artifacts);
    finish(c, result, Ok(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_attempts_both_retirements_and_both_absence_probes() {
        let name = format!("weekend-delivered-faults-{}", "a".repeat(32));
        let volume = format!("{name}-data");
        for mode in ["ok", "container-present", "volume-present", "query-error"] {
            let mut commands = vec![];
            let result = cleanup_with(&name, Some(&volume), |args, _, _| {
                commands.push(args.clone());
                match commands.len() {
                    1 => anyhow::bail!("remove timeout"),
                    2 => Ok(String::new()),
                    3 if mode == "query-error" => anyhow::bail!("daemon query failed"),
                    3 if mode == "container-present" => Ok(format!("{name}\n")),
                    4 if mode == "volume-present" => Ok(format!("{volume}\n")),
                    _ => Ok(String::new()),
                }
            });
            assert_eq!(commands.len(), 4);
            assert_eq!(commands[1], v(&["docker", "volume", "rm", &volume]));
            if mode == "query-error" {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap(), mode == "ok");
            }
        }
        assert!(
            cleanup_with("unrelated", Some("data"), |_, _, _| panic!(
                "must reject before external work"
            ))
            .is_err()
        );
    }
    #[test]
    fn explicit_cleanup_failure_never_qualifies_campaign() {
        let mut owned =
            crate::support::Scratch::new(&env::temp_dir(), "xtask-negative-proof").unwrap();
        let output = owned.path().join("out");
        fs::create_dir(&output).unwrap();
        let c = Campaign::new(owned.path(), output.clone()).unwrap();
        assert!(finish(c, Ok(()), Err(anyhow::anyhow!("daemon unavailable"))).is_err());
        let proof: Value =
            serde_json::from_slice(&fs::read(output.join("evidence.json")).unwrap()).unwrap();
        assert_eq!(proof["verified"], false);
        assert_eq!(proof["owned_resources_absent"], false);
        assert!(
            proof["cleanup_failure"]
                .as_str()
                .unwrap()
                .contains("daemon unavailable")
        );
        owned.close().unwrap();
    }
}
