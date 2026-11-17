//! Run the existing recovery oracle from an immutable delivered source package.
#[cfg(unix)]
use crate::assurance::Campaign;
#[cfg(windows)]
use crate::windows_compatibility::Campaign;
use crate::{
    delivery,
    support::{Run, cargo_build_env, cmd, path, qualification_inventory, sha},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const TEST: &str =
    "pending_restore::restored_pending_pairs_replay_once_and_preserve_retained_roots";
const BINARY: &str = "WEEKEND_PENDING_RESTORE_BINARY";
const FIXTURES: &[(&str, &[u8])] = &[
    (
        "apps/write-coordinator/tests/writers.rs",
        include_bytes!("../../apps/write-coordinator/tests/writers.rs"),
    ),
    (
        "apps/write-coordinator/tests/writers/pending_restore.rs",
        include_bytes!("../../apps/write-coordinator/tests/writers/pending_restore.rs"),
    ),
];

#[derive(Args)]
pub struct Options {
    /// Verified private package produced with `package --rust-source`.
    #[arg(long)]
    package: PathBuf,
    /// Full source ID expected from the package's manifest and library identity.
    #[arg(long)]
    expected_source: String,
    #[arg(long)]
    output: PathBuf,
}

fn regular(file: &Path, parent: &Path) -> Result<PathBuf> {
    ensure!(file.is_absolute(), "absolute artifact path required");
    let meta = fs::symlink_metadata(file)?;
    let canonical = dunce::canonicalize(file)?;
    ensure!(
        meta.is_file()
            && !delivery::reparse(&meta)
            && !meta.is_symlink()
            && canonical.starts_with(parent),
        "artifact escaped its owned build or is not regular"
    );
    Ok(canonical)
}

fn artifacts(messages: &str, source: &Path, target: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let manifest = source.join("apps/write-coordinator/Cargo.toml");
    let mut found = BTreeMap::new();
    for line in messages.lines() {
        let row = delivery::json(line.as_bytes())?;
        if row["reason"] != "compiler-artifact" || !row["executable"].is_string() {
            continue;
        }
        ensure!(
            dunce::canonicalize(
                row["manifest_path"]
                    .as_str()
                    .context("artifact manifest missing")?
            )? == manifest,
            "unexpected executable package"
        );
        let name = row["target"]["name"]
            .as_str()
            .context("artifact name missing")?;
        let (kind, test, entry) = match name {
            "writers" => ("test", true, "tests/writers.rs"),
            "write-coordinator" => ("bin", false, "src/main.rs"),
            _ => anyhow::bail!("unexpected executable target"),
        };
        ensure!(
            row["target"]["kind"] == json!([kind])
                && row["profile"]["test"] == test
                && dunce::canonicalize(
                    row["target"]["src_path"]
                        .as_str()
                        .context("artifact source missing")?
                )? == source.join("apps/write-coordinator").join(entry),
            "executable target/source mismatch"
        );
        let file = regular(Path::new(row["executable"].as_str().unwrap()), target)?;
        ensure!(
            found.insert(name.to_owned(), file).is_none(),
            "duplicate executable target"
        );
    }
    ensure!(found.len() == 2, "missing coordinator or recovery driver");
    Ok(found)
}

fn validate(output: &str) -> Result<()> {
    let progress = format!("test {TEST} has been running for over 60 seconds");
    let mut lines = Vec::new();
    let mut running_notice = false;
    for line in output.lines().filter(|line| !line.is_empty()) {
        if line == progress {
            ensure!(
                !running_notice && (1..=5).contains(&lines.len()),
                "unexpected recovery progress notice"
            );
            running_notice = true;
        } else {
            lines.push(line);
        }
    }
    ensure!(
        lines.len() == 7 && lines[0] == "running 1 test",
        "unexpected recovery test output"
    );
    let mut index = 1;
    for prefix in [0, 1] {
        for forwarded in [0, 1] {
            ensure!(
                lines[index]
                    == format!(
                        "pending-restore prefix={prefix} forwarded={forwarded} revision={} value={} receipt_sequence={} second_reply_lost=true local_replay_stable=true retained_roots=2 originals_preserved=true",
                        if prefix == 0 { 5 } else { 6 },
                        if prefix == 0 { 42 } else { 99 },
                        prefix + 1
                    ),
                "missing, reordered or incorrect recovery schedule"
            );
            index += 1;
        }
    }
    ensure!(
        lines[5] == format!("test {TEST} ... ok"),
        "recovery test did not pass"
    );
    let summary = regex::Regex::new(
        r"^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; [0-9]+ filtered out; finished in ([0-9]+(?:\.[0-9]+)?)s$",
    )?;
    ensure!(
        summary
            .captures(lines[6])
            .is_some_and(|m| m[1].parse::<f64>().is_ok_and(|n| n.is_finite())),
        "invalid or missing recovery completion"
    );
    Ok(())
}

fn retain(file: &Path, target: &Path) -> Result<String> {
    let digest = sha(file)?;
    fs::copy(file, target)?;
    ensure!(sha(target)? == digest, "retained executable differs");
    // Only these newly owned Cargo outputs are removed. Any accidental use of
    // the compiled-in CARGO_BIN_EXE path must now fail rather than qualify.
    fs::remove_file(file)?;
    ensure!(
        fs::symlink_metadata(file).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "build-path executable survived retirement"
    );
    Ok(digest)
}

pub fn run(run: &Run, options: Options) -> Result<()> {
    // Reject overlap before Campaign creates logs, including through an alias
    // in an existing output parent. The delivered package is read-only input.
    let package = std::path::absolute(&options.package)?;
    let before = delivery::verified(&package)?;
    ensure!(
        before["source"] == options.expected_source,
        "package is not the expected source"
    );
    let package = dunce::canonicalize(package)?;
    let output = crate::notices::resolve(&run.root.join(&options.output))?;
    ensure!(
        !output.starts_with(&package) && !package.starts_with(&output),
        "package and output must be separate trees"
    );
    let mut c = Campaign::new(
        run,
        &output,
        "delivered Rust-source coordinator recovery; debug/release, four schedules each",
    )?;
    let result = (|| -> Result<()> {
        c.limits(1024 * 1024, None)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&c.output, fs::Permissions::from_mode(0o700))?;
        }
        #[cfg(windows)]
        c.restrict_directories(&[("coordinator-recovery-output", &c.output.clone())])?;
        c.env = cargo_build_env(&c.env);
        c.env.retain(|key, _| {
            !key.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("WEEKEND_PENDING_RESTORE_")
        });
        c.env.insert("CARGO_TERM_COLOR".into(), "never".into());
        ensure!(
            before["rust_source"]["native_compile_verified"] == true,
            "verified Rust source profile required"
        );
        let source = package.join("rust-source");
        for (name, expected) in FIXTURES {
            ensure!(
                fs::read(source.join(name))? == *expected,
                "delivered recovery fixture differs from compiled oracle: {name}"
            );
        }
        c.env.insert(
            "WEEKEND_DB_SOURCE_ID".into(),
            path(
                before["source"]
                    .as_str()
                    .context("package source missing")?,
            ),
        );
        c.evidence["package"] = json!({"path":package,"source":before["source"],"manifest_sha256":sha(&package.join("manifest.json"))?});
        c.evidence["rustc"] = json!(c.command(cmd(["rustc", "-Vv"]), "rustc", None, 15.)?);
        c.evidence["cargo"] = json!(c.command(cmd(["cargo", "-V"]), "cargo", None, 15.)?);
        c.evidence["profiles"] = json!({});
        let target = c.output.join("cargo-target");
        for profile in ["debug", "release"] {
            let mut build = cmd([
                "cargo",
                "test",
                "--offline",
                "--locked",
                "--no-run",
                "--message-format=json",
                "--manifest-path",
            ]);
            build.push(path(source.join("apps/write-coordinator/Cargo.toml")));
            build.extend([
                path("--target-dir"),
                path(&target),
                path("--test"),
                path("writers"),
            ]);
            if profile == "release" {
                build.push(path("--release"));
            }
            let metadata = c.command_at(build, &format!("{profile}-build"), None, 300., &source)?;
            let binaries = artifacts(&metadata, &source, &dunce::canonicalize(&target)?)?;
            let retained = c.output.join("binaries").join(profile);
            fs::create_dir_all(&retained)?;
            let cli = retained.join(format!("write-coordinator{}", std::env::consts::EXE_SUFFIX));
            let driver = retained.join(format!("writers{}", std::env::consts::EXE_SUFFIX));
            let cli_hash = retain(&binaries["write-coordinator"], &cli)?;
            let driver_hash = retain(&binaries["writers"], &driver)?;
            c.evidence["profiles"][profile] = json!({"cli":cli,"cli_sha256":cli_hash,"driver":driver,"driver_sha256":driver_hash,"build_paths_retired":true,"verified":false});
            c.save()?;
            let runtime = c.output.join(format!("runtime-{profile}"));
            fs::create_dir(&runtime)?;
            let build_env = c.env.clone();
            c.env.retain(|key, _| {
                !matches!(
                    key.to_string_lossy().to_ascii_uppercase().as_str(),
                    "TMPDIR" | "TMP" | "TEMP"
                )
            });
            for key in ["TMPDIR", "TMP", "TEMP"] {
                c.env.insert(key.into(), path(&runtime));
            }
            c.env.insert(BINARY.into(), path(&cli));
            let list = c.command_at(
                vec![
                    path(&driver),
                    path("--list"),
                    path("--format=terse"),
                    path("--exact"),
                    path(TEST),
                ],
                &format!("{profile}-list"),
                None,
                15.,
                &runtime,
            )?;
            ensure!(
                list.trim() == format!("{TEST}: test"),
                "wrong or missing recovery test inventory"
            );
            let output = c.command_at(
                vec![
                    path(&driver),
                    path("--exact"),
                    path(TEST),
                    path("--nocapture"),
                ],
                &format!("{profile}-recover"),
                None,
                180.,
                &runtime,
            )?;
            validate(&output)?;
            ensure!(
                fs::read_dir(&runtime)?.next().is_none(),
                "recovery fixture left runtime files"
            );
            ensure!(
                sha(&cli)? == cli_hash && sha(&driver)? == driver_hash,
                "executables changed during recovery"
            );
            c.evidence["profiles"][profile]["verified"] = json!(true);
            c.save()?;
            c.env = build_env;
        }
        ensure!(
            delivery::verified(&package)? == before
                && sha(&package.join("manifest.json"))? == c.evidence["package"]["manifest_sha256"],
            "delivered package changed during recovery"
        );
        c.evidence["package_unchanged"] = json!(true);
        c.evidence["artifact_sha256"] = json!(qualification_inventory(
            &c.output,
            &["evidence.json", "cargo-target"]
        )?);
        Ok(())
    })();
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn overlapping_output_is_rejected_before_modifying_package() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "coordinator-overlap").unwrap();
        let package = scratch.path().join("package");
        fs::create_dir(&package).unwrap();
        fs::write(package.join("payload"), b"immutable input").unwrap();
        for (name, bytes) in FIXTURES {
            let file = package.join("rust-source").join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        let manifest = json!({"verified":true,"source":"a".repeat(40),"source_id":"a".repeat(40),"rust_source":{"native_compile_verified":true},"sha256":qualification_inventory(&package, &[]).unwrap()});
        fs::write(package.join("manifest.json"), manifest.to_string()).unwrap();
        delivery::verified(&package).unwrap();
        let before = qualification_inventory(&package, &[]).unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let result = run(
            &Run::new(root.to_owned()),
            Options {
                package: package.clone(),
                expected_source: "a".repeat(40),
                output: package.join("recovery"),
            },
        );
        assert!(result.is_err());
        assert_eq!(qualification_inventory(&package, &[]).unwrap(), before);
        #[cfg(unix)]
        {
            let alias = scratch.path().join("package-alias");
            std::os::unix::fs::symlink(&package, &alias).unwrap();
            assert!(
                run(
                    &Run::new(root.to_owned()),
                    Options {
                        package: package.clone(),
                        expected_source: "a".repeat(40),
                        output: alias.join("recovery")
                    }
                )
                .is_err()
            );
            assert_eq!(qualification_inventory(&package, &[]).unwrap(), before);
        }
        let separate = scratch.path().join("wrong-source-output");
        assert!(
            run(
                &Run::new(root.to_owned()),
                Options {
                    package: package.clone(),
                    expected_source: "b".repeat(40),
                    output: separate.clone()
                }
            )
            .is_err()
        );
        assert!(!separate.exists());
        assert_eq!(qualification_inventory(&package, &[]).unwrap(), before);
        scratch.close().unwrap();
    }

    fn transcript() -> String {
        concat!(
            "\nrunning 1 test\n",
            "pending-restore prefix=0 forwarded=0 revision=5 value=42 receipt_sequence=1 second_reply_lost=true local_replay_stable=true retained_roots=2 originals_preserved=true\n",
            "pending-restore prefix=0 forwarded=1 revision=5 value=42 receipt_sequence=1 second_reply_lost=true local_replay_stable=true retained_roots=2 originals_preserved=true\n",
            "pending-restore prefix=1 forwarded=0 revision=6 value=99 receipt_sequence=2 second_reply_lost=true local_replay_stable=true retained_roots=2 originals_preserved=true\n",
            "pending-restore prefix=1 forwarded=1 revision=6 value=99 receipt_sequence=2 second_reply_lost=true local_replay_stable=true retained_roots=2 originals_preserved=true\n",
            "test pending_restore::restored_pending_pairs_replay_once_and_preserve_retained_roots ... ok\n\n",
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.10s\n",
        ).into()
    }

    #[test]
    fn recovery_requires_all_exact_schedules_and_actual_nonignored_completion() {
        let valid = transcript();
        validate(&valid).unwrap();
        let notice = format!("test {TEST} has been running for over 60 seconds\n");
        let completed = format!("test {TEST} ... ok");
        validate(&valid.replace(&completed, &(notice.clone() + &completed))).unwrap();
        assert!(validate(&(valid.clone() + &notice)).is_err());
        assert!(validate(&valid.replace(&completed, &(notice.repeat(2) + &completed))).is_err());
        for bad in [
            valid.replace("running 1 test", "running 0 tests"),
            valid.replace("revision=6 value=99", "revision=5 value=99"),
            valid.replace("second_reply_lost=true", "second_reply_lost=false"),
            valid.replace(" ... ok", " ... ignored"),
            valid.replace(
                "1 passed; 0 failed; 0 ignored",
                "0 passed; 0 failed; 1 ignored",
            ),
            valid.replace("0.10s", "NaNs"),
            valid.replace("0.10s", "infs"),
            valid.clone() + "extra\n",
            valid.replace(&format!("test {TEST} ... ok"), "test another ... ok"),
            "test result: ok. 0 passed; 0 failed; 0 ignored;".into(),
        ] {
            assert!(validate(&bad).is_err(), "{bad}");
        }
        assert!(
            validate(
                &valid
                    .lines()
                    .filter(|l| !l.contains("prefix=0 forwarded=1"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
            .is_err()
        );
    }

    #[test]
    fn cargo_admission_requires_both_exact_owned_executables() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "coordinator-artifacts").unwrap();
        let root = dunce::canonicalize(scratch.path()).unwrap();
        let source = root.join("source");
        let package = source.join("apps/write-coordinator");
        let target = root.join("target");
        fs::create_dir_all(package.join("src")).unwrap();
        fs::create_dir_all(package.join("tests")).unwrap();
        fs::create_dir(&target).unwrap();
        for name in ["Cargo.toml", "src/main.rs", "tests/writers.rs"] {
            fs::write(package.join(name), "fixture").unwrap();
        }
        let rows: Vec<_> = [("write-coordinator", "bin", false, "src/main.rs"), ("writers", "test", true, "tests/writers.rs")].into_iter().map(|(name, kind, test, entry)| {
            let file = target.join(name);
            fs::write(&file, "owned artifact").unwrap();
            json!({"reason":"compiler-artifact","manifest_path":package.join("Cargo.toml"),"target":{"name":name,"kind":[kind],"src_path":package.join(entry)},"profile":{"test":test},"executable":file})
        }).collect();
        let encode = |rows: &[Value]| {
            rows.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            artifacts(&encode(&rows), &source, &target).unwrap().len(),
            2
        );
        assert!(artifacts(&encode(&rows[..1]), &source, &target).is_err());
        let mut duplicate = rows.clone();
        duplicate.push(rows[0].clone());
        assert!(artifacts(&encode(&duplicate), &source, &target).is_err());
        for (field, value) in [
            ("manifest_path", json!(package.join("src/main.rs"))),
            ("executable", json!(package.join("src/main.rs"))),
            ("executable", json!("relative-artifact")),
            ("executable", json!(target)),
        ] {
            let mut bad = rows.clone();
            bad[0][field] = value;
            assert!(
                artifacts(&encode(&bad), &source, &target).is_err(),
                "{field}"
            );
        }
        for (field, value) in [
            ("name", json!("another")),
            ("kind", json!(["test"])),
            ("src_path", json!(package.join("tests/writers.rs"))),
        ] {
            let mut bad = rows.clone();
            bad[0]["target"][field] = value;
            assert!(
                artifacts(&encode(&bad), &source, &target).is_err(),
                "{field}"
            );
        }
        let mut bad = rows.clone();
        bad[0]["profile"]["test"] = json!(true);
        assert!(artifacts(&encode(&bad), &source, &target).is_err());
        scratch.close().unwrap();
    }

    #[test]
    fn retained_executable_matches_and_build_path_is_unavailable() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "coordinator-retain").unwrap();
        let original = scratch.path().join("build");
        let retained = scratch.path().join("retained");
        fs::write(&original, b"exact executable bytes").unwrap();
        let digest = sha(&original).unwrap();
        assert_eq!(retain(&original, &retained).unwrap(), digest);
        assert_eq!(sha(&retained).unwrap(), digest);
        assert!(!original.exists());
        assert!(retain(&original, &retained).is_err());
        assert_eq!(sha(&retained).unwrap(), digest);
        scratch.close().unwrap();
    }
}
