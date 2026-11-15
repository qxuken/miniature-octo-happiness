//! Frozen storage and wire-v1 snapshot compatibility.
use crate::{
    artifact_trace::inventory,
    assurance::{self, Campaign},
    delivery, serialization_scale,
    support::{Run, cmd, path, sha},
};
use anyhow::{Result, ensure};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

const ENDPOINTS: [&str; 2] = ["historical", "current"];
const NATIVE_IDENTITY: &str = r###"fn main() {
    println!(r#"{{"identity_kind":"engine_version_api","source_id":"{}","features":{},"abi_version":{},"format_version":{}}}"#,
        weekend_db::version::SOURCE_ID,weekend_db::version::FEATURES,
        weekend_db::version::ABI_VERSION,weekend_db::version::FORMAT_VERSION);
}
"###;
const BUILD_STAMP: &str = r###"fn main() {
    let database = weekend_db::Database::new().unwrap();
    println!(r#"{{"identity_kind":"qualification_build_stamp","source_id":"{}","features":{},"empty_revision":{}}}"#,
        env!("WEEKEND_DB_SOURCE_ID"),cfg!(feature="replication") as u32,database.snapshot().revision());
}
"###;

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum Profile {
    Storage,
    Snapshot,
}
impl Profile {
    fn name(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::Snapshot => "snapshot",
        }
    }
    fn historical(self) -> &'static str {
        match self {
            Self::Storage => "a7dc343",
            Self::Snapshot => "dcd2402",
        }
    }
    fn fixture(self) -> &'static str {
        match self {
            Self::Storage => "tools/compatibility-driver.rs",
            Self::Snapshot => "tools/compatibility-snapshot.rs",
        }
    }
}

#[derive(Args)]
pub struct Options {
    #[arg(long, value_enum, default_value = "storage")]
    profile: Profile,
    #[arg(long)]
    historical: Option<String>,
    #[arg(long, default_value = "HEAD")]
    current: String,
    #[arg(long)]
    output: PathBuf,
}

fn overlay(source: &Path, fixture: &[u8], profile: Profile) -> Result<bool> {
    let manifest = source.join("Cargo.toml");
    let original: toml::Value = toml::from_str(&fs::read_to_string(&manifest)?)?;
    ensure!(
        original["package"]["publish"].as_bool() == Some(false),
        "historical engine manifest must be private"
    );
    let native = source.join("src/version.rs").is_file();
    ensure!(
        native || profile == Profile::Snapshot,
        "storage endpoint lacks native artifact identity API"
    );
    for (name, text) in [
        ("tools/qualification-fixture.rs", fixture),
        (
            "tools/qualification-identity.rs",
            if native {
                NATIVE_IDENTITY.as_bytes()
            } else {
                BUILD_STAMP.as_bytes()
            },
        ),
    ] {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(source.join(name))?
            .write_all(text)?;
    }
    let mut manifest = OpenOptions::new().append(true).open(manifest)?;
    writeln!(
        manifest,
        "\n[[example]]\nname=\"qualification_fixture\"\npath=\"tools/qualification-fixture.rs\""
    )?;
    if profile == Profile::Snapshot {
        writeln!(manifest, "required-features=[\"replication\"]")?;
    }
    writeln!(
        manifest,
        "\n[[example]]\nname=\"qualification_identity\"\npath=\"tools/qualification-identity.rs\""
    )?;
    Ok(native)
}

fn build_plan(source: &Path, target: &Path, profile: Profile) -> Vec<std::ffi::OsString> {
    let mut command = cmd([
        "cargo",
        "build",
        "--offline",
        "--locked",
        "-p",
        "weekend-db",
        "--example",
        "qualification_fixture",
        "--example",
        "qualification_identity",
    ]);
    command.extend([
        "--manifest-path".into(),
        path(source.join("Cargo.toml")),
        "--target-dir".into(),
        path(target),
    ]);
    if profile == Profile::Snapshot {
        command.extend(cmd(["--features", "replication"]));
    }
    command
}

