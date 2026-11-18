//! Restore unresolved client intents using real CLI processes and fresh readers.
use super::*;
use std::{net::TcpListener, path::Path};
use weekend_db::{Database, Limits, Snapshot, storage};

const TEST: &str =
    "pending_restore::restored_pending_pairs_replay_once_and_preserve_retained_roots";
const OBSERVE: &str = "WEEKEND_PENDING_RESTORE_OBSERVE";
const BINARY: &str = "WEEKEND_PENDING_RESTORE_BINARY";

fn command() -> Command {
    // A supplied artifact is mandatory. Never silently recover to Cargo's
    // build-path executable when the selected artifact is missing or invalid.
    let binary = std::env::var_os(BINARY).unwrap_or_else(|| BIN.into());
    assert!(
        Path::new(&binary).is_absolute(),
        "absolute coordinator artifact required"
    );
    Command::new(binary)
}

fn start(args: &[&str]) -> Process {
    let mut command = command();
    command.args(args);
    Process::start_command(command)
}

fn configured_service(
    command: &str,
    path: &str,
    secret: &str,
    token: &str,
) -> (Process, AuthenticatedClient) {
    authenticated_server(start(&[command, path, "0", "--secret-file", secret]), token)
}

#[derive(Clone, Copy)]
struct Case {
    prefix: bool,
    forwarded: bool,
}
impl Case {
    fn expected(self) -> u64 {
        if self.prefix { 5 } else { 4 }
    }
    fn value(self) -> u64 {
        if self.prefix { 99 } else { 42 }
    }
    fn sequence(self) -> u64 {
        if self.prefix { 2 } else { 1 }
    }
    fn receipt(self) -> String {
        format!(
            "receipt {} {} {} {}",
            self.sequence(),
            self.expected(),
            self.value(),
            self.expected() + 1
        )
    }
    fn pending_head(self) -> u64 {
        if self.prefix { 5 } else { 3 }
    }
}

fn cli(args: &[&str]) -> String {
    let output = command()
        .args(args)
        .output()
        .expect("selected coordinator invocation");
    assert!(output.status.success(), "coordinator CLI: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn retained(path: &Path) {
    let db = Database::open_file(path, Limits::default()).unwrap();
    assert!(db.retained_ids().unwrap().is_empty());
    assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 1);
}

