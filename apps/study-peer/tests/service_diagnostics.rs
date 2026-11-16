#![cfg(any(target_os = "macos", target_os = "linux", windows))]
#[path = "../../../tests/support/service_diagnostics.rs"]
mod diagnostics;
use diagnostics::{BUDGET, CREDENTIAL_CANARY, ID, PATH_CANARY, Process, RECORD_CANARY, Root, safe};
use std::{
    fs,
    io::Write,
    net::{Shutdown, TcpStream},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};
fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_study-peer"))
}

#[test]
fn operational_errors_do_not_echo_private_inputs() {
    let root = Root::new("study-peer");
    let database = root.0.join("study.wdb");
    let mut missing = command();
    missing
        .arg("init")
        .arg(root.0.join("missing").join("study.wdb"))
        .args(["a", ID]);
    let logs = Process::start(&root.0, "missing-database-parent", missing).finish(false);
    safe(&logs);
    assert_eq!(logs.1.trim(), "NotFound");

    let mut init = command();
    init.arg("init").arg(&database).args(["a", ID]);
    Process::start(&root.0, "init", init).finish(true);
    let before = fs::read(&database).unwrap();
    let local = root.credentials("local");
    let peer = root.credentials("peer");
    let pin = peer.join("cert.der");
    let run = |credentials: &Path| {
        let mut c = command();
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
    assert!(logs.0.is_empty() && logs.1.contains("os error"));

    let key = local.join("key.der");
    let original = fs::read(&key).unwrap();
    fs::write(&key, CREDENTIAL_CANARY).unwrap();
    let logs = Process::start(&root.0, "malformed-key", run(&local)).finish(false);
    safe(&logs);
    assert!(logs.0.is_empty() && logs.1.contains("key"));
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
    service.until(|s| s.contains("error invalid digit"));
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
        if stdout.contains("failed=1") {
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
    assert!(logs.0.contains("incoming served=0 failed=1 busy=0 error "));
    assert_eq!(fs::read(&database).unwrap(), before);
}
