#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

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
    fs::write(
        &fake,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$TRACE\"\nexit 29\n",
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .env("PATH", &dir.0)
        .env("TRACE", &trace)
        .output()
        .unwrap();
    let calls = fs::read_to_string(trace).unwrap();
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
