use crate::{
    assurance::{self, Campaign},
    instrumentation::environment,
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Kind {
    Address,
    Thread,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Thread => "thread",
        }
    }
}
#[derive(Debug, Args)]
pub struct Options {
    #[arg(value_enum)]
    pub kind: Kind,
    #[arg(long, default_value = "nightly")]
    pub toolchain: String,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Use an explicit Darwin arm64 ThreadSanitizer runtime for this diagnostic.
    #[arg(long, requires = "runtime_sha256")]
    pub runtime: Option<PathBuf>,
    #[arg(long, requires = "runtime")]
    pub runtime_sha256: Option<String>,
}

fn runtime_flags(runtime: &Path, pin: &str, flags: &mut Vec<String>) -> Result<()> {
    let file = runtime
        .to_str()
        .context("non-UTF-8 ThreadSanitizer runtime path")?;
    let parent = runtime
        .parent()
        .and_then(Path::to_str)
        .context("runtime parent path")?;
    ensure!(
        runtime
            .file_name()
            .is_some_and(|name| name == "libclang_rt.tsan_osx_dynamic.dylib")
            && !file.contains([',', '\u{1f}'])
            && runtime.is_file()
            && sha(runtime)? == pin,
        "invalid path or SHA-256 mismatch for the Darwin ThreadSanitizer runtime"
    );
    flags.extend([
        "-Zexternal-clangrt".to_owned(),
        format!("-Clink-arg={file}"),
        format!("-Clink-arg=-Wl,-rpath,{parent}"),
    ]);
    Ok(())
}

fn calibrate_runtime(
    campaign: &mut Campaign<'_>,
    toolchain: &str,
    target: &str,
    build: &Path,
    runtime: &Path,
) -> Result<()> {
    let runtime_path = runtime.to_str().context("non-UTF-8 runtime path")?;
    let source = campaign.output.join("runtime-control");
    fs::create_dir(&source)?;
    fs::write(
        source.join("main.rs"),
        include_str!("../../tools/macos-tsan/calibration.rs"),
    )?;
    let manifest = source.join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "macos-tsan-calibration"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[[bin]]
name = "macos-tsan-calibration"
path = "main.rs"
"#,
    )?;
    fs::write(
        source.join("Cargo.lock"),
        r#"version = 4

[[package]]
name = "macos-tsan-calibration"
version = "0.0.0"
"#,
    )?;
    let mut args = cmd([
        "cargo",
        toolchain,
        "build",
        "--locked",
        "-Zbuild-std",
        "--target",
        target,
        "--manifest-path",
    ]);
    args.push(path(&manifest));
    campaign.command(args, "runtime-calibration-build", None, 300.)?;
    let binary = build.join(target).join("debug/macos-tsan-calibration");
    let expected = "WARNING: ThreadSanitizer: data race";
    campaign
        .env
        .insert("DYLD_PRINT_LIBRARIES".into(), "1".into());
    for (case, marker) in [
        ("errno-reuse", None),
        ("race", Some(expected)),
        ("errno-race", Some(expected)),
    ] {
        let name = format!("runtime-{case}");
        campaign.command(vec![path(&binary), path(case)], &name, marker, 30.)?;
        let stderr = fs::read_to_string(campaign.output.join(format!("{name}.stderr.log")))?;
        ensure!(
            stderr.lines().any(|line| {
                line.starts_with("dyld[")
                    && line
                        .split_once("> ")
                        .is_some_and(|(_, loaded)| loaded == runtime_path)
            }) && if marker.is_some() {
                stderr.contains("write_volatile")
            } else {
                !stderr.contains("ThreadSanitizer: data race")
            },
            "unexpected loaded runtime or diagnostic for {case}"
        );
    }
    campaign
        .env
        .remove(std::ffi::OsStr::new("DYLD_PRINT_LIBRARIES"));
    campaign.evidence["runtime_calibration_sha256"] = json!(sha(&binary)?);
    Ok(())
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    let output = options.output.unwrap_or_else(|| {
        PathBuf::from(format!("target/sanitizer-{}-evidence", options.kind.name()))
    });
    let mut c = Campaign::new(
        run,
        &output,
        "Workspace sanitizer with instrumented std, including xtask; diagnostic artifact only",
    )?;
    let result = (|| {
        let toolchain = format!("+{}", options.toolchain);
        let version = c.command(
            cmd(["rustc", &toolchain, "-vV"]),
            "rustc-version",
            None,
            30.,
        )?;
        let host = assurance::host(&version)?;
        let target = options.target.as_deref().unwrap_or(host);
        c.evidence["rustc"] = json!(version);
        let mut flags = vec![format!("-Zsanitizer={}", options.kind.name())];
        let runtime = if let Some(file) = &options.runtime {
            ensure!(
                cfg!(target_os = "macos")
                    && matches!(options.kind, Kind::Thread)
                    && host == "aarch64-apple-darwin"
                    && target == host,
                "external ThreadSanitizer runtime requires native macOS arm64"
            );
            let file = dunce::canonicalize(file)?;
            let pin = options
                .runtime_sha256
                .as_deref()
                .context("missing runtime SHA-256")?;
            runtime_flags(&file, pin, &mut flags)?;
            c.evidence["runtime"] = json!({"path":file,"sha256":pin});
            Some((file, pin))
        } else {
            None
        };
        let build = run
            .root
            .join(format!("target/sanitizer-{}", options.kind.name()));
        c.env = environment(&run.env, &build, &flags);
        if let Some((file, _)) = &runtime {
            c.env
                .retain(|key, _| !key.to_string_lossy().starts_with("DYLD_"));
            calibrate_runtime(&mut c, &toolchain, target, &build, file)?;
        }
        c.command(
            cmd([
                "cargo",
                &toolchain,
                "test",
                "--locked",
                "-Zbuild-std",
                "--workspace",
                "--all-features",
                "--target",
                target,
            ]),
            "workspace",
            None,
            match options.kind {
                Kind::Thread => 2400.,
                Kind::Address => 1200.,
            },
        )?;
        if let Some((file, pin)) = runtime {
            ensure!(
                sha(&file)? == pin,
                "external ThreadSanitizer runtime changed during tests"
            );
        }
        Ok(())
    })();
    c.finish(result)
}