fn stop_without_acknowledgment(client: &mut Process) {
    client.stop();
    assert!(
        matches!(
            client.output.recv_timeout(Duration::from_secs(15)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ),
        "interrupted client printed a result or left stdout open"
    );
}

// This oracle intentionally uses the documented version-1 row layout, not the
// coordinator's receipt or journal decoder.
fn receipt(sequence: u64, expected: u64, value: u64, revision: u64) -> Vec<u8> {
    let mut bytes = vec![1];
    for field in [sequence, expected, value, revision] {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes
}

fn journal_row(
    snapshot: &Snapshot<'_>,
    db: &Database,
    identity: &[u8; 16],
    case: Case,
    pending: bool,
) {
    let rows = snapshot.scan(db.collection_id(1)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, b"coordinator-journal");
    let bytes = rows[0].1.as_bytes().unwrap();
    assert_eq!(bytes.len(), 76);
    assert_eq!(&bytes[..8], b"WDBJRN01");
    assert_eq!(&bytes[8..24], identity);
    assert_eq!(&bytes[24..26], &[0, 0]);
    assert_eq!(
        bytes[26],
        if pending {
            if case.prefix { 3 } else { 2 }
        } else {
            1
        }
    );
    let previous = if !pending {
        receipt(
            case.sequence(),
            case.expected(),
            case.value(),
            case.expected() + 1,
        )
    } else if case.prefix {
        receipt(1, 4, 42, 5)
    } else {
        vec![0; 33]
    };
    assert_eq!(&bytes[27..60], previous);
    if pending {
        assert_eq!(&bytes[60..68], case.expected().to_le_bytes());
        assert_eq!(&bytes[68..76], case.value().to_le_bytes());
    } else {
        assert_eq!(&bytes[60..76], &[0; 16]);
    }
}

fn observe(server: &Path, journal: &Path, case: Case, phase: &str) {
    let db = Database::open_file(server, Limits::default()).unwrap();
    let identity = db.storage_identity().unwrap();
    assert_eq!(db.retained_ids().unwrap(), [1]);
    let head = db.snapshot();
    let committed = phase != "captured" || case.forwarded;
    let revision = if committed {
        case.expected() + 1
    } else {
        case.expected()
    };
    let value = if committed {
        case.value()
    } else if case.prefix {
        42
    } else {
        0
    };
    assert_eq!(head.revision(), revision, "coordinator revision mismatch");
    for snapshot in [&head, &db.retained_snapshot(1).unwrap()] {
        assert_eq!(snapshot.count(db.collection_id(1)).unwrap(), 1);
        assert_eq!(
            snapshot
                .get(db.collection_id(1), b"protocol")
                .unwrap()
                .unwrap()
                .as_bytes(),
            Ok(b"coordinator-sequenced-v1".as_slice())
        );
    }
    assert_eq!(
        head.get(db.collection_id(2), b"counter")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(value)
    );
    let slot = head.get(db.collection_id(2), b"seq/\0\0").unwrap();
    let expected = if committed {
        Some(receipt(
            case.sequence(),
            case.expected(),
            case.value(),
            case.expected() + 1,
        ))
    } else if case.prefix {
        Some(receipt(1, 4, 42, 5))
    } else {
        None
    };
    assert_eq!(
        slot.as_ref().map(|value| value.as_bytes().unwrap()),
        expected.as_deref()
    );
    assert_eq!(
        head.count(db.collection_id(2)).unwrap(),
        if expected.is_some() { 2 } else { 1 }
    );
    let old = db.retained_snapshot(1).unwrap();
    assert_eq!(old.revision(), 3);
    assert_eq!(
        old.get(db.collection_id(2), b"counter")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(0)
    );
    assert_eq!(old.count(db.collection_id(2)).unwrap(), 1);

    let db = Database::open_file(journal, Limits::default()).unwrap();
    assert_eq!(db.retained_ids().unwrap(), [1]);
    assert_eq!(
        db.snapshot().revision(),
        case.pending_head() + if phase == "acknowledged" { 2 } else { 1 }
    );
    journal_row(
        &db.snapshot(),
        &db,
        &identity,
        case,
        phase != "acknowledged",
    );
    let old = db.retained_snapshot(1).unwrap();
    assert_eq!(old.revision(), case.pending_head());
    journal_row(&old, &db, &identity, case, true);
}

fn run_observer(server: &Path, journal: &Path, case: Case, phase: &str) -> std::process::Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .env(OBSERVE, server)
        .env("WEEKEND_PENDING_RESTORE_JOURNAL", journal)
        .env(
            "WEEKEND_PENDING_RESTORE_PREFIX",
            if case.prefix { "1" } else { "0" },
        )
        .env(
            "WEEKEND_PENDING_RESTORE_FORWARDED",
            if case.forwarded { "1" } else { "0" },
        )
        .env("WEEKEND_PENDING_RESTORE_PHASE", phase)
        .output()
        .unwrap()
}

fn fresh_observer(server: &Path, journal: &Path, case: Case, phase: &str) {
    let before = [fs::read(server).unwrap(), fs::read(journal).unwrap()];
    let output = run_observer(server, journal, case, phase);
    assert!(output.status.success(), "fresh observer: {output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|line| line == format!("test {TEST} ... ok"))
    );
    assert!(
        stdout
            .lines()
            .any(|line| { line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;") })
    );
    assert_eq!(fs::read(server).unwrap(), before[0]);
    assert_eq!(fs::read(journal).unwrap(), before[1]);
}

fn rejects_stale_observation(server: &Path, journal: &Path, case: Case) {
    let before = [fs::read(server).unwrap(), fs::read(journal).unwrap()];
    let output = run_observer(server, journal, case, "acknowledged");
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // libtest captures the observer's panic diagnostic in its stdout report.
    assert!(
        stdout.contains("coordinator revision mismatch")
            || stderr.contains("coordinator revision mismatch"),
        "unexpected observer rejection: {stdout} {stderr}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == format!("test {TEST} ... FAILED"))
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("test result: FAILED. 0 passed; 1 failed; 0 ignored;"))
    );
    assert_eq!(fs::read(server).unwrap(), before[0]);
    assert_eq!(fs::read(journal).unwrap(), before[1]);
}

fn rejected_check(journal: &Path, port: &str, key: &str) {
    let before = fs::read(journal).unwrap();
    let output = command()
        .args([
            "journal",
            "check",
            journal.to_str().unwrap(),
            port,
            "--secret-file",
            key,
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("behind 4 0")
    );
    assert_eq!(fs::read(journal).unwrap(), before);
}

#[test]
fn restored_pending_pairs_replay_once_and_preserve_retained_roots() {
    if let Some(server) = std::env::var_os(OBSERVE) {
        let journal = std::env::var_os("WEEKEND_PENDING_RESTORE_JOURNAL").unwrap();
        let case = Case {
            prefix: std::env::var("WEEKEND_PENDING_RESTORE_PREFIX").unwrap() == "1",
            forwarded: std::env::var("WEEKEND_PENDING_RESTORE_FORWARDED").unwrap() == "1",
        };
        let phase = std::env::var("WEEKEND_PENDING_RESTORE_PHASE").unwrap();
        assert!(["captured", "reply-lost", "acknowledged"].contains(&phase.as_str()));
        observe(Path::new(&server), Path::new(&journal), case, &phase);
        return;
    }
    let _exclusive = storage_workload();
    for prefix in [false, true] {
        for forwarded in [false, true] {
            exercise(Case { prefix, forwarded });
        }
    }
}

#[test]
fn selected_artifact_does_not_fall_back_to_cargo_binary() {
    let dir = Directory::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST])
        .env(BINARY, dir.0.join("missing-coordinator"))
        .env_remove(OBSERVE)
        .env("TMPDIR", &dir.0)
        .env("TMP", &dir.0)
        .env("TEMP", &dir.0)
        .output()
        .unwrap();
    assert!(!output.status.success(), "selected artifact was ignored");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stdout.contains("selected coordinator invocation")
            || stderr.contains("selected coordinator invocation"),
        "unexpected selected-artifact rejection: {stdout} {stderr}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == format!("test {TEST} ... FAILED"))
    );
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
}

