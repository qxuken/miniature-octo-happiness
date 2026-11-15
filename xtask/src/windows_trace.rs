//! Windows delivered-artifact policy and finite cases, independent of the engine.
#[path = "windows_faults.rs"]
mod faults;

#[cfg(windows)]
use crate::trace_process::{call, write_proof};
use crate::{
    delivery,
    support::{path, sha, text},
    trace_model::{self, Trace},
    trace_process::admission,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs,
    path::Path,
};

pub fn relative_name(name: &str) -> Result<String> {
    ensure!(
        !name.is_empty() && !name.contains('\0'),
        "invalid Windows payload name"
    );
    let name = name.replace('\\', "/");
    for part in name.split('/') {
        ensure!(
            !part.is_empty()
                && !matches!(part, "." | "..")
                && !part.contains(':')
                && !part.ends_with([' ', '.']),
            "absolute/escaping/ambiguous Windows payload"
        );
        let device = part.split('.').next().unwrap().to_ascii_uppercase();
        ensure!(
            !matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(device.len() == 4
                    && (device.starts_with("COM") || device.starts_with("LPT"))
                    && matches!(device.as_bytes()[3], b'1'..=b'9')),
            "Windows device payload"
        );
    }
    Ok(name)
}
fn hash(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn verified(package: &Path, pin: &str) -> Result<(Value, u32)> {
    let root = fs::symlink_metadata(package)?;
    ensure!(
        root.is_dir() && !root.is_symlink(),
        "package directory alias"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            root.file_attributes() & 0x400 == 0,
            "package reparse directory"
        );
    }
    // Enforce the regular-file/JSON size bound before hashing its bytes.
    let info = delivery::read_json(&package.join("manifest.json"))?;
    ensure!(
        hash(pin, 64) && sha(&package.join("manifest.json"))? == pin,
        "pinned manifest SHA-256 mismatch"
    );
    ensure!(
        info["verified"] == true
            && info["candidate_eligible"] == true
            && info["dirty_status"] == "",
        "clean verified package required"
    );
    let source = text(&info["source"])?;
    ensure!(
        hash(source, 40) && info["source_id"] == source,
        "exact package source identity required"
    );
    ensure!(
        text(&info["toolchain"])?
            .lines()
            .any(|s| s == "host: x86_64-pc-windows-msvc"),
        "native x64 MSVC package required"
    );
    let mut expected = BTreeMap::new();
    let mut folded = BTreeSet::new();
    for (name, digest) in info["sha256"].as_object().context("package hashes")? {
        let name = relative_name(name)?;
        ensure!(
            name.to_lowercase() != "manifest.json"
                && folded.insert(name.to_uppercase())
                && hash(text(digest)?, 64),
            "duplicate/self-hashed payload or invalid digest"
        );
        expected.insert(name, text(digest)?.to_owned());
    }
    ensure!(!expected.is_empty(), "empty package inventory");
    let mut actual = delivery::inventory(package)?;
    let mut aliases = BTreeSet::new();
    for name in actual.keys() {
        relative_name(name)?;
        ensure!(
            aliases.insert(name.to_uppercase()),
            "case-aliased package inventory"
        );
    }
    ensure!(
        actual.remove("manifest.json").is_some() && actual == expected,
        "package payload/inventory mismatch"
    );
    for required in ["weekend_db.dll", "weekend_db.dll.lib", "weekend_db.h"] {
        ensure!(
            expected.contains_key(required),
            "missing DLL/import library/header"
        );
    }
    let names = text(&info["features"])?;
    let mut features = BTreeSet::new();
    if !names.is_empty() {
        for name in names.split(',') {
            ensure!(
                matches!(name, "crdt" | "replication") && features.insert(name),
                "unsupported/duplicate package feature"
            );
        }
    }
    let bits =
        u32::from(features.contains("replication")) | (u32::from(features.contains("crdt")) * 2);
    Ok((info, bits))
}
fn canonical_windows(path: &str) -> Result<String> {
    ensure!(!path.contains('\0'), "NUL loaded DLL path");
    let mut path = path.replace('/', "\\");
    if path.starts_with("\\\\?\\UNC\\") {
        path = format!("\\\\{}", &path[8..]);
    } else if path.starts_with("\\\\?\\") {
        path = path[4..].into();
    }
    let (prefix, rest, min) = if let Some(rest) = path.strip_prefix("\\\\") {
        ("\\\\".to_owned(), rest, 2)
    } else {
        let b = path.as_bytes();
        ensure!(
            b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\',
            "absolute loaded DLL path required"
        );
        (path[..3].to_owned(), &path[3..], 0)
    };
    let mut parts = vec![];
    for part in rest.split('\\') {
        match part {
            "" | "." => (),
            ".." => {
                if parts.len() > min {
                    parts.pop();
                }
            }
            _ => parts.push(part),
        }
    }
    ensure!(min == 0 || parts.len() >= 2, "UNC server/share required");
    Ok(format!("{prefix}{}", parts.join("\\")).to_lowercase())
}
pub fn attestation(
    record: &Value,
    phase: &str,
    dll: &str,
    digest: &str,
    source: &str,
    features: u32,
    pid: u32,
) -> Result<()> {
    let expected = json!({"schema":1,"phase":phase,"pid":pid,"loaded_dll":dll,"sha256":digest,"source_id":source,"abi":1,"format":2,"features":features});
    let actual = record.as_object().context("loader proof object")?;
    ensure!(
        actual.keys().eq(expected.as_object().unwrap().keys()),
        "loader proof schema mismatch"
    );
    for (key, value) in expected.as_object().unwrap() {
        if key == "loaded_dll" {
            ensure!(
                canonical_windows(text(&actual[key])?)? == canonical_windows(text(value)?)?,
                "wrong DLL actually loaded"
            );
        } else {
            ensure!(
                &actual[key] == value,
                "loader proof type/value mismatch: {key}"
            );
        }
    }
    Ok(())
}
pub struct Case {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub expected: Option<Vec<u8>>,
}
fn case(name: &'static str, trace: Trace, expected: Option<&str>) -> Result<Case> {
    let bytes = trace.encode()?;
    let oracle = Trace::decode(&bytes)?.expected()?.into_bytes();
    if let Some(expected) = expected {
        ensure!(
            oracle == expected.as_bytes(),
            "hand-computed trace control changed"
        );
    }
    Ok(Case {
        name,
        bytes,
        expected: Some(oracle),
    })
}
pub fn cases() -> Result<Vec<Case>> {
    let begin = vec![
        [1, 0, 0, 1],
        [1, 1, 0, 0],
        [5, 1, 0, 0],
        [5, 0, 0, 0],
        [7, 0, 0, 0],
        [7, 1, 0, 0],
    ];
    let conflict = vec![
        [1, 0, 0, 1],
        [1, 1, 0, 0],
        [5, 1, 0, 0],
        [5, 0, 0, 0],
        [3, 0, 0, 0],
        [5, 0, 0, 0],
        [7, 0, 0, 0],
        [7, 1, 0, 0],
    ];
    let mut invalid = Trace {
        version: 1,
        cells: vec![[1, 0, 0, 2]],
    }
    .encode()?;
    invalid[..8].copy_from_slice(trace_model::MAGIC_V2);
    let mut unknown = b"WDBTRC3\n".to_vec();
    unknown.extend(0u32.to_le_bytes());
    Ok(vec![
        case("full-v2", trace_model::fixed(2)?, None)?,
        case("legacy-v1", trace_model::fixed(1)?, None)?,
        case(
            "legacy-begin",
            Trace {
                version: 1,
                cells: begin,
            },
            Some(
                "0 1 0 1 [] H=- R=- X=-\n1 1 0 1 [] H=- R=- X=-\n2 5 0 2 [] H=- R=- X=2\n3 5 0 3 [] H=- R=- X=3\n4 7 0 3 [] H=- R=- X=-\n5 7 0 3 [] H=- R=- X=-\n",
            ),
        )?,
        case(
            "whole-conflict",
            Trace {
                version: 2,
                cells: conflict,
            },
            Some(
                "0 1 0 1 [] H=- R=- X=-\n1 1 0 1 [] H=- R=- X=-\n2 5 0 2 [] H=- R=- X=2\n3 5 10 2 [] H=- R=- X=-\n4 3 11 2 [] H=- R=- X=-\n5 5 11 2 [] H=- R=- X=-\n6 7 0 2 [] H=- R=- X=-\n7 7 0 2 [] H=- R=- X=-\n",
            ),
        )?,
        case(
            "whole-success",
            Trace {
                version: 2,
                cells: vec![[1, 0, 0, 1], [5, 0, 0, 0], [7, 0, 0, 0]],
            },
            Some("0 1 0 1 [] H=- R=- X=-\n1 5 0 2 [] H=- R=- X=2\n2 7 0 2 [] H=- R=- X=-\n"),
        )?,
        Case {
            name: "invalid-mode",
            bytes: invalid,
            expected: None,
        },
        Case {
            name: "unknown-version",
            bytes: unknown,
            expected: None,
        },
    ])
}
pub fn compiler_command(
    compiler: &OsStr,
    cpp: bool,
    inputs: &Path,
    package: &Path,
    executable: &Path,
) -> Vec<OsString> {
    vec![
        path(compiler),
        "/nologo".into(),
        "/W4".into(),
        "/WX".into(),
        "/O2".into(),
        "/UNDEBUG".into(),
        "/MD".into(),
        "/utf-8".into(),
        "/showIncludes".into(),
        if cpp { "/TP" } else { "/TC" }.into(),
        if cpp { "/std:c++17" } else { "/std:c11" }.into(),
        format!("/I{}", package.display()).into(),
        path(inputs.join("windows_consumer.c")),
        format!("/Fo{}", executable.with_extension("obj").display()).into(),
        format!("/Fe{}", executable.display()).into(),
        "/link".into(),
        "/INCREMENTAL:NO".into(),
        path(package.join("weekend_db.dll.lib")),
        "bcrypt.lib".into(),
    ]
}
pub fn clean_env(env: &BTreeMap<OsString, OsString>) -> BTreeMap<OsString, OsString> {
    env.iter()
        .filter(|(k, _)| {
            !matches!(
                k.to_string_lossy().to_ascii_uppercase().as_str(),
                "CL" | "_CL_" | "LINK" | "_LINK_" | "PYTHONPATH" | "PYTHONHOME" | "PYTHONOPTIMIZE"
            )
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use sha2::{Digest, Sha256};
    fn package(root: &Path) -> Value {
        for (name, bytes) in [
            ("weekend_db.dll", "dll"),
            ("weekend_db.dll.lib", "imports"),
            ("weekend_db.h", "header"),
        ] {
            fs::write(root.join(name), bytes).unwrap();
        }
        let v = json!({"verified":true,"candidate_eligible":true,"dirty_status":"","source":"a".repeat(40),"source_id":"a".repeat(40),"toolchain":"host: x86_64-pc-windows-msvc","features":"crdt,replication","sha256":delivery::inventory(root).unwrap()});
        write(root, &v);
        v
    }
    fn write(root: &Path, v: &Value) -> String {
        fs::write(root.join("manifest.json"), serde_json::to_vec(v).unwrap()).unwrap();
        sha(&root.join("manifest.json")).unwrap()
    }
    #[test]
    fn qualified_oracle_bytes_and_hand_computed_cases_are_preserved() {
        let cases = cases().unwrap();
        assert_eq!(cases.len(), 7);
        assert_eq!(
            format!("{:x}", Sha256::digest(&cases[0].bytes)),
            trace_model::TRACE_SHA256
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(cases[0].expected.as_ref().unwrap())),
            trace_model::EXPECTED_SHA256
        );
        for c in cases {
            if let Some(expected) = c.expected {
                assert_eq!(
                    Trace::decode(&c.bytes)
                        .unwrap()
                        .expected()
                        .unwrap()
                        .as_bytes(),
                    expected,
                    "{}",
                    c.name
                );
            } else {
                assert!(Trace::decode(&c.bytes).is_err());
            }
        }
    }
    #[test]
    fn optimized_c_and_cpp_link_only_the_delivered_import_library() {
        for (cpp, language, standard) in [(false, "/TC", "/std:c11"), (true, "/TP", "/std:c++17")] {
            let command = compiler_command(
                OsStr::new("cl.exe"),
                cpp,
                Path::new("inputs"),
                Path::new("package"),
                Path::new("out/trace.exe"),
            );
            for flag in ["/O2", "/UNDEBUG", "/W4", "/WX", "/MD", language, standard] {
                assert!(command.contains(&flag.into()));
            }
            let split = command.iter().position(|s| s == "/link").unwrap();
            let library = path(Path::new("package").join("weekend_db.dll.lib"));
            assert!(command[split + 1..].contains(&library));
            assert!(!command[..split].contains(&library));
            assert!(command[split + 1..].contains(&"bcrypt.lib".into()));
        }
    }
    #[test]
    fn manifest_pin_payload_and_complete_inventory_are_required() {
        let t = Scratch::new(&std::env::temp_dir(), "win-trace-pin").unwrap();
        package(t.path());
        let pin = sha(&t.path().join("manifest.json")).unwrap();
        assert_eq!(verified(t.path(), &pin).unwrap().1, 3);
        assert!(verified(t.path(), &"0".repeat(64)).is_err());
        fs::write(t.path().join("weekend_db.dll"), "wrong").unwrap();
        assert!(verified(t.path(), &pin).is_err());
        fs::write(t.path().join("weekend_db.dll"), "dll").unwrap();
        fs::write(t.path().join("extra"), "unknown").unwrap();
        assert!(verified(t.path(), &pin).is_err());
    }
    #[test]
    fn dirty_wrong_target_unknown_and_duplicate_features_reject() {
        let t = Scratch::new(&std::env::temp_dir(), "win-trace-profile").unwrap();
        let original = package(t.path());
        for (key, value) in [
            ("verified", json!(false)),
            ("candidate_eligible", json!(false)),
            ("dirty_status", json!("M file")),
            ("source_id", json!(format!("{}-dirty", "a".repeat(40)))),
            ("toolchain", json!("host: x86_64-apple-darwin")),
            ("features", json!("crdt,unknown")),
            ("features", json!("crdt,crdt")),
        ] {
            let mut v = original.clone();
            v[key] = value;
            let pin = write(t.path(), &v);
            assert!(verified(t.path(), &pin).is_err(), "{key}");
        }
    }
    #[test]
    fn windows_path_escapes_devices_and_aliases_reject() {
        assert_eq!(
            relative_name("folder\\payload.bin").unwrap(),
            "folder/payload.bin"
        );
        for path in [
            r"C:\payload",
            r"\payload",
            r"\\host\share\payload",
            "../payload",
            "a/../payload",
            "a//payload",
            "payload:stream",
            "CON",
            "aux.txt",
            "folder/NUL",
            "trailing.",
            "trailing ",
            ".",
        ] {
            assert!(relative_name(path).is_err(), "{path}");
        }
        let t = Scratch::new(&std::env::temp_dir(), "win-trace-alias").unwrap();
        let mut v = package(t.path());
        v["sha256"]["WEEKEND_DB.DLL"] = v["sha256"]["weekend_db.dll"].clone();
        let pin = write(t.path(), &v);
        assert!(verified(t.path(), &pin).is_err());
    }
    #[test]
    fn backslash_payload_keys_are_accepted_and_package_aliases_reject() {
        let t = Scratch::new(&std::env::temp_dir(), "win-trace-slash").unwrap();
        let mut v = package(t.path());
        fs::create_dir(t.path().join("folder")).unwrap();
        fs::write(t.path().join("folder/payload"), "extra").unwrap();
        v["sha256"]["folder\\payload"] = json!(sha(&t.path().join("folder/payload")).unwrap());
        let pin = write(t.path(), &v);
        verified(t.path(), &pin).unwrap();
        #[cfg(unix)]
        {
            let alias = t.path().join("alias");
            std::os::unix::fs::symlink("weekend_db.dll", alias).unwrap();
            assert!(verified(t.path(), &pin).is_err());
        }
        #[cfg(windows)]
        {
            let alias = t.path().join("alias");
            let folder = t.path().join("folder");
            let mut context = crate::support::Run::new(t.path().to_owned());
            context
                .capture_private(&crate::support::cmd([
                    OsStr::new("cmd.exe"),
                    OsStr::new("/D"),
                    OsStr::new("/Q"),
                    OsStr::new("/C"),
                    OsStr::new("mklink"),
                    OsStr::new("/J"),
                    alias.as_os_str(),
                    folder.as_os_str(),
                ]))
                .unwrap();
            // An explicitly hashed in-tree junction must reject too.
            v["sha256"]["alias/payload"] = v["sha256"]["folder\\payload"].clone();
            let pin = write(t.path(), &v);
            let result = verified(t.path(), &pin);
            fs::remove_dir(alias).unwrap();
            assert!(result.is_err());
        }
    }
    fn proof() -> Value {
        json!({"schema":1,"phase":"before","pid":123,"loaded_dll":r"C:\proof\weekend_db.dll","sha256":"b".repeat(64),"source_id":"a".repeat(40),"abi":1,"format":2,"features":3})
    }
    fn check(v: &Value) -> Result<()> {
        attestation(
            v,
            "before",
            r"C:\proof\weekend_db.dll",
            &"b".repeat(64),
            &"a".repeat(40),
            3,
            123,
        )
    }
    #[test]
    fn actual_loaded_path_hash_identity_features_pid_and_types_are_required() {
        let row = proof();
        check(&row).unwrap();
        let mut v = row.clone();
        v["loaded_dll"] = json!(r"c:\PROOF\weekend_db.dll");
        check(&v).unwrap();
        v["loaded_dll"] = json!(r"\\?\C:\proof\weekend_db.dll");
        check(&v).unwrap();
        for (key, value) in [
            ("loaded_dll", json!(r"C:\other\weekend_db.dll")),
            ("loaded_dll", json!("weekend_db.dll")),
            ("sha256", json!("c".repeat(64))),
            ("source_id", json!("c".repeat(40))),
            ("features", json!(0)),
            ("features", json!(true)),
            ("pid", json!(124)),
            ("schema", json!(true)),
            ("phase", json!("after")),
            ("abi", json!(2)),
            ("format", json!(3)),
        ] {
            let mut v = row.clone();
            v[key] = value;
            assert!(check(&v).is_err(), "{key}");
        }
        let mut v = row.clone();
        v["extra"] = json!("untrusted");
        assert!(check(&v).is_err());
        let mut v = row;
        v.as_object_mut().unwrap().remove("sha256");
        assert!(check(&v).is_err());
    }
    #[test]
    fn duplicate_and_oversized_loader_json_reject() {
        let t = Scratch::new(&std::env::temp_dir(), "win-trace-json").unwrap();
        let p = t.path().join("proof.json");
        fs::write(&p, b"{\"schema\":1,\"schema\":1}").unwrap();
        assert!(delivery::read_json(&p).is_err());
        fs::write(&p, vec![b' '; 2 * 1024 * 1024 + 1]).unwrap();
        assert!(delivery::read_json(&p).is_err());
    }
    #[test]
    fn existing_output_rejects_before_inputs_or_processes() {
        let t = Scratch::new(&std::env::temp_dir(), "win-trace-admission").unwrap();
        let output = t.path().join("existing");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep"), "preserve").unwrap();
        let mut context = crate::support::Run::new(t.path().to_owned());
        context.env.insert("PATH".into(), "/no-programs".into());
        let result = run(
            &mut context,
            Options {
                package: t.path().join("missing"),
                manifest_sha256: "a".repeat(64),
                output: output.clone(),
            },
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("output already exists")
        );
        assert!(context.commands.is_empty());
        assert_eq!(fs::read_to_string(output.join("keep")).unwrap(), "preserve");
        assert_eq!(fs::read_dir(output).unwrap().count(), 1);
    }
    #[test]
    fn compiler_environment_overrides_are_removed_case_insensitively() {
        let env = [
            ("CL", "/DNDEBUG"),
            ("_cl_", "/DNDEBUG"),
            ("LINK", "inject"),
            ("_LINK_", "inject"),
            ("PYTHONPATH", "inject"),
            ("PATH", "tools"),
            ("INCLUDE", "sdk"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        assert_eq!(
            clean_env(&env),
            [
                ("PATH".into(), "tools".into()),
                ("INCLUDE".into(), "sdk".into())
            ]
            .into()
        );
    }
}

#[derive(clap::Args)]
pub struct Options {
    #[arg(long)]
    pub package: std::path::PathBuf,
    #[arg(long)]
    pub manifest_sha256: String,
    #[arg(long)]
    pub output: std::path::PathBuf,
}
pub fn run(run: &mut crate::support::Run, opts: Options) -> Result<()> {
    qualify(run, opts, Campaign::State)
}

#[cfg(windows)]
pub fn run_faults(run: &mut crate::support::Run, opts: Options) -> Result<()> {
    qualify(run, opts, Campaign::InjectedFaults)
}

#[cfg(windows)]
pub fn run_disk_full(run: &mut crate::support::Run, opts: Options) -> Result<()> {
    qualify(run, opts, Campaign::DiskFull)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Campaign {
    State,
    #[cfg(windows)]
    InjectedFaults,
    #[cfg(windows)]
    DiskFull,
}

fn qualify(run: &mut crate::support::Run, opts: Options, campaign: Campaign) -> Result<()> {
    let output = admission(&run.root, &opts.output)?;
    let requested = if opts.package.is_absolute() {
        opts.package.clone()
    } else {
        run.root.join(&opts.package)
    };
    let (info, bits) = verified(&requested, &opts.manifest_sha256)?;
    ensure!(
        campaign == Campaign::State || bits == 3,
        "combined native package required"
    );
    let package = dunce::canonicalize(&requested)?;
    ensure!(
        !output.starts_with(&package) && !package.starts_with(&output),
        "output overlaps package"
    );
    #[cfg(not(windows))]
    {
        let _ = (info, bits);
        anyhow::bail!("run on native x64 Windows with MSVC");
    }
    #[cfg(windows)]
    {
        native(
            run,
            &package,
            &output,
            &opts.manifest_sha256,
            info,
            bits,
            campaign,
        )
    }
}
#[cfg(windows)]
fn inputs(root: &Path) -> Result<BTreeMap<String, String>> {
    [
        "Cargo.lock",
        "xtask/Cargo.toml",
        "xtask/src/main.rs",
        "xtask/src/windows_trace.rs",
        "xtask/src/windows_faults.rs",
        "xtask/src/windows_volume.rs",
        "xtask/src/windows_process.rs",
        "xtask/src/trace_process.rs",
        "xtask/src/trace_model.rs",
        "xtask/src/delivery.rs",
        "xtask/src/toolchain.rs",
        "xtask/src/support.rs",
        "xtask/tests/trace-model.json",
        "xtask/tests/trace-v1.expected",
        "xtask/tests/trace-v2.expected",
        "tools/artifact-trace/consumer.c",
        "tools/artifact-trace/windows_consumer.c",
        "tools/delivered-faults/consumer.c",
        "tools/delivered-faults/windows_injector.h",
    ]
    .into_iter()
    .map(|name| Ok((name.into(), sha(&root.join(name))?)))
    .collect()
}
#[cfg(windows)]
fn copy_package(from: &Path, to: &Path) -> Result<()> {
    let hashes = delivery::inventory(from)?;
    fs::create_dir(to)?;
    for name in hashes.keys() {
        let dest = to.join(name);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::copy(from.join(name), dest)?;
    }
    ensure!(
        hashes == delivery::inventory(from)? && hashes == delivery::inventory(to)?,
        "package changed during copy"
    );
    Ok(())
}
#[cfg(windows)]
fn native(
    run: &mut crate::support::Run,
    package: &Path,
    output: &Path,
    pin: &str,
    info: Value,
    features: u32,
    campaign: Campaign,
) -> Result<()> {
    fs::create_dir_all(output.parent().context("output parent")?)?;
    fs::create_dir(output)?;
    let before = inputs(&run.root)?;
    let scope = match campaign {
        Campaign::State => {
            "Delivered Windows x64 C/C++ finite state trace; not full API, MC/DC or power loss"
        }
        Campaign::InjectedFaults => {
            "Delivered Windows x64 C publication errors and fresh-process recovery"
        }
        Campaign::DiskFull => {
            "Delivered Windows x64 C publication and recovery on a full private fixed NTFS virtual disk"
        }
    };
    let mut proof = json!({"verified":false,"native_windows_verified":false,"scope":scope,"engine_source_id":info["source_id"],"features":features,"manifest_sha256":pin,"commands":[],"source_before":before,"runner_sha256":sha(&std::env::current_exe()?)?});
    write_proof(output, &proof)?;
    let result = (|| -> Result<()> {
        proof["data_volume"] = crate::windows_process::volume(output)?;
        let input = output.join("inputs");
        fs::create_dir(&input)?;
        for name in ["consumer.c", "windows_consumer.c"] {
            fs::copy(
                run.root.join("tools/artifact-trace").join(name),
                input.join(name),
            )?;
        }
        let relocated = output.join("relocated-package");
        copy_package(package, &relocated)?;
        verified(&relocated, pin)?;
        run.env = clean_env(&run.env);
        let selected = crate::toolchain::compiler_environment(run, output)?;
        proof["compiler_setup_commands"] = json!(run.commands);
        let selected: BTreeMap<_, _> = selected
            .into_iter()
            .filter(|(k, _)| {
                matches!(
                    k.to_string_lossy().to_ascii_uppercase().as_str(),
                    "PATH"
                        | "INCLUDE"
                        | "LIB"
                        | "LIBPATH"
                        | "SYSTEMROOT"
                        | "WINDIR"
                        | "TEMP"
                        | "TMP"
                        | "COMSPEC"
                        | "VCTOOLSVERSION"
                        | "WINDOWSSDKVERSION"
                        | "UCRTVERSION"
                        | "VSCMD_ARG_TGT_ARCH"
                )
            })
            .collect();
        let env = clean_env(&selected);
        let lower: BTreeMap<_, _> = env
            .iter()
            .map(|(k, v)| (k.to_string_lossy().to_ascii_lowercase(), v.clone()))
            .collect();
        let target = lower
            .get("vscmd_arg_tgt_arch")
            .context("VS target architecture missing")?
            .to_string_lossy()
            .to_ascii_lowercase();
        ensure!(
            matches!(target.as_str(), "x64" | "amd64")
                && lower.get("vctoolsversion").is_some_and(|v| !v.is_empty())
                && lower
                    .get("windowssdkversion")
                    .is_some_and(|v| !v.is_empty()),
            "identified x64 VS developer environment required"
        );
        let compiler =
            crate::toolchain::which(OsStr::new("cl.exe"), &env).context("MSVC compiler missing")?;
        proof["msvc"] = json!({"vctoolsversion":lower["vctoolsversion"].to_string_lossy(),"windowssdkversion":lower["windowssdkversion"].to_string_lossy(),"vscmd_arg_tgt_arch":lower["vscmd_arg_tgt_arch"].to_string_lossy()});
        proof["compiler_sha256"] = json!(sha(&compiler)?);
        let mut runtime: BTreeMap<_, _> = env
            .iter()
            .filter(|(k, _)| {
                matches!(
                    k.to_string_lossy().to_ascii_uppercase().as_str(),
                    "SYSTEMROOT" | "WINDIR" | "TEMP" | "TMP" | "COMSPEC"
                )
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let system =
            std::path::PathBuf::from(lower.get("systemroot").context("SystemRoot missing")?);
        runtime.insert(
            "PATH".into(),
            std::env::join_paths([system.join("System32"), system])?,
        );
        if campaign != Campaign::State {
            let driver = run.root.join("tools/delivered-faults");
            fs::copy(driver.join("consumer.c"), input.join("fault_consumer.c"))?;
            fs::copy(
                driver.join("windows_injector.h"),
                input.join("windows_injector.h"),
            )?;
            faults::run(
                &mut proof,
                faults::Environment {
                    output,
                    input: &input,
                    package: &relocated,
                    compiler: compiler.as_os_str(),
                    build: &env,
                    runtime: &runtime,
                },
                text(&info["source_id"])?,
                campaign == Campaign::DiskFull,
            )?;
            verified(package, pin)?;
            verified(&relocated, pin)?;
            ensure!(inputs(&run.root)? == before, "fault tool inputs changed");
            return Ok(());
        }
        let mut outcomes = vec![];
        for (cpp, language) in [(false, "c"), (true, "cpp")] {
            let build = output.join(language);
            fs::create_dir(&build)?;
            let exe = build.join(format!("consumer-{language}.exe"));
            let dll = build.join("weekend_db.dll");
            fs::copy(relocated.join("weekend_db.dll"), &dll)?;
            let digest = sha(&dll)?;
            call(
                &mut proof,
                output,
                compiler_command(compiler.as_os_str(), cpp, &input, &relocated, &exe),
                &build,
                "compile",
                &env,
                false,
            )?;
            let mut entries = cases()?;
            for name in ["wrong-hash", "wrong-path", "wrong-source"] {
                entries.push(Case {
                    name,
                    bytes: trace_model::fixed(2)?.encode()?,
                    expected: None,
                });
            }
            for entry in entries {
                let dir = build.join(entry.name);
                fs::create_dir(&dir)?;
                let trace = dir.join("trace.bin");
                fs::write(&trace, &entry.bytes)?;
                let db = dir.join("database.wdb");
                let before = dir.join("before.json");
                let after = dir.join("after.json");
                let expected_dll = if entry.name == "wrong-path" {
                    package.join("weekend_db.dll")
                } else {
                    dll.clone()
                };
                let mut expected_hash = digest.clone();
                let mut expected_source = text(&info["source_id"])?.to_owned();
                if entry.name == "wrong-hash" {
                    let c = if expected_hash.starts_with('0') {
                        "1"
                    } else {
                        "0"
                    };
                    expected_hash.replace_range(..1, c);
                }
                if entry.name == "wrong-source" {
                    let c = if expected_source.starts_with('0') {
                        "1"
                    } else {
                        "0"
                    };
                    expected_source.replace_range(..1, c);
                }
                let report = call(
                    &mut proof,
                    output,
                    vec![
                        path(&exe),
                        path(&trace),
                        path(&db),
                        expected_source.into(),
                        path(&expected_dll),
                        expected_hash.into(),
                        path(&before),
                        path(&after),
                    ],
                    &dir,
                    "run",
                    &runtime,
                    entry.expected.is_none(),
                )?;
                let pid = report.pid.context("consumer PID missing")?;
                if let Some(expected) = entry.expected {
                    ensure!(
                        fs::read(dir.join("run.stdout"))? == expected,
                        "oracle transcript mismatch: {language}/{}",
                        entry.name
                    );
                    for (p, phase) in [(&before, "before"), (&after, "after")] {
                        attestation(
                            &delivery::read_json(p)?,
                            phase,
                            dll.to_str().context("UTF-8 DLL path")?,
                            &digest,
                            text(&info["source_id"])?,
                            features,
                            pid,
                        )?;
                    }
                    ensure!(db.is_file(), "successful trace did not create database");
                    fs::write(dir.join("expected.txt"), expected)?;
                } else {
                    ensure!(
                        report.exit.is_some_and(|n| n != 0) && !db.exists() && !after.exists(),
                        "rejection control accepted/created database"
                    );
                    if matches!(entry.name, "invalid-mode" | "unknown-version") {
                        attestation(
                            &delivery::read_json(&before)?,
                            "before",
                            dll.to_str().context("UTF-8 DLL path")?,
                            &digest,
                            text(&info["source_id"])?,
                            features,
                            pid,
                        )?;
                    } else {
                        ensure!(!before.exists(), "identity control reached trace");
                    }
                }
                ensure!(sha(&dll)? == digest, "runtime DLL changed");
                outcomes.push(json!({"language":language,"case":entry.name,"exit":report.exit,"verified":true,"stdout_sha256":sha(&dir.join("run.stdout"))?}));
            }
            proof[language] = json!({"executable_sha256":sha(&exe)?,"dll_sha256":digest,"profile":if cpp {"/O2 /UNDEBUG /MD /TP /std:c++17"}else{"/O2 /UNDEBUG /MD /TC /std:c11"}});
        }
        verified(package, pin)?;
        verified(&relocated, pin)?;
        ensure!(inputs(&run.root)? == before, "trace tool inputs changed");
        ensure!(outcomes.len() == 20, "missing native trace cases");
        proof["outcomes"] = json!(outcomes);
        proof["trace_cells"] = json!(134);
        proof["trace_sha256"] = json!(trace_model::TRACE_SHA256);
        proof["expected_sha256"] = json!(trace_model::EXPECTED_SHA256);
        Ok(())
    })();
    proof["verified"] = json!(result.is_ok());
    proof["native_windows_verified"] = json!(result.is_ok());
    if let Err(e) = &result {
        proof["failure"] = json!(format!("{e:#}"));
    }
    let inventory = delivery::inventory(output);
    match inventory {
        Ok(mut hashes) => {
            hashes.remove("evidence.json");
            proof["artifact_sha256"] = json!(hashes);
        }
        Err(e) => {
            proof["verified"] = json!(false);
            proof["native_windows_verified"] = json!(false);
            proof["inventory_failure"] = json!(format!("{e:#}"));
            write_proof(output, &proof)?;
            return Err(e);
        }
    }
    write_proof(output, &proof)?;
    result?;
    let outcomes = match campaign {
        Campaign::State => json!({"cases":20,"cells":134,"dll_sha256":proof["c"]["dll_sha256"]}),
        Campaign::InjectedFaults => {
            json!({"cases":6,"disabled_injection_control":true,"dll_sha256":proof["dll_sha256"]})
        }
        Campaign::DiskFull => {
            json!({"cases":1,"dll_sha256":proof["dll_sha256"],"disk_full":proof["disk_full"],"volume_cleanup":proof["volume_cleanup"]})
        }
    };
    let mut summary = json!({"verified":true,"output":output,"source_id":info["source_id"],
        "manifest_sha256":pin,"evidence_sha256":sha(&output.join("evidence.json"))?});
    summary
        .as_object_mut()
        .unwrap()
        .extend(outcomes.as_object().unwrap().clone());
    println!("{summary}");
    Ok(())
}
