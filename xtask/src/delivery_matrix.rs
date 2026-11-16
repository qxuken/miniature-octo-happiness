//! Verify the retained minimum-Rust delivery reports without running delivered binaries.
use crate::{
    delivery::read_json_hashed,
    msrv::{EVIDENCE_EXCLUSIONS, ISOLATED_PACKAGES, check_isolated_manifest, minimum},
    source_archive,
    support::{Platform, Run, Scratch, qualification_inventory, sha, text},
    trace_model::{EXPECTED_SHA256, TRACE_SHA256},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(clap::Args)]
pub struct Options {
    /// Full Git source identity used by every lane.
    #[arg(long)]
    source: String,
    /// Directory containing the four extracted minimum-Rust evidence trees.
    #[arg(long)]
    matrix: PathBuf,
    /// Require the three isolated packages in every lane.
    #[arg(long)]
    require_isolated: bool,
}

#[derive(Clone, Copy)]
struct Lane {
    name: &'static str,
    host: &'static str,
    platform: Platform,
}

const LANES: [Lane; 4] = [
    Lane {
        name: "macos-arm64",
        host: "aarch64-apple-darwin",
        platform: Platform::Mac,
    },
    Lane {
        name: "linux-arm64",
        host: "aarch64-unknown-linux-gnu",
        platform: Platform::Linux,
    },
    Lane {
        name: "linux-x64",
        host: "x86_64-unknown-linux-gnu",
        platform: Platform::Linux,
    },
    Lane {
        name: "windows-x64",
        host: "x86_64-pc-windows-msvc",
        platform: Platform::Windows,
    },
];

type Hashes = BTreeMap<String, String>;
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const WINDOWS_CASES: [&str; 10] = [
    "full-v2",
    "legacy-v1",
    "legacy-begin",
    "whole-conflict",
    "whole-success",
    "invalid-mode",
    "unknown-version",
    "wrong-hash",
    "wrong-path",
    "wrong-source",
];

fn compare_inventory(
    files: &Hashes,
    prefix: &str,
    manifest: &str,
    recorded: &Value,
    windows_paths: bool,
) -> Result<()> {
    let recorded = recorded.as_object().context("missing recorded inventory")?;
    let mut expected = BTreeMap::new();
    for (name, digest) in recorded {
        let name = if windows_paths {
            name.replace('\\', "/")
        } else {
            name.clone()
        };
        ensure!(
            expected.insert(name, text(digest)?).is_none(),
            "duplicate recorded path"
        );
    }
    let actual: BTreeMap<_, _> = files
        .range(prefix.to_owned()..)
        .take_while(|(path, _)| path.starts_with(prefix))
        .filter_map(|(path, digest)| {
            let name = &path[prefix.len()..];
            (name != manifest).then_some((name.to_owned(), digest.as_str()))
        })
        .collect();
    ensure!(
        !expected.is_empty() && actual == expected,
        "payload inventory mismatch ({prefix})"
    );
    Ok(())
}

fn verify_workspace(controller: &Value, files: &Hashes, windows: bool) -> Result<()> {
    let commands = controller["commands"]
        .as_array()
        .context("missing workspace commands")?;
    for command in commands {
        let passed = if windows {
            command["passed"] == true
                && command["cleanup_verified"] == true
                && command["exit_code"] == 0
        } else {
            command["supervision"]["cleanup_verified"] == true
                && command["supervision"]["returncode"] == 0
        };
        ensure!(passed, "workspace command failed or cleanup missing");
    }
    for (label, release) in [("debug-1", false), ("release-1", true)] {
        let mut argv = vec![
            "cargo",
            "test",
            "--locked",
            "--offline",
            "--workspace",
            "--all-features",
        ];
        if release {
            argv.push("--release");
        }
        let key = if windows { "label" } else { "name" };
        let mut matching = commands
            .iter()
            .filter(|row| row[key] == label && row["argv"] == json!(argv));
        let command = matching
            .next()
            .with_context(|| format!("missing {label} workspace test run"))?;
        let log = format!("{label}.stdout{}", if windows { "" } else { ".log" });
        let hash = files
            .get(&log)
            .with_context(|| format!("missing {label} workspace transcript"))?;
        ensure!(
            matching.next().is_none() && (!windows || command["stdout_sha256"] == *hash),
            "missing or ambiguous {label} workspace test run"
        );
    }
    Ok(())
}