fn expected_identity(profile: Profile, native: bool, commit: &str) -> Value {
    let mut expected =
        json!({"source_id":commit,"features":u32::from(profile == Profile::Snapshot)});
    if native {
        expected["identity_kind"] = json!("engine_version_api");
        expected["abi_version"] = json!(1);
        expected["format_version"] = json!(2);
    } else {
        expected["identity_kind"] = json!("qualification_build_stamp");
        expected["empty_revision"] = json!(0);
    }
    expected
}

struct Endpoint {
    source: PathBuf,
    sources: BTreeMap<String, String>,
    binary: PathBuf,
    binary_sha256: String,
    identity_binary: PathBuf,
    identity_sha256: String,
}

fn build_endpoint(
    c: &mut Campaign<'_>,
    name: &str,
    revision: &str,
    profile: Profile,
    fixture: &[u8],
) -> Result<Endpoint> {
    let (commit, source, archive) = assurance::archive_revision(c, revision, Some(name))?;
    c.evidence["sources"][name] = json!(commit);
    let manifest = source.join("Cargo.toml");
    let lock = source.join("Cargo.lock");
    let manifest_hash = sha(&manifest)?;
    let lock_hash = sha(&lock)?;
    fs::copy(&lock, c.output.join(format!("{name}-Cargo.lock")))?;
    let native = overlay(&source, fixture, profile)?;
    let sources = inventory(&source, &[])?;
    let target = c.output.join("build").join(name);
    c.env
        .insert("WEEKEND_DB_SOURCE_ID".into(), commit.clone().into());
    c.command_at(
        build_plan(&source, &target, profile),
        &format!("{name}-build"),
        None,
        180.,
        &source,
    )?;
    ensure!(
        sha(&lock)? == lock_hash,
        "locked build changed original lock"
    );
    ensure!(
        inventory(&source, &[])? == sources,
        "build changed frozen source"
    );
    let examples = target.join("debug/examples");
    let binary = c.output.join(format!("{name}-consumer"));
    fs::copy(examples.join("qualification_fixture"), &binary)?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
    let binary_sha256 = sha(&binary)?;
    let identity_binary = examples.join("qualification_identity");
    let identity_sha256 = sha(&identity_binary)?;
    let result = c.command(
        vec![path(&identity_binary)],
        &format!("{name}-identity"),
        None,
        30.,
    )?;
    let identity = delivery::json(result.as_bytes())?;
    ensure!(
        identity == expected_identity(profile, native, &commit),
        "runtime artifact identity mismatch"
    );
    c.evidence["builds"][name] = json!({
        "archive_sha256":archive, "original_manifest_sha256":manifest_hash,
        "overlaid_manifest_sha256":sha(&manifest)?, "original_lock_sha256":lock_hash,
        "binary_sha256":binary_sha256,"identity_binary_sha256":identity_sha256,
        "runtime_identity":identity,"identity_fixture_sha256":sha(&source.join("tools/qualification-identity.rs"))?,
        "source_sha256":sources,"source_directory":source,
    });
    c.save()?;
    Ok(Endpoint {
        source,
        sources,
        binary,
        binary_sha256,
        identity_binary,
        identity_sha256,
    })
}

