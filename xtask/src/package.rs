use crate::support::{Platform, Run, Scratch, cmd, hash_tree, path, sha, text};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[derive(clap::Args, Debug)]
pub struct Options {
    #[arg(short = 'o', long, value_name = "NEW-DIR")]
    output: PathBuf,
    #[arg(short = 'F', long, default_value = "", value_name = "LIST")]
    features: String,
    /// Require packaged bindings and an exact-artifact Odin consumer.
    #[arg(long)]
    odin: bool,
    /// Verify a development package that is ineligible as a release candidate.
    #[arg(long = "allow-dirty")]
    dirty: bool,
    /// Deliver checked Rust source and qualify a fresh typed/CRDT consumer.
    #[arg(long = "rust-source")]
    source: bool,
    /// Also verify source with the original checkout unavailable in Docker.
    #[arg(long = "rust-source-isolated", requires = "source")]
    isolated: bool,
    #[arg(
        long = "rust-source-image",
        default_value = "rust:1-bookworm",
        value_name = "IMAGE",
        value_parser = clap::builder::NonEmptyStringValueParser::new()
    )]
    image: String,
}

fn strings(v: &Value) -> Result<Vec<OsString>> {
    v.as_array()
        .context("expected command array")?
        .iter()
        .map(|v| Ok(path(text(v)?)))
        .collect()
}

fn source_hashes(run: &mut Run, output: &Path) -> Result<BTreeMap<String, String>> {
    let names = run.capture(&cmd([
        "git",
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
    ]))?;
    let mut out = BTreeMap::new();
    for name in names.split('\0').filter(|s| !s.is_empty()) {
        let p = run.root.join(name);
        if p.is_file() && !dunce::canonicalize(&p)?.starts_with(output) {
            out.insert(name.to_owned(), sha(&p)?);
        }
    }
    Ok(out)
}

fn identifiers(s: &str) -> impl Iterator<Item = (&str, usize)> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let bytes = s.as_bytes();
        while offset < bytes.len()
            && !(bytes[offset].is_ascii_alphanumeric() || bytes[offset] == b'_')
        {
            offset += 1;
        }
        if offset == bytes.len() {
            return None;
        }
        let start = offset;
        while offset < bytes.len()
            && (bytes[offset].is_ascii_alphanumeric() || bytes[offset] == b'_')
        {
            offset += 1;
        }
        Some((&s[start..offset], offset))
    })
}

fn verify_exports(header: &str, symbols: &str) -> Result<()> {
    let declarations: BTreeSet<_> = identifiers(header)
        .filter(|(name, end)| {
            name.starts_with("wdb_") && header[*end..].trim_start().starts_with('(')
        })
        .map(|(name, _)| name)
        .collect();
    let exports: BTreeSet<_> = identifiers(symbols)
        .map(|(name, _)| name.strip_prefix('_').unwrap_or(name))
        .filter(|name| name.starts_with("wdb_"))
        .collect();
    ensure!(
        !declarations.is_empty() && declarations == exports,
        "header/export mismatch: missing {:?}, extra {:?}",
        declarations.difference(&exports).collect::<Vec<_>>(),
        exports.difference(&declarations).collect::<Vec<_>>()
    );
    Ok(())
}

fn attribution(
    run: &mut Run,
    metadata: &Value,
    packages: &Value,
    lockfile: &Path,
    destination: &Path,
    compiler: &Value,
) -> Result<Value> {
    let sysroot = run.capture(&cmd(["rustc", "--print", "sysroot"]))?;
    run.helper("notices",&json!({"metadata":metadata,"packages":packages,"lockfile":lockfile,"destination":destination,"sysroot":sysroot,"compiler":compiler}))
}

fn add_attribution(e: &mut Value, attribution: &Value) -> Result<()> {
    let complete = attribution["attribution_inputs_complete"]
        .as_bool()
        .context("attribution gate missing")?;
    e["attribution_inputs_complete"] =
        json!(e["attribution_inputs_complete"].as_bool().unwrap_or(true) && complete);
    if e["attribution_review_items"].is_null() {
        e["attribution_review_items"] = json!([]);
    }
    e["attribution_review_items"]
        .as_array_mut()
        .unwrap()
        .extend(
            attribution["review_items"]
                .as_array()
                .context("attribution review items missing")?
                .iter()
                .cloned(),
        );
    Ok(())
}