fn verify_unix_trace(
    trace: &Value,
    files: &Hashes,
    manifest: &str,
    library: &str,
    odin: &Value,
) -> Result<()> {
    let native = &trace["native"];
    let commands = trace["commands"]
        .as_array()
        .context("missing Unix trace commands")?;
    let odin_path = commands
        .iter()
        .find(|row| row["name"] == "odin-version")
        .and_then(|row| row["argv"][0].as_str())
        .context("missing Odin compiler command")?;
    ensure!(
        trace["passed"] == true
            && trace["package_manifest_sha256"] == manifest
            && trace["source_manifest_sha256"] == manifest
            && native["library_sha256"] == library
            && native["languages"] == json!(["rust", "c", "odin"])
            && native["tool_binary_sha256"][odin_path] == *odin
            && commands.iter().all(|row| {
                row["supervision"]["cleanup_verified"] == true
                    && row["supervision"]["returncode"] == 0
            })
            && files.get("trace/trace.bin").map(String::as_str) == Some(TRACE_SHA256)
            && files.get("trace/expected.txt").map(String::as_str) == Some(EXPECTED_SHA256),
        "Unix package, compiler or oracle binding mismatch"
    );
    for language in ["rust", "c", "odin"] {
        let output = format!("trace/run-consumer-{language}.stdout.log");
        ensure!(
            native["transcript_sha256"][language] == EXPECTED_SHA256
                && files.get(&output).map(String::as_str) == Some(EXPECTED_SHA256),
            "{language} transcript differs from the common oracle"
        );
    }
    Ok(())
}

fn verify_windows_trace(
    trace: &Value,
    files: &Hashes,
    manifest: &str,
    library: &str,
    odin: &Value,
) -> Result<()> {
    ensure!(
        trace["native_windows_verified"] == true
            && trace["manifest_sha256"] == manifest
            && trace["features"] == 3
            && trace["odin"]["compiler_sha256"] == *odin
            && trace["commands"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()
                    && rows
                        .iter()
                        .all(|row| row["report"]["cleanup_verified"] == true)),
        "Windows package or compiler binding mismatch"
    );
    let outcomes = trace["outcomes"]
        .as_array()
        .context("missing Windows trace outcomes")?;
    ensure!(outcomes.len() == 30, "expected 30 Windows trace outcomes");
    for language in ["c", "cpp", "odin"] {
        ensure!(
            trace[language]["dll_sha256"] == library,
            "{language} DLL binding mismatch"
        );
        for (index, case) in WINDOWS_CASES.iter().enumerate() {
            let mut rows = outcomes
                .iter()
                .filter(|row| row["language"] == language && row["case"] == *case);
            let row = rows
                .next()
                .with_context(|| format!("missing {language} {case}"))?;
            let base = format!("trace/{language}/{case}");
            let output = files.get(&format!("{base}/run.stdout")).map(String::as_str);
            ensure!(
                rows.next().is_none()
                    && row["verified"] == true
                    && output == Some(text(&row["stdout_sha256"])?),
                "unverified or duplicate {language} {case} outcome"
            );
            if index < 5 {
                ensure!(
                    row["exit"] == 0
                        && output
                            == files
                                .get(&format!("{base}/expected.txt"))
                                .map(String::as_str),
                    "{language} {case} transcript mismatch"
                );
            } else {
                ensure!(
                    row["exit"]
                        .as_u64()
                        .is_some_and(|exit| u32::try_from(exit).is_ok() && exit != 0)
                        && output == Some(EMPTY_SHA256)
                        && !files.contains_key(&format!("{base}/database.wdb"))
                        && !files.contains_key(&format!("{base}/after.json"))
                        && files.contains_key(&format!("{base}/before.json"))
                            == matches!(*case, "invalid-mode" | "unknown-version"),
                    "{language} {case} rejection mismatch"
                );
            }
        }
        let full = format!("trace/{language}/full-v2");
        ensure!(
            files.get(&format!("{full}/trace.bin")).map(String::as_str) == Some(TRACE_SHA256)
                && files.get(&format!("{full}/run.stdout")).map(String::as_str)
                    == Some(EXPECTED_SHA256),
            "{language} full trace differs from the common oracle"
        );
    }
    Ok(())
}

