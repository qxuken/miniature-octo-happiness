//! Explicit delivered Rust source closure, isolated targets and dependency-path
//! verification. This checker never imports the engine's implementation.
use crate::support::{Run, Scratch, cmd, path, sha, text};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const CONSUMER: &str = "examples/rust_package/Cargo.toml";
const FILES: [&str; 3] = [
    CONSUMER,
    "examples/rust_package/Cargo.lock",
    "examples/rust_package/src/main.rs",
];
fn files(root: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut out = vec![];
    for e in fs::read_dir(root)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            out.extend(files(&e.path(), extension)?);
        } else if e.path().extension().is_some_and(|s| s == extension) {
            out.push(e.path());
        }
    }
    Ok(out)
}
pub fn copy_sources(
    root: &Path,
    destination: &Path,
    tracked: &[String],
) -> Result<BTreeMap<String, String>> {
    let root = dunce::canonicalize(root)?;
    let names: BTreeSet<_> = tracked.iter().map(String::as_str).chain(FILES).collect();
    let mut inventory = BTreeMap::new();
    for name in names {
        let relative = Path::new(name);
        ensure!(
            !relative.is_absolute()
                && !relative
                    .components()
                    .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))),
            "invalid source inventory path"
        );
        if relative.components().any(|c| {
            [".git", "target", "__pycache__"]
                .iter()
                .any(|s| c.as_os_str() == *s)
        }) {
            continue;
        }
        let source = root.join(relative);
        ensure!(
            !fs::symlink_metadata(&source)?.is_symlink()
                && dunce::canonicalize(&source)?.starts_with(&root)
                && source.is_file(),
            "source inventory requires regular in-tree files"
        );
        if source.file_name().is_some_and(|s| s == "Cargo.toml") {
            let manifest: toml::Table = toml::from_str(&fs::read_to_string(&source)?)?;
            ensure!(
                manifest
                    .get("package")
                    .is_none_or(|p| p.get("publish").and_then(toml::Value::as_bool) == Some(false)),
                "source Cargo package must declare publish=false"
            );
        }
        let target = destination.join(relative);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source, &target)?;
        inventory.insert(name.replace('\\', "/"), sha(&target)?);
    }
    Ok(inventory)
}
pub fn validate_metadata(metadata: &Value, source: &Path) -> Result<Vec<String>> {
    let source = crate::notices::resolve(source)?;
    let mut local = vec![];
    for package in metadata["packages"]
        .as_array()
        .context("source metadata packages")?
    {
        let file = PathBuf::from(text(&package["manifest_path"])?);
        let file = crate::notices::resolve(&file)?;
        if package["source"].is_null() {
            ensure!(
                file.starts_with(&source),
                "path dependency escapes delivered source"
            );
            ensure!(
                package["publish"] == json!([]),
                "private source package must have publish=false"
            );
            local.push(file.strip_prefix(&source)?.to_string_lossy().into_owned());
        } else {
            ensure!(
                text(&package["source"])?.starts_with("registry+"),
                "unqualified non-registry dependency source"
            );
        }
    }
    ensure!(!local.is_empty(), "no local source dependencies");
    Ok(local)
}
fn normalized(text: &str) -> String {
    let text = text
        .replace("\\ ", " ")
        .replace("\\:", ":")
        .replace('\\', "/");
    let unc = Regex::new(r"(?i)(^|[\s=])/{2,}\?/UNC/")
        .unwrap()
        .replace_all(&text, "${1}//")
        .into_owned();
    Regex::new(r"(^|[\s=])/{2,}\?/([A-Za-z]:/)")
        .unwrap()
        .replace_all(&unc, "$1$2")
        .into_owned()
}
fn pattern(prefix: &str) -> Regex {
    Regex::new(&format!(
        r"(^|[\s=]){}($|[/\s:])",
        regex::escape(prefix.trim_end_matches('/'))
    ))
    .unwrap()
}
pub fn references_original(text: &str, original: &Path, allowed: &[PathBuf]) -> Result<bool> {
    let mut data = normalized(text);
    let mut allowed: Vec<_> = allowed
        .iter()
        .map(|p| {
            let absolute = std::path::absolute(p)?;
            Ok(normalized(
                &dunce::canonicalize(&absolute)
                    .unwrap_or(absolute)
                    .to_string_lossy(),
            ))
        })
        .collect::<Result<_>>()?;
    allowed.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for prefix in allowed {
        data = pattern(&prefix)
            .replace_all(&data, "$1<delivered-or-registry>$2")
            .into_owned();
    }
    let absolute = std::path::absolute(original)?;
    let original = dunce::canonicalize(&absolute).unwrap_or(absolute);
    Ok(pattern(&normalized(&original.to_string_lossy())).is_match(&data))
}
fn build_commands(source: &Path, target: &Path) -> (Vec<OsString>, Vec<OsString>) {
    let manifest = source.join(CONSUMER);
    (
        [
            cmd([
                "cargo",
                "metadata",
                "--offline",
                "--locked",
                "--format-version=1",
                "--manifest-path",
            ]),
            vec![path(&manifest)],
        ]
        .concat(),
        [
            cmd([
                "cargo",
                "build",
                "--offline",
                "--locked",
                "--release",
                "--message-format=json-render-diagnostics",
                "--manifest-path",
            ]),
            vec![path(&manifest), path("--target-dir"), path(target)],
        ]
        .concat(),
    )
}
fn isolated_command(
    source: &Path,
    original: &Path,
    source_id: &str,
    image: &str,
    name: &str,
) -> Vec<OsString> {
    let script = "set -eu\n[ ! -e \"$ORIGINAL_SOURCE\" ]\nrustc -Vv\ncargo build --locked --release --manifest-path /package-source/examples/rust_package/Cargo.toml --target-dir /tmp/rust-source-build\n/tmp/rust-source-build/release/private-rust-consumer \"$WEEKEND_DB_SOURCE_ID\"\n";
    vec![
        path("docker"),
        path("run"),
        path("--rm"),
        path("--name"),
        path(name),
        path("--memory=2g"),
        path("--pids-limit=256"),
        path("--cpus=2"),
        path("--mount"),
        path(format!(
            "type=bind,source={},target=/package-source,readonly",
            source.display()
        )),
        path("--env"),
        path("CARGO_INCREMENTAL=0"),
        path("--env"),
        path(format!("WEEKEND_DB_SOURCE_ID={source_id}")),
        path("--env"),
        path(format!("ORIGINAL_SOURCE={}", original.display())),
        path(image),
        path("sh"),
        path("-c"),
        path(script),
    ]
}
fn call(run: &mut Run, args: &[OsString], cwd: &Path, timeout: f64) -> Result<String> {
    let old = std::mem::replace(&mut run.root, cwd.to_owned());
    let result = run.bounded_capture(args, timeout);
    run.root = old;
    result
}
pub struct Profile {
    pub info: Value,
    pub metadata: Value,
    pub packages: BTreeSet<String>,
}
pub fn qualify(
    run: &mut Run,
    output: &Path,
    tracked: &[String],
    source_id: &str,
    image: Option<&str>,
) -> Result<Profile> {
    let root = run.root.clone();
    let source = output.join("rust-source");
    let inventory = copy_sources(&root, &source, tracked)?;
    let mut info = json!({"source_files_sha256":inventory,"registry_dependencies_vendored":false,"native_compile_verified":false,"linux_isolation_verified":false});
    let mut scratch = Scratch::new(&std::env::temp_dir(), "rust-source")?;
    let (metadata_command, build_command) = build_commands(&source, scratch.path());
    let metadata: Value = serde_json::from_str(
        call(run, &metadata_command, &source, 300.)?
            .lines()
            .find(|l| l.starts_with('{'))
            .context("source metadata JSON")?,
    )?;
    info["local_manifests"] = json!(validate_metadata(&metadata, &source)?);
    let mut added = BTreeMap::new();
    for package in metadata["packages"].as_array().unwrap() {
        if package["source"].is_null() && package["license"] == "MIT OR Apache-2.0" {
            for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
                let target = Path::new(text(&package["manifest_path"])?)
                    .parent()
                    .unwrap()
                    .join(name);
                if !target.exists() {
                    fs::copy(source.join(name), &target)?;
                    added.insert(
                        target.strip_prefix(&source)?.to_string_lossy().into_owned(),
                        sha(&target)?,
                    );
                }
            }
        }
    }
    info["added_project_license_copies"] = json!(added);
    let built = call(run, &build_command, &source, 300.)?;
    let artifacts: Vec<Value> = built
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    let mut packages = BTreeSet::new();
    let mut executables = vec![];
    for item in artifacts
        .iter()
        .filter(|v| v["reason"] == "compiler-artifact")
    {
        packages.insert(text(&item["package_id"])?.to_owned());
        if item["target"]["name"] == "private-rust-consumer" {
            executables.push(PathBuf::from(text(&item["executable"])?));
        }
    }
    ensure!(
        executables.len() == 1,
        "missing/duplicate Rust consumer executable"
    );
    let consumer = &executables[0];
    ensure!(
        consumer.is_file()
            && dunce::canonicalize(consumer)?.starts_with(dunce::canonicalize(scratch.path())?),
        "consumer executable escapes isolated target"
    );
    let mut allowed = vec![dunce::canonicalize(&source)?];
    for package in metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| !p["source"].is_null())
    {
        allowed.push(dunce::canonicalize(
            Path::new(text(&package["manifest_path"])?)
                .parent()
                .unwrap(),
        )?);
    }
    for file in files(scratch.path(), "d")? {
        ensure!(
            !references_original(&fs::read_to_string(&file)?, &root, &allowed)?,
            "compiler dependency references original source: {}",
            file.display()
        );
    }
    call(run, &[path(consumer), path(source_id)], &source, 90.)?;
    info["native_compile_verified"] = json!(true);
    info["consumer_sha256"] = json!(sha(consumer)?);
    scratch.close()?;
    if let Some(image) = image {
        let name = format!(
            "weekend-source-{:x}-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            std::process::id()
        );
        let result = (|| -> Result<()> {
            let id = call(
                run,
                &cmd(["docker", "image", "inspect", "--format={{.Id}}", image]),
                &source,
                30.,
            )?
            .trim()
            .to_owned();
            ensure!(
                Regex::new(r"^sha256:[0-9a-f]{64}$").unwrap().is_match(&id),
                "immutable local Docker image required"
            );
            call(
                run,
                &isolated_command(&source, &root, source_id, &id, &name),
                &source,
                300.,
            )?;
            info["docker_image"] = json!(id);
            Ok(())
        })();
        let _ = call(run, &cmd(["docker", "rm", "-f", &name]), &source, 30.);
        let names = call(
            run,
            &cmd(["docker", "ps", "-a", "--format={{.Names}}"]),
            &source,
            30.,
        );
        let absent = names.as_ref().is_ok_and(|t| !t.lines().any(|s| s == name));
        info["owned_container_absent"] = json!(absent);
        result?;
        names?;
        ensure!(absent, "source isolation container remains");
        info["linux_isolation_verified"] = json!(true);
    }
    Ok(Profile {
        info,
        metadata,
        packages,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_inventory_is_explicit_private_and_in_tree() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "source-copy").unwrap();
        let root = scratch.path().join("repo");
        fs::create_dir(&root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"root\"\npublish=false\n",
        )
        .unwrap();
        fs::write(root.join("secret"), "never distribute").unwrap();
        for name in FILES {
            let file = root.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(
                &file,
                if file.file_name().unwrap() == "Cargo.toml" {
                    "[package]\nname=\"consumer\"\npublish=false\n"
                } else {
                    "fixture"
                },
            )
            .unwrap();
        }
        let destination = scratch.path().join("delivered");
        assert_eq!(
            copy_sources(&root, &destination, &["Cargo.toml".into()])
                .unwrap()
                .len(),
            4
        );
        assert!(!destination.join("secret").exists());
        assert!(copy_sources(&root, &scratch.path().join("bad"), &["../secret".into()]).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("secret"), root.join("linked")).unwrap();
            assert!(
                copy_sources(&root, &scratch.path().join("symlink"), &["linked".into()]).is_err()
            );
        }
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"root\"\n").unwrap();
        assert!(
            copy_sources(
                &root,
                &scratch.path().join("public"),
                &["Cargo.toml".into()]
            )
            .is_err()
        );
        scratch.close().unwrap();
    }
    #[test]
    fn metadata_rejects_escapes_publishable_packages_and_git() {
        let source = std::path::absolute("/delivered").unwrap();
        let local = json!({"name":"private","manifest_path":source.join("Cargo.toml"),"source":null,"publish":[]});
        assert_eq!(
            validate_metadata(&json!({"packages":[local]}), &source).unwrap(),
            ["Cargo.toml"]
        );
        for (key, value) in [
            (
                "manifest_path",
                json!(source.parent().unwrap().join("outside/Cargo.toml")),
            ),
            ("publish", Value::Null),
            ("source", json!("git+https://example.invalid/repo")),
        ] {
            let mut bad = local.clone();
            bad[key] = value;
            assert!(validate_metadata(&json!({"packages":[bad]}), &source).is_err());
        }
    }
    #[test]
    fn source_build_is_locked_and_isolation_mounts_only_delivered_source() {
        let source = Path::new("/delivered");
        let original = Path::new("/original");
        let (metadata, build) = build_commands(source, Path::new("/unique-target"));
        for command in [metadata, build] {
            assert!(command.contains(&path("--locked")));
            assert!(command.contains(&path("--offline")));
            assert!(command.contains(&path(source.join(CONSUMER))));
        }
        let command = isolated_command(source, original, "source-id", "rust:image", "private-test");
        let mounts: Vec<_> = command
            .windows(2)
            .filter(|r| r[0] == "--mount")
            .map(|r| r[1].clone())
            .collect();
        assert_eq!(
            mounts,
            [path(format!(
                "type=bind,source={},target=/package-source,readonly",
                source.display()
            ))]
        );
        assert!(command.contains(&path("CARGO_INCREMENTAL=0")));
        let script = command.last().unwrap().to_string_lossy();
        assert!(script.contains("[ ! -e \"$ORIGINAL_SOURCE\" ]"));
        assert!(script.contains("--locked"));
    }
    #[test]
    fn dependency_path_tokens_exclude_registry_and_delivered_siblings() {
        let original = std::path::absolute("/src").unwrap();
        let delivered = original.join("target/package/rust-source");
        let allowed = vec![delivered.clone()];
        for text in [
            "out: /usr/local/cargo/registry/src/crate/lib.rs",
            "out: /src-other/lib.rs",
        ] {
            assert!(!references_original(text, &original, &[]).unwrap());
        }
        for text in [
            format!("out: {}/lib.rs", original.display()),
            format!("# env-dep:ROOT={}", original.display()),
            format!(
                "out: {}/target/package/rust-source-escape/lib.rs",
                original.display()
            ),
        ] {
            assert!(
                references_original(&text, &original, &allowed).unwrap(),
                "{text}"
            );
        }
        assert!(
            !references_original(
                &format!("out: {}/src/lib.rs", delivered.display()),
                &original,
                &allowed
            )
            .unwrap()
        );
        assert!(
            !references_original(
                &format!("# env-dep:ROOT={}", delivered.display()),
                &original,
                &allowed
            )
            .unwrap()
        );
    }
}
