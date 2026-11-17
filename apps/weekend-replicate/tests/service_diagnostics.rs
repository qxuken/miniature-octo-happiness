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
fn command(json: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_weekend-replicate"));
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
fn scenario(json: bool) {
    let root = Root::new("weekend-replicate");
    let source = root.0.join("source.wdb");
    fs::write(&source, RECORD_CANARY).unwrap();
    let mut inspect = command(json);
    inspect.arg("describe").arg(&source).arg(ID);
    let logs = Process::start(&root.0, "corrupt-image", inspect).finish(false);
    safe(&logs);
    if json {
        diagnostic(&logs.1, "storage_verify", "engine", "Corrupt");
        assert!(
            logs.1
                .contains("\"verification\":{\"stage\":\"image_length\",\"offset\":null}")
        );
    } else {
        assert_eq!(logs.1.trim(), "Corrupt");
    }
    assert_eq!(fs::read(&source).unwrap(), RECORD_CANARY.as_bytes());

    let server = root.credentials("server");
    let peer = root.credentials("peer");
    let pin = peer.join("cert.der");
    let primary = root.0.join("primary.wdb");
    {
        let db =
            weekend_db::Database::create_file(&primary, weekend_db::Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        let mut transaction = db.begin();
        transaction
            .put(
                collection,
                RECORD_CANARY.as_bytes(),
                &db.bytes(CREDENTIAL_CANARY.as_bytes()).unwrap(),
            )
            .unwrap();
        transaction.commit().unwrap();
    }
    let mut describe = command(json);
    describe.arg("describe").arg(&primary).arg(ID);
    let description = Process::start(&root.0, "primary-description", describe).finish(true);
    safe(&description);
    let identity = description
        .0
        .split_whitespace()
        .find_map(|word| word.strip_prefix("file_identity="))
        .unwrap();
    let follower = root.0.join("follower");
    fs::create_dir(&follower).unwrap();
    let mut init = command(json);
    init.arg("follower-init")
        .arg(&follower)
        .args([ID, identity]);
    assert_eq!(
        Process::start(&root.0, "init", init).finish(true).0,
        "follower empty\n"
    );
    let serve = |credentials: &Path| {
        let mut c = command(json);
        c.arg("serve")
            .arg(&follower)
            .args([ID, identity, "127.0.0.1:0"])
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
    assert!(logs.0.is_empty());
    if json {
        diagnostic(&logs.1, "credentials_load", "io", "not_found");
    } else {
        assert!(logs.1.contains("os error"));
    }

    let key = server.join("key.der");
    let original = fs::read(&key).unwrap();
    fs::write(&key, CREDENTIAL_CANARY).unwrap();
    let logs = Process::start(&root.0, "malformed-key", serve(&server)).finish(false);
    safe(&logs);
    assert!(logs.0.is_empty());
    if json {
        diagnostic(&logs.1, "tls_config", "tls", "other");
    } else {
        assert!(logs.1.contains("key"));
    }
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
    let mut status = command(json);
    status.arg("status").arg(&follower).args([ID, identity]);
    let logs = Process::start(&root.0, "status", status).finish(true);
    safe(&logs);
    assert!(logs.0.starts_with("follower empty\n"));

    let mut service = Process::start(&root.0, "authenticated-service", serve(&server));
    let ready = service.until(|text| text.starts_with("ready "));
    let address = ready
        .lines()
        .next()
        .unwrap()
        .strip_prefix("ready ")
        .unwrap();
    let mut ship = command(json);
    ship.arg("ship")
        .arg(&primary)
        .arg(ID)
        .arg(root.0.join("export.wdb"))
        .arg(address)
        .arg(&peer)
        .arg(server.join("cert.der"))
        .args(["--attempts", "1"]);
    let shipment = Process::start(&root.0, "shipment", ship).finish(true);
    safe(&shipment);
    safe(&service.finish(true));
    assert!(shipment.1.is_empty());
    let receipt = shipment
        .0
        .lines()
        .find(|line| line.starts_with("follower durable_revision=2 "))
        .unwrap();
    let mut status = command(json);
    status.arg("status").arg(&follower).args([ID, identity]);
    let reopened = Process::start(&root.0, "reopened", status).finish(true);
    safe(&reopened);
    assert_eq!(reopened.0.lines().next().unwrap(), receipt);

    let descriptor = root.0.join("invalid-descriptor.wdb");
    {
        let db =
            weekend_db::Database::create_file(&descriptor, weekend_db::Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        let mut bytes = [0; 88];
        bytes[..RECORD_CANARY.len()].copy_from_slice(RECORD_CANARY.as_bytes());
        let mut transaction = db.begin();
        transaction
            .put(collection, b"snapshot-capture", &db.bytes(&bytes).unwrap())
            .unwrap();
        transaction.commit().unwrap();
    }
    let before = fs::read(&descriptor).unwrap();
    for (label, record, phase, code) in [
        (
            "invalid-descriptor",
            descriptor.clone(),
            "storage_verify",
            "Corrupt",
        ),
        (
            "missing-descriptor",
            root.0.join("missing-descriptor"),
            "storage_open",
            "NotFound",
        ),
    ] {
        let mut ship = command(json);
        ship.arg("ship-captured")
            .arg(&primary)
            .arg(ID)
            .arg(root.0.join("export.wdb"))
            .arg(record)
            .arg("127.0.0.1:1")
            .arg(&peer)
            .arg(server.join("cert.der"));
        let logs = Process::start(&root.0, label, ship).finish(false);
        safe(&logs);
        assert!(logs.0.is_empty());
        if json {
            diagnostic(&logs.1, phase, "engine", code);
            assert!(logs.1.contains("\"follower_outcome\":\"unconfirmed\""));
        } else {
            assert_eq!(logs.1.trim(), code);
        }
    }
    assert_eq!(fs::read(&descriptor).unwrap(), before);
}