fn verify_isolated(
    root: &Path,
    controller: &Value,
    combined: &Value,
    files: &Hashes,
    lane: Lane,
    required: bool,
) -> Result<Option<Value>> {
    let recorded = &controller["isolated"];
    let package_names = files
        .keys()
        .filter_map(|key| key.strip_prefix("isolated/"))
        .map(|key| key.split('/').next().unwrap_or_default());
    if recorded.is_null() {
        ensure!(!required, "isolated feature package evidence required");
        ensure!(
            package_names.count() == 0,
            "unrecorded isolated feature package"
        );
        return Ok(None);
    }
    ensure!(
        recorded
            .as_object()
            .is_some_and(|rows| rows.len() == ISOLATED_PACKAGES.len())
            && package_names
                .clone()
                .all(|name| ISOLATED_PACKAGES.iter().any(|(known, _)| *known == name)),
        "unexpected isolated feature package"
    );

    let windows = lane.platform == Platform::Windows;
    let command_key = if windows { "label" } else { "name" };
    let commands = controller["commands"]
        .as_array()
        .context("missing commands")?;
    let mut summaries = BTreeMap::new();
    for (name, mask) in ISOLATED_PACKAGES {
        let prefix = format!("isolated/{name}/");
        let manifest_path = format!("{prefix}manifest.json");
        let library_path = format!("{prefix}{}", lane.platform.library());
        let (manifest, pin) = read_json_hashed(&root.join(&manifest_path))?;
        check_isolated_manifest(&manifest, combined, name, mask, true)?;
        compare_inventory(
            files,
            &prefix,
            "manifest.json",
            &manifest["sha256"],
            windows,
        )?;
        let library = files
            .get(&library_path)
            .context("missing isolated shared library")?;
        let summary = json!({"feature_mask":mask,"manifest_sha256":pin,"library_sha256":library});
        let label = format!("package-{name}");
        ensure!(
            files.get(&manifest_path) == Some(&pin)
                && recorded[name] == summary
                && commands
                    .iter()
                    .filter(|row| row[command_key] == label)
                    .count()
                    == 1,
            "isolated {name} package binding mismatch"
        );
        summaries.insert(name, summary);
    }
    Ok(Some(json!(summaries)))
}

