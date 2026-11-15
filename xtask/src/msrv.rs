//! Native minimum-compiler qualification using the normal check plan.
#[cfg(target_os = "linux")]
use crate::assurance::Campaign;
#[cfg(windows)]
use crate::windows_compatibility::Campaign;
use crate::{
    check, delivery,
    support::{Platform, Run, Scratch, cargo_build_env, cmd, path, qualification_inventory, sha},
};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[derive(clap::Args)]
pub struct Options {
    #[arg(long)]
    output: PathBuf,
    /// Include the installed Odin compiler and optimized Odin trace cases.
    #[arg(long)]
    odin: bool,
}

fn minimum(root: &Path) -> Result<String> {
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
    let version = manifest["package"]["rust-version"]
        .as_str()
        .context("declared Rust minimum missing")?;
    let parts: Vec<_> = version.split('.').collect();
    ensure!(
        (2..=3).contains(&parts.len()) && parts.iter().all(|p| p.parse::<u32>().is_ok()),
        "invalid declared Rust minimum"
    );
    Ok(if parts.len() == 2 {
        format!("{version}.0")
    } else {
        version.to_owned()
    })
}

fn committed_locks(
    c: &mut Campaign<'_>,
    root: &Path,
    phase: &str,
) -> Result<BTreeMap<String, String>> {
    let tracked = c.command(
        cmd(["git", "ls-files", "--stage", "--", ":(glob)**/Cargo.lock"]),
        &format!("{phase}-lock-index"),
        None,
        30.,
    )?;
    let mut names = Vec::new();
    let mut expected = Vec::new();
    for line in tracked.lines() {
        let (entry, name) = line.split_once('\t').context("malformed lock index row")?;
        let mut fields = entry.split_ascii_whitespace();
        ensure!(fields.next() == Some("100644"), "nonregular tracked lock");
        let object = fields.next().context("lock object missing")?;
        ensure!(
            fields.next() == Some("0") && fields.next().is_none(),
            "unmerged tracked lock"
        );
        names.push(name);
        expected.push(object);
    }
    ensure!(
        names.contains(&"Cargo.lock")
            && names.contains(&"crates/weekend-db-typed/tests/renamed/Cargo.lock"),
        "workspace and renamed consumer locks required"
    );
    let mut args = cmd(["git", "hash-object", "--no-filters", "--"]);
    args.extend(cmd(names.iter().copied()));
    let actual = c.command(args, &format!("{phase}-lock-bytes"), None, 30.)?;
    ensure!(
        actual.lines().eq(expected),
        "checkout lock differs from Git blob"
    );
    names
        .into_iter()
        .map(|name| Ok((name.into(), sha(&root.join(name))?)))
        .collect()
}

fn run_checks(
    c: &mut Campaign<'_>,
    root: &Path,
    target: &Path,
    runtime_parent: &Path,
    xtask: &Path,
) -> Result<()> {
    let platform = Platform::host()?;
    let mut scratch = Scratch::new(runtime_parent, "msrv-native")?;
    let run = |c: &mut Campaign<'_>, prefix: &str, commands: Vec<Vec<OsString>>| -> Result<()> {
        for (index, mut args) in commands.into_iter().enumerate() {
            if args[0] == "cargo"
                && matches!(args[1].to_str(), Some("clippy" | "test" | "run" | "build"))
            {
                args.insert(2, "--locked".into());
                if args[1] == "test" {
                    // Compile first so cold builds and native test execution have independent deadlines.
                    let mut compile = args.clone();
                    compile.push("--no-run".into());
                    c.command(compile, &format!("{prefix}-compile"), None, 300.)?;
                }
            }
            c.command(args, &format!("{prefix}-{}", index + 1), None, 300.)?;
        }
        Ok(())
    };
    run(c, "prelude", check::prelude())?;
    for (release, profile) in [(false, "debug"), (true, "release")] {
        let data = scratch.path().join(profile);
        fs::create_dir(&data)?;
        run(
            c,
            profile,
            check::profile(
                root,
                target,
                platform,
                std::env::consts::ARCH,
                true,
                xtask.as_os_str(),
                &data,
                release,
            ),
        )?;
        #[cfg(windows)]
        c.command(
            vec![
                path(xtask),
                "windows-consumers".into(),
                "--library".into(),
                path(target.join(profile).join("weekend_db.dll")),
            ],
            &format!("{profile}-native-consumers"),
            None,
            180.,
        )?;
    }
    scratch.close()?;
    Ok(())
}

