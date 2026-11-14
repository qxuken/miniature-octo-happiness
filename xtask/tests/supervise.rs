#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "xtask-process-controls-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request(root: &Path, body: String) -> Value {
    json!({"argv":[std::env::var("WEEKEND_PYTHON").unwrap_or_else(|_|"python3".into()),"-c",body],"cwd":null,"env":null,"stdout":root.join("stdout"),"stderr":root.join("stderr"),"report":root.join("report.json"),"timeout_seconds":5.,"maximum_bytes":8192,"file_limit":null,"journal":null})
}
#[test]
fn cancellation_retires_worker_and_writes_terminal_report() {
    let owned = Fixture::new();
    let marker = owned.0.join("pid");
    let req = request(
        &owned.0,
        format!(
            "import os,time;from pathlib import Path;Path({:?}).write_text(str(os.getpid()));time.sleep(30)",
            marker.to_str().unwrap()
        ),
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("supervise")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&req).unwrap())
        .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while !marker.exists() {
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: this is our still-unreaped supervisor child, whose SIGTERM handler
    // must retire its separate fixture session before returning its report.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["cancelled"], true);
    assert_eq!(report["cleanup_verified"], true);
    assert_eq!(report["returncode"], -9);
    assert_eq!(
        report["pid"].to_string(),
        fs::read_to_string(marker).unwrap()
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(owned.0.join("report.json")).unwrap()).unwrap();
    assert_eq!(report, saved);
}
#[test]
fn malformed_request_and_unknown_fields_do_not_launch() {
    let owned = Fixture::new();
    let mut req = request(&owned.0, "raise Exception('must not execute')".into());
    req["unknown"] = json!(true);
    let mut child = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("supervise")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&req).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert_eq!(fs::read_dir(&owned.0).unwrap().count(), 0);
}
#[test]
fn fault_admission_rejects_all_existing_outputs_before_external_work() {
    let owned = Fixture::new();
    let keep = owned.0.join("keep");
    fs::write(&keep, b"preserve").unwrap();
    let link = owned.0.join("link");
    std::os::unix::fs::symlink(owned.0.join("missing"), &link).unwrap();
    let package = owned.0.join("package");
    fs::create_dir(&package).unwrap();
    for task in ["native", "delivered"] {
        for path in [&keep, &link, &owned.0, &owned.0.join("absent/child")] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
            command
                .env("PATH", "/definitely/no/programs")
                .args(["faults", task, "--output"])
                .arg(path);
            if task == "delivered" {
                command.arg("--package").arg(&package);
            }
            let out = command.output().unwrap();
            assert!(!out.status.success());
            assert!(
                !String::from_utf8_lossy(&out.stderr).contains("spawn child"),
                "admission must precede git/docker"
            );
        }
    }
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["faults", "delivered", "--package"])
        .arg(&package)
        .arg("--output")
        .arg(package.join("new"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!package.join("new").exists());
    assert_eq!(fs::read(keep).unwrap(), b"preserve");
    assert!(!owned.0.join("missing").exists());
}
#[test]
fn fault_setup_failure_keeps_negative_evidence() {
    let owned = Fixture::new();
    let package = owned.0.join("package");
    fs::create_dir(&package).unwrap();
    let output = owned.0.join("out");
    let status = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["faults", "delivered", "--package"])
        .arg(package)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!status.status.success());
    let proof: Value =
        serde_json::from_slice(&fs::read(output.join("evidence.json")).unwrap()).unwrap();
    assert_eq!(proof["verified"], false);
    assert!(proof["failure"].is_string());
}

#[test]
fn fault_campaign_cancellation_still_runs_absence_checks() {
    use std::os::unix::fs::PermissionsExt;
    let owned = Fixture::new();
    let bin = owned.0.join("bin");
    fs::create_dir(&bin).unwrap();
    let marker = owned.0.join("running");
    let ledger = owned.0.join("docker-commands");
    let python = std::env::var("WEEKEND_PYTHON").unwrap_or_else(|_| "python3".into());
    let resolved = Command::new(&python)
        .args(["-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    let python = String::from_utf8(resolved.stdout).unwrap();
    let script = format!(
        "#!{}\nimport sys,time\nfrom pathlib import Path\na=sys.argv[1:]\nwith Path({:?}).open('a')as f:f.write(' '.join(a)+'\\n')\nif a[:2]==['image','inspect']:print('sha256:'+'a'*64)\nif a and a[0]=='run':\n Path({:?}).write_text('running')\n time.sleep(30)\n",
        python.trim(),
        ledger.to_str().unwrap(),
        marker.to_str().unwrap()
    );
    fs::write(bin.join("docker"), script).unwrap();
    fs::set_permissions(bin.join("docker"), fs::Permissions::from_mode(0o700)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = owned.0.join("out");
    let mut child = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["faults", "native", "--image"])
        .arg(format!("sha256:{}", "a".repeat(64)))
        .arg("--output")
        .arg(&output)
        .env("PATH", path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        if child.try_wait().unwrap().is_some() {
            panic!("fault campaign exited before fixture");
        }
        if Instant::now() >= end {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("fault fixture deadline");
        }
        thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: this is our unreaped campaign child. It must handle cancellation
    // across commands, retire its fixture and still execute Docker absence checks.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    let end = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= end {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("campaign cleanup deadline");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(!status.success());
    let proof: Value =
        serde_json::from_slice(&fs::read(output.join("evidence.json")).unwrap()).unwrap();
    assert_eq!(proof["verified"], false);
    assert_eq!(proof["owned_resources_absent"], true);
    let calls = fs::read_to_string(ledger).unwrap();
    assert!(
        calls
            .lines()
            .any(|l| l.starts_with("rm -f weekend-native-faults-"))
    );
    assert!(calls.lines().any(|l| l.starts_with("ps -a --format=")));
}