fn verify_lane(
    root: &Path,
    source: &str,
    version: &str,
    lane: Lane,
    require_isolated: bool,
) -> Result<(Value, Value, Value)> {
    let controller_path = root.join("evidence.json");
    let (controller, controller_pin) = read_json_hashed(&controller_path)?;
    ensure!(
        controller["source"] == source
            && controller["dirty"] == false
            && controller["passed"] == true,
        "controller source or pass status mismatch"
    );
    let files = qualification_inventory(root, EVIDENCE_EXCLUSIONS)?;
    compare_inventory(
        &files,
        "",
        "evidence.json",
        &controller["artifact_sha256"],
        false,
    )?;
    let (manifest, manifest_pin) = read_json_hashed(&root.join("package/manifest.json"))?;
    let (trace, trace_report_pin) = read_json_hashed(&root.join("trace/evidence.json"))?;
    let windows = lane.platform == Platform::Windows;
    compare_inventory(
        &files,
        "package/",
        "manifest.json",
        &manifest["sha256"],
        windows,
    )?;
    compare_inventory(
        &files,
        "trace/",
        "evidence.json",
        &trace["artifact_sha256"],
        false,
    )?;

    let rustc = text(&controller["rustc"])?;
    let volume = &controller["volume"];
    let volume_matches = match lane.platform {
        Platform::Mac => volume["filesystem"] == "apfs",
        Platform::Linux => volume["fstype"] == "ext4",
        Platform::Windows => {
            volume["filesystem"] == "NTFS"
                && volume["drive_type"] == 3
                && volume["native_machine"] == 0x8664
                && volume["process_machine"] == 0
        }
        Platform::Other => false,
    };
    ensure!(
        volume_matches
            && controller["minimum_version"] == version
            && rustc
                .lines()
                .any(|line| line == format!("host: {}", lane.host))
            && rustc
                .lines()
                .any(|line| line == format!("release: {version}"))
            && text(&controller["cargo"])?.split_ascii_whitespace().nth(1) == Some(version)
            && manifest["toolchain"] == rustc,
        "native host, filesystem or minimum compiler mismatch"
    );
    verify_workspace(&controller, &files, windows)?;
    let locks = &controller["lockfiles_sha256"];
    let sources = &manifest["source_files_sha256"];
    ensure!(
        locks.as_object().is_some_and(|entries| entries
            .iter()
            .all(|(name, digest)| sources[name] == *digest)),
        "locked inputs differ from the recorded source"
    );
    let lock = text(&locks["Cargo.lock"])?;
    let packaged_sources = manifest["rust_source"]["source_files_sha256"]
        .as_object()
        .context("missing packaged Rust source inventory")?;
    ensure!(
        !packaged_sources.is_empty()
            && packaged_sources.iter().all(|(name, digest)| {
                sources[name] == *digest
                    && files
                        .get(&format!("package/rust-source/{name}"))
                        .map(String::as_str)
                        == digest.as_str()
            }),
        "packaged Rust source differs from the recorded source"
    );
    ensure!(
        files
            .get("package/rust-source/Cargo.lock")
            .map(String::as_str)
            == Some(lock)
            && manifest["lockfile_sha256"] == lock
            && manifest["source"] == source
            && manifest["source_id"] == source
            && manifest["dirty_status"] == ""
            && [
                "verified",
                "candidate_eligible",
                "relocation_verified",
                "attribution_inputs_complete"
            ]
            .iter()
            .all(|flag| manifest[flag] == true)
            && manifest["panic_strategy"] == "unwind"
            && manifest["cargo_feature_mask"] == 3
            && manifest["runtime_feature_mask"] == 3
            && manifest["odin"]["verified"] == true
            && manifest["rust_source"]["native_compile_verified"] == true,
        "package source, lock, features or delivery flags mismatch"
    );
    let get = |name: &str| {
        files
            .get(name)
            .map(String::as_str)
            .with_context(|| format!("missing {name}"))
    };
    let pin = get("package/manifest.json")?;
    let trace_pin = get("trace/evidence.json")?;
    ensure!(
        pin == manifest_pin && trace_pin == trace_report_pin,
        "report digest differs from audited inventory"
    );
    let name = lane.platform.library();
    let library = get(&format!("package/{name}"))?;
    let delivery = &controller["delivery"];
    let library_key = if windows {
        "dll_sha256"
    } else {
        "library_sha256"
    };
    ensure!(
        delivery["package_manifest_sha256"] == pin
            && delivery["trace_evidence_sha256"] == trace_pin
            && delivery[library_key] == library
            && delivery["odin_verified"] == true
            && delivery["rust_source_verified"] == true
            && delivery["trace_cells"] == 134
            && trace["verified"] == true
            && trace["engine_source_id"] == source
            && trace["trace_cells"] == 134
            && trace["trace_sha256"] == TRACE_SHA256
            && trace["expected_sha256"] == EXPECTED_SHA256,
        "controller, package and trace identities disagree"
    );
    for copy in if windows {
        &["relocated-package"][..]
    } else {
        &["relocated-package", "relocated-source"][..]
    } {
        ensure!(
            get(&format!("trace/{copy}/manifest.json"))? == pin
                && get(&format!("trace/{copy}/{name}"))? == library,
            "relocated artifact mismatch"
        );
    }
    let odin = &manifest["odin"]["compiler_sha256"];
    let odin_hash = text(odin)?;
    ensure!(
        odin_hash.len() == 64
            && odin_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "missing or malformed packaged Odin compiler hash"
    );
    if windows {
        verify_windows_trace(&trace, &files, pin, library, odin)?;
    } else {
        verify_unix_trace(&trace, &files, pin, library, odin)?;
    }
    let mut report = json!({
        "host": lane.host, "minimum_rust": version, "volume": volume,
        "controller_sha256": controller_pin, "manifest_sha256": pin,
        "trace_sha256": trace_pin, "library_sha256": library, "payloads": files.len()
    });
    if let Some(isolated) =
        verify_isolated(root, &controller, &manifest, &files, lane, require_isolated)?
    {
        report["isolated"] = isolated;
    }
    Ok((report, sources.clone(), locks.clone()))
}

fn verify(
    matrix: &Path,
    source: &str,
    version: &str,
    expected_sources: &Value,
    expected_locks: &Value,
    require_isolated: bool,
) -> Result<Value> {
    let mut profiles = BTreeMap::new();
    for lane in LANES {
        let (report, sources, locks) = verify_lane(
            &matrix.join(lane.name),
            source,
            version,
            lane,
            require_isolated,
        )
        .with_context(|| format!("{} delivery", lane.name))?;
        ensure!(
            sources == *expected_sources && locks == *expected_locks,
            "{} recorded source or locks differ from the Git archive",
            lane.name
        );
        profiles.insert(lane.name, report);
    }
    let scope = if profiles
        .values()
        .all(|report| report["isolated"].is_object())
    {
        "minimum-Rust combined and isolated private delivery"
    } else {
        "minimum-Rust combined private delivery"
    };
    Ok(json!({"source":source,"verified":true,"scope":scope,"profiles":profiles}))
}