fn storage_cases(c: &mut Campaign<'_>, binaries: [&Path; 2]) -> Result<()> {
    for (writer, binary) in ENDPOINTS.into_iter().zip(binaries) {
        let name = format!("{writer}-writer");
        let directory = c.output.join(&name);
        fs::create_dir(&directory)?;
        let image = directory.join("current.wdb");
        c.command(
            cmd([binary.as_os_str(), "write".as_ref(), image.as_os_str()]),
            &format!("{name}-write"),
            None,
            30.,
        )?;
        let original = fs::read(&image)?;
        ensure!(original.len() >= 12, "writer produced an incomplete image");
        for (reader, binary) in ENDPOINTS.into_iter().zip(binaries) {
            c.command(
                cmd([binary.as_os_str(), "read".as_ref(), image.as_os_str()]),
                &format!("{name}-{reader}-read"),
                None,
                30.,
            )?;
            ensure!(
                fs::read(&image)? == original,
                "{reader} changed witnessed image"
            );
        }
        let unsupported = directory.join("unsupported.wdb");
        let mut changed = original;
        changed[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&unsupported, &changed)?;
        for (reader, binary) in ENDPOINTS.into_iter().zip(binaries) {
            c.command(
                cmd([
                    binary.as_os_str(),
                    "reject".as_ref(),
                    unsupported.as_os_str(),
                ]),
                &format!("{name}-{reader}-reject"),
                None,
                30.,
            )?;
            ensure!(
                fs::read(&unsupported)? == changed,
                "{reader} wrote unsupported image"
            );
        }
        c.evidence["results"][name] = json!({
            "current_revision":4,"retained_revision":2,"readers":ENDPOINTS,
            "read_bytes_unchanged":true,"unsupported_rejected_unchanged":true,
            "image_sha256":sha(&image)?,"unsupported_image_sha256":sha(&unsupported)?,
        });
        c.save()?;
    }
    Ok(())
}

fn require_receipts(primary: &Value, follower: &Value, export: &Path) -> Result<String> {
    ensure!(primary == follower, "sender/receiver receipts differ");
    let hash = sha(export)?;
    ensure!(
        primary["digest"].as_str() == Some(&hash),
        "receipt differs from independent export hash"
    );
    ensure!(
        primary["revision"].as_u64() == Some(4) && primary["retained_revision"].as_u64() == Some(2),
        "unexpected snapshot revisions"
    );
    Ok(hash)
}

fn snapshot_cases(c: &mut Campaign<'_>, binaries: [&Path; 2]) -> Result<()> {
    for (send, receive) in [(0, 1), (1, 0)] {
        let name = format!("{}-to-{}", ENDPOINTS[send], ENDPOINTS[receive]);
        let directory = c.output.join(&name);
        fs::create_dir(&directory)?;
        c.pair(
            [
                (
                    &format!("{name}-primary"),
                    vec![path(binaries[send]), "primary".into(), path(&directory)],
                ),
                (
                    &format!("{name}-follower"),
                    vec![path(binaries[receive]), "follower".into(), path(&directory)],
                ),
            ],
            90.,
        )?;
        let primary = delivery::read_json(&directory.join("primary.json"))?;
        let follower = delivery::read_json(&directory.join("follower.json"))?;
        let hash = require_receipts(&primary, &follower, &directory.join("export.wdb"))?;
        let mut record = primary;
        for (field, observation) in [
            ("independent_export_sha256", json!(hash)),
            ("reopened", json!(true)),
            ("duplicate_ack_equal", json!(true)),
            ("unsupported_rejected_without_file_changes", json!(true)),
            ("primary_advanced_locally_to", json!(5)),
        ] {
            record[field] = observation;
        }
        c.evidence["results"][name] = record;
        c.save()?;
    }
    Ok(())
}

