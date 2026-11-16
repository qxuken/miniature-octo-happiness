#![cfg(any(target_os = "macos", target_os = "linux", windows))]
#[path = "../../../tests/support/service_diagnostics.rs"]
mod diagnostics;
use diagnostics::{CREDENTIAL_CANARY, ID, PATH_CANARY, Process, RECORD_CANARY, Root, safe};
use std::{
    fs,
    io::Write,
    net::{Shutdown, TcpStream},
    path::Path,
    process::Command,
    time::Duration,
};
fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_weekend-replicate"))
}

#[test]
fn operational_errors_do_not_echo_private_inputs() {
    let root = Root::new("weekend-replicate");
    let source = root.0.join("source.wdb");
    fs::write(&source, RECORD_CANARY).unwrap();
    let mut inspect = command();
    inspect.arg("describe").arg(&source).arg(ID);
    let logs = Process::start(&root.0, "corrupt-image", inspect).finish(false);
    safe(&logs);
    assert_eq!(logs.1.trim(), "Corrupt");
    assert_eq!(fs::read(&source).unwrap(), RECORD_CANARY.as_bytes());

    let server = root.credentials("server");
    let peer = root.credentials("peer");
    let pin = peer.join("cert.der");
    let follower = root.0.join("follower");
    fs::create_dir(&follower).unwrap();
    let mut init = command();
    init.arg("follower-init").arg(&follower).args([ID, ID]);
    assert_eq!(
        Process::start(&root.0, "init", init).finish(true).0,
        "follower empty\n"
    );
    let serve = |credentials: &Path| {
        let mut c = command();
        c.arg("serve")
            .arg(&follower)
            .args([ID, ID, "127.0.0.1:0"])
            .arg(credentials)
            .arg(&pin)
            .args(["--max-exchanges", "1"]);
        c
    };
    let logs = Process::start(
        &root.0,
        "missing-credentials",
        serve(&root.0.join("missing")),
    )
    .finish(false);
    safe(&logs);
    assert!(logs.0.is_empty() && logs.1.contains("os error"));

    let key = server.join("key.der");
    let original = fs::read(&key).unwrap();
    fs::write(&key, CREDENTIAL_CANARY).unwrap();
    let logs = Process::start(&root.0, "malformed-key", serve(&server)).finish(false);
    safe(&logs);
    assert!(logs.0.is_empty() && logs.1.contains("key"));
    fs::write(&key, original).unwrap();

    let mut service = Process::start(&root.0, "service", serve(&server));
    let ready = service.until(|s| s.starts_with("ready "));
    let address = ready
        .lines()
        .next()
        .unwrap()
        .strip_prefix("ready ")
        .unwrap()
        .parse()
        .unwrap();
    let mut rejected = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    rejected
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    rejected
        .write_all(format!("{RECORD_CANARY} {CREDENTIAL_CANARY} {PATH_CANARY}").as_bytes())
        .unwrap();
    rejected.shutdown(Shutdown::Both).unwrap();
    drop(rejected);
    let logs = service.finish(true);
    safe(&logs);
    // Peer-controlled failures deliberately have no blocking stderr write.
    assert!(logs.1.is_empty());
    assert_eq!(logs.0.lines().count(), 1);
    let mut status = command();
    status.arg("status").arg(&follower).args([ID, ID]);
    let logs = Process::start(&root.0, "status", status).finish(true);
    safe(&logs);
    assert!(logs.0.starts_with("follower empty\n"));
}
