#[cfg(unix)]
use crate::{
    assurance::{self, Campaign},
    support::{Run, Scratch, cmd, path, sha},
};
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::{
    delivery::{read_json_hashed, reparse},
    source_inventory,
};
use anyhow::{Context, Result, ensure};
#[cfg(unix)]
use clap::Args;
use regex::Regex;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use serde_json::Value;
use serde_json::json;
use std::fs;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
const SCOPE: &str =
    "Bounded ASan fuzz execution; report completed executions, not requested budgets";

#[cfg(unix)]
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long = "target")]
    pub targets: Vec<String>,
    #[arg(long,default_value_t=1000, value_parser=clap::value_parser!(u64).range(1..))]
    pub runs: u64,
    #[arg(long,default_value_t=60, value_parser=clap::value_parser!(u64).range(1..=85800))]
    pub seconds: u64,
    #[arg(long,default_value_t=2048, value_parser=clap::value_parser!(u64).range(1..))]
    pub rss_mb: u64,
    #[arg(long,default_value_t=262144, value_parser=clap::value_parser!(u64).range(1..))]
    pub max_len: u64,
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    #[arg(long, default_value = "nightly")]
    pub toolchain: String,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, default_value = "target/fuzz-build")]
    pub build_directory: PathBuf,
    /// Require an owned runtime directory on Linux ext4. Caller must also place build outputs on native Linux storage.
    #[arg(long, value_name = "DIRECTORY")]
    pub linux_ext4_runtime: Option<PathBuf>,
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn ext4_mount(text: &str, runtime: &str) -> Result<Value> {
    let report: Value = serde_json::from_str(text)?;
    let rows = report["filesystems"]
        .as_array()
        .context("missing runtime filesystem records")?;
    ensure!(rows.len() == 1, "ambiguous runtime filesystem records");
    let row = &rows[0];
    let mount = row["target"]
        .as_str()
        .context("missing runtime mount target")?;
    // These are Linux paths even when inspecting retained proof on Windows.
    ensure!(
        runtime.starts_with('/')
            && mount.starts_with('/')
            && (mount == "/"
                || runtime == mount
                || runtime
                    .strip_prefix(mount)
                    .is_some_and(|s| s.starts_with('/')))
            && row["fstype"] == "ext4"
            && row["source"].as_str().is_some_and(|s| !s.is_empty())
            && row["options"].as_str().is_some_and(|s| !s.is_empty()),
        "runtime requires one covering native ext4 mount"
    );
    Ok(row.clone())
}
fn targets(manifest: &str) -> Result<Vec<String>> {
    let manifest: toml::Table = toml::from_str(manifest)?;
    let names = manifest
        .get("bin")
        .and_then(toml::Value::as_array)
        .context("fuzz bins missing")?
        .iter()
        .map(|row| {
            let name = row
                .get("name")
                .and_then(toml::Value::as_str)
                .context("fuzz bin name")?;
            ensure!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
                "unsafe fuzz target name"
            );
            Ok(name.to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(!names.is_empty(), "empty fuzz target inventory");
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    ensure!(unique.len() == names.len(), "duplicate fuzz targets");
    Ok(names)
}
fn completed_runs(log: &str) -> Option<u64> {
    Regex::new(r"(?m)^Done (\d+) runs in \d+ second\(s\)\r?$")
        .unwrap()
        .captures_iter(log)
        .last()
        .and_then(|m| m[1].parse().ok())
}
#[cfg(unix)]
pub fn run(run: &Run, options: Options) -> Result<()> {
    let available = targets(&fs::read_to_string(run.root.join("fuzz/Cargo.toml"))?)?;
    let selected = if options.targets.is_empty() {
        available.clone()
    } else {
        options.targets
    };
    let unique: std::collections::BTreeSet<_> = selected.iter().collect();
    ensure!(
        unique.len() == selected.len() && selected.iter().all(|name| available.contains(name)),
        "unknown or duplicate target"
    );
    let output = options.output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "target/fuzz-evidence/{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    });
    let mut c = Campaign::new(run, &output, SCOPE)?;
    let mut runtime = None;
    let result = (|| {
        if let Some(parent) = options.linux_ext4_runtime {
            ensure!(
                cfg!(target_os = "linux"),
                "ext4 runtime qualification requires Linux"
            );
            let parent = dunce::canonicalize(assurance::absolute(&run.root, &parent))?;
            ensure!(
                parent.is_dir() && !parent.starts_with(&run.root),
                "runtime parent must be a directory outside the source checkout"
            );
            runtime = Some(Scratch::new(&parent, "fuzz-runtime")?);
            let directory = runtime.as_ref().unwrap().path();
            c.evidence["runtime"] = json!({"directory":directory,"required_filesystem":"ext4","admitted":false,"cleanup_verified":false});
            let mount = c.command(
                vec![
                    "findmnt".into(),
                    "--json".into(),
                    "--target".into(),
                    path(directory),
                    "--output".into(),
                    "TARGET,SOURCE,FSTYPE,OPTIONS".into(),
                ],
                "runtime-filesystem",
                None,
                30.,
            )?;
            c.evidence["runtime"]["mount"] =
                ext4_mount(&mount, directory.to_str().context("UTF-8 runtime path")?)?;
            c.evidence["runtime"]["admitted"] = json!(true);
            c.env.insert("TMPDIR".into(), path(directory));
            c.save()?;
        }
        let before_seeds = assurance::seeds(&run.root)?;
        let before_lock = sha(&run.root.join("fuzz/Cargo.lock"))?;
        let toolchain = format!("+{}", options.toolchain);
        let version = c.command(
            cmd(["rustc", &toolchain, "-vV"]),
            "rustc-version",
            None,
            30.,
        )?;
        let fuzz = c.command(
            cmd(["cargo", "fuzz", "--version"]),
            "cargo-fuzz-version",
            None,
            30.,
        )?;
        c.evidence["toolchain"] = json!(version);
        c.evidence["cargo_fuzz"] = json!(fuzz);
        c.evidence["seed_sha256"] = json!(before_seeds);
        c.evidence["lockfile_sha256"] = json!(before_lock);
        c.evidence["targets"] = json!([]);
        c.save()?;
        let mut failed = false;
        for name in selected {
            let directory = c.output.join(&name);
            let corpus = directory.join("corpus");
            let artifacts = directory.join("artifacts");
            fs::create_dir(&directory)?;
            fs::create_dir(&corpus)?;
            fs::create_dir(&artifacts)?;
            let mut command = cmd([
                "cargo",
                &toolchain,
                "fuzz",
                "run",
                "--sanitizer",
                "address",
                "--target-dir",
            ]);
            command.extend([
                path(assurance::absolute(&run.root, &options.build_directory)),
                path(&name),
                path(&corpus),
            ]);
            let seeds = run.root.join("fuzz/seeds").join(&name);
            if seeds.is_dir() {
                command.push(path(seeds));
            }
            command.push(path("--"));
            command.extend([
                path(format!("-runs={}", options.runs)),
                path(format!("-max_total_time={}", options.seconds)),
                path(format!("-rss_limit_mb={}", options.rss_mb)),
                path(format!("-max_len={}", options.max_len)),
                path(format!("-seed={}", options.seed)),
                path("-timeout=10"),
                path(format!("-artifact_prefix={}/", artifacts.display())),
            ]);
            if let Some(runtime) = &runtime {
                // Bind the selected directory to the supervised argv without
                // retaining unrelated (potentially sensitive) environment values.
                command.splice(
                    ..0,
                    [
                        path("/usr/bin/env"),
                        path(format!("TMPDIR={}", runtime.path().display())),
                    ],
                );
            }
            let label = format!("target-{name}");
            let outcome = c.command(command, &label, None, (options.seconds + 600) as f64);
            let mut log =
                fs::read(c.output.join(format!("{label}.stdout.log"))).unwrap_or_default();
            log.extend(fs::read(c.output.join(format!("{label}.stderr.log"))).unwrap_or_default());
            fs::write(directory.join("run.log"), &log)?;
            let log = String::from_utf8_lossy(&log);
            let actual = completed_runs(&log);
            let source_changed = !c.unchanged()?;
            let seeds_changed = assurance::seeds(&run.root)? != before_seeds;
            let lock_changed = sha(&run.root.join("fuzz/Cargo.lock"))? != before_lock;
            let passed = outcome.is_ok()
                && actual.is_some_and(|n| n > 0)
                && !source_changed
                && !seeds_changed
                && !lock_changed;
            let command = c.evidence["commands"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .clone();
            c.evidence["targets"].as_array_mut().unwrap().push(json!({"name":name,"command":command["argv"],"status":command["exit_code"],"elapsed_seconds":command["supervision"]["elapsed_seconds"],"completed_runs":actual,"passed":passed,"source_changed":source_changed,"seeds_changed":seeds_changed,"lockfile_changed":lock_changed}));
            c.save()?;
            eprintln!(
                "{name}: completed {actual:?}, passed {passed}; evidence {}",
                c.output.display()
            );
            failed |= !passed;
        }
        ensure!(
            !failed,
            "one or more fuzz targets failed or completed no executions"
        );
        Ok(())
    })();
    let cleanup = runtime.as_mut().map_or(Ok(()), Scratch::close);
    if runtime.is_some() {
        c.evidence["runtime"]["cleanup_verified"] = json!(cleanup.is_ok());
    }
    c.finish(result.and(cleanup))
}
// A retained campaign is evidence, not executable input. Bound every read and
// reject links (including Windows reparse points) before inspecting its contents.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn retained_log(root: &Path, name: &str) -> Result<Vec<u8>> {
    use std::io::Read;
    const LIMIT: u64 = 64 * 1024 * 1024;
    let file = root.join(name);
    let meta = fs::symlink_metadata(&file)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && !reparse(&meta) && meta.len() <= LIMIT,
        "nonregular or oversized fuzz log {name}"
    );
    let mut bytes = Vec::new();
    fs::File::open(file)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= LIMIT, "fuzz log grew beyond limit");
    Ok(bytes)
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn directory(directory: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(directory)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink() && !reparse(&meta),
        "fuzz directory alias"
    );
    Ok(())
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn successful_command<'a>(root: &Path, commands: &'a [Value], name: &str) -> Result<&'a Value> {
    let mut matching = commands.iter().filter(|row| row["name"] == name);
    let row = matching
        .next()
        .with_context(|| format!("missing fuzz command {name}"))?;
    let supervision = &row["supervision"];
    ensure!(
        matching.next().is_none()
            && row["passed"] == true
            && row["exit_code"] == 0
            && row["invocation_error"].is_null()
            && row["expected_assertion"].is_null()
            && row["expected_exit"].is_null()
            && supervision["returncode"] == 0
            && supervision["cleanup_verified"] == true
            && supervision["failure"].is_null()
            && supervision["cleanup_failure"].is_null()
            && supervision["timed_out"] == false
            && supervision["cancelled"] == false
            && supervision["stopped"] != true,
        "unsuccessful or ambiguous fuzz command {name}"
    );
    let (process, _) = read_json_hashed(&root.join(format!("{name}.process.json")))?;
    ensure!(
        process == *supervision,
        "fuzz process report mismatch {name}"
    );
    for stream in ["stdout", "stderr"] {
        let filename = format!("{name}.{stream}.log");
        let bytes = retained_log(root, &filename)?;
        ensure!(
            row[stream] == filename && supervision[format!("{stream}_bytes")] == bytes.len() as u64,
            "fuzz command transcript mismatch {name}"
        );
    }
    Ok(row)
}
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn budget(value: &Value, prefix: &str, minimum: u64, maximum: u64) -> bool {
    value
        .as_str()
        .and_then(|s| s.strip_prefix(prefix))
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|n| (minimum..=maximum).contains(&n))
}
/// Bind a complete retained xtask fuzz campaign to the exact delivery archive.
/// This confirms bounded executions and process retirement, not a fuzz-duration
/// policy, native-platform matrix, or absence of undiscovered bugs.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub fn verify(root: &Path, source: &str, tree: &Path) -> Result<Value> {
    use sha2::{Digest, Sha256};
    directory(root)?;
    let (report, report_hash) = read_json_hashed(&root.join("evidence.json"))?;
    ensure!(
        report["scope"] == SCOPE
            && report["source"] == source
            && report["git_identity_available"] == true
            && report["dirty"] == false
            && report["passed"] == true
            && report["cancelled"] == false,
        "fuzz source, clean checkout or pass status mismatch"
    );
    let inputs = source_inventory::snapshot(tree)?;
    ensure!(
        report["source_hashes"] == json!(inputs)
            && inputs.contains_key("fuzz/Cargo.toml")
            && inputs.contains_key("fuzz/Cargo.lock")
            && report["lockfile_sha256"] == inputs["fuzz/Cargo.lock"]
            && report["seed_sha256"] == json!(source_inventory::seeds(tree)?),
        "fuzz inputs or seeds differ from the Git archive"
    );
    let available = targets(&fs::read_to_string(tree.join("fuzz/Cargo.toml"))?)?;
    let rows = report["targets"]
        .as_array()
        .context("missing fuzz target results")?;
    let commands = report["commands"]
        .as_array()
        .context("missing fuzz commands")?;
    let runtime = report.get("runtime");
    ensure!(
        rows.len() == available.len()
            && commands.len() == available.len() + 4 + usize::from(runtime.is_some()),
        "incomplete fuzz target or command inventory"
    );
    if let Some(runtime) = runtime {
        let directory = runtime["directory"]
            .as_str()
            .context("missing runtime directory")?;
        let command = successful_command(root, commands, "runtime-filesystem")?;
        let log = String::from_utf8(retained_log(root, "runtime-filesystem.stdout.log")?)?;
        ensure!(
            runtime["required_filesystem"] == "ext4"
                && runtime["admitted"] == true
                && runtime["cleanup_verified"] == true
                && command["argv"]
                    == json!([
                        "findmnt",
                        "--json",
                        "--target",
                        directory,
                        "--output",
                        "TARGET,SOURCE,FSTYPE,OPTIONS"
                    ])
                && runtime["mount"] == ext4_mount(&log, directory)?,
            "runtime filesystem qualification mismatch"
        );
    }
    let source_command = successful_command(root, commands, "source")?;
    let dirty = successful_command(root, commands, "dirty")?;
    let rustc = successful_command(root, commands, "rustc-version")?;
    let cargo = successful_command(root, commands, "cargo-fuzz-version")?;
    let rust_args = rustc["argv"]
        .as_array()
        .context("missing fuzz rustc arguments")?;
    ensure!(
        source_command["argv"] == json!(["git", "rev-parse", "HEAD"])
            && dirty["argv"] == json!(["git", "status", "--porcelain"])
            && cargo["argv"] == json!(["cargo", "fuzz", "--version"])
            && matches!(rust_args.as_slice(), [program, toolchain, version]
            if program == "rustc" && version == "-vV"
                && toolchain.as_str().is_some_and(|s| s.starts_with('+') && s.len() > 1)),
        "fuzz identity or toolchain command mismatch"
    );
    let toolchain = &rust_args[1];
    let cwd = source_command["cwd"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("missing fuzz working directory")?;
    ensure!(
        commands.iter().all(|row| row["cwd"] == cwd),
        "fuzz working directory mismatch"
    );
    for (name, expected) in [
        ("source", json!(source)),
        ("dirty", json!("")),
        ("rustc-version", report["toolchain"].clone()),
        ("cargo-fuzz-version", report["cargo_fuzz"].clone()),
    ] {
        let log = retained_log(root, &format!("{name}.stdout.log"))?;
        let log = String::from_utf8(log)?;
        ensure!(
            expected
                .as_str()
                .is_some_and(|s| s == log.trim_end_matches(['\r', '\n']))
                && (name == "dirty" || !log.trim().is_empty()),
            "fuzz identity transcript mismatch"
        );
    }
    let mut summaries = std::collections::BTreeMap::new();
    for name in available {
        let mut matching = rows.iter().filter(|row| row["name"] == name);
        let row = matching
            .next()
            .with_context(|| format!("missing fuzz target {name}"))?;
        ensure!(
            matching.next().is_none()
                && row["passed"] == true
                && row["status"] == 0
                && row["source_changed"] == false
                && row["seeds_changed"] == false
                && row["lockfile_changed"] == false,
            "failed or duplicate fuzz target {name}"
        );
        let label = format!("target-{name}");
        let command = successful_command(root, commands, &label)?;
        let argv = command["argv"]
            .as_array()
            .context("missing fuzz arguments")?;
        let argv = if let Some(runtime) = runtime {
            ensure!(
                argv.len() >= 2
                    && argv[0] == "/usr/bin/env"
                    && argv[1] == format!("TMPDIR={}", runtime["directory"].as_str().unwrap()),
                "fuzz target runtime directory mismatch {name}"
            );
            &argv[2..]
        } else {
            argv.as_slice()
        };
        let seeds = tree.join("fuzz/seeds").join(&name).is_dir();
        let offset = 11 + usize::from(seeds);
        ensure!(
            argv.len() == offset + 7
                && argv[..7]
                    == json!([
                        "cargo",
                        toolchain,
                        "fuzz",
                        "run",
                        "--sanitizer",
                        "address",
                        "--target-dir"
                    ])
                    .as_array()
                    .unwrap()[..]
                && argv[7].as_str().is_some_and(|s| !s.is_empty())
                && argv[8] == name
                && argv[offset - 1] == "--"
                && row["command"] == command["argv"]
                && row["elapsed_seconds"] == command["supervision"]["elapsed_seconds"]
                && budget(&argv[offset], "-runs=", 1, u64::MAX)
                && budget(&argv[offset + 1], "-max_total_time=", 1, 85800)
                && budget(&argv[offset + 2], "-rss_limit_mb=", 1, u64::MAX)
                && budget(&argv[offset + 3], "-max_len=", 1, u64::MAX)
                && budget(&argv[offset + 4], "-seed=", 0, u64::MAX)
                && argv[offset + 5] == "-timeout=10",
            "fuzz invocation mismatch {name}"
        );
        // Campaigns are produced on Unix, but retained proof can be verified on Windows.
        let corpus = argv[9].as_str().context("missing fuzz corpus path")?;
        let target_directory = corpus
            .strip_suffix("/corpus")
            .context("missing fuzz target directory")?;
        ensure!(
            corpus.starts_with('/')
                && target_directory.rsplit('/').next() == Some(name.as_str())
                && argv[offset + 6] == format!("-artifact_prefix={target_directory}/artifacts/")
                && (!seeds || argv[10] == format!("{cwd}/fuzz/seeds/{name}")),
            "fuzz corpus, seeds or artifact path mismatch {name}"
        );
        directory(&root.join(&name))?;
        let artifacts = root.join(&name).join("artifacts");
        directory(&artifacts)?;
        ensure!(
            fs::read_dir(artifacts)?.next().is_none(),
            "fuzz artifacts present for {name}"
        );
        let mut log = retained_log(root, &format!("{label}.stdout.log"))?;
        log.extend(retained_log(root, &format!("{label}.stderr.log"))?);
        ensure!(
            log == retained_log(root, &format!("{name}/run.log"))?,
            "fuzz retained log mismatch {name}"
        );
        let count = completed_runs(&String::from_utf8_lossy(&log))
            .filter(|n| *n > 0)
            .context("fuzz log has no completed executions")?;
        ensure!(
            row["completed_runs"] == count,
            "fuzz completed count mismatch {name}"
        );
        summaries.insert(
            name,
            json!({"completed_runs":count,"log_sha256":format!("{:x}", Sha256::digest(&log))}),
        );
    }
    Ok(json!({"evidence_sha256":report_hash,"targets":summaries}))
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos", windows)))]
mod tests {
    use super::*;
    #[test]
    fn ext4_runtime_admission_rejects_other_filesystems_and_ambiguous_mounts() {
        let row = json!({"target":"/runtime","source":"/dev/vdb","fstype":"ext4","options":"rw,relatime"});
        let valid = json!({"filesystems":[row.clone()]});
        let check = |value: &Value| ext4_mount(&value.to_string(), "/runtime/owned");
        assert_eq!(check(&valid).unwrap(), row);
        for filesystem in ["fuseblk", "virtiofs", "overlay", "tmpfs", ""] {
            let mut wrong = valid.clone();
            wrong["filesystems"][0]["fstype"] = json!(filesystem);
            assert!(check(&wrong).is_err(), "{filesystem}");
        }
        for wrong in [
            json!({}),
            json!({"filesystems":[]}),
            json!({"filesystems":[row.clone(),row.clone()]}),
        ] {
            assert!(check(&wrong).is_err());
        }
        for target in ["runtime", "/runtime/other", "/runtim", ""] {
            let mut wrong = valid.clone();
            wrong["filesystems"][0]["target"] = json!(target);
            assert!(check(&wrong).is_err(), "{target}");
        }
        for field in ["source", "options", "fstype", "target"] {
            let mut wrong = valid.clone();
            wrong["filesystems"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(check(&wrong).is_err(), "{field}");
        }
        assert!(ext4_mount("not json", "/runtime/owned").is_err());
        assert!(ext4_mount(&valid.to_string(), "runtime/owned").is_err());
        let mut root = valid;
        root["filesystems"][0]["target"] = json!("/");
        assert!(check(&root).is_ok());
    }
    fn fixture(tree: &Path, root: &Path) -> Value {
        fs::create_dir_all(tree.join("fuzz/seeds/first")).unwrap();
        fs::write(
            tree.join("fuzz/Cargo.toml"),
            "[[bin]]\nname='first'\n[[bin]]\nname='second'\n",
        )
        .unwrap();
        fs::write(tree.join("fuzz/Cargo.lock"), "locked dependencies").unwrap();
        fs::write(tree.join("fuzz/seeds/first/input"), "seed bytes").unwrap();
        fs::create_dir(root).unwrap();
        let mut commands = vec![];
        let mut targets = vec![];
        let mut command = |name: &str, argv: Value, stdout: &str, stderr: &str| {
            fs::write(root.join(format!("{name}.stdout.log")), stdout).unwrap();
            fs::write(root.join(format!("{name}.stderr.log")), stderr).unwrap();
            let supervisor = json!({"returncode":0,"cleanup_verified":true,"timed_out":false,
                "cancelled":false,"elapsed_seconds":0.01,"stdout_bytes":stdout.len(),"stderr_bytes":stderr.len()});
            fs::write(
                root.join(format!("{name}.process.json")),
                supervisor.to_string(),
            )
            .unwrap();
            let row = json!({"name":name,"argv":argv,"cwd":"/source","passed":true,"exit_code":0,
                "stdout":format!("{name}.stdout.log"),"stderr":format!("{name}.stderr.log"),"supervision":supervisor});
            commands.push(row.clone());
            row
        };
        command(
            "source",
            json!(["git", "rev-parse", "HEAD"]),
            "source-id\n",
            "",
        );
        command("dirty", json!(["git", "status", "--porcelain"]), "", "");
        command(
            "rustc-version",
            json!(["rustc", "+nightly", "-vV"]),
            "rustc version\n",
            "",
        );
        command(
            "cargo-fuzz-version",
            json!(["cargo", "fuzz", "--version"]),
            "cargo-fuzz version\n",
            "",
        );
        for name in ["first", "second"] {
            fs::create_dir_all(root.join(name).join("artifacts")).unwrap();
            let mut argv = json!([
                "cargo",
                "+nightly",
                "fuzz",
                "run",
                "--sanitizer",
                "address",
                "--target-dir",
                "/build",
                name,
                format!("/proof/{name}/corpus")
            ]);
            let args = argv.as_array_mut().unwrap();
            if name == "first" {
                args.push(json!("/source/fuzz/seeds/first"));
            }
            args.extend(
                json!([
                    "--",
                    "-runs=1000",
                    "-max_total_time=1",
                    "-rss_limit_mb=512",
                    "-max_len=64",
                    "-seed=1",
                    "-timeout=10",
                    format!("-artifact_prefix=/proof/{name}/artifacts/")
                ])
                .as_array()
                .unwrap()
                .clone(),
            );
            let log = "Done 3 runs in 1 second(s)\n";
            let row = command(&format!("target-{name}"), argv, "", log);
            fs::write(root.join(name).join("run.log"), log).unwrap();
            targets.push(json!({"name":name,"command":row["argv"],"status":0,"elapsed_seconds":0.01,
                "completed_runs":3,"passed":true,"source_changed":false,"seeds_changed":false,"lockfile_changed":false}));
        }
        json!({"scope":SCOPE,"source":"source-id","git_identity_available":true,"dirty":false,"passed":true,"cancelled":false,
            "source_hashes":source_inventory::snapshot(tree).unwrap(),"seed_sha256":source_inventory::seeds(tree).unwrap(),
            "lockfile_sha256":crate::support::sha(&tree.join("fuzz/Cargo.lock")).unwrap(),
            "toolchain":"rustc version","cargo_fuzz":"cargo-fuzz version","commands":commands,"targets":targets})
    }
    #[test]
    fn delivery_fuzz_binds_optional_ext4_runtime_to_mount_command_and_cleanup() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "fuzz-runtime-proof").unwrap();
        let tree = scratch.path().join("source");
        let root = scratch.path().join("proof");
        let mut report = fixture(&tree, &root);
        let mount = json!({"target":"/runtime","source":"/dev/vdb","fstype":"ext4","options":"rw,relatime"});
        let output = json!({"filesystems":[mount.clone()]}).to_string();
        let mut command = report["commands"][2].clone();
        command["name"] = json!("runtime-filesystem");
        command["argv"] = json!([
            "findmnt",
            "--json",
            "--target",
            "/runtime/owned",
            "--output",
            "TARGET,SOURCE,FSTYPE,OPTIONS"
        ]);
        command["stdout"] = json!("runtime-filesystem.stdout.log");
        command["stderr"] = json!("runtime-filesystem.stderr.log");
        command["supervision"]["stdout_bytes"] = json!(output.len());
        command["supervision"]["stderr_bytes"] = json!(0);
        fs::write(root.join("runtime-filesystem.stdout.log"), output).unwrap();
        fs::write(root.join("runtime-filesystem.stderr.log"), "").unwrap();
        fs::write(
            root.join("runtime-filesystem.process.json"),
            command["supervision"].to_string(),
        )
        .unwrap();
        report["commands"].as_array_mut().unwrap().push(command);
        report["runtime"] = json!({"directory":"/runtime/owned","required_filesystem":"ext4","admitted":true,"cleanup_verified":true,"mount":mount});
        for index in 0..2 {
            let command = &mut report["commands"][index + 4]["argv"];
            command
                .as_array_mut()
                .unwrap()
                .splice(..0, [json!("/usr/bin/env"), json!("TMPDIR=/runtime/owned")]);
            report["targets"][index]["command"] = command.clone();
        }
        let check = |record: &Value| {
            fs::write(root.join("evidence.json"), record.to_string()).unwrap();
            verify(&root, "source-id", &tree)
        };
        assert!(check(&report).is_ok());
        for (field, value) in [
            ("directory", json!("/another/runtime")),
            ("required_filesystem", json!("overlay")),
            ("admitted", json!(false)),
            ("cleanup_verified", json!(false)),
            ("mount", json!({})),
        ] {
            let mut wrong = report.clone();
            wrong["runtime"][field] = value;
            assert!(check(&wrong).is_err(), "{field}");
        }
        let mut wrong = report.clone();
        wrong["commands"][4]["argv"][1] = json!("TMPDIR=/host-bind");
        wrong["targets"][0]["command"] = wrong["commands"][4]["argv"].clone();
        assert!(check(&wrong).is_err());
        report.as_object_mut().unwrap().remove("runtime");
        assert!(check(&report).is_err());
        scratch.close().unwrap();
    }
    #[test]
    fn delivery_fuzz_requires_archive_inputs_complete_targets_and_actual_logs() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "fuzz-binding").unwrap();
        let tree = scratch.path().join("source");
        let root = scratch.path().join("proof");
        let report = fixture(&tree, &root);
        let check = |record: &Value| {
            // Recompute the enclosing evidence bytes for every negative control.
            fs::write(root.join("evidence.json"), record.to_string()).unwrap();
            verify(&root, "source-id", &tree)
        };
        assert_eq!(
            check(&report).unwrap()["targets"]["first"]["completed_runs"],
            3
        );
        for (field, replacement, message) in [
            ("source", json!("another commit"), "source"),
            ("dirty", json!(true), "clean checkout"),
            ("source_hashes", json!({}), "inputs"),
            ("seed_sha256", json!({}), "seeds"),
            ("lockfile_sha256", json!("substituted"), "inputs"),
        ] {
            let mut wrong = report.clone();
            wrong[field] = replacement;
            assert!(check(&wrong).unwrap_err().to_string().contains(message));
        }
        let mut incomplete = report.clone();
        incomplete["targets"].as_array_mut().unwrap().pop();
        incomplete["commands"].as_array_mut().unwrap().pop();
        assert!(
            check(&incomplete)
                .unwrap_err()
                .to_string()
                .contains("inventory")
        );
        let mut duplicate = report.clone();
        duplicate["targets"][1] = duplicate["targets"][0].clone();
        assert!(check(&duplicate).is_err());
        let mut counts = report.clone();
        counts["targets"][0]["completed_runs"] = json!(1000);
        assert!(check(&counts).unwrap_err().to_string().contains("count"));
        fs::write(
            root.join("first/run.log"),
            "Done 1000 runs in 1 second(s)\n",
        )
        .unwrap();
        assert!(
            check(&counts)
                .unwrap_err()
                .to_string()
                .contains("log mismatch")
        );
        // Even coherent edits of all retained logs, process byte counts, and
        // enclosing evidence cannot substitute a requested budget for execution.
        let mut forged = report.clone();
        let no_completion = "cargo fuzz -runs=1000\nerror: failed to compile\n";
        fs::write(root.join("first/run.log"), no_completion).unwrap();
        fs::write(root.join("target-first.stderr.log"), no_completion).unwrap();
        forged["commands"][4]["supervision"]["stderr_bytes"] = json!(no_completion.len());
        fs::write(
            root.join("target-first.process.json"),
            forged["commands"][4]["supervision"].to_string(),
        )
        .unwrap();
        assert!(
            check(&forged)
                .unwrap_err()
                .to_string()
                .contains("no completed")
        );
        let log = "Done 3 runs in 1 second(s)\n";
        fs::write(root.join("first/run.log"), log).unwrap();
        fs::write(root.join("target-first.stderr.log"), log).unwrap();
        fs::write(
            root.join("target-first.process.json"),
            report["commands"][4]["supervision"].to_string(),
        )
        .unwrap();
        let mut unclean = report.clone();
        unclean["commands"][4]["supervision"]["cleanup_verified"] = json!(false);
        fs::write(
            root.join("target-first.process.json"),
            unclean["commands"][4]["supervision"].to_string(),
        )
        .unwrap();
        assert!(
            check(&unclean)
                .unwrap_err()
                .to_string()
                .contains("unsuccessful")
        );
        fs::write(
            root.join("target-first.process.json"),
            report["commands"][4]["supervision"].to_string(),
        )
        .unwrap();
        fs::write(root.join("first/artifacts/crash"), "failure").unwrap();
        assert!(
            check(&report)
                .unwrap_err()
                .to_string()
                .contains("artifacts present")
        );
        fs::remove_file(root.join("first/artifacts/crash")).unwrap();
        fs::write(tree.join("fuzz/seeds/first/input"), "changed seed").unwrap();
        // A recomputed enclosing report cannot replace the archive's seed bytes.
        assert!(check(&report).unwrap_err().to_string().contains("seeds"));
        scratch.close().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn delivery_fuzz_rejects_symlinked_artifacts_and_logs() {
        use std::os::unix::fs::symlink;
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "fuzz-alias").unwrap();
        let tree = scratch.path().join("source");
        let root = scratch.path().join("proof");
        let report = fixture(&tree, &root);
        fs::write(root.join("evidence.json"), report.to_string()).unwrap();
        let artifacts = root.join("first/artifacts");
        fs::remove_dir(&artifacts).unwrap();
        symlink(root.join("second/artifacts"), &artifacts).unwrap();
        assert!(verify(&root, "source-id", &tree).is_err());
        fs::remove_file(&artifacts).unwrap();
        fs::create_dir(&artifacts).unwrap();
        fs::remove_file(root.join("first/run.log")).unwrap();
        symlink(
            root.join("target-first.stderr.log"),
            root.join("first/run.log"),
        )
        .unwrap();
        assert!(verify(&root, "source-id", &tree).is_err());
        scratch.close().unwrap();
    }

    #[test]
    fn requested_runs_and_failed_builds_are_not_completed_runs() {
        assert_eq!(
            completed_runs("cargo fuzz -runs=1000\nerror: failed to compile\n"),
            None
        );
        assert_eq!(completed_runs("Done 1000 runs in fake second(s)\n"), None);
        assert_eq!(completed_runs("Done 0 runs in 0 second(s)\n"), Some(0));
        assert_eq!(
            completed_runs("#538 DONE\nDone 538 runs in 21 second(s)\n"),
            Some(538)
        );
        assert_eq!(
            completed_runs("Done 1 runs in 0 second(s)\r\nDone 2 runs in 1 second(s)\r\n"),
            Some(2)
        );
        assert_eq!(
            completed_runs("Done 18446744073709551616 runs in 1 second(s)\n"),
            None
        );
    }
    #[test]
    fn target_inventory_rejects_empty_duplicate_or_unsafe_bins() {
        assert_eq!(
            targets("[[bin]]\nname='valid_target'\n").unwrap(),
            ["valid_target"]
        );
        for text in [
            "",
            "[[bin]]\nname='../escape'",
            "[[bin]]\nname='a'\n[[bin]]\nname='a'",
            "[[bin]]\nname=''",
            "[[bin]]\nname='.'",
        ] {
            assert!(targets(text).is_err(), "{text}");
        }
    }
}
