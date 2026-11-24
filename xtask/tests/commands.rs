#![cfg(unix)]

use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn real_mutation_cli_shards_cover_the_prefix_without_outputs_or_case_loss() {
    let list = |extra: &[String]| {
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["mutations", "--list", "--case", "r1-"])
            .args(extra)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let expected = list(&[]);
    let fixture: serde_json::Value = serde_json::from_str(include_str!("mutations.json")).unwrap();
    let registered = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["os"]
                .as_str()
                .is_none_or(|os| os == std::env::consts::OS)
        })
        .filter_map(|case| case["name"].as_str().filter(|name| name.starts_with("r1-")))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(expected, registered);
    assert!(!expected.is_empty());
    let mut seen = BTreeSet::new();
    for index in 0..3 {
        let args = [
            "--shard-index".into(),
            index.to_string(),
            "--shard-count".into(),
            "3".into(),
        ];
        let selected = list(&args);
        assert!(!selected.is_empty());
        assert_eq!(selected, list(&args));
        for name in selected {
            assert!(seen.insert(name));
        }
    }
    assert_eq!(seen, expected.into_iter().collect());
    for (index, count) in [(0, 0), (3, 3), (0, usize::MAX)] {
        let dir = std::env::temp_dir().join(format!(
            "weekend-invalid-shard-{}-{index}-{count}",
            std::process::id()
        ));
        assert!(!dir.exists());
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args([
                "mutations",
                "--shard-index",
                &index.to_string(),
                "--shard-count",
                &count.to_string(),
                "--output",
            ])
            .arg(&dir)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(!dir.exists());
    }
}

struct Fixture(PathBuf);
static NEXT: AtomicU64 = AtomicU64::new(0);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn failed_cargo(args: &[&str]) -> (Output, String) {
    let name = format!(
        "weekend-xtask-cli-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let directory = std::env::temp_dir().join(name);
    fs::create_dir(&directory).unwrap();
    let dir = Fixture(directory);
    let fake = dir.0.join("cargo");
    let trace = dir.0.join("trace");
    // A sibling fork can inherit a temporary executable's writable file
    // descriptor. Link to the checked-in script instead of writing it here.
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/failing-cargo.sh");
    symlink(script, &fake).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .env("PATH", &dir.0)
        .env("TRACE", &trace)
        .output()
        .unwrap();
    let calls = fs::read_to_string(trace).unwrap_or_else(|error| {
        panic!(
            "fake Cargo did not record its calls: {error}\n{}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out, calls)
}

#[test]
fn a_failed_check_stops_before_later_checks_and_consumers() {
    let (out, calls) = failed_cargo(&["check", "--skip-odin"]);
    assert!(!out.status.success());
    assert_eq!(calls, "fmt --all --check\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("29"));
}

#[test]
fn a_failed_build_never_reports_success() {
    let (out, calls) = failed_cargo(&["build", "--release"]);
    assert!(!out.status.success());
    assert_eq!(calls, "build --offline --locked --workspace --release\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("29"));
}
