//! Independent operation traces against relocated, exact delivered artifacts.
//! No engine imports; builds and execution remain bounded and network-free.
use crate::{
    assurance::{self, Campaign},
    delivery, faults, rust_source,
    supervise::{self, Request},
    support::{Run, cmd, path, sha, text},
    toolchain,
    trace_model::Trace,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    pub package: Option<PathBuf>,
    #[arg(long)]
    pub source_package: Option<PathBuf>,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long)]
    pub odin: Option<PathBuf>,
    /// Already-local immutable Linux image; the original checkout is not mounted.
    #[arg(long)]
    pub docker_image: Option<String>,
    #[arg(long, hide = true)]
    pub worker: bool,
}
fn clean_env(env: &BTreeMap<OsString, OsString>, source: &str) -> BTreeMap<OsString, OsString> {
    let mut env = env.clone();
    env.retain(|key, _| {
        let key = key.to_string_lossy();
        matches!(key.as_ref(), "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN")
            || !(key.starts_with("RUST")
                || key.starts_with("CARGO_BUILD_")
                || key.starts_with("CARGO_TARGET_")
                || key.starts_with("CARGO_PROFILE_")
                || matches!(
                    key.as_ref(),
                    "CARGO_ENCODED_RUSTFLAGS"
                        | "CARGO_ENCODED_RUSTDOCFLAGS"
                        | "LLVM_PROFILE_FILE"
                        | "LD_PRELOAD"
                        | "DYLD_INSERT_LIBRARIES"
                        | "DYLD_LIBRARY_PATH"
                        | "LD_LIBRARY_PATH"
                        | "CARGO_HOME"
                ))
    });
    for (k, v) in [
        ("CARGO_INCREMENTAL", "0"),
        ("CARGO_NET_OFFLINE", "true"),
        ("WEEKEND_DB_SOURCE_ID", source),
    ] {
        env.insert(k.into(), v.into());
    }
    env
}
// Check the final entry before resolving parents or reading any package input.
fn output_path(root: &Path, requested: &Path) -> Result<PathBuf> {
    let output = assurance::absolute(root, requested);
    match fs::symlink_metadata(&output) {
        Ok(_) => anyhow::bail!("output entry already exists: {}", output.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    crate::notices::resolve(&output)
}
// Qualification trees include vendored crates, unlike the small delivered package.
// Prune caches BEFORE visiting them, and reject aliases/nonregular inputs.
pub(crate) fn inventory(root: &Path, skip: &[&str]) -> Result<BTreeMap<String, String>> {
    fn walk(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<String, String>,
        skip: &[&str],
        depth: usize,
        entries: &mut usize,
        total: &mut u64,
    ) -> Result<()> {
        ensure!(depth <= 64, "trace directory depth budget");
        ensure!(fs::symlink_metadata(dir)?.is_dir(), "trace directory alias");
        for entry in fs::read_dir(dir)? {
            let file = entry?.path();
            let name = file
                .strip_prefix(root)?
                .to_str()
                .context("UTF-8 trace path")?
                .replace('\\', "/");
            if (skip.contains(&"cargo-target")
                && file.file_name().is_some_and(|n| n == "cargo-target"))
                || skip
                    .iter()
                    .any(|s| name == *s || name.starts_with(&format!("{s}/")))
            {
                continue;
            }
            *entries += 1;
            ensure!(*entries <= 100_000, "trace entry budget");
            let meta = fs::symlink_metadata(&file)?;
            if meta.is_dir() {
                walk(root, &file, out, skip, depth + 1, entries, total)?;
            } else {
                ensure!(
                    meta.is_file() && meta.len() <= 512 * 1024 * 1024,
                    "nonregular/oversized trace input"
                );
                *total = total
                    .checked_add(meta.len())
                    .context("trace byte overflow")?;
                ensure!(*total <= 8 * 1024 * 1024 * 1024, "trace byte budget");
                out.insert(name, sha(&file)?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out, skip, 0, &mut 0, &mut 0)?;
    Ok(out)
}
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    // Validate every input before creating any destination, then verify the copy.
    let before = inventory(from, &[])?;
    fn copy(from: &Path, to: &Path) -> Result<()> {
        fs::create_dir(to)?;
        for e in fs::read_dir(from)? {
            let e = e?;
            let meta = fs::symlink_metadata(e.path())?;
            if meta.is_dir() {
                copy(&e.path(), &to.join(e.file_name()))?;
            } else {
                ensure!(meta.is_file(), "nonregular trace copy input");
                fs::copy(e.path(), to.join(e.file_name()))?;
            }
        }
        Ok(())
    }
    copy(from, to)?;
    ensure!(
        before == inventory(from, &[])? && before == inventory(to, &[])?,
        "trace copy changed inputs"
    );
    Ok(())
}
fn write_json(file: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(file, bytes)?;
    Ok(())
}
fn source_files(root: &Path) -> Result<BTreeMap<String, String>> {
    ["consumer.c", "odin/main.odin", "rust/src/main.rs"]
        .into_iter()
        .map(|n| Ok((n.into(), sha(&root.join(n))?)))
        .collect()
}
fn vendor_config(source: &Path, vendor: &Path) -> Result<()> {
    let cargo = source.join(".cargo");
    fs::create_dir_all(&cargo)?;
    let old = cargo.join("config");
    let modern = cargo.join("config.toml");
    ensure!(
        !(old.exists() && modern.exists()),
        "ambiguous Cargo configuration"
    );
    let config = if old.exists() { old } else { modern };
    let mut contents = if config.exists() {
        fs::read_to_string(&config)?
    } else {
        String::new()
    };
    let table: toml::Table = toml::from_str(&contents)?;
    ensure!(
        !table.contains_key("source"),
        "packaged Cargo source policy cannot be overwritten"
    );
    contents.push_str(&format!("\n[source.crates-io]\nreplace-with = \"trace-vendor\"\n[source.trace-vendor]\ndirectory = {}\n", toml::Value::String(vendor.to_str().context("UTF-8 vendor path")?.into())));
    let _: toml::Table = toml::from_str(&contents)?;
    fs::write(config, contents)?;
    Ok(())
}
fn dep_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = vec![];
    for e in fs::read_dir(root)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            out.extend(dep_files(&e.path())?);
        } else if e.path().extension().is_some_and(|s| s == "d") {
            out.push(e.path());
        }
    }
    Ok(out)
}
fn local(c: &mut Campaign<'_>, work: &Path, source_id: &str, odin: Option<&Path>) -> Result<Value> {
    let package = work.join("relocated-package");
    let source = work.join("relocated-source/rust-source");
    let library = package.join(if cfg!(target_os = "macos") {
        "libweekend_db.dylib"
    } else {
        "libweekend_db.so"
    });
    ensure!(library.is_file(), "package is not for the native platform");
    c.env = clean_env(&c.env, source_id);
    let overlay = work.join("source-overlay");
    copy_tree(&source, &overlay)?;
    if work.join("vendor").is_dir() {
        vendor_config(&overlay, &work.join("vendor"))?;
    }
    let driver = overlay.join("examples/rust_package/src/main.rs");
    let original_main = sha(&driver)?;
    fs::copy(work.join("inputs/rust/src/main.rs"), &driver)?;
    // Check every manifest, including crates outside the consumer's dependency graph.
    for name in inventory(&overlay, &[])?
        .keys()
        .filter(|s| s.ends_with("Cargo.toml"))
    {
        let manifest: toml::Table = toml::from_str(&fs::read_to_string(overlay.join(name))?)?;
        ensure!(
            manifest
                .get("package")
                .is_none_or(|p| p.get("publish").and_then(toml::Value::as_bool) == Some(false)),
            "packaged Rust crate must declare publish=false"
        );
    }
    let lock = overlay.join("examples/rust_package/Cargo.lock");
    let original_lock = sha(&lock)?;
    let manifest = overlay.join("examples/rust_package/Cargo.toml");
    for (name, args) in [
        ("rustc-version", cmd(["rustc", "-Vv"])),
        ("cargo-version", cmd(["cargo", "-V"])),
        ("cc-version", cmd(["cc", "--version"])),
    ] {
        c.command(args, name, None, 30.)?;
    }
    let metadata = c.command_at(
        vec![
            "cargo".into(),
            "metadata".into(),
            "--offline".into(),
            "--locked".into(),
            "--format-version=1".into(),
            "--manifest-path".into(),
            path(&manifest),
        ],
        "metadata",
        None,
        300.,
        &overlay,
    )?;
    let local_dependencies =
        rust_source::validate_metadata(&delivery::json(metadata.as_bytes())?, &overlay)?;
    let target = work.join("cargo-target");
    c.command_at(
        vec![
            "cargo".into(),
            "build".into(),
            "--release".into(),
            "--locked".into(),
            "--offline".into(),
            "--manifest-path".into(),
            path(&manifest),
            "--target-dir".into(),
            path(&target),
        ],
        "build-rust",
        None,
        300.,
        &overlay,
    )?;
    ensure!(
        sha(&lock)? == original_lock,
        "locked dependency inventory changed"
    );
    let context = delivery::read_json(&work.join("context.json"))?;
    let original = PathBuf::from(text(&context["original_checkout"])?);
    for dep in dep_files(&target)? {
        ensure!(
            !rust_source::references_original(
                &fs::read_to_string(&dep)?,
                &original,
                &[work.to_owned()]
            )?,
            "compiler consumed original checkout: {}",
            dep.display()
        );
    }
    let rust = work.join("consumer-rust");
    fs::copy(target.join("release/private-rust-consumer"), &rust)?;
    let cc = work.join("consumer-c");
    let rpath = if cfg!(target_os = "macos") {
        "@loader_path/relocated-package"
    } else {
        "$ORIGIN/relocated-package"
    };
    c.command(
        vec![
            "cc".into(),
            "-std=c11".into(),
            "-O2".into(),
            "-UNDEBUG".into(),
            "-Wall".into(),
            "-Wextra".into(),
            "-Werror".into(),
            "-I".into(),
            path(&package),
            path(work.join("inputs/consumer.c")),
            path(&library),
            format!("-Wl,-rpath,{rpath}").into(),
            "-o".into(),
            path(&cc),
        ],
        "build-c",
        None,
        300.,
    )?;
    let mut binaries = vec![("rust", rust), ("c", cc)];
    if let Some(odin) = odin {
        ensure!(
            package.join("bindings/odin/weekend_db.odin").is_file(),
            "Odin bindings absent"
        );
        c.command(
            cmd([odin.as_os_str(), OsStr::new("version")]),
            "odin-version",
            None,
            30.,
        )?;
        ensure!(
            c.command(
                cmd([odin.as_os_str(), OsStr::new("build"), OsStr::new("-help")]),
                "odin-build-help",
                None,
                30.
            )?
            .contains("-o:speed"),
            "Odin speed optimization is not documented"
        );
        let exe = work.join("consumer-odin");
        c.command(
            vec![
                path(odin),
                "build".into(),
                path(work.join("inputs/odin")),
                "-o:speed".into(),
                format!("-out:{}", exe.display()).into(),
                format!("-collection:weekend={}", package.join("bindings").display()).into(),
                format!(
                    "-define:WEEKEND_DB_LIBRARY=../../{}",
                    library.file_name().unwrap().to_str().unwrap()
                )
                .into(),
                format!("-extra-linker-flags:-Wl,-rpath,{rpath}").into(),
            ],
            "build-odin",
            None,
            300.,
        )?;
        binaries.push(("odin", exe));
    }
    let expected = fs::read(work.join("expected.txt"))?;
    let mut transcripts = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    for (language, binary) in &binaries {
        let label = format!("run-consumer-{language}");
        let saved = c.env.clone();
        if *language != "rust" {
            c.env.insert(
                if cfg!(target_os = "macos") {
                    "DYLD_PRINT_LIBRARIES"
                } else {
                    "LD_DEBUG"
                }
                .into(),
                if cfg!(target_os = "macos") {
                    "1"
                } else {
                    "libs"
                }
                .into(),
            );
        }
        let execution = c.command_at(
            vec![
                path(binary),
                path(work.join("trace.bin")),
                path(work.join(format!("consumer-{language}.db"))),
                source_id.into(),
            ],
            &label,
            None,
            300.,
            work,
        );
        c.env = saved;
        execution?;
        let stdout = c.output.join(format!("{label}.stdout.log"));
        ensure!(
            fs::read(&stdout)? == expected,
            "independent model mismatch: {language}"
        );
        if *language != "rust" {
            let stderr = fs::read_to_string(c.output.join(format!("{label}.stderr.log")))?;
            let loaded = if cfg!(target_os = "macos") {
                library.display().to_string()
            } else {
                format!("calling init: {}", library.display())
            };
            ensure!(
                stderr.contains(&loaded),
                "loader did not confirm exact relocated library: {language}"
            );
            c.command(
                vec![
                    if cfg!(target_os = "macos") {
                        "otool"
                    } else {
                        "readelf"
                    }
                    .into(),
                    if cfg!(target_os = "macos") {
                        "-L"
                    } else {
                        "-d"
                    }
                    .into(),
                    path(binary),
                ],
                &format!("dependencies-{language}"),
                None,
                30.,
            )?;
        }
        transcripts.insert(*language, sha(&stdout)?);
        hashes.insert(*language, sha(binary)?);
    }
    let mut tools = BTreeMap::new();
    for name in [OsStr::new("rustc"), OsStr::new("cargo"), OsStr::new("cc")]
        .into_iter()
        .chain(odin.map(Path::as_os_str))
    {
        let executable = toolchain::which(name, &c.env).context("tool executable missing")?;
        tools.insert(
            name.to_string_lossy().into_owned(),
            sha(&dunce::canonicalize(executable)?)?,
        );
    }
    Ok(
        json!({"languages":binaries.iter().map(|(n,_)|n).collect::<Vec<_>>(),"consumer_profiles":{"rust":"cargo --release; runtime assert! enabled","c":"-O2 -UNDEBUG; assert enabled","odin":if odin.is_some(){json!("-o:speed; ODIN_DISABLE_ASSERT=false")}else{Value::Null}},"tool_binary_sha256":tools,"original_checkout_available":original.exists(),"overlay":{"path":"examples/rust_package/src/main.rs","original_sha256":original_main,"driver_sha256":sha(&driver)?,"original_lock_sha256":original_lock},"local_dependencies":local_dependencies,"consumer_sha256":hashes,"library_sha256":sha(&library)?,"transcript_sha256":transcripts}),
    )
}
pub(crate) fn nonce() -> Result<String> {
    let mut b = [0; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b.iter().map(|n| format!("{n:02x}")).collect())
}
fn cleanup_commands(container: &str, volume: &str) -> Result<[Vec<OsString>; 2]> {
    let nonce = container
        .strip_prefix("weekend-trace-")
        .context("unowned container")?;
    ensure!(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && volume == format!("weekend-trace-data-{nonce}"),
        "cleanup requires matching private UUID pair"
    );
    Ok([
        cmd(["docker", "rm", "-f", container]),
        cmd(["docker", "volume", "rm", volume]),
    ])
}
fn cleanup(c: &mut Campaign<'_>, container: &str, volume: &str) -> Result<()> {
    docker_retire(c, container, volume, cleanup_commands(container, volume)?)
}
pub(crate) fn docker_retire(
    c: &mut Campaign<'_>,
    container: &str,
    volume: &str,
    commands: [Vec<OsString>; 2],
) -> Result<()> {
    let mut processes_retired = true;
    for (i, args) in commands.into_iter().enumerate() {
        // Removal may return 1 for an already absent object. Successful absence
        // queries below, independently of removal's status, are mandatory.
        let label = format!("cleanup-{i}");
        let result = supervise::run(&Request {
            argv: args
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect(),
            cwd: Some(c.run.root.clone()),
            env: Some(
                c.env
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.to_string_lossy().into_owned(),
                            v.to_string_lossy().into_owned(),
                        )
                    })
                    .collect(),
            ),
            stdout: c.output.join(format!("{label}.stdout.log")),
            stderr: c.output.join(format!("{label}.stderr.log")),
            report: Some(c.output.join(format!("{label}.process.json"))),
            timeout_seconds: 30.,
            maximum_bytes: 1024 * 1024,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        });
        c.evidence[&label] = match result {
            Ok(r) => {
                processes_retired &= retirement(&r);
                serde_json::to_value(r)?
            }
            Err(e) => {
                processes_retired = false;
                json!({"error":format!("{e:#}")})
            }
        };
    }
    let containers = c.retirement_command(
        cmd(["docker", "ps", "-a", "--format={{.Names}}"]),
        "container-absence",
    );
    let volumes = c.retirement_command(
        cmd(["docker", "volume", "ls", "--format={{.Name}}"]),
        "volume-absence",
    );
    c.save()?;
    ensure!(
        !containers?.lines().any(|s| s == container) && !volumes?.lines().any(|s| s == volume),
        "private Docker resource still exists"
    );
    ensure!(
        processes_retired,
        "Docker cleanup command supervision failed"
    );
    Ok(())
}
fn retirement(r: &supervise::Report) -> bool {
    matches!(r.returncode, Some(0 | 1))
        && !r.timed_out
        && !r.cancelled
        && r.cleanup_verified
        && r.cleanup_failure.is_none()
        && (r.failure.is_none() || r.failure.as_deref() == Some("child exit Some(1)"))
}
fn docker(c: &mut Campaign<'_>, image: &str, source_id: &str) -> Result<Value> {
    ensure!(
        image.len() == 71
            && image.starts_with("sha256:")
            && image[7..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "immutable local Docker image required"
    );
    ensure!(
        c.command(
            cmd(["docker", "image", "inspect", "--format={{.Id}}", image]),
            "image-inspect",
            None,
            30.
        )? == image,
        "Docker image identity mismatch"
    );
    let work = c.output.clone();
    let source = work.join("relocated-source/rust-source");
    c.env = clean_env(&c.env, source_id);
    c.command_at(
        vec![
            "cargo".into(),
            "vendor".into(),
            "--offline".into(),
            "--locked".into(),
            "--manifest-path".into(),
            path(source.join("examples/rust_package/Cargo.toml")),
            path(work.join("vendor")),
        ],
        "vendor",
        None,
        300.,
        &source,
    )?;
    let seed = work.join("seed");
    fs::create_dir(&seed)?;
    for n in ["inputs", "relocated-package", "relocated-source", "vendor"] {
        copy_tree(&work.join(n), &seed.join(n))?;
    }
    for n in ["context.json", "trace.bin", "expected.txt"] {
        fs::copy(work.join(n), seed.join(n))?;
    }
    faults::stage_runner(&c.run.root, &seed.join("runner"))?;
    let frozen = inventory(&seed, &[])?;
    write_json(&seed.join("seed-manifest.json"), &frozen)?;
    let id = nonce()?;
    let container = format!("weekend-trace-{id}");
    let volume = format!("weekend-trace-data-{id}");
    c.evidence["docker"] = json!({"image":image,"container":container,"volume":volume,"cleanup_verified":false,"seed_sha256":sha(&seed.join("seed-manifest.json"))?});
    c.save()?;
    let result = (|| -> Result<Value> {
        c.command(
            cmd(["docker", "volume", "create", &volume]),
            "create-volume",
            None,
            30.,
        )?;
        let registry = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo"))
            .join("registry");
        // No original-checkout mount. Only the immutable seed and registry cache.
        let script = "set -eu; mkdir /private-volume/tmp; export TMPDIR=/private-volume/tmp; cargo build --offline --locked --manifest-path /seed/runner/Cargo.toml -p xtask --target-dir /private-volume/tool; exec /private-volume/tool/debug/xtask artifact-trace --worker --output /private-volume/result";
        let executed = c.command(
            vec![
                "docker".into(),
                "run".into(),
                "--pull=never".into(),
                "--name".into(),
                container.clone().into(),
                "--network=none".into(),
                "--memory=2g".into(),
                "--cpus=2".into(),
                "--pids-limit=256".into(),
                "--ulimit=core=0".into(),
                "--mount".into(),
                format!("type=bind,source={},target=/seed,readonly", seed.display()).into(),
                "--mount".into(),
                format!(
                    "type=bind,source={},target=/usr/local/cargo/registry,readonly",
                    registry.display()
                )
                .into(),
                "--mount".into(),
                format!("type=volume,source={volume},target=/private-volume").into(),
                image.into(),
                "sh".into(),
                "-c".into(),
                script.into(),
            ],
            "linux",
            None,
            300.,
        );
        let copied = c.command(
            cmd([
                OsStr::new("docker"),
                OsStr::new("cp"),
                OsStr::new(&format!("{container}:/private-volume/result")),
                work.join("linux-result").as_os_str(),
            ]),
            "copyback",
            None,
            120.,
        );
        executed?;
        copied?;
        let output = work.join("linux-result");
        let result = delivery::read_json(&output.join("evidence.json"))?;
        ensure!(
            result["passed"] == true
                && result["verified"] == true
                && result["native"]["original_checkout_available"] == false
                && result["frozen_inputs_verified"] == true,
            "Linux worker did not qualify"
        );
        let declared: BTreeMap<String, String> =
            serde_json::from_value(result["artifact_sha256"].clone())?;
        ensure!(
            inventory(&output, &["evidence.json", "cargo-target"])? == declared,
            "copied worker evidence mismatch"
        );
        ensure!(
            inventory(&seed, &["seed-manifest.json"])? == frozen,
            "host seed changed"
        );
        verify_frozen(&output, &frozen)?;
        delivery::verified(&output.join("relocated-package"))?;
        delivery::verified(&output.join("relocated-source"))?;
        c.evidence["worker_evidence_sha256"] = json!(sha(&output.join("evidence.json"))?);
        Ok(result["native"].clone())
    })();
    let retired = cleanup(c, &container, &volume);
    c.evidence["docker"]["cleanup_verified"] = json!(retired.is_ok());
    c.save()?;
    retired?;
    result
}
fn verify_frozen(root: &Path, expected: &BTreeMap<String, String>) -> Result<()> {
    for (name, digest) in expected {
        ensure!(
            Path::new(name)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "unsafe frozen path"
        );
        ensure!(
            fs::symlink_metadata(root.join(name))?.is_file() && sha(&root.join(name))? == *digest,
            "frozen input changed: {name}"
        );
    }
    Ok(())
}
fn worker(run: &Run, output: &Path) -> Result<()> {
    let seed = Path::new("/seed");
    let frozen: BTreeMap<String, String> =
        serde_json::from_value(delivery::read_json(&seed.join("seed-manifest.json"))?)?;
    ensure!(
        inventory(seed, &["seed-manifest.json"])? == frozen,
        "frozen seed mismatch"
    );
    let mut c = Campaign::detached(
        run,
        output,
        "detached delivered trace worker; no Git identity",
    )?;
    c.evidence["verified"] = json!(false);
    c.limits(16 * 1024 * 1024, Some(64 * 1024 * 1024))?;
    c.save()?;
    let work = c.output.clone();
    let result = (|| -> Result<()> {
        for entry in fs::read_dir(seed)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                copy_tree(&entry.path(), &work.join(entry.file_name()))?;
            } else {
                fs::copy(entry.path(), work.join(entry.file_name()))?;
            }
        }
        verify_frozen(&work, &frozen)?;
        write_json(&work.join("frozen-before.json"), &frozen)?;
        fs::create_dir(work.join("cargo-target"))?;
        let mut filesystems = BTreeMap::new();
        for (label, p) in [
            ("build", work.join("cargo-target")),
            ("runtime", work.clone()),
        ] {
            let data = c.command(
                cmd([
                    OsStr::new("findmnt"),
                    OsStr::new("--json"),
                    OsStr::new("--target"),
                    p.as_os_str(),
                    OsStr::new("--output"),
                    OsStr::new("TARGET,SOURCE,FSTYPE,OPTIONS"),
                ]),
                &format!("filesystem-{label}"),
                None,
                30.,
            )?;
            let info = delivery::json(data.as_bytes())?;
            ensure!(
                info["filesystems"]
                    .as_array()
                    .context("findmnt records")?
                    .len()
                    == 1
                    && info["filesystems"][0]["fstype"] == "ext4",
                "native Linux profile requires ext4"
            );
            filesystems.insert(label, info);
        }
        let info = delivery::verified(&work.join("relocated-package"))?;
        let source = delivery::verified(&work.join("relocated-source"))?;
        ensure!(
            info["source_id"] == source["source_id"],
            "package identities differ"
        );
        let native = local(&mut c, &work, text(&info["source_id"])?, None)?;
        ensure!(
            native["original_checkout_available"] == false,
            "worker can access original checkout"
        );
        verify_frozen(&work, &frozen)?;
        ensure!(
            inventory(seed, &["seed-manifest.json"])? == frozen,
            "read-only seed changed"
        );
        delivery::verified(&work.join("relocated-package"))?;
        delivery::verified(&work.join("relocated-source"))?;
        write_json(&work.join("frozen-after.json"), &frozen)?;
        c.evidence["native"] = native;
        c.evidence["filesystem"] = json!(filesystems);
        c.evidence["frozen_inputs_verified"] = json!(true);
        Ok(())
    })();
    complete(c, result)
}
fn complete(mut c: Campaign<'_>, result: Result<()>) -> Result<()> {
    let result = result.and_then(|()| {
        ensure!(c.unchanged()?, "runner inputs changed");
        Ok(())
    });
    c.evidence["verified"] = json!(result.is_ok());
    c.evidence["artifact_sha256"] =
        json!(inventory(&c.output, &["evidence.json", "cargo-target"])?);
    c.finish(result)
}
pub fn run(run: &Run, opts: Options) -> Result<()> {
    if opts.worker {
        return worker(run, &opts.output);
    }
    let output = output_path(&run.root, &opts.output)?;
    let package = dunce::canonicalize(opts.package.context("--package required")?)?;
    let source = dunce::canonicalize(opts.source_package.unwrap_or_else(|| package.clone()))?;
    ensure!(
        !output.starts_with(&package) && !output.starts_with(&source),
        "output must be outside inputs"
    );
    ensure!(
        !(opts.docker_image.is_some() && opts.odin.is_some()),
        "Linux Odin is not qualified by this runner"
    );
    let package_info = delivery::verified(&package)?;
    let source_info = delivery::verified(&source)?;
    ensure!(
        package_info["source_id"] == source_info["source_id"]
            && source.join("rust-source").is_dir(),
        "native/source package identities differ or Rust source missing"
    );
    let source_id = text(&package_info["source_id"])?;
    let input = run.root.join("tools/artifact-trace");
    let before = source_files(&input)?;
    let pin = sha(&package.join("manifest.json"))?;
    let source_pin = sha(&source.join("manifest.json"))?;
    let mut c = Campaign::new(
        run,
        &output,
        "delivered 134-cell exact-artifact trace; no Windows qualification",
    )?;
    c.evidence["verified"] = json!(false);
    c.limits(16 * 1024 * 1024, Some(64 * 1024 * 1024))?;
    c.save()?;
    let result = (|| -> Result<()> {
        c.evidence["engine_source_id"] = json!(source_id);
        c.evidence["package_manifest_sha256"] = json!(pin);
        c.evidence["source_manifest_sha256"] = json!(source_pin);
        c.evidence["consumer_inputs_sha256"] = json!(before);
        let inputs = output.join("inputs");
        fs::create_dir(&inputs)?;
        for name in before.keys() {
            let dest = inputs.join(name);
            fs::create_dir_all(dest.parent().unwrap())?;
            fs::copy(input.join(name), dest)?;
        }
        write_json(
            &output.join("context.json"),
            &json!({"original_checkout":run.root}),
        )?;
        let trace = crate::trace_model::fixed(2)?;
        let bytes = trace.encode()?;
        fs::write(output.join("trace.bin"), &bytes)?;
        fs::write(
            output.join("expected.txt"),
            Trace::decode(&bytes)?.expected()?,
        )?;
        ensure!(
            sha(&output.join("trace.bin"))? == crate::trace_model::TRACE_SHA256
                && sha(&output.join("expected.txt"))? == crate::trace_model::EXPECTED_SHA256,
            "frozen model hash drift"
        );
        for (from, staged, relocated) in [
            (&package, "staged-package", "relocated-package"),
            (&source, "staged-source", "relocated-source"),
        ] {
            copy_tree(from, &output.join(staged))?;
            fs::rename(output.join(staged), output.join(relocated))?;
            delivery::verified(&output.join(relocated))?;
        }
        c.evidence["trace_cells"] = json!(trace.cells.len());
        c.evidence["trace_sha256"] = json!(sha(&output.join("trace.bin"))?);
        c.evidence["expected_sha256"] = json!(sha(&output.join("expected.txt"))?);
        let native = if let Some(image) = opts.docker_image {
            docker(&mut c, &image, source_id)?
        } else {
            local(&mut c, &output, source_id, opts.odin.as_deref())?
        };
        ensure!(
            native["transcript_sha256"]
                .as_object()
                .context("transcript hashes")?
                .values()
                .all(|v| v == crate::trace_model::EXPECTED_SHA256),
            "copied transcript mismatch"
        );
        c.evidence["native"] = native;
        for p in [
            &package,
            &source,
            &output.join("relocated-package"),
            &output.join("relocated-source"),
        ] {
            delivery::verified(p)?;
        }
        ensure!(
            sha(&package.join("manifest.json"))? == pin
                && sha(&source.join("manifest.json"))? == source_pin
                && source_files(&input)? == before,
            "qualification inputs changed"
        );
        Ok(())
    })();
    complete(c, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use std::os::unix::fs::symlink;
    #[test]
    fn absence_cannot_hide_unbounded_or_unretired_cleanup_commands() {
        let good = || supervise::Report {
            returncode: Some(1),
            failure: Some("child exit Some(1)".into()),
            cleanup_verified: true,
            ..Default::default()
        };
        assert!(retirement(&good()));
        let mut r = good();
        r.returncode = Some(2);
        assert!(!retirement(&r));
        let mut r = good();
        r.timed_out = true;
        assert!(!retirement(&r));
        let mut r = good();
        r.cancelled = true;
        assert!(!retirement(&r));
        let mut r = good();
        r.cleanup_verified = false;
        assert!(!retirement(&r));
        let mut r = good();
        r.cleanup_failure = Some("unretired descendant".into());
        assert!(!retirement(&r));
        let mut r = good();
        r.failure = Some("output limit".into());
        assert!(!retirement(&r));
    }
    #[test]
    fn evidence_prunes_nested_build_caches_before_visiting_them() {
        let temp = Scratch::new(&std::env::temp_dir(), "trace-inventory").unwrap();
        let nested = temp.path().join("linux-result/cargo-target");
        fs::create_dir_all(&nested).unwrap();
        symlink(temp.path().join("missing"), nested.join("ignored-alias")).unwrap();
        fs::write(temp.path().join("proof"), "kept").unwrap();
        assert!(inventory(temp.path(), &[]).is_err());
        let hashes = inventory(temp.path(), &["cargo-target"]).unwrap();
        assert_eq!(
            hashes,
            [("proof".into(), sha(&temp.path().join("proof")).unwrap())].into()
        );
    }
    #[test]
    fn environment_preserves_selection_and_removes_overrides() {
        let initial = [
            ("RUSTUP_HOME", "/selected/toolchains"),
            ("RUSTUP_TOOLCHAIN", "1.99.1"),
            ("RUSTFLAGS", "bad"),
            ("RUSTC_WRAPPER", "bad"),
            ("CARGO_BUILD_RUSTFLAGS", "bad"),
            ("CARGO_PROFILE_RELEASE_PANIC", "abort"),
            ("LD_PRELOAD", "bad"),
            ("CARGO_ENCODED_RUSTDOCFLAGS", "bad"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        let result = clean_env(&initial, &"a".repeat(40));
        assert_eq!(result.len(), 5);
        for key in ["RUSTUP_HOME", "RUSTUP_TOOLCHAIN"] {
            assert_eq!(result[OsStr::new(key)], initial[OsStr::new(key)]);
        }
        assert_eq!(result[OsStr::new("CARGO_NET_OFFLINE")], "true");
        assert_eq!(result[OsStr::new("CARGO_INCREMENTAL")], "0");
        assert_eq!(
            result[OsStr::new("WEEKEND_DB_SOURCE_ID")],
            OsString::from("a".repeat(40))
        );
    }
    #[test]
    fn cleanup_requires_matching_owned_uuid_pair() {
        let container = format!("weekend-trace-{}", "a".repeat(32));
        let volume = format!("weekend-trace-data-{}", "a".repeat(32));
        assert_eq!(
            cleanup_commands(&container, &volume).unwrap(),
            [
                cmd(["docker", "rm", "-f", &container]),
                cmd(["docker", "volume", "rm", &volume])
            ]
        );
        for (c, v) in [
            ("unrelated", volume.as_str()),
            (container.as_str(), "unrelated"),
            ("--all", volume.as_str()),
            (container.as_str(), &format!("{volume}x")),
            (
                container.as_str(),
                &format!("weekend-trace-data-{}", "b".repeat(32)),
            ),
        ] {
            assert!(cleanup_commands(c, v).is_err());
        }
    }
    fn opts(output: PathBuf) -> Options {
        Options {
            package: Some(PathBuf::from("nonexistent-package")),
            source_package: None,
            output,
            odin: None,
            docker_image: None,
            worker: false,
        }
    }
    #[test]
    fn existing_output_rejects_before_inputs_or_processes() {
        let temp = Scratch::new(&std::env::temp_dir(), "trace-existing").unwrap();
        let output = temp.path().join("existing");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep"), "preserve").unwrap();
        let run_context = Run::new(temp.path().to_owned());
        let error = run(&run_context, opts(output.clone()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("output entry already exists"));
        assert_eq!(fs::read_to_string(output.join("keep")).unwrap(), "preserve");
        assert_eq!(fs::read_dir(output).unwrap().count(), 1);
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
    #[test]
    fn dangling_output_rejects_before_inputs_or_work() {
        let temp = Scratch::new(&std::env::temp_dir(), "trace-dangling").unwrap();
        let output = temp.path().join("requested");
        let target = temp.path().join("redirected");
        symlink(&target, &output).unwrap();
        let run_context = Run::new(temp.path().to_owned());
        assert!(
            run(&run_context, opts(output.clone()))
                .unwrap_err()
                .to_string()
                .contains("output entry already exists")
        );
        assert!(fs::symlink_metadata(output).unwrap().is_symlink());
        assert!(!target.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
    #[test]
    fn vendor_configuration_preserves_aliases_and_rejects_source_collisions() {
        let temp = Scratch::new(&std::env::temp_dir(), "trace-vendor").unwrap();
        fs::create_dir(temp.path().join(".cargo")).unwrap();
        let file = temp.path().join(".cargo/config.toml");
        let contents = "[alias]\nxtask = \"run -p xtask --\"\n";
        fs::write(&file, contents).unwrap();
        let vendor = temp.path().join("vendor with spaces");
        vendor_config(temp.path(), &vendor).unwrap();
        let after = fs::read_to_string(&file).unwrap();
        assert!(after.starts_with(contents));
        let parsed: toml::Table = toml::from_str(&after).unwrap();
        assert_eq!(parsed["alias"]["xtask"].as_str(), Some("run -p xtask --"));
        assert_eq!(
            parsed["source"]["trace-vendor"]["directory"].as_str(),
            vendor.to_str()
        );
        assert!(vendor_config(temp.path(), &vendor).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), after);
        fs::write(temp.path().join(".cargo/config"), contents).unwrap();
        assert!(vendor_config(temp.path(), &vendor).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), after);
        fs::remove_file(&file).unwrap();
        vendor_config(temp.path(), &vendor).unwrap();
        assert!(!file.exists());
        assert!(
            fs::read_to_string(temp.path().join(".cargo/config"))
                .unwrap()
                .starts_with(contents)
        );
    }
    #[test]
    fn detached_worker_identity_does_not_invent_git_provenance() {
        let temp = Scratch::new(&std::env::temp_dir(), "trace-detached").unwrap();
        let mut run = Run::new(temp.path().to_owned());
        run.env.insert("PATH".into(), "/no-programs".into());
        let output = temp.path().join("proof");
        let campaign = Campaign::detached(&run, &output, "frozen worker").unwrap();
        assert_eq!(campaign.evidence["source"], Value::Null);
        assert_eq!(campaign.evidence["dirty"], Value::Null);
        assert_eq!(campaign.evidence["git_identity_available"], false);
        assert_eq!(campaign.evidence["commands"], json!([]));
        assert_eq!(
            campaign.evidence["runner_sha256"].as_str().unwrap().len(),
            64
        );
        campaign.finish(Ok(())).unwrap();
        assert_eq!(
            delivery::read_json(&output.join("evidence.json")).unwrap()["passed"],
            true
        );
    }
}