fn require_source_profile(info: &Value, isolated: bool) -> Result<()> {
    ensure!(
        info["native_compile_verified"] == true,
        "requested native Rust source profile did not qualify"
    );
    ensure!(
        !isolated || info["linux_isolation_verified"] == true,
        "requested isolated Rust source profile did not qualify"
    );
    Ok(())
}

pub fn run(run: &mut Run, args: Options) -> Result<()> {
    let platform = Platform::host()?;
    ensure!(
        platform != Platform::Other,
        "native packages require Mac, Linux or Windows MSVC"
    );
    let dirty = run.capture(&cmd(["git", "status", "--porcelain"]))?;
    ensure!(
        dirty.is_empty() || args.dirty,
        "a candidate requires clean source; --allow-dirty creates an ineligible development package"
    );
    let output = std::path::absolute(&args.output)?;
    let parent = output.parent().context("package output has no parent")?;
    fs::create_dir_all(parent)?;
    fs::create_dir(&output).context("package output must be a new directory")?;
    let output = dunce::canonicalize(&output)?;
    run.log_dir = Some(output.clone());
    let source = run.capture(&cmd(["git", "rev-parse", "HEAD"]))?;
    let source_id = format!("{source}{}", if dirty.is_empty() { "" } else { "-dirty" });
    let initial = source_hashes(run, &output)?;
    let inherited = run.helper("configuration", &json!({}))?;
    run.env
        .insert(path("WEEKEND_DB_SOURCE_ID"), path(&source_id));
    run.env
        .insert(path("CARGO_PROFILE_RELEASE_PANIC"), path("unwind"));
    run.env.insert(path("CARGO_INCREMENTAL"), path("0"));
    let toolchain = run.capture(&cmd(["rustc", "-Vv"]))?;
    let cargo = run.capture(&cmd(["cargo", "-V"]))?;
    let mut e = json!({"source":source,"source_id":source_id,"dirty_status":dirty,"candidate_eligible":false,"verified":false,"features":args.features,
        "requested_profiles":{"odin":args.odin,"rust_source":args.source,"rust_source_isolated":args.isolated},"toolchain":toolchain,"cargo":cargo,"lockfile_sha256":sha(&run.root.join("Cargo.lock"))?,"source_files_sha256":initial,"inherited_build_configuration":inherited});
    let result = qualify(run, &args, platform, &output, &initial, &mut e);
    // The actual location may differ only after a failed relocation restore.
    let actual = run
        .log_dir
        .as_ref()
        .context("package output location missing")?;
    e["commands"] = json!(run.commands);
    e["sha256"] = serde_json::to_value(hash_tree(actual)?)?;
    if let Err(error) = &result {
        e["failure"] = json!(format!("{error:#}"));
    }
    fs::write(
        actual.join("manifest.json"),
        serde_json::to_string_pretty(&e)? + "\n",
    )?;
    result?;
    println!(
        "Verified private package {}, candidate eligible {}",
        output.display(),
        e["candidate_eligible"]
    );
    Ok(())
}

