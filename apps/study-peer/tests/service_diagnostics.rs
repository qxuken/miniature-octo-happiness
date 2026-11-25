#![cfg(any(target_os = "macos", target_os = "linux", windows))]
#[path = "../../../tests/support/service_diagnostics.rs"]
mod diagnostics;
use diagnostics::{BUDGET, CREDENTIAL_CANARY, ID, PATH_CANARY, Process, RECORD_CANARY, Root, safe};
use std::{
    fs,
    io::{self, Write},
    net::{Shutdown, TcpStream},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};
fn command(json: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_study-peer"));
    if json {
        command.arg("--diagnostics=json");
    }
    command
}
fn diagnostic(text: &str, phase: &str, category: &str, code: &str) {
    assert!(text.len() <= 1024 && text.lines().count() == 1);
    assert!(text.starts_with("{\"schema\":\"weekend-diagnostic/1\","));
    for field in [
        format!("\"phase\":\"{phase}\""),
        format!("\"category\":\"{category}\""),
        format!("\"code\":\"{code}\""),
    ] {
        assert!(text.contains(&field), "missing closed diagnostic field");
    }
    assert!(text.trim_end().ends_with('}'));
}

#[test]
fn operational_errors_do_not_echo_private_inputs() {
    scenario(false);
}
#[test]
fn structured_errors_keep_categories_without_echoing_private_inputs() {
    scenario(true);
}

#[test]
fn closed_ready_pipe_is_a_local_command_error() {
    let root = Root::new("study-ready");
    let database = root.0.join("study.wdb");
    let init = command(true)
        .arg("init")
        .arg(&database)
        .args(["a", ID])
        .output()
        .unwrap();
    assert!(init.status.success());
    let original = fs::read(&database).unwrap();
    let local = root.credentials("local");
    let pin = root.credentials("peer").join("cert.der");
    let (read, write) = io::pipe().unwrap();
    drop(read);
    let output = command(true)
        .arg("run")
        .arg(&database)
        .args(["a", ID, "127.0.0.1:0", "127.0.0.1:1"])
        .arg(local)
        .arg(pin)
        .arg("--no-push")
        .stdout(write)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    diagnostic(
        &String::from_utf8(output.stderr).unwrap(),
        "local_command",
        "io",
        "broken_pipe",
    );
    assert_eq!(fs::read(&database).unwrap(), original);
}

#[test]
fn roster_stdout_refusal_reports_io_and_preserves_the_staged_pair() {
    let root = Root::new("study-cutover");
    let [a, b, new_a, new_b, old, new, expected] = [
        "a",
        "b",
        "new-a",
        "new-b",
        "old.policy",
        "new.policy",
        "old.checkpoint",
    ]
    .map(|name| root.0.join(name));
    for (name, path) in ["a", "b"].into_iter().zip([&a, &b]) {
        let out = command(true)
            .arg("init")
            .arg(path)
            .args([name, ID])
            .output()
            .unwrap();
        assert!(out.status.success());
    }
    fs::write(
        &old,
        format!(
            "study-policy-v1 {ID} 7 {} {}\n",
            "01".repeat(16),
            "02".repeat(16)
        ),
    )
    .unwrap();
    fs::write(
        &new,
        format!(
            "study-policy-v1 {} 8 {} {}\n",
            "0a".repeat(16),
            "03".repeat(16),
            "04".repeat(16)
        ),
    )
    .unwrap();
    let sources = [&a, &b];
    let original = sources.map(|path| fs::read(path).unwrap());
    let checkpoint = || {
        let mut c = command(true);
        c.arg("checkpoint-roster").args(sources).arg(&old);
        c
    };
    let refuse_stdout = |mut c: Command| {
        let (read, write) = io::pipe().unwrap();
        drop(read);
        let out = c.stdout(write).output().unwrap();
        assert_eq!(out.status.code(), Some(1));
        let text = String::from_utf8(out.stderr).unwrap();
        diagnostic(&text, "local_command", "io", "broken_pipe");
    };
    refuse_stdout(checkpoint());
    assert!(!new_a.exists() && !new_b.exists());
    let receipt = checkpoint().output().unwrap();
    assert!(receipt.status.success() && receipt.stderr.is_empty());
    assert!(receipt.stdout.starts_with(b"study-roster-checkpoint-v1 "));
    fs::write(&expected, receipt.stdout).unwrap();

    let mut stage = command(true);
    stage
        .arg("stage-roster")
        .args(sources)
        .arg(&old)
        .args([&new_a, &new_b, &new])
        .arg("--expect-old")
        .arg(&expected);
    refuse_stdout(stage);
    let destination = [&new_a, &new_b];
    let staged = destination.map(|path| fs::read(path).unwrap());
    let verify = || {
        let mut c = command(true);
        c.arg("verify-roster").args(destination).arg(&new);
        c
    };
    refuse_stdout(verify());
    let recovered = verify().output().unwrap();
    assert!(recovered.status.success() && recovered.stderr.is_empty());
    assert!(recovered.stdout.starts_with(b"verified generation=8 "));
    assert_eq!(sources.map(|path| fs::read(path).unwrap()), original);
    assert_eq!(destination.map(|path| fs::read(path).unwrap()), staged);
}