pub fn run(run: &mut Run, options: Options) -> Result<()> {
    ensure!(
        options.source.len() == 40
            && options
                .source
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "expected a full lowercase Git commit ID"
    );
    let mut scratch = Scratch::new(&std::env::temp_dir(), "delivery-source")?;
    let (source, tree, archive) =
        source_archive::revision(scratch.path(), &options.source, None, |command, _, _| {
            run.capture(&command)
        })?;
    ensure!(source == options.source, "Git archive source mismatch");
    let files = qualification_inventory(&tree, &[])?;
    let locks: BTreeMap<_, _> = files
        .iter()
        .filter(|(name, _)| name.as_str() == "Cargo.lock" || name.ends_with("/Cargo.lock"))
        .collect();
    let mut report = verify(
        &options.matrix,
        &source,
        &minimum(&tree)?,
        &json!(files),
        &json!(locks),
        options.require_isolated,
    )?;
    report["source_archive_sha256"] = json!(archive);
    report["verifier_sha256"] = json!(sha(&std::env::current_exe()?)?);
    scratch.close()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_distinguishes_historical_and_required_isolated_profiles() {
        let lane = LANES[0];
        let root = Path::new("unused");
        let controller = json!({});
        let combined = json!({});
        let mut files = Hashes::new();
        assert!(
            verify_isolated(root, &controller, &combined, &files, lane, false)
                .unwrap()
                .is_none()
        );
        assert!(verify_isolated(root, &controller, &combined, &files, lane, true).is_err());
        files.insert("isolated/base/manifest.json".into(), "unrecorded".into());
        assert!(verify_isolated(root, &controller, &combined, &files, lane, false).is_err());
    }

    #[test]
    fn delivery_isolated_profiles_require_exact_masks_and_compiler() {
        let combined = json!({
            "source":"git","source_id":"git","toolchain":"rustc","cargo":"cargo",
            "lockfile_sha256":"lock","source_files_sha256":{"Cargo.lock":"lock"},
            "odin":{"compiler_sha256":"compiler"}
        });
        for (name, mask) in ISOLATED_PACKAGES {
            let mut manifest = combined.clone();
            let fields = manifest.as_object_mut().unwrap();
            fields.extend(
                json!({
                    "dirty_status":"","verified":true,"candidate_eligible":true,
                    "relocation_verified":true,"attribution_inputs_complete":true,
                    "panic_strategy":"unwind","features":if mask == 0 { "" } else { name },
                    "cargo_feature_mask":mask,"runtime_feature_mask":mask,
                    "requested_profiles":{"odin":true},
                    "odin":{"verified":true,"compiler_sha256":"compiler"}
                })
                .as_object()
                .unwrap()
                .clone(),
            );
            check_isolated_manifest(&manifest, &combined, name, mask, true).unwrap();
            for (key, value) in [
                ("runtime_feature_mask", json!(3)),
                ("cargo_feature_mask", json!(3)),
                ("source_id", json!("other")),
                (
                    "odin",
                    json!({"verified":true,"compiler_sha256":"substituted"}),
                ),
            ] {
                let mut invalid = manifest.clone();
                invalid[key] = value;
                assert!(check_isolated_manifest(&invalid, &combined, name, mask, true).is_err());
            }
        }
    }

    #[test]
    fn delivery_inventories_match_bytes_and_normalize_only_windows_manifests() {
        let mut files = Hashes::from([
            ("package/source/input".into(), "digest".into()),
            ("package/manifest.json".into(), "manifest".into()),
            ("trace/unrelated".into(), "other".into()),
        ]);
        let records = json!({"source\\input":"digest"});
        let verify = |files: &Hashes, record: &Value, windows| {
            compare_inventory(files, "package/", "manifest.json", record, windows)
        };
        verify(&files, &records, true).unwrap();
        assert!(verify(&files, &records, false).is_err());
        assert!(
            verify(
                &files,
                &json!({"source\\input":"digest","source/input":"digest"}),
                true
            )
            .is_err()
        );
        files.insert("package/extra".into(), "digest".into());
        assert!(verify(&files, &records, true).is_err());
        files.remove("package/extra");
        *files.get_mut("package/source/input").unwrap() = "corrupted".into();
        assert!(verify(&files, &records, true).is_err());
    }

    #[test]
    fn delivery_requires_both_clean_workspace_executions() {
        for windows in [false, true] {
            let mut rows = Vec::new();
            let mut files = Hashes::new();
            for (label, release) in [("debug-1", false), ("release-1", true)] {
                let mut argv = vec![
                    "cargo",
                    "test",
                    "--locked",
                    "--offline",
                    "--workspace",
                    "--all-features",
                ];
                if release {
                    argv.push("--release");
                }
                let filename = format!("{label}.stdout{}", if windows { "" } else { ".log" });
                files.insert(filename, "digest".into());
                rows.push(if windows {
                    json!({"label":label,"argv":argv,"exit_code":0,"passed":true,
                        "cleanup_verified":true,"stdout_sha256":"digest"})
                } else {
                    json!({"name":label,"argv":argv,"supervision":{"returncode":0,"cleanup_verified":true}})
                });
            }
            let controller = json!({"commands":rows});
            verify_workspace(&controller, &files, windows).unwrap();
            let mut missing = controller.clone();
            missing["commands"].as_array_mut().unwrap().pop();
            assert!(verify_workspace(&missing, &files, windows).is_err());
            let mut compile_only = controller.clone();
            compile_only["commands"][1]["argv"]
                .as_array_mut()
                .unwrap()
                .push(json!("--no-run"));
            assert!(verify_workspace(&compile_only, &files, windows).is_err());
            let mut unclean = controller;
            if windows {
                unclean["commands"][0]["cleanup_verified"] = json!(false);
            } else {
                unclean["commands"][0]["supervision"]["cleanup_verified"] = json!(false);
            }
            assert!(verify_workspace(&unclean, &files, windows).is_err());
        }
    }

    #[test]
    fn delivery_windows_trace_requires_each_outcome_and_the_delivered_dll() {
        let mut files = Hashes::new();
        let mut outcomes = Vec::new();
        for language in ["c", "cpp", "odin"] {
            for (index, case) in WINDOWS_CASES.iter().enumerate() {
                let base = format!("trace/{language}/{case}");
                let digest = match index {
                    0 => EXPECTED_SHA256,
                    1..5 => "sample",
                    _ => EMPTY_SHA256,
                };
                files.insert(format!("{base}/run.stdout"), digest.into());
                if index < 5 {
                    files.insert(format!("{base}/expected.txt"), digest.into());
                }
                if matches!(*case, "invalid-mode" | "unknown-version") {
                    files.insert(format!("{base}/before.json"), "attestation".into());
                }
                let exit = match (index < 5, language) {
                    (true, _) => 0_u32,
                    // Odin assertions on this Windows runtime return an NTSTATUS.
                    (false, "odin") => 0xc000_001d,
                    (false, _) => 3,
                };
                outcomes.push(json!({
                    "case":case,"language":language,"stdout_sha256":digest,
                    "verified":true,"exit":exit
                }));
            }
            files.insert(
                format!("trace/{language}/full-v2/trace.bin"),
                TRACE_SHA256.into(),
            );
        }
        let odin = json!("odin");
        let trace = json!({
            "native_windows_verified":true,"manifest_sha256":"manifest","features":3,
            "c":{"dll_sha256":"dll"},"cpp":{"dll_sha256":"dll"},
            "odin":{"dll_sha256":"dll","compiler_sha256":odin},
            "commands":[{"report":{"cleanup_verified":true}}],"outcomes":outcomes
        });
        let verify = |trace: &Value, files: &Hashes| {
            verify_windows_trace(trace, files, "manifest", "dll", &odin)
        };
        verify(&trace, &files).unwrap();
        let mut substitution = trace.clone();
        substitution["cpp"]["dll_sha256"] = json!("other");
        assert!(verify(&substitution, &files).is_err());
        let mut duplicate = trace.clone();
        duplicate["outcomes"][29] = duplicate["outcomes"][0].clone();
        assert!(verify(&duplicate, &files).is_err());
        let mut accepted = trace.clone();
        accepted["outcomes"][29]["exit"] = json!(0);
        assert!(verify(&accepted, &files).is_err());
        files.insert(
            "trace/odin/wrong-source/before.json".into(),
            "unexpected".into(),
        );
        assert!(verify(&trace, &files).is_err());
        files.remove("trace/odin/wrong-source/before.json");
        files.insert(
            "trace/odin/wrong-source/database.wdb".into(),
            "unexpected".into(),
        );
        assert!(verify(&trace, &files).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn delivery_inventory_rejects_backslash_aliases_on_unix() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "delivery-alias").unwrap();
        std::fs::write(scratch.path().join("package\\manifest.json"), b"alias").unwrap();
        assert!(qualification_inventory(scratch.path(), &[]).is_err());
        scratch.close().unwrap();
    }
}
