//! Native package policy and compiler discovery. These functions inspect the
//! toolchain and artifact contract without linking the database implementation.
use crate::support::{Platform, Run, Scratch, cmd, path, sha};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

pub(crate) const RUST_BOOKWORM_IMAGE: &str = "docker.io/library/rust:1.100.0-bookworm@sha256:ade39a710ef85f3506cffcfec3e8a24ba090badef1dff2fb798f71d878833fea";

fn env<'a>(values: &'a BTreeMap<OsString, OsString>, name: &str) -> Option<&'a OsStr> {
    values
        .iter()
        .find(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_os_str())
}
fn cargo_env<'a>(values: &'a BTreeMap<OsString, OsString>, name: &str) -> Option<&'a OsStr> {
    if cfg!(windows) {
        env(values, name)
    } else {
        values.get(OsStr::new(name)).map(OsString::as_os_str)
    }
}
fn utf8(value: &OsStr) -> Result<&str> {
    value
        .to_str()
        .context("toolchain paths/overrides must be UTF-8")
}
fn command_json(args: &[OsString]) -> Result<Value> {
    Ok(json!(
        args.iter()
            .map(|a| utf8(a).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?
    ))
}
pub fn configuration(root: &Path, values: &BTreeMap<OsString, OsString>) -> Result<Value> {
    let names = [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTUP_TOOLCHAIN",
        "CARGO_HOME",
        "CC",
        "CXX",
        "CFLAGS",
        "CXXFLAGS",
        "LDFLAGS",
        "CARGO_INCREMENTAL",
    ];
    let mut overrides = BTreeMap::new();
    for (name, value) in values {
        let name = utf8(name)?;
        if names.contains(&name)
            || ["CARGO_PROFILE_", "CARGO_BUILD_", "CARGO_TARGET_"]
                .iter()
                .any(|p| name.starts_with(p))
        {
            overrides.insert(
                name,
                format!("{:x}", Sha256::digest(utf8(value)?.as_bytes())),
            );
        }
    }
    let cargo = cargo_home(root, values)?;
    let mut locations: Vec<_> = root.ancestors().map(|p| p.join(".cargo")).collect();
    locations.push(cargo);
    let mut configs = BTreeMap::new();
    for directory in locations {
        let legacy = directory.join("config");
        let file = if legacy.is_file() {
            legacy
        } else {
            directory.join("config.toml")
        };
        if file.is_file() {
            configs.insert(
                dunce::canonicalize(&file)?.to_string_lossy().into_owned(),
                sha(&file)?,
            );
        }
    }
    Ok(json!({"override_sha256":overrides,"cargo_config_sha256":configs}))
}
pub fn cargo_home(root: &Path, values: &BTreeMap<OsString, OsString>) -> Result<PathBuf> {
    let home = cargo_env(values, if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
        })
        .context("home directory unavailable")?;
    let cargo = match cargo_env(values, "CARGO_HOME") {
        Some(v) => {
            let text = utf8(v)?;
            if text == "~" {
                home.clone()
            } else if let Some(relative) =
                text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\"))
            {
                home.join(relative)
            } else {
                PathBuf::from(v)
            }
        }
        None => home.join(".cargo"),
    };
    Ok(if cargo.is_absolute() {
        cargo
    } else {
        root.join(cargo)
    })
}
pub fn require_cfg(cfg: &str, platform: Platform) -> Result<()> {
    let strategies: Vec<_> = cfg
        .lines()
        .filter_map(|l| l.strip_prefix("panic=\"").and_then(|s| s.strip_suffix('"')))
        .collect();
    ensure!(
        strategies == ["unwind"],
        "delivered library must use exactly one panic=unwind strategy"
    );
    ensure!(
        platform != Platform::Windows || !cfg.lines().any(|l| l == "target_feature=\"crt-static\""),
        "Windows packages require dynamic CRT"
    );
    Ok(())
}
pub fn candidate_eligible(dirty: &str, complete: bool) -> bool {
    dirty.is_empty() && complete
}
pub fn artifacts(platform: Platform) -> Result<Vec<&'static str>> {
    Ok(match platform {
        Platform::Windows => vec!["weekend_db.dll", "weekend_db.dll.lib"],
        Platform::Mac => vec!["libweekend_db.dylib"],
        Platform::Linux => vec!["libweekend_db.so"],
        Platform::Other => bail!("native packages require Mac, Linux or Windows MSVC"),
    })
}
pub fn which(name: &OsStr, values: &BTreeMap<OsString, OsString>) -> Option<PathBuf> {
    let executable = |p: &Path| {
        if !p.is_file() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            p.metadata()
                .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            true
        }
    };
    let names = {
        let mut names = vec![PathBuf::from(name)];
        if cfg!(windows) && Path::new(name).extension().is_none() {
            for ext in env(values, "PATHEXT")
                .unwrap_or(OsStr::new(".EXE;.CMD;.BAT"))
                .to_string_lossy()
                .split(';')
            {
                let mut n = name.to_os_string();
                n.push(ext);
                names.push(PathBuf::from(n));
            }
        }
        names
    };
    if Path::new(name).components().count() > 1 {
        return names.into_iter().find(|p| executable(p));
    }
    for directory in std::env::split_paths(env(values, "PATH").unwrap_or_default()) {
        for n in &names {
            let p = directory.join(n);
            if executable(&p) {
                return Some(p);
            }
        }
    }
    None
}
fn windows_tool(name: &str, values: &BTreeMap<OsString, OsString>) -> Result<OsString> {
    Ok(path(which(OsStr::new(name), values).with_context(
        || format!("MSVC environment did not provide {name}"),
    )?))
}
pub fn windows_identity(values: &BTreeMap<OsString, OsString>) -> Result<Value> {
    let mut identity = BTreeMap::new();
    for name in [
        "vctoolsversion",
        "windowssdkversion",
        "ucrtversion",
        "vscmd_arg_tgt_arch",
    ] {
        if let Some(v) = env(values, name) {
            identity.insert(name, utf8(v)?);
        }
    }
    ensure!(
        ["vctoolsversion", "windowssdkversion"]
            .iter()
            .all(|k| identity.get(k).is_some_and(|s| !s.is_empty())),
        "MSVC must identify VCToolsVersion and WindowsSDKVersion"
    );
    Ok(json!(identity))
}
pub fn parse_environment(bytes: &[u8]) -> Result<BTreeMap<OsString, OsString>> {
    ensure!(
        bytes.len().is_multiple_of(2),
        "truncated UTF-16 compiler environment"
    );
    let words: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let text = String::from_utf16(&words)?;
    Ok(text
        .lines()
        .filter(|l| !l.starts_with('='))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (path(k), path(v)))
        .collect())
}
pub fn compiler_environment(
    run: &mut Run,
    directory: &Path,
) -> Result<BTreeMap<OsString, OsString>> {
    #[cfg(windows)]
    {
        let previous = run.private_deadline;
        let end = std::time::Instant::now() + std::time::Duration::from_secs(120);
        run.private_deadline = Some(previous.map_or(end, |p| p.min(end)));
        let result = compiler_environment_inner(run, directory);
        run.private_deadline = previous;
        result
    }
    #[cfg(not(windows))]
    {
        compiler_environment_inner(run, directory)
    }
}
fn compiler_environment_inner(
    run: &mut Run,
    directory: &Path,
) -> Result<BTreeMap<OsString, OsString>> {
    if which(OsStr::new("cl.exe"), &run.env).is_some() {
        return Ok(run.env.clone());
    }
    let installer = PathBuf::from(
        env(&run.env, "ProgramFiles(x86)").unwrap_or(OsStr::new("C:\\Program Files (x86)")),
    )
    .join("Microsoft Visual Studio/Installer/vswhere.exe");
    let installation = run.capture_private(&[
        path(installer),
        path("-utf8"),
        path("-latest"),
        path("-products"),
        path("*"),
        path("-requires"),
        path("Microsoft.VisualStudio.Component.VC.Tools.x86.x64"),
        path("-property"),
        path("installationPath"),
    ])?;
    let installation = String::from_utf8(installation)?;
    ensure!(
        !installation.trim().is_empty(),
        "Visual Studio C/C++ build tools required"
    );
    let vcvars = Path::new(installation.trim()).join("VC/Auxiliary/Build/vcvarsall.bat");
    let batch = directory.join("compiler-env.cmd");
    fs::write(
        &batch,
        format!(
            "@echo off\ncall \"{}\" x64 >nul\nif errorlevel 1 exit /b 1\nset\n",
            vcvars.display()
        ),
    )?;
    parse_environment(&run.capture_private(&[
        path("cmd.exe"),
        path("/d"),
        path("/u"),
        path("/c"),
        path(batch),
    ])?)
}
pub fn library_flags(platform: Platform) -> Vec<OsString> {
    match platform {
        Platform::Mac => cmd([
            "-C",
            "link-arg=-Wl,-install_name,@rpath/libweekend_db.dylib",
        ]),
        Platform::Linux => cmd(["-C", "link-arg=-Wl,-soname,libweekend_db.so"]),
        _ => vec![],
    }
}
fn dependency(
    platform: Platform,
    file: &Path,
    values: &BTreeMap<OsString, OsString>,
) -> Result<Vec<OsString>> {
    Ok(match platform {
        Platform::Windows => vec![
            windows_tool("dumpbin.exe", values)?,
            path("/nologo"),
            path("/dependents"),
            path(file),
        ],
        Platform::Mac => vec![path("otool"), path("-L"), path(file)],
        _ => vec![path("readelf"), path("-d"), path(file)],
    })
}
pub fn consumers(
    root: &Path,
    platform: Platform,
    output: &Path,
    library: &Path,
    values: &BTreeMap<OsString, OsString>,
) -> Result<Value> {
    let mut consumers = vec![];
    for (language, standard, name) in [("c", "c11", "consumer-c"), ("c++", "c++17", "consumer-cpp")]
    {
        let executable = output.join(format!(
            "{name}{}",
            if platform == Platform::Windows {
                ".exe"
            } else {
                ""
            }
        ));
        let (version, command) = if platform == Platform::Windows {
            let compiler = windows_tool("cl.exe", values)?;
            let mut command = vec![compiler.clone()];
            command.extend(cmd([
                "/nologo",
                "/W4",
                "/WX",
                "/MD",
                if language == "c" { "/TC" } else { "/TP" },
            ]));
            command.extend([
                path(format!("/std:{standard}")),
                path(format!("/I{}", output.display())),
                path(root.join("examples/memory.c")),
                path(format!(
                    "/Fo{}",
                    output.join(format!("{name}.obj")).display()
                )),
                path(format!("/Fe{}", executable.display())),
                path("/link"),
                path(output.join("weekend_db.dll.lib")),
            ]);
            (vec![compiler, path("/?")], command)
        } else {
            let compiler = cargo_env(values, if language == "c" { "CC" } else { "CXX" })
                .unwrap_or(OsStr::new(if language == "c" { "cc" } else { "c++" }));
            let mut command = vec![path(compiler)];
            command.extend(cmd(["-x", language]));
            command.push(path(format!("-std={standard}")));
            command.extend(cmd(["-Wall", "-Wextra", "-Werror", "-I"]));
            command.extend([path(output), path(root.join("examples/memory.c"))]);
            command.extend(cmd(["-x", "none"]));
            command.extend([
                path(library),
                path(format!(
                    "-Wl,-rpath,{}",
                    if platform == Platform::Mac {
                        "@loader_path"
                    } else {
                        "$ORIGIN"
                    }
                )),
                path("-o"),
                path(&executable),
            ]);
            (vec![path(compiler), path("--version")], command)
        };
        consumers
            .push(json!({"name":name,"version":command_json(&version)?,"command":command_json(&command)?,"executable":executable}));
    }
    Ok(json!(consumers))
}
pub fn plan(run: &Run, platform: Platform, output: &Path, odin: bool) -> Result<Value> {
    let names = artifacts(platform)?;
    let library = output.join(names[0]);
    let clients = consumers(&run.root, platform, output, &library, &run.env)?;
    let mut files = vec![library.clone()];
    for c in clients.as_array().unwrap() {
        files.push(PathBuf::from(c["executable"].as_str().unwrap()));
    }
    let symbols = match platform {
        Platform::Windows => vec![
            windows_tool("dumpbin.exe", &run.env)?,
            path("/nologo"),
            path("/exports"),
            path(&library),
        ],
        Platform::Mac => vec![path("nm"), path("-gU"), path(&library)],
        _ => vec![
            path("nm"),
            path("-D"),
            path("--defined-only"),
            path(&library),
        ],
    };
    let mut result = json!({"artifacts":names,"library_flags":command_json(&library_flags(platform))?,"symbols":command_json(&symbols)?,"consumers":clients});
    if odin {
        let compiler =
            dunce::canonicalize(which(OsStr::new("odin"), &run.env).context(
                "--odin requires an installed compiler; qualification cannot be skipped",
            )?)?;
        let executable = output.join(if platform == Platform::Windows {
            "consumer-odin.exe"
        } else {
            "consumer-odin"
        });
        let linked = if platform == Platform::Windows {
            "weekend_db.dll.lib"
        } else {
            names[0]
        };
        let mut command = vec![
            path(&compiler),
            path("build"),
            path(output.join("odin-consumer")),
            path(format!("-out:{}", executable.display())),
            path(format!(
                "-collection:weekend={}",
                output.join("bindings").display()
            )),
            path(format!("-define:WEEKEND_DB_LIBRARY=../../{linked}")),
        ];
        if platform != Platform::Windows {
            command.push(path(format!(
                "-extra-linker-flags:-Wl,-rpath,{}",
                if platform == Platform::Mac {
                    "@loader_path"
                } else {
                    "$ORIGIN"
                }
            )));
        }
        result["odin"] =
            json!({"compiler":compiler,"command":command_json(&command)?,"executable":executable});
        files.push(executable);
    }
    result["dependencies"] = json!(
        files
            .iter()
            .map(|p| Ok(
                json!({"artifact":utf8(p.file_name().context("artifact filename")?)?,"command":command_json(&dependency(platform,p,&run.env)?)?})
            ))
            .collect::<Result<Vec<Value>>>()?
    );
    result["runtime_dependencies"] = command_json(&dependency(platform, &library, &run.env)?)?;
    if platform == Platform::Windows {
        result["windows_toolchain"] = windows_identity(&run.env)?;
        let mut hashes = BTreeMap::new();
        for name in ["cl.exe", "dumpbin.exe"] {
            hashes.insert(name, sha(Path::new(&windows_tool(name, &run.env)?))?);
        }
        result["windows_tool_sha256"] = json!(hashes);
    }
    Ok(result)
}
#[derive(clap::Args)]
pub struct WindowsConsumers {
    #[arg(long)]
    library: PathBuf,
}
pub fn windows_consumers(run: &mut Run, options: WindowsConsumers) -> Result<()> {
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")),
        "native Windows consumer check requires x86_64 MSVC"
    );
    let library = dunce::canonicalize(options.library)?;
    let imports = library.with_extension("dll.lib");
    ensure!(imports.is_file(), "DLL import library required");
    let mut scratch = Scratch::new(&std::env::temp_dir(), "native-consumers")?;
    run.env = compiler_environment(run, scratch.path())?;
    fs::copy(&library, scratch.path().join(library.file_name().unwrap()))?;
    fs::copy(&imports, scratch.path().join("weekend_db.dll.lib"))?;
    // Use the same packaged-header/linker-input planner as private packaging.
    fs::copy(
        run.root.join("include/weekend_db.h"),
        scratch.path().join("weekend_db.h"),
    )?;
    let plan = consumers(
        &run.root,
        Platform::Windows,
        scratch.path(),
        &library,
        &run.env,
    )?;
    for client in plan.as_array().unwrap() {
        let args: Vec<OsString> = client["command"]
            .as_array()
            .context("consumer command")?
            .iter()
            .map(|v| v.as_str().map(path).context("command string"))
            .collect::<Result<_>>()?;
        run.status(&args)?;
        run.status(&[path(client["executable"].as_str().unwrap())])?;
    }
    scratch.close()?;
    println!("Compiled Windows C and C++ consumers passed against delivered DLL");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_crt_and_artifact_contracts() {
        for cfg in [
            "",
            "panic=\"abort\"\n",
            "panic=\"unwind\"\npanic=\"abort\"\n",
        ] {
            assert!(require_cfg(cfg, Platform::Linux).is_err());
        }
        require_cfg("panic=\"unwind\"\n", Platform::Linux).unwrap();
        assert!(
            require_cfg(
                "panic=\"unwind\"\ntarget_feature=\"crt-static\"\n",
                Platform::Windows
            )
            .is_err()
        );
        assert_eq!(
            artifacts(Platform::Windows).unwrap(),
            ["weekend_db.dll", "weekend_db.dll.lib"]
        );
        assert_eq!(artifacts(Platform::Mac).unwrap(), ["libweekend_db.dylib"]);
        assert_eq!(artifacts(Platform::Linux).unwrap(), ["libweekend_db.so"]);
        assert!(artifacts(Platform::Other).is_err());
    }
    #[test]
    fn compiler_environment_is_utf16_case_insensitive_and_redacted() {
        let data:Vec<_>="Path=compiler\r\nVCToolsVersion=14.44\r\nWindowsSDKVersion=10.0\r\nSECRET=private-value\r\n=C:=C:\\hidden\r\n".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let parsed = parse_environment(&data).unwrap();
        assert_eq!(env(&parsed, "PATH"), Some(OsStr::new("compiler")));
        assert!(parse_environment(&data[..data.len() - 1]).is_err());
        let identity = windows_identity(&parsed).unwrap();
        assert_eq!(
            identity,
            json!({"vctoolsversion":"14.44","windowssdkversion":"10.0"})
        );
        assert!(!identity.to_string().contains("private-value"));
        assert!(windows_identity(&BTreeMap::new()).is_err());
    }
    #[test]
    fn configuration_hashes_override_and_config_without_credentials() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "tool-config").unwrap();
        let root = scratch.path().join("repo");
        let cargo = scratch.path().join("cargo");
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::create_dir(&cargo).unwrap();
        let config = root.join(".cargo/config.toml");
        fs::write(&config, "[registry]\ntoken=\"private-credential\"\n").unwrap();
        fs::write(cargo.join("credentials.toml"), "other-credential").unwrap();
        let mut values = BTreeMap::from([
            (path("CARGO_HOME"), path(&cargo)),
            (path("RUSTFLAGS"), path("--cfg private_configuration")),
            (
                path("CARGO_REGISTRIES_EXAMPLE_TOKEN"),
                path("env-credential"),
            ),
        ]);
        let first = configuration(&root, &values).unwrap();
        let text = first.to_string();
        for secret in [
            "private-credential",
            "other-credential",
            "env-credential",
            "private_configuration",
        ] {
            assert!(!text.contains(secret));
        }
        assert!(
            first["cargo_config_sha256"][dunce::canonicalize(&config).unwrap().to_str().unwrap()]
                .is_string()
        );
        fs::write(&config, "[build]\nrustflags=[]\n").unwrap();
        assert_ne!(first, configuration(&root, &values).unwrap());
        let second = configuration(&root, &values).unwrap();
        values.insert(path("CARGO_PROFILE_RELEASE_PANIC"), path("unwind"));
        assert_ne!(second, configuration(&root, &values).unwrap());
        scratch.close().unwrap();
    }
    #[test]
    fn cargo_home_lookup_matches_platform_environment_case_rules() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "cargo-home-case").unwrap();
        let root = scratch.path().join("repo");
        let home = scratch.path().join("home");
        let alternate = scratch.path().join("alternate");
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(home.join(".cargo")).unwrap();
        fs::create_dir(&alternate).unwrap();
        for p in [
            home.join(".cargo/config.toml"),
            alternate.join("config.toml"),
        ] {
            fs::write(p, "[build]\nrustflags=[]\n").unwrap();
        }
        let values = BTreeMap::from([
            (path("HOME"), path(&home)),
            (path("USERPROFILE"), path(&home)),
            (path("cargo_home"), path(&alternate)),
        ]);
        let config = configuration(&root, &values).unwrap();
        let expected = if cfg!(windows) {
            alternate.join("config.toml")
        } else {
            home.join(".cargo/config.toml")
        };
        assert!(
            config["cargo_config_sha256"][dunce::canonicalize(expected).unwrap().to_str().unwrap()]
                .is_string()
        );
        scratch.close().unwrap();
    }
    #[test]
    fn exact_unix_commands_include_library_header_and_relocatable_rpath() {
        for platform in [Platform::Mac, Platform::Linux] {
            let output = Path::new("package with spaces");
            let library = output.join(artifacts(platform).unwrap()[0]);
            let plan = consumers(
                Path::new("repo"),
                platform,
                output,
                &library,
                &BTreeMap::new(),
            )
            .unwrap();
            for c in plan.as_array().unwrap() {
                let args: Vec<String> = serde_json::from_value(c["command"].clone()).unwrap();
                assert!(args.contains(&library.to_string_lossy().into_owned()));
                assert!(args.contains(&output.to_string_lossy().into_owned()));
                assert!(args.contains(&format!(
                    "-Wl,-rpath,{}",
                    if platform == Platform::Mac {
                        "@loader_path"
                    } else {
                        "$ORIGIN"
                    }
                )));
                assert_eq!(c["version"][1], "--version");
            }
        }
    }
    #[test]
    fn odin_links_only_packaged_artifact_and_compiler_is_required() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "odin-plan").unwrap();
        for name in ["odin", "odin.exe", "cl.exe", "dumpbin.exe"] {
            let p = scratch.path().join(name);
            fs::write(&p, b"compiler").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let mut run = Run::new(PathBuf::from("repo"));
        run.env = BTreeMap::from([
            (path("PATH"), path(scratch.path())),
            (path("VCToolsVersion"), path("14.44")),
            (path("WindowsSDKVersion"), path("10.0")),
        ]);
        let output = Path::new("/package with spaces");
        for platform in [Platform::Mac, Platform::Linux, Platform::Windows] {
            let p = plan(&run, platform, output, true).unwrap();
            let args: Vec<String> = serde_json::from_value(p["odin"]["command"].clone()).unwrap();
            let link = if platform == Platform::Windows {
                "weekend_db.dll.lib"
            } else {
                artifacts(platform).unwrap()[0]
            };
            assert!(args.contains(&format!("-define:WEEKEND_DB_LIBRARY=../../{link}")));
            assert!(args.contains(&format!(
                "-collection:weekend={}",
                output.join("bindings").display()
            )));
            if platform != Platform::Windows {
                assert!(
                    args.iter()
                        .any(|s| s.starts_with("-extra-linker-flags:-Wl,-rpath,"))
                );
            }
        }
        run.env.clear();
        assert!(plan(&run, Platform::Mac, output, true).is_err());
        scratch.close().unwrap();
    }
    #[test]
    fn runtime_success_and_candidate_eligibility_are_separate() {
        assert!(candidate_eligible("", true));
        assert!(!candidate_eligible(" M source", true));
        assert!(!candidate_eligible("", false));
        assert!(!candidate_eligible(" M source", false));
    }
    #[test]
    fn windows_commands_put_import_library_only_after_link() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "msvc-plan").unwrap();
        for name in ["cl.exe", "dumpbin.exe"] {
            let file = scratch.path().join(name);
            fs::write(&file, b"tool").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(file, fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let values = BTreeMap::from([(path("Path"), path(scratch.path()))]);
        let output = Path::new("package with spaces");
        let plan = consumers(
            Path::new("repo"),
            Platform::Windows,
            output,
            &output.join("weekend_db.dll"),
            &values,
        )
        .unwrap();
        for (c, language) in plan.as_array().unwrap().iter().zip(["/TC", "/TP"]) {
            let args: Vec<String> = serde_json::from_value(c["command"].clone()).unwrap();
            let link = args.iter().position(|s| s == "/link").unwrap();
            assert_eq!(
                &args[link + 1..],
                [output.join("weekend_db.dll.lib").to_string_lossy()]
            );
            assert!(args[..link].contains(&language.to_owned()));
            assert!(args[..link].contains(&"/MD".to_owned()));
            assert!(args[..link].contains(&format!("/I{}", output.display())));
        }
        assert!(
            consumers(
                Path::new("repo"),
                Platform::Windows,
                output,
                &output.join("weekend_db.dll"),
                &BTreeMap::new()
            )
            .is_err()
        );
        scratch.close().unwrap();
    }
}