fn scenario(json: bool) {
    let root = Root::new("study-peer");
    let database = root.0.join("study.wdb");
    let mut missing = command(json);
    missing
        .arg("init")
        .arg(root.0.join("missing").join("study.wdb"))
        .args(["a", ID]);
    let logs = Process::start(&root.0, "missing-database-parent", missing).finish(false);
    safe(&logs);
    if json {
        diagnostic(&logs.1, "storage_create", "engine", "NotFound");
        assert!(logs.1.contains("\"native\":{\"operation\":"));
    } else {
        assert_eq!(logs.1.trim(), "NotFound");
    }

    let mut init = command(json);
    init.arg("init").arg(&database).args(["a", ID]);
    Process::start(&root.0, "init", init).finish(true);
    let before = fs::read(&database).unwrap();
    let local = root.credentials("local");
    let peer = root.credentials("peer");
    let pin = peer.join("cert.der");
    let run = |credentials: &Path| {
        let mut c = command(json);
        c.arg("run")
            .arg(&database)
            .args(["a", ID, "127.0.0.1:0", "127.0.0.1:1"])
            .arg(credentials)
            .arg(&pin)
            .arg("--no-push");
        c
    };
    let logs =
        Process::start(&root.0, "missing-credentials", run(&root.0.join("missing"))).finish(false);
    safe(&logs);
    assert!(logs.0.is_empty());
    if json {
        diagnostic(&logs.1, "credentials_load", "io", "not_found");
    } else {
        assert!(logs.1.contains("os error"));
    }

    let key = local.join("key.der");
    let original = fs::read(&key).unwrap();
    fs::write(&key, CREDENTIAL_CANARY).unwrap();
    let logs = Process::start(&root.0, "malformed-key", run(&local)).finish(false);
    safe(&logs);
    assert!(logs.0.is_empty());
    if json {
        diagnostic(&logs.1, "tls_config", "tls", "other");
    } else {
        assert!(logs.1.contains("key"));
    }
    fs::write(&key, original).unwrap();

    let mut service = Process::start(&root.0, "service", run(&local));
    let ready = service.until(|s| s.starts_with("ready "));
    let address = ready
        .lines()
        .next()
        .unwrap()
        .strip_prefix("ready ")
        .unwrap()
        .parse()
        .unwrap();
    service.input(&format!("practice {RECORD_CANARY}\n"));
    if json {
        let output = service.until(|s| s.contains("\"phase\":\"local_command\""));
        diagnostic(
            output.lines().nth(1).unwrap(),
            "local_command",
            "input",
            "invalid_input",
        );
    } else {
        service.until(|s| s.contains("error invalid digit"));
    }
    let mut rejected = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    rejected
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    rejected
        .write_all(format!("{RECORD_CANARY} {CREDENTIAL_CANARY} {PATH_CANARY}").as_bytes())
        .unwrap();
    rejected.shutdown(Shutdown::Both).unwrap();
    drop(rejected);
    let deadline = Instant::now() + BUDGET;
    loop {
        service.input("incoming\n");
        thread::sleep(Duration::from_millis(10));
        let (stdout, _) = service.logs();
        if stdout.contains(if json { "\"failed\":1" } else { "failed=1" }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "deferred authentication diagnostic deadline"
        );
    }
    service.input("quit\n");
    let logs = service.finish(true);
    safe(&logs);
    assert!(logs.1.is_empty());
    if json {
        let incoming = logs
            .0
            .lines()
            .find(|line| line.contains("\"failed\":1"))
            .unwrap();
        assert!(incoming.len() <= 1024);
        assert!(incoming.contains("\"phase\":\"incoming\",\"served\":0,\"failed\":1,\"busy\":0,\"last\":{\"phase\":\"tls_handshake\""));
        assert!(incoming.contains("\"verification\":null"));
    } else {
        assert!(logs.0.contains("incoming served=0 failed=1 busy=0 error "));
    }
    assert_eq!(fs::read(&database).unwrap(), before);
}