fn exercise(case: Case) {
    let dir = Directory::new();
    let original = dir.0.join("original.db");
    let journal = dir.0.join("journal.db");
    let secret = dir.0.join("secret");
    let token = "b5".repeat(32);
    write_secret(&secret, &token);
    let key = secret.to_str().unwrap();
    cli(&["init-sequenced", original.to_str().unwrap()]);
    retained(&original);
    let stale_backup = dir.0.join("pre-intent.backup");
    let stale = dir.0.join("stale.db");
    storage::backup_file(&original, &stale_backup, Limits::default()).unwrap();
    storage::restore_file(&stale_backup, &stale, Limits::default()).unwrap();
    let stale_bytes = fs::read(&stale).unwrap();
    let stale_backup_bytes = fs::read(&stale_backup).unwrap();
    let (mut server, client) =
        configured_service("serve-sequenced", original.to_str().unwrap(), key, &token);
    let port = client.address.port().to_string();
    cli(&[
        "journal",
        "init",
        journal.to_str().unwrap(),
        &port,
        "0",
        "--secret-file",
        key,
    ]);
    if case.prefix {
        assert_eq!(
            cli(&[
                "journal",
                "set",
                journal.to_str().unwrap(),
                &port,
                "4",
                "42",
                "--secret-file",
                key
            ]),
            "committed 5\n"
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = listener.local_addr().unwrap().port().to_string();
    let expected = case.expected().to_string();
    let value = case.value().to_string();
    let mut writer = start(&[
        "journal",
        "set",
        journal.to_str().unwrap(),
        &proxy,
        &expected,
        &value,
        "--secret-file",
        key,
    ]);
    let (mut preflight, request) = accept_journal_request(&listener, &client, "journal-get");
    let before_value = if case.prefix { 42 } else { 0 };
    let reply = client.request(&request);
    assert_eq!(reply, format!("value {} {before_value}", case.expected()));
    writeln!(preflight, "{reply}").unwrap();
    drop(preflight);
    let (held, request) = accept_journal_request(&listener, &client, "journal-set");
    // Cut the client before forwarding: no app deadline depends on a slow
    // server flush. The forwarded branch commits after its requester is gone.
    stop_without_acknowledgment(&mut writer);
    if case.forwarded {
        assert_eq!(
            client.request(&request),
            format!("committed {}", case.expected() + 1)
        );
    }
    drop(held);
    server.stop();
    retained(&journal);
    fresh_observer(&original, &journal, case, "captured");
    let originals = [fs::read(&original).unwrap(), fs::read(&journal).unwrap()];
    let server_backup = dir.0.join("server.backup");
    let journal_backup = dir.0.join("journal.backup");
    let restored = dir.0.join("restored.db");
    let restored_journal = dir.0.join("restored-journal.db");
    for (source, backup, target) in [
        (&original, &server_backup, &restored),
        (&journal, &journal_backup, &restored_journal),
    ] {
        storage::backup_file(source, backup, Limits::default()).unwrap();
        storage::restore_file(backup, target, Limits::default()).unwrap();
        assert_eq!(fs::read(backup).unwrap(), fs::read(target).unwrap());
    }
    let backups = [
        fs::read(&server_backup).unwrap(),
        fs::read(&journal_backup).unwrap(),
    ];
    fresh_observer(&restored, &restored_journal, case, "captured");
    let pending_bytes = fs::read(&restored_journal).unwrap();
    let (mut stale_server, stale_client) =
        configured_service("serve-sequenced", stale.to_str().unwrap(), key, &token);
    let stale_port = stale_client.address.port().to_string();
    if case.prefix {
        rejected_check(&restored_journal, &stale_port, key);
    } else {
        assert_eq!(
            cli(&[
                "journal",
                "check",
                restored_journal.to_str().unwrap(),
                &stale_port,
                "--secret-file",
                key
            ]),
            "pending-eligible 4 0\n"
        );
    }
    // Eligibility for a first pending write cannot distinguish an older backup
    // from an authority that never executed it. Inspect only; never replay here.
    assert_eq!(stale_client.request("get"), "value 4 0");
    stale_server.stop();
    assert_eq!(fs::read(&stale).unwrap(), stale_bytes);
    assert_eq!(fs::read(&restored_journal).unwrap(), pending_bytes);
    let before = fs::read(&restored).unwrap();
    let (mut server, client) =
        configured_service("serve-sequenced", restored.to_str().unwrap(), key, &token);
    // Lose the first recovery reply too. The exact captured request is forwarded
    // after the resumed client stops; an existing receipt must be reused.
    let mut writer = start(&[
        "journal",
        "resume",
        restored_journal.to_str().unwrap(),
        &proxy,
        "--secret-file",
        key,
    ]);
    let (held, request) = accept_journal_request(&listener, &client, "journal-set");
    stop_without_acknowledgment(&mut writer);
    assert_eq!(
        client.request(&request),
        format!("committed {}", case.expected() + 1)
    );
    drop(held);
    assert_eq!(client.request("receipt 0"), case.receipt());
    server.stop();
    assert_eq!(fs::read(&restored_journal).unwrap(), pending_bytes);
    if case.forwarded {
        assert_eq!(fs::read(&restored).unwrap(), before);
    }
    fresh_observer(&restored, &restored_journal, case, "reply-lost");
    let before = fs::read(&restored).unwrap();
    let (mut server, client) =
        configured_service("serve-sequenced", restored.to_str().unwrap(), key, &token);
    let port = client.address.port().to_string();
    assert_eq!(
        cli(&[
            "journal",
            "check",
            restored_journal.to_str().unwrap(),
            &port,
            "--secret-file",
            key
        ]),
        format!(
            "pending-committed {} {}\n",
            case.expected() + 1,
            case.value()
        )
    );
    assert_eq!(
        cli(&[
            "journal",
            "resume",
            restored_journal.to_str().unwrap(),
            &port,
            "--secret-file",
            key
        ]),
        format!("committed {}\n", case.expected() + 1)
    );
    assert_eq!(
        client.request("get"),
        format!("value {} {}", case.expected() + 1, case.value())
    );
    assert_eq!(client.request("receipt 0"), case.receipt());
    server.stop();
    assert_eq!(fs::read(&restored).unwrap(), before);
    let acknowledged = fs::read(&restored_journal).unwrap();
    // The empty-pending result is only the journal's local acknowledgment. It
    // succeeds with the old endpoint stopped and must not mutate either image.
    assert_eq!(
        cli(&[
            "journal",
            "resume",
            restored_journal.to_str().unwrap(),
            &port,
            "--secret-file",
            key
        ]),
        format!("committed {}\n", case.expected() + 1)
    );
    assert_eq!(fs::read(&restored_journal).unwrap(), acknowledged);
    assert_eq!(fs::read(&restored).unwrap(), before);
    fresh_observer(&restored, &restored_journal, case, "acknowledged");
    let (mut stale_server, stale_client) =
        configured_service("serve-sequenced", stale.to_str().unwrap(), key, &token);
    rejected_check(
        &restored_journal,
        &stale_client.address.port().to_string(),
        key,
    );
    assert_eq!(stale_client.request("get"), "value 4 0");
    stale_server.stop();
    assert_eq!(fs::read(&stale).unwrap(), stale_bytes);
    rejects_stale_observation(&stale, &restored_journal, case);
    assert_eq!(fs::read(stale_backup).unwrap(), stale_backup_bytes);
    assert_eq!(fs::read(original).unwrap(), originals[0]);
    assert_eq!(fs::read(journal).unwrap(), originals[1]);
    assert_eq!(fs::read(server_backup).unwrap(), backups[0]);
    assert_eq!(fs::read(journal_backup).unwrap(), backups[1]);
    println!(
        "pending-restore prefix={} forwarded={} revision={} value={} receipt_sequence={} second_reply_lost=true local_replay_stable=true retained_roots=2 originals_preserved=true",
        u8::from(case.prefix),
        u8::from(case.forwarded),
        case.expected() + 1,
        case.value(),
        case.sequence()
    );
}