fn run_delivery(
    c: &mut Campaign<'_>,
    xtask: &Path,
    compiler: &str,
    root_lock: &str,
    odin: bool,
) -> Result<()> {
    let package = c.output.join("package");
    let mut command = vec![
        path(xtask),
        "package".into(),
        "--output".into(),
        path(&package),
        "--features".into(),
        "crdt,replication".into(),
        "--rust-source".into(),
    ];
    if odin {
        command.push("--odin".into());
    }
    c.command(command, "native-package", None, 300.)?;
    let manifest_path = package.join("manifest.json");
    let manifest = delivery::read_json(&manifest_path)?;
    let pin = sha(&manifest_path)?;
    ensure!(
        manifest["verified"] == true
            && manifest["candidate_eligible"] == true
            && manifest["relocation_verified"] == true
            && manifest["source"] == c.evidence["source"]
            && manifest["toolchain"] == compiler
            && manifest["lockfile_sha256"] == root_lock
            && manifest["runtime_feature_mask"] == 3
            && manifest["rust_source"]["native_compile_verified"] == true
            && (!odin || manifest["odin"]["verified"] == true),
        "minimum-compiler package identity or consumer mismatch"
    );
    run_trace(c, xtask, &package, &pin, &manifest, odin)
}

#[cfg(windows)]
fn run_trace(
    c: &mut Campaign<'_>,
    xtask: &Path,
    package: &Path,
    pin: &str,
    manifest: &serde_json::Value,
    odin: bool,
) -> Result<()> {
    let trace = c.output.join("trace");
    c.command(
        vec![
            path(xtask),
            "windows-trace".into(),
            "--package".into(),
            path(package),
            "--manifest-sha256".into(),
            pin.into(),
            "--output".into(),
            path(&trace),
        ],
        "native-trace",
        None,
        300.,
    )?;
    let trace_path = trace.join("evidence.json");
    let proof = delivery::read_json(&trace_path)?;
    let dll = sha(&package.join("weekend_db.dll"))?;
    let trace_cases = if odin { 30 } else { 20 };
    ensure!(
        proof["native_windows_verified"] == true
            && proof["manifest_sha256"] == pin
            && proof["trace_cells"] == 134
            && proof["outcomes"]
                .as_array()
                .is_some_and(|rows| rows.len() == trace_cases)
            && proof["c"]["dll_sha256"] == dll
            && proof["cpp"]["dll_sha256"] == dll
            && (!odin
                || (proof["odin"]["dll_sha256"] == dll
                    && proof["odin"]["compiler_sha256"] == manifest["odin"]["compiler_sha256"])),
        "minimum-compiler delivered trace mismatch"
    );
    c.evidence["delivery"] = json!({
        "package_manifest_sha256":pin, "trace_evidence_sha256":sha(&trace_path)?,
        "dll_sha256":dll, "trace_cells":134, "trace_cases":trace_cases, "rust_source_verified":true,
        "odin_verified":odin,
    });
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_trace(
    c: &mut Campaign<'_>,
    xtask: &Path,
    package: &Path,
    pin: &str,
    manifest: &serde_json::Value,
    odin: bool,
) -> Result<()> {
    use crate::{support::text, toolchain, trace_model::EXPECTED_SHA256};
    use std::ffi::OsStr;

    let trace = c.output.join("trace");
    let mut command = vec![
        path(xtask),
        "artifact-trace".into(),
        "--package".into(),
        path(package),
        "--output".into(),
        path(&trace),
    ];
    let compiler = if odin {
        let compiler = toolchain::which(OsStr::new("odin"), &c.env).context("Odin missing")?;
        let compiler = dunce::canonicalize(compiler)?;
        ensure!(
            manifest["odin"]["compiler_sha256"] == sha(&compiler)?,
            "Odin compiler differs from the package compiler"
        );
        command.extend(["--odin".into(), path(&compiler)]);
        Some(compiler)
    } else {
        None
    };
    c.command(command, "native-trace", None, 300.)?;
    let trace_path = trace.join("evidence.json");
    let proof = delivery::read_json(&trace_path)?;
    let library = sha(&package.join("libweekend_db.so"))?;
    let mut languages = vec!["rust", "c"];
    if let Some(compiler) = compiler {
        let name = compiler.to_str().context("UTF-8 Odin compiler path")?;
        ensure!(
            proof["native"]["tool_binary_sha256"][name] == manifest["odin"]["compiler_sha256"],
            "Odin trace compiler identity mismatch"
        );
        languages.push("odin");
    }
    ensure!(
        proof["passed"] == true
            && proof["verified"] == true
            && proof["engine_source_id"] == manifest["source_id"]
            && proof["package_manifest_sha256"] == pin
            && proof["source_manifest_sha256"] == pin
            && proof["native"]["library_sha256"] == library
            && proof["trace_cells"] == 134
            && proof["native"]["languages"] == json!(languages)
            && languages.iter().all(|name| {
                text(&proof["native"]["transcript_sha256"][*name]).ok() == Some(EXPECTED_SHA256)
            }),
        "minimum-compiler delivered trace mismatch"
    );
    c.evidence["delivery"] = json!({
        "package_manifest_sha256":pin, "trace_evidence_sha256":sha(&trace_path)?,
        "library_sha256":library, "trace_cells":134, "languages":languages,
        "rust_source_verified":true, "odin_verified":odin,
    });
    Ok(())
}

pub fn run(run: &Run, opts: Options) -> Result<()> {
    let scope = format!(
        "native {} declared minimum Rust compiler",
        std::env::consts::OS
    );
    let mut c = Campaign::new(run, &opts.output, &scope)?;
    let result = (|| -> Result<()> {
        ensure!(c.evidence["dirty"] == false, "clean checkout required");
        let compiler_version = minimum(&run.root)?;
        let target = c.output.join("build");
        c.env = cargo_build_env(&c.env);
        c.env.retain(|k, _| {
            !k.eq_ignore_ascii_case("CARGO_NET_OFFLINE")
                && !k.eq_ignore_ascii_case("WEEKEND_DB_SOURCE_ID")
        });
        c.env.insert("CARGO_NET_OFFLINE".into(), "true".into());
        c.env.insert("CARGO_TARGET_DIR".into(), path(&target));
        #[cfg(windows)]
        let host = "x86_64-pc-windows-msvc";
        #[cfg(windows)]
        let runtime_parent = c.output.clone();
        #[cfg(target_os = "linux")]
        let mut runtime = {
            // The copied-source build rejects compiler references to the original tree.
            let temp = dunce::canonicalize(std::env::temp_dir())?;
            ensure!(
                !temp.starts_with(&run.root),
                "TMPDIR must be outside the source checkout"
            );
            Scratch::new(&temp, "msrv-runtime")?
        };
        #[cfg(target_os = "linux")]
        let runtime_parent = runtime.path().to_owned();
        #[cfg(target_os = "linux")]
        let host = {
            let arch = std::env::consts::ARCH;
            ensure!(
                matches!(arch, "x86_64" | "aarch64"),
                "unsupported Linux architecture"
            );
            let kernel = c.command(cmd(["uname", "-a"]), "kernel", None, 30.)?;
            let machine = c.command(cmd(["uname", "-m"]), "machine", None, 30.)?;
            ensure!(machine == arch, "native Linux architecture mismatch");
            let mount = c.command(
                vec![
                    "findmnt".into(),
                    "--json".into(),
                    "--output".into(),
                    "TARGET,SOURCE,FSTYPE,OPTIONS".into(),
                    "--target".into(),
                    path(runtime.path()),
                ],
                "mount",
                None,
                30.,
            )?;
            let mount: serde_json::Value = serde_json::from_str(&mount)?;
            ensure!(
                mount["filesystems"][0]["fstype"] == "ext4",
                "native ext4 runtime required"
            );
            c.evidence["kernel"] = json!(kernel);
            c.evidence["volume"] = mount["filesystems"][0].clone();
            c.env.insert("TMPDIR".into(), path(runtime.path()));
            format!("{arch}-unknown-linux-gnu")
        };
        let rustc = c.command(cmd(["rustc", "-Vv"]), "rustc", None, 30.)?;
        let cargo = c.command(cmd(["cargo", "-V"]), "cargo", None, 30.)?;
        ensure!(
            rustc
                .lines()
                .any(|line| line == format!("release: {compiler_version}"))
                && rustc.lines().any(|line| line == format!("host: {host}"))
                && cargo.split_ascii_whitespace().nth(1) == Some(&compiler_version),
            "select Rust and Cargo {compiler_version} for {host}"
        );
        c.evidence["minimum_version"] = json!(compiler_version);
        c.evidence["rustc"] = json!(rustc);
        c.evidence["cargo"] = json!(cargo);
        let locks = committed_locks(&mut c, &run.root, "before")?;
        c.evidence["lockfiles_sha256"] = json!(locks);
        c.save()?;
        let xtask = std::env::current_exe()?;
        run_delivery(&mut c, &xtask, &rustc, &locks["Cargo.lock"], opts.odin)?;
        c.save()?;
        run_checks(&mut c, &run.root, &target, &runtime_parent, &xtask)?;
        #[cfg(target_os = "linux")]
        runtime.close()?;
        ensure!(
            committed_locks(&mut c, &run.root, "after")? == locks,
            "locked inputs changed"
        );
        ensure!(
            c.command(
                cmd(["git", "status", "--porcelain"]),
                "final-status",
                None,
                30.
            )?
            .is_empty(),
            "qualification modified checkout"
        );
        c.evidence["artifact_sha256"] = json!(qualification_inventory(
            &c.output,
            &["build", "trace/cargo-target", "evidence.json"]
        )?);
        Ok(())
    })();
    c.finish(result)
}