fn qualify(
    run: &mut Run,
    args: &Options,
    platform: Platform,
    output: &Path,
    initial: &BTreeMap<String, String>,
    e: &mut Value,
) -> Result<()> {
    let root = run.root.clone();
    let source = text(&e["source"])?.to_owned();
    let source_id = text(&e["source_id"])?.to_owned();
    let mut build = Scratch::new(&std::env::temp_dir(), "release")?;
    if platform == Platform::Windows {
        ensure!(
            text(&e["toolchain"])?
                .lines()
                .any(|line| line == "host: x86_64-pc-windows-msvc"),
            "Windows packaging qualifies only the x86_64 MSVC host"
        );
        let env: BTreeMap<String, String> =
            serde_json::from_value(run.helper("environment", &json!({"directory":build.path()}))?)?;
        run.env = env.into_iter().map(|(k, v)| (path(k), path(v))).collect();
        run.env
            .insert(path("WEEKEND_DB_SOURCE_ID"), path(&source_id));
        run.env
            .insert(path("CARGO_PROFILE_RELEASE_PANIC"), path("unwind"));
        run.env.insert(path("CARGO_INCREMENTAL"), path("0"));
    }
    e["effective_build_configuration"] = run.helper("configuration", &json!({}))?;
    let plan = run.helper("plan", &json!({"output":output,"odin":args.odin}))?;
    run.status(&cmd(["cargo", "fetch", "--locked"]))?;
    let mut command = cmd([
        "cargo",
        "rustc",
        "--locked",
        "--release",
        "-p",
        "weekend-db",
        "--lib",
        "--target-dir",
    ]);
    command.push(path(build.path()));
    if !args.features.is_empty() {
        command.extend([path("--features"), path(&args.features)]);
    }
    let split = command.len();
    command.extend(cmd(["--", "-C", "panic=unwind"]));
    command.extend(strings(&plan["library_flags"])?);
    let mut query = command.clone();
    query.push(path("--print=cfg"));
    let cfg = run.capture(&query)?;
    e["panic_strategy"] =
        run.helper("compiler-cfg", &json!({"cfg":cfg}))?["panic_strategy"].clone();
    command.insert(split, path("--message-format=json-render-diagnostics"));
    let built = run.capture(&command)?;
    let mut ids = BTreeSet::new();
    for line in built.lines().filter(|line| line.starts_with('{')) {
        let v: Value = serde_json::from_str(line)?;
        if v["reason"] == "compiler-artifact" {
            ids.insert(text(&v["package_id"])?.to_owned());
        }
    }
    let mut command = cmd([
        "cargo",
        "metadata",
        "--offline",
        "--locked",
        "--format-version=1",
    ]);
    if !args.features.is_empty() {
        command.extend([path("--features"), path(&args.features)]);
    }
    let metadata: Value = serde_json::from_str(&run.capture(&command)?)?;
    let notices = attribution(
        run,
        &metadata,
        &json!(ids),
        &root.join("Cargo.lock"),
        &output.join("notices"),
        &e["toolchain"],
    )?;
    add_attribution(e, &notices)?;
    let artifacts = plan["artifacts"].as_array().context("artifacts missing")?;
    let filename = text(&artifacts[0])?;
    let library = output.join(filename);
    for artifact in artifacts {
        let name = text(artifact)?;
        fs::copy(build.path().join("release").join(name), output.join(name))?;
    }
    for name in ["LICENSE-MIT", "LICENSE-APACHE", "Cargo.lock"] {
        fs::copy(root.join(name), output.join(name))?;
    }
    fs::copy(
        root.join("include/weekend_db.h"),
        output.join("weekend_db.h"),
    )?;
    // Consumers must not be able to load anything from the ephemeral build.
    build.close()?;
    ensure!(
        !build.path().exists(),
        "build remains before consumer checks"
    );
    let symbols = run.capture(&strings(&plan["symbols"])?)?;
    fs::write(output.join("symbols.txt"), format!("{symbols}\n"))?;
    verify_exports(&fs::read_to_string(output.join("weekend_db.h"))?, &symbols)?;
    if platform == Platform::Windows {
        let dependencies = run.capture(&strings(&plan["runtime_dependencies"])?)?;
        fs::write(output.join("native-dependencies.txt"), dependencies)?;
        e["native_runtime_policy"] = json!(
            "System/MSVC runtime DLLs are listed, not bundled; consumer executables use /MD."
        );
        e["windows_toolchain"] = plan["windows_toolchain"].clone();
        e["windows_tool_sha256"] = plan["windows_tool_sha256"].clone();
        fs::write(
            output.join("WINDOWS-RUNTIME.md"),
            "This package contains the project DLL/import library and compiled C/C++ clients. It does not redistribute Microsoft runtime DLLs. Install compatible Microsoft Visual C++ runtime dependencies listed in native-dependencies.txt. Clients use /MD; static CRT builds are rejected.\n\nhttps://learn.microsoft.com/en-us/cpp/windows/deploying-visual-cpp-application-by-using-the-vcpp-redistributable-package\nhttps://learn.microsoft.com/en-us/cpp/build/reference/md-mt-ld-use-run-time-library\n",
        )?;
    }
    let consumers = plan["consumers"].as_array().context("consumers missing")?;
    let mut executables = vec![];
    for consumer in consumers {
        let name = text(&consumer["name"])?;
        e[format!("{name}_compiler")] = json!(run.capture(&strings(&consumer["version"])?)?);
        run.status(&strings(&consumer["command"])?)?;
        let exe = PathBuf::from(text(&consumer["executable"])?);
        run.status(&[path(&exe)])?;
        if platform == Platform::Windows {
            let mut c = strings(&plan["runtime_dependencies"])?;
            c.pop();
            c.push(path(&exe));
            let deps = run.capture(&c)?;
            fs::write(output.join(format!("{name}-dependencies.txt")), deps)?;
        }
        executables.push(exe);
    }
    run.leaf("tools/ffi_smoke.py", [path(&library), path(&source_id)])?;
    if args.odin {
        run.leaf("tools/odin_bindings.py", [path("--check")])?;
        copy_tree(&root.join("bindings/odin"), &output.join("bindings/odin"))?;
        copy_tree(
            &root.join("examples/odin_package"),
            &output.join("odin-consumer"),
        )?;
        let compiler = text(&plan["odin"]["compiler"])?;
        let version = run.capture(&[path(compiler), path("version")])?;
        let odin_root =
            dunce::canonicalize(PathBuf::from(run.capture(&[path(compiler), path("root")])?))?;
        e["odin"] = json!({"version":version,"compiler_sha256":sha(Path::new(compiler))?});
        let license = odin_root.join("LICENSE");
        if license.is_file() && dunce::canonicalize(&license)?.starts_with(&odin_root) {
            let notice = output.join("notices/odin/LICENSE");
            fs::create_dir_all(notice.parent().unwrap())?;
            fs::copy(license, &notice)?;
            e["odin"]["license_sha256"] = json!(sha(&notice)?);
        } else {
            e["attribution_inputs_complete"] = json!(false);
            e["attribution_review_items"]
                .as_array_mut()
                .unwrap()
                .push(json!(
                    "Missing Odin core/runtime license from compiler root/LICENSE for consumer-odin"
                ));
        }
        run.status(&strings(&plan["odin"]["command"])?)?;
        let mut scratch = Scratch::new(&std::env::temp_dir(), "package-odin")?;
        let exe = PathBuf::from(text(&plan["odin"]["executable"])?);
        run.status(&[
            path(&exe),
            path(scratch.path().join("retained.db")),
            path(&source_id),
        ])?;
        scratch.close()?;
        executables.push(exe);
        e["odin"]["verified"] = json!(true);
    }
    for dependency in plan["dependencies"]
        .as_array()
        .context("dependencies missing")?
    {
        let deps = run.capture(&strings(&dependency["command"])?)?;
        fs::write(
            output.join(format!(
                "{}-dependencies.txt",
                text(&dependency["artifact"])?
            )),
            deps,
        )?;
    }
    relocate(run, output, &executables, filename, &source_id)?;
    e["relocation_verified"] = json!(true);
    if args.source {
        let tracked = run.capture(&cmd(["git", "ls-files", "--cached", "-z"]))?;
        let profile=run.helper("rust-source",&json!({"output":output,"tracked":tracked.split('\0').filter(|n|!n.is_empty()).collect::<Vec<_>>(),"source_id":source_id,"image":if args.isolated {Some(args.image.as_str())}else{None}}))?;
        require_source_profile(&profile["info"], args.isolated)?;
        for (name, hash) in profile["info"]["source_files_sha256"]
            .as_object()
            .context("source inventory missing")?
        {
            ensure!(
                initial.get(name).map(String::as_str) == hash.as_str(),
                "delivered source differs from initial input: {name}"
            );
        }
        run.commands.extend(
            profile["commands"]
                .as_array()
                .context("source commands missing")?
                .iter()
                .cloned(),
        );
        let notices = attribution(
            run,
            &profile["metadata"],
            &profile["packages"],
            &output.join("rust-source/examples/rust_package/Cargo.lock"),
            &output.join("notices/rust-source"),
            &e["toolchain"],
        )?;
        add_attribution(e, &notices)?;
        e["rust_source"] = profile["info"].clone();
    }
    run.leaf("tools/ffi_smoke.py", [path(&library), path(&source_id)])?;
    ensure!(
        source_hashes(run, output)? == *initial
            && run.capture(&cmd(["git", "rev-parse", "HEAD"]))? == source
            && run.helper("configuration", &json!({}))? == e["effective_build_configuration"],
        "source or build configuration changed during packaging"
    );
    e["verified"] = json!(true);
    e["candidate_eligible"] =
        json!(text(&e["dirty_status"])?.is_empty() && e["attribution_inputs_complete"] == true);
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        ensure!(!ty.is_symlink(), "symlink in package input");
        if ty.is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn relocate(
    run: &mut Run,
    output: &Path,
    executables: &[PathBuf],
    filename: &str,
    source_id: &str,
) -> Result<()> {
    let mut scratch = Scratch::new(
        output.parent().context("package parent missing")?,
        "relocated",
    )?;
    let relocated = scratch.path().join("package");
    fs::rename(output, &relocated)?;
    run.log_dir = Some(relocated.clone());
    let result = (|| -> Result<()> {
        ensure!(
            !output.exists(),
            "original package remains during relocation"
        );
        for exe in executables {
            let mut c = vec![path(
                relocated.join(exe.file_name().context("consumer name missing")?),
            )];
            if exe
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("consumer-odin")
            {
                c.extend([path(scratch.path().join("relocated.db")), path(source_id)]);
            }
            run.status(&c)?;
        }
        run.leaf(
            "tools/ffi_smoke.py",
            [path(relocated.join(filename)), path(source_id)],
        )
    })();
    if let Err(error) = fs::rename(&relocated, output) {
        scratch.keep();
        bail!(
            "cannot restore relocated package; retained at {}: {error}",
            relocated.display()
        );
    }
    run.log_dir = Some(output.to_owned());
    scratch.close()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    fn options(args: Vec<OsString>) -> Result<Options> {
        let cli = crate::Cli::try_parse_from(cmd(["xtask", "package"]).into_iter().chain(args))?;
        let crate::Task::Package(options) = cli.task else {
            unreachable!()
        };
        Ok(options)
    }
    #[test]
    fn export_comparison_rejects_missing_and_extra_symbols() {
        let header = "wdb_status wdb_open(void);\nuint32_t wdb_features(void);";
        verify_exports(header, "001 T _wdb_open\n002 T _wdb_features\n").unwrap();
        verify_exports(header, "001 T wdb_open\n002 T wdb_features").unwrap();
        for symbols in [
            "wdb_open",
            "wdb_open wdb_features wdb_extra",
            "wdb_open _wdb_features_else",
        ] {
            assert!(verify_exports(header, symbols).is_err());
        }
        assert!(verify_exports("", "").is_err());
    }
    #[test]
    fn package_options_preserve_required_profiles() {
        assert!(options(cmd(["--output", "new", "--rust-source-isolated"])).is_err());
        assert!(options(cmd(["--output", "new", "--odin", "--odin"])).is_err());
        assert!(options(cmd(["--features", "crdt"])).is_err());
        assert!(
            options(cmd([
                "--output",
                "new",
                "--rust-source",
                "--rust-source-isolated",
                "--rust-source-image",
                ""
            ]))
            .is_err()
        );
        let args = options(cmd([
            "--output",
            "with spaces",
            "--features",
            "crdt,replication",
            "--rust-source",
            "--rust-source-isolated",
            "--odin",
        ]))
        .unwrap();
        assert!(args.odin && args.source && args.isolated);
        assert_eq!(args.output, Path::new("with spaces"));
    }

    #[test]
    fn requested_source_profiles_require_explicit_success() {
        for info in [
            Value::Null,
            json!({}),
            json!({"native_compile_verified":false,"linux_isolation_verified":true}),
            json!({"native_compile_verified":1}),
        ] {
            assert!(require_source_profile(&info, false).is_err());
        }
        let native = json!({"native_compile_verified":true,"linux_isolation_verified":false});
        require_source_profile(&native, false).unwrap();
        assert!(require_source_profile(&native, true).is_err());
        assert!(
            require_source_profile(
                &json!({"native_compile_verified":true,"linux_isolation_verified":"true"}),
                true
            )
            .is_err()
        );
        require_source_profile(
            &json!({"native_compile_verified":true,"linux_isolation_verified":true}),
            true,
        )
        .unwrap();
    }
}
