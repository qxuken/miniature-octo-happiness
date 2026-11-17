use super::*;
use std::{net::TcpListener, os::unix::fs::MetadataExt, path::Path, process::Output};
use weekend_db::{Limits, storage};

fn assert_io(output: Output) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(output.stderr, b"Io\n");
}

fn witnessed(path: &Path, revision: u64) {
    let image = storage::inspect_file(path, Limits::default()).unwrap();
    assert_eq!((image.revision, image.witnessed), (revision, true));
}

#[test]
#[ignore = "requires a dedicated empty Linux tmpfs no larger than 8 MiB"]
fn full_journal_preserves_intent_and_replays_exact_coordinator_receipt() {
    let _exclusive = STORAGE_WORKLOAD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (mount, empty_space) = test_tmpfs::empty_mount("WEEKEND_TEST_JOURNAL_TMPFS");
    let owned = mount.join("journal-case");
    fs::create_dir(&owned).unwrap();
    let local = Directory(owned);
    let journal = local.0.join("writer.wdb");
    let dir = Directory::new();
    // The server needs independent space while the journal is full.
    assert_ne!(
        fs::metadata(&dir.0).unwrap().dev(),
        fs::metadata(&mount).unwrap().dev()
    );
    let remote = dir.0.join("coordinator.wdb");
    let secret = dir.0.join("secret");
    let token = "3f".repeat(32);
    write_secret(&secret, &token);
    let key = secret.to_str().unwrap();
    let remote_path = remote.to_str().unwrap();
    assert!(
        Command::new(BIN)
            .args(["init-sequenced", remote_path])
            .status()
            .unwrap()
            .success()
    );
    let (mut server, client) = configured_service("serve-sequenced", remote_path, key, &token);
    let port = client.address.port().to_string();
    let identity = client.request("identity");
    let identity = identity.strip_prefix("identity ").unwrap();
    let run = |action: &str, tail: &[&str]| {
        Command::new(BIN)
            .args(["journal", action])
            .arg(&journal)
            .args(tail)
            .output()
            .unwrap()
    };
    let checked = |action: &str, tail: &[&str]| {
        let output = run(action, tail);
        assert!(output.status.success(), "journal {action}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let initial = format!("journal {identity} 0\nreceipt 0\n");
    let pending = format!("{initial}pending 1 3 42\n");
    let acknowledged = format!("journal {identity} 0\nreceipt 1 3 42 4\n");
    assert_eq!(
        checked("init", &[&port, "0", "--secret-file", key]),
        initial
    );
    let journal_before = fs::read(&journal).unwrap();
    let remote_before = fs::read(&remote).unwrap();

    // A valid request cannot reach publication if the client cannot stage it.
    let ballast = test_tmpfs::fill(&local.0, 0);
    test_tmpfs::assert_full(&local.0);
    assert_io(run("set", &[&port, "3", "42", "--secret-file", key]));
    witnessed(&journal, 2);
    assert_eq!(checked("status", &[]), initial);
    assert_eq!(client.request("get"), "value 3 0");
    assert_eq!(client.request("receipt 0"), "receipt 0");
    assert_eq!(fs::read(&journal).unwrap(), journal_before);
    assert_eq!(fs::read(&remote).unwrap(), remote_before);
    test_tmpfs::assert_full(&local.0);
    fs::remove_file(ballast).unwrap();

    // Allow preflight and stage at the real CLI entry point. Hold the
    // mutating request before delivery and kill the client after the stage.
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let proxy_port = proxy.local_addr().unwrap().port().to_string();
    let mut interrupted = Process::start(&[
        "journal",
        "set",
        journal.to_str().unwrap(),
        &proxy_port,
        "3",
        "42",
        "--secret-file",
        key,
    ]);
    let (mut preflight, command) = accept_journal_request(&proxy, &client, "journal-get");
    assert_eq!(command, format!("journal-get {identity}"));
    let reply = client.request(&command);
    assert_eq!(reply, "value 3 0");
    writeln!(preflight, "{reply}").unwrap();
    drop(preflight);
    let (undelivered, command) = accept_journal_request(&proxy, &client, "journal-set");
    assert_eq!(command, format!("journal-set {identity} 0 1 - 3 42"));
    interrupted.stop();
    drop(undelivered);
    drop(proxy);
    witnessed(&journal, 3);
    assert_eq!(checked("status", &[]), pending);
    assert_eq!(
        checked("check", &[&port, "--secret-file", key]),
        "pending-eligible 3 0\n"
    );
    assert_eq!(fs::read(&remote).unwrap(), remote_before);
    let staged_bytes = fs::read(&journal).unwrap();

    // The resumed client receives the real durable acknowledgment, but its
    // own append cannot allocate space for clearing the pending intent.
    let ballast = test_tmpfs::fill(&local.0, 0);
    test_tmpfs::assert_full(&local.0);
    assert_io(run("resume", &[&port, "--secret-file", key]));
    assert_eq!(client.request("get"), "value 4 42");
    assert_eq!(client.request("receipt 0"), "receipt 1 3 42 4");
    assert_eq!(client.request("receipt 1"), "receipt 0");
    server.stop();
    witnessed(&remote, 4);
    let committed_bytes = fs::read(&remote).unwrap();

    let (mut server, client) = configured_service("serve-sequenced", remote_path, key, &token);
    let port = client.address.port().to_string();
    witnessed(&journal, 3);
    assert_eq!(checked("status", &[]), pending);
    assert_eq!(
        checked("check", &[&port, "--secret-file", key]),
        "pending-committed 4 42\n"
    );
    assert_io(run("resume", &[&port, "--secret-file", key]));
    assert_eq!(client.request("receipt 0"), "receipt 1 3 42 4");
    assert_eq!(fs::read(&remote).unwrap(), committed_bytes);
    assert_eq!(fs::read(&journal).unwrap(), staged_bytes);
    test_tmpfs::assert_full(&local.0);

    fs::remove_file(ballast).unwrap();
    assert_eq!(
        checked("resume", &[&port, "--secret-file", key]),
        "committed 4\n"
    );
    witnessed(&journal, 4);
    assert_eq!(checked("status", &[]), acknowledged);
    assert_eq!(
        checked("check", &[&port, "--secret-file", key]),
        "aligned 4 42\n"
    );
    server.stop();
    let committed_journal = fs::read(&journal).unwrap();
    assert_eq!(
        checked("resume", &[&port, "--secret-file", key]),
        "committed 4\n"
    );
    assert_eq!(fs::read(&journal).unwrap(), committed_journal);
    assert_eq!(fs::read(&remote).unwrap(), committed_bytes);
    drop(local);
    assert_eq!(test_tmpfs::space(&mount).1, empty_space);
    assert!(fs::read_dir(&mount).unwrap().next().is_none());
    eprintln!(
        "kernel ENOSPC: staging reopened journal 2 / server 3; receipt reopened journal 3 / server 4; retry acknowledged journal 4 / server 4"
    );
}