pub fn run(run: &Run, opts: Options) -> Result<()> {
    // This profile supports a new output hierarchy, retaining atomic
    // create-dir admission for the final path.
    let mut c = Campaign::new(
        run,
        &opts.output,
        "two frozen checkpoints on native files and private loopback",
    )?;
    c.limits(8 * 1024 * 1024, None)?;
    c.evidence["verified"] = json!(false);
    let historical = opts
        .historical
        .as_deref()
        .unwrap_or_else(|| opts.profile.historical());
    c.evidence["requested_sources"] = json!({"historical":historical,"current":opts.current});
    c.evidence["qualification_profile"] = json!(opts.profile.name());
    c.evidence["profile"] = json!("debug");
    c.env = serialization_scale::clean_env(&c.env);
    c.save()?;
    let result = (|| -> Result<()> {
        let fixture = run.root.join(opts.profile.fixture());
        let fixture_hash = sha(&fixture)?;
        let bytes = fs::read(&fixture)?;
        c.evidence["fixture_sha256"] = json!(fixture_hash);
        c.evidence["rustc"] = json!(c.command(cmd(["rustc", "-Vv"]), "rustc", None, 30.)?);
        c.evidence["cargo"] = json!(c.command(cmd(["cargo", "-V"]), "cargo", None, 30.)?);
        let old = build_endpoint(&mut c, "historical", historical, opts.profile, &bytes)?;
        let new = build_endpoint(&mut c, "current", &opts.current, opts.profile, &bytes)?;
        c.env.remove(std::ffi::OsStr::new("WEEKEND_DB_SOURCE_ID"));
        let binaries = [old.binary.as_path(), new.binary.as_path()];
        match opts.profile {
            Profile::Storage => storage_cases(&mut c, binaries)?,
            Profile::Snapshot => snapshot_cases(&mut c, binaries)?,
        }
        for endpoint in [old, new] {
            ensure!(
                inventory(&endpoint.source, &[])? == endpoint.sources
                    && sha(&endpoint.binary)? == endpoint.binary_sha256
                    && sha(&endpoint.identity_binary)? == endpoint.identity_sha256,
                "frozen endpoint source or executable changed during qualification"
            );
        }
        ensure!(
            sha(&fixture)? == fixture_hash,
            "compatibility fixture changed"
        );
        c.evidence["artifact_sha256"] =
            json!(inventory(&c.output, &["evidence.json", "source", "build"])?);
        Ok(())
    })();
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use std::os::unix::fs::symlink;

    #[test]
    fn both_profiles_reject_existing_and_dangling_outputs_before_build() {
        let tmp = Scratch::new(&std::env::temp_dir(), "compatibility").unwrap();
        let root = tmp.path();
        let file = root.join("file");
        let directory = root.join("directory");
        let link = root.join("link");
        let bad = root.join("dangling");
        fs::write(&file, b"preserve").unwrap();
        fs::create_dir(&directory).unwrap();
        symlink(&directory, &link).unwrap();
        symlink(root.join("missing"), &bad).unwrap();
        let run = Run::new(root.to_owned());
        for profile in [Profile::Storage, Profile::Snapshot] {
            for output in [&file, &directory, &link, &bad] {
                let opts = Options {
                    profile,
                    historical: None,
                    current: "unused".into(),
                    output: output.clone(),
                };
                assert!(
                    super::run(&run, opts)
                        .unwrap_err()
                        .to_string()
                        .contains("new evidence directory")
                );
            }
        }
        assert_eq!(fs::read(file).unwrap(), b"preserve");
        assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
        assert!(!root.join("missing").exists());
    }

    #[test]
    fn storage_matrix_preserves_both_writer_directions_and_rejects_mutating_readers() {
        let tmp = Scratch::new(&std::env::temp_dir(), "compatibility").unwrap();
        let run = Run::new(tmp.path().to_owned());
        let consumer = tmp.path().join("consumer");
        for (fault, expected_failure) in [
            ("none", None),
            ("read", Some("changed witnessed image")),
            ("reject", Some("wrote unsupported image")),
        ] {
            fs::write(&consumer, format!("#!/bin/sh\ncase \"$1\" in\nwrite) dd if=/dev/zero of=\"$2\" bs=64 count=1 2>/dev/null ;;\n{fault}) printf bad >> \"$2\" ;;\nesac\n")).unwrap();
            fs::set_permissions(&consumer, fs::Permissions::from_mode(0o700)).unwrap();
            let output = tmp.path().join(fault);
            let mut campaign = Campaign::detached(&run, &output, "storage fixture").unwrap();
            campaign.evidence["verified"] = json!(false);
            campaign.evidence["requested_sources"] = json!({"current":"explicit-current"});
            let result = storage_cases(&mut campaign, [&consumer, &consumer]);
            campaign.evidence["verified"] = json!(result.is_ok());
            let outcome = campaign.finish(result);
            let proof = delivery::read_json(&output.join("evidence.json")).unwrap();
            assert_eq!(proof["requested_sources"]["current"], "explicit-current");
            let commands = proof["commands"].as_array().unwrap();
            assert!(
                commands
                    .iter()
                    .all(|row| row["exit_code"] == 0 && row["passed"] == true)
            );
            if let Some(marker) = expected_failure {
                assert!(outcome.unwrap_err().to_string().contains(marker));
                assert!(proof["error"].as_str().unwrap().contains(marker));
                assert_eq!(proof["verified"], false);
            } else {
                outcome.unwrap();
                assert_eq!(proof["verified"], true);
                assert_eq!(commands.len(), 10);
                for writer in ["historical-writer", "current-writer"] {
                    assert_eq!(proof["results"][writer]["read_bytes_unchanged"], true);
                    assert_eq!(
                        proof["results"][writer]["unsupported_rejected_unchanged"],
                        true
                    );
                }
            }
        }
    }

    #[test]
    fn locked_fixture_overlays_preserve_original_locks_and_identity_scopes() {
        let tmp = Scratch::new(&std::env::temp_dir(), "compatibility").unwrap();
        let root = tmp.path();
        for (profile, native) in [(Profile::Storage, true), (Profile::Snapshot, false)] {
            let source = root.join(profile.name());
            fs::create_dir_all(source.join("tools")).unwrap();
            fs::create_dir(source.join("src")).unwrap();
            if native {
                fs::write(source.join("src/version.rs"), b"native").unwrap();
            }
            let lock = source.join("Cargo.lock");
            fs::write(&lock, b"original-lock").unwrap();
            fs::write(source.join("Cargo.toml"), b"[package]\npublish=false\n").unwrap();
            assert_eq!(overlay(&source, b"fixture", profile).unwrap(), native);
            assert_eq!(fs::read(&lock).unwrap(), b"original-lock");
            assert_eq!(
                fs::read(source.join("tools/qualification-fixture.rs")).unwrap(),
                b"fixture"
            );
            let identity_file = source.join("tools/qualification-identity.rs");
            assert_eq!(
                fs::read_to_string(identity_file).unwrap(),
                if native { NATIVE_IDENTITY } else { BUILD_STAMP }
            );
            assert!(overlay(&source, b"do not overwrite the fixture", profile).is_err());
            assert_eq!(
                fs::read(source.join("tools/qualification-fixture.rs")).unwrap(),
                b"fixture"
            );
            let args = build_plan(&source, &root.join("target"), profile);
            assert!(args.contains(&"--offline".into()) && args.contains(&"--locked".into()));
            assert_eq!(
                args.contains(&"--features".into()),
                profile == Profile::Snapshot
            );
            assert!(!args.contains(&"generate-lockfile".into()));
        }
        assert_ne!(
            expected_identity(Profile::Snapshot, false, "old"),
            expected_identity(Profile::Snapshot, false, "new")
        );
        assert_ne!(
            expected_identity(Profile::Snapshot, false, "old"),
            expected_identity(Profile::Snapshot, true, "old")
        );
    }

    #[test]
    fn snapshot_receipts_reject_wrong_revisions_digest_or_peer() {
        let tmp = Scratch::new(&std::env::temp_dir(), "compatibility").unwrap();
        let export = tmp.path().join("export");
        fs::write(&export, b"export").unwrap();
        let expected = json!({"revision":4,"retained_revision":2,"digest":sha(&export).unwrap()});
        require_receipts(&expected, &expected, &export).unwrap();
        for (field, value) in [
            ("retained_revision", json!(4)),
            ("revision", json!(5)),
            ("digest", json!("invalid")),
        ] {
            let mut wrong = expected.clone();
            wrong[field] = value;
            assert!(require_receipts(&expected, &wrong, &export).is_err());
            assert!(require_receipts(&wrong, &wrong, &export).is_err());
        }
    }
}
