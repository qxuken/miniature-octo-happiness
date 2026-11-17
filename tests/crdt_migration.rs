#![cfg(feature = "crdt")]
#[allow(dead_code)]
#[path = "../examples/crdt_policy_migration.rs"]
mod migration;
use migration::{NEW, OLD, OldProgress, ProgressV2, converge, exchange, review, stage_pair};
use std::io::{self, Cursor, Read, Write};
use weekend_db::{
    Database, Error, Limits,
    crdt::{GrowCounter, Policy, Record, Replica},
    storage::SimulatedDisk,
};

fn enroll_old<'a, 'b>(
    a: &'a Database,
    b: &'b Database,
) -> (Replica<'a, OldProgress>, Replica<'b, OldProgress>) {
    (
        Replica::enroll(a, a.create_collection().unwrap(), OLD, OLD.actors[0]).unwrap(),
        Replica::enroll(b, b.create_collection().unwrap(), OLD, OLD.actors[1]).unwrap(),
    )
}
fn revisions(a: &Database, b: &Database) -> (u64, u64) {
    (a.snapshot().revision(), b.snapshot().revision())
}

#[test]
fn reconciled_history_is_seeded_once_and_changed_policy_converges() {
    let a = Database::new().unwrap();
    let b = Database::new().unwrap();
    let (old_a, old_b) = enroll_old(&a, &b);
    old_a
        .update(|s, actor| {
            s.practice_minutes.increment(actor, 20)?;
            s.completed.insert(1)?;
            s.planned.insert(2, actor)?;
            s.planned.insert(5, actor)?;
            assert!(s.planned.remove(5)?);
            s.weekly_goal.assign(actor, 60)?;
            s.session_limit.assign(actor, 30)
        })
        .unwrap();
    old_b
        .update(|s, actor| {
            s.practice_minutes.increment(actor, 10)?;
            s.completed.insert(3)?;
            s.planned.insert(4, actor)?;
            s.weekly_goal.assign(actor, 90)?;
            s.session_limit.assign(actor, 20)
        })
        .unwrap();
    assert_eq!(
        review(old_a.read().unwrap(), old_b.read().unwrap()),
        Err(Error::Conflict)
    );
    converge(&old_a, &old_b).unwrap();
    let baseline = old_a.read().unwrap();
    let retained = a.retain_snapshot(&a.snapshot()).unwrap();
    let old_revisions = revisions(&a, &b);
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    let (new_a, new_b) = stage_pair(
        &x,
        &y,
        OLD,
        NEW,
        review(baseline, old_b.read().unwrap()).unwrap(),
    )
    .unwrap();
    let converted = new_a.read().unwrap();
    assert_eq!(converted, new_b.read().unwrap());
    assert_eq!(converted.practice_seconds.total(), 1800);
    assert_eq!(converted.completed.iter().collect::<Vec<_>>(), [1, 3]);
    assert_eq!(converted.planned.iter().collect::<Vec<_>>(), [2, 4]);
    assert!(!converted.planned.contains(5));
    assert_eq!(converted.weekly_goal.values().collect::<Vec<_>>(), [90]);
    assert_eq!(converted.session_limit.values().collect::<Vec<_>>(), [20]);
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    new_a
        .update(|s, a| {
            s.practice_seconds.increment(a, 60)?;
            s.weekly_goal.assign(a, 120)
        })
        .unwrap();
    new_b
        .update(|s, a| {
            s.practice_seconds.increment(a, 30)?;
            s.weekly_goal.assign(a, 45)
        })
        .unwrap();
    converge(&new_a, &new_b).unwrap();
    assert_eq!(new_a.read().unwrap(), new_b.read().unwrap());
    assert_eq!(new_a.read().unwrap().practice_seconds.total(), 1890);
    assert_eq!(
        new_a
            .read()
            .unwrap()
            .weekly_goal
            .values()
            .collect::<Vec<_>>(),
        [45]
    );
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    assert_eq!(revisions(&a, &b), old_revisions);
    assert_eq!(
        old_a
            .read_at(&a.retained_snapshot(retained).unwrap())
            .unwrap(),
        baseline
    );
}

struct Channel {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}
impl Read for Channel {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.input.read(b)
    }
}
impl Write for Channel {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn raw<P: Record>(db: &Database) -> Vec<u8> {
    db.snapshot()
        .get(db.collection_id(1), P::KEY)
        .unwrap()
        .unwrap()
        .as_bytes()
        .unwrap()
        .to_vec()
}

#[test]
fn obsolete_wire_generation_collection_and_actor_roster_reject_without_publication() {
    let db = Database::new().unwrap();
    let receiver =
        Replica::<ProgressV2>::enroll(&db, db.create_collection().unwrap(), NEW, NEW.actors[1])
            .unwrap();
    let old = Database::new().unwrap();
    Replica::<OldProgress>::enroll(&old, old.create_collection().unwrap(), OLD, OLD.actors[0])
        .unwrap();
    let mut cases = vec![(raw::<OldProgress>(&old), Error::UnsupportedFormat)];
    for (policy, error) in [
        (
            Policy {
                generation: OLD.generation,
                ..NEW
            },
            Error::Conflict,
        ),
        (
            Policy {
                collection: OLD.collection,
                ..NEW
            },
            Error::ForeignStore,
        ),
        (
            Policy {
                actors: OLD.actors,
                ..NEW
            },
            Error::Conflict,
        ),
    ] {
        let foreign = Database::new().unwrap();
        Replica::<ProgressV2>::enroll(
            &foreign,
            foreign.create_collection().unwrap(),
            policy,
            policy.actors[0],
        )
        .unwrap();
        cases.push((raw::<ProgressV2>(&foreign), error));
    }
    let before = (db.snapshot().revision(), raw::<ProgressV2>(&db));
    for (bytes, error) in cases {
        let mut channel = Channel {
            input: Cursor::new(bytes),
            output: Vec::new(),
        };
        assert_eq!(receiver.serve_authenticated(&mut channel), Err(error));
        assert!(channel.output.is_empty());
        assert_eq!((db.snapshot().revision(), raw::<ProgressV2>(&db)), before);
    }
    assert!(matches!(
        Replica::<ProgressV2>::open(&db, db.collection_id(1), NEW, OLD.actors[0]),
        Err(Error::ForeignStore)
    ));
    assert_eq!((db.snapshot().revision(), raw::<ProgressV2>(&db)), before);
}

#[test]
fn stale_backup_is_inspected_without_resuming_identity_and_recovery_starts_new_history() {
    let limits = Limits::default();
    let disk = SimulatedDisk::new().unwrap();
    let a = Database::create_on(disk.clone(), [21; 16], limits).unwrap();
    let b = Database::new().unwrap();
    let (old_a, old_b) = enroll_old(&a, &b);
    let backup = disk.durable_image().unwrap();
    old_a
        .update(|s, actor| {
            s.practice_minutes.increment(actor, 5)?;
            s.completed.insert(7)?;
            s.weekly_goal.assign(actor, 90)
        })
        .unwrap();
    converge(&old_a, &old_b).unwrap();
    // Actor A is now retired. The surviving peer also has an offline event.
    drop(a);
    old_b
        .update(|s, actor| s.practice_minutes.increment(actor, 3))
        .unwrap();
    let restored = Database::open_on(SimulatedDisk::from_image(&backup).unwrap(), limits).unwrap();
    let stale =
        Replica::<OldProgress>::open(&restored, restored.collection_id(1), OLD, OLD.actors[0])
            .unwrap();
    // Opening alone cannot identify rollback. Test the old sync path, without
    // making any new local edit under the restored actor's identity.
    let before = revisions(&restored, &b);
    let (ack, served) = exchange(&stale, &old_b);
    assert_eq!(served, Ok(()));
    assert_eq!(ack, Err(Error::Conflict));
    assert_eq!(before, revisions(&restored, &b));
    assert_eq!(stale.read().unwrap().practice_minutes.total(), 0);
    let survivor = old_b.read().unwrap();
    let mut reviewed = survivor;
    reviewed.merge(stale.read().unwrap());
    assert_eq!(reviewed, survivor); // survivor covers every event in this backup
    let seed = review(reviewed, reviewed).unwrap();
    let da = SimulatedDisk::new().unwrap();
    let db = SimulatedDisk::new().unwrap();
    let x = Database::create_on(da.clone(), [31; 16], limits).unwrap();
    let y = Database::create_on(db.clone(), [32; 16], limits).unwrap();
    let (new_a, new_b) = stage_pair(&x, &y, OLD, NEW, seed).unwrap();
    assert_eq!(new_a.read().unwrap().practice_seconds.total(), 480);
    assert!(new_a.read().unwrap().completed.contains(7));
    assert_eq!(new_a.read().unwrap(), new_b.read().unwrap());
    drop(x);
    drop(y);
    let x = Database::open_on(da, limits).unwrap();
    let y = Database::open_on(db, limits).unwrap();
    let new_a = Replica::<ProgressV2>::open(&x, x.collection_id(1), NEW, NEW.actors[0]).unwrap();
    let new_b = Replica::<ProgressV2>::open(&y, y.collection_id(1), NEW, NEW.actors[1]).unwrap();
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    new_b
        .update(|s, a| s.practice_seconds.increment(a, 60))
        .unwrap();
    converge(&new_a, &new_b).unwrap();
    assert_eq!(new_a.read().unwrap().practice_seconds.total(), 540);
    let stable = revisions(&x, &y);
    converge(&new_a, &new_b).unwrap();
    assert_eq!(stable, revisions(&x, &y));
    assert_eq!(before, revisions(&restored, &b));
}

#[test]
fn migration_rejects_identity_reuse_and_unrepresentable_values_before_staging() {
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    let seed = review(OldProgress::default(), OldProgress::default()).unwrap();
    let before = revisions(&x, &y);
    for policy in [
        OLD,
        Policy {
            actors: OLD.actors,
            ..NEW
        },
        Policy {
            generation: OLD.generation,
            ..NEW
        },
        Policy {
            collection: OLD.collection,
            ..NEW
        },
    ] {
        assert!(matches!(
            stage_pair(&x, &y, OLD, policy, seed),
            Err(Error::Invalid)
        ));
        assert_eq!(before, revisions(&x, &y));
    }
    for (minutes, accepted) in [
        (u64::MAX / 2 / 60, true),
        (u64::MAX / 2 / 60 + 1, false),
        (u64::MAX / 2, false),
    ] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&minutes.to_le_bytes());
        let old = OldProgress {
            practice_minutes: GrowCounter::from_le_bytes(bytes).unwrap(),
            ..Default::default()
        };
        assert_eq!(review(old, old).is_ok(), accepted);
    }
    assert_eq!(before, revisions(&x, &y));
}

#[cfg(feature = "fault-injection")]
#[test]
fn failed_staging_leaves_old_pair_usable_and_requires_discarding_partial_targets() {
    let a = Database::new().unwrap();
    let b = Database::new().unwrap();
    let (old_a, old_b) = enroll_old(&a, &b);
    old_a
        .update(|s, actor| s.practice_minutes.increment(actor, 2))
        .unwrap();
    converge(&old_a, &old_b).unwrap();
    let payload = old_a.read().unwrap();
    let reviewed = review(payload, old_b.read().unwrap()).unwrap();
    let old_revisions = revisions(&a, &b);
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    y.fail_allocations_after(Some(0));
    assert!(matches!(
        stage_pair(&x, &y, OLD, NEW, reviewed),
        Err(Error::OutOfMemory)
    ));
    assert!(x.snapshot().revision() > 0); // cross-file staging is not atomic
    assert_eq!(revisions(&a, &b), old_revisions);
    assert_eq!(old_a.read().unwrap(), payload);
    drop(x);
    drop(y);
    let x = Database::new().unwrap();
    let y = Database::new().unwrap();
    let retry_policy = Policy {
        collection: [11; 16],
        generation: 9,
        actors: [[5; 16], [6; 16]],
    };
    let (left, right) = stage_pair(&x, &y, OLD, retry_policy, reviewed).unwrap();
    assert_eq!(left.read().unwrap().practice_seconds.total(), 120);
    assert_eq!(left.read().unwrap(), right.read().unwrap());
    assert_eq!(revisions(&a, &b), old_revisions);
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod native_files {
    use super::*;
    use std::{
        fs,
        io::{BufRead, BufReader},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc::{self, Receiver},
        },
        time::{Duration, Instant},
    };
    use weekend_db::Result;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "crdt-migration-process-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct Process {
        child: Child,
        lines: Receiver<String>,
    }
    impl Process {
        fn start(mode: &str, root: &Path, stage: &Path, attempt: u8, cut: &str) -> Self {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "native_files::worker",
                    "--ignored",
                    "--nocapture",
                    "--quiet",
                ])
                .env("MIGRATION_MODE", mode)
                .env("MIGRATION_ROOT", root)
                .env("MIGRATION_STAGE", stage)
                .env("MIGRATION_ATTEMPT", attempt.to_string())
                .env("MIGRATION_CUT", cut)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (send, lines) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout)
                    .lines()
                    .map_while(std::result::Result::ok)
                {
                    if send.send(line).is_err() {
                        break;
                    }
                }
            });
            Self { child, lines }
        }
        fn marker(&self, expected: &str) {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let line = self
                    .lines
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("worker did not reach boundary");
                if line.starts_with("MIGRATION ") {
                    assert_eq!(line, expected);
                    break;
                }
            }
        }
        fn wait(&mut self) -> std::process::ExitStatus {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return status;
                }
                assert!(Instant::now() < deadline, "worker did not exit");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn done(mut self) {
            self.marker("MIGRATION DONE");
            assert!(self.wait().success());
        }
    }
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
    fn policy(attempt: u8) -> Policy {
        Policy {
            collection: [10 + attempt; 16],
            generation: 8 + u64::from(attempt),
            actors: [[3 + 2 * attempt; 16], [4 + 2 * attempt; 16]],
        }
    }
    fn old_path(root: &Path, index: usize) -> PathBuf {
        root.join(format!("old-{index}.wdb"))
    }
    fn new_path(stage: &Path, index: usize) -> PathBuf {
        stage.join(format!("new-{index}.wdb"))
    }
    fn mark_cut(name: &str) {
        if std::env::var("MIGRATION_CUT").unwrap() == name {
            println!("MIGRATION CUT {name}");
            io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
    }
    fn assert_old(state: OldProgress) {
        assert_eq!(state.practice_minutes.total(), 30);
        assert_eq!(state.completed.iter().collect::<Vec<_>>(), [1, 3]);
        assert_eq!(state.planned.iter().collect::<Vec<_>>(), [2, 4]);
        assert_eq!(state.weekly_goal.values().collect::<Vec<_>>(), [90]);
        assert_eq!(state.session_limit.values().collect::<Vec<_>>(), [20]);
    }
    fn verify_old(db: &Database, index: usize) -> OldProgress {
        let replica =
            Replica::<OldProgress>::open(db, db.collection_id(1), OLD, OLD.actors[index]).unwrap();
        let state = replica.read().unwrap();
        assert_old(state);
        assert_eq!(db.retained_ids().unwrap(), [1, 2]);
        assert_eq!(
            replica.read_at(&db.retained_snapshot(1).unwrap()).unwrap(),
            OldProgress::default()
        );
        assert_eq!(
            replica.read_at(&db.retained_snapshot(2).unwrap()).unwrap(),
            state
        );
        state
    }
    fn initialize(root: &Path) -> Result<()> {
        let a = Database::create_file(old_path(root, 0), Limits::default())?;
        let b = Database::create_file(old_path(root, 1), Limits::default())?;
        let (left, right) = enroll_old(&a, &b);
        a.retain_snapshot(&a.snapshot())?;
        b.retain_snapshot(&b.snapshot())?;
        left.update(|s, actor| {
            s.practice_minutes.increment(actor, 20)?;
            s.completed.insert(1)?;
            s.planned.insert(2, actor)?;
            s.weekly_goal.assign(actor, 60)?;
            s.session_limit.assign(actor, 30)
        })?;
        right.update(|s, actor| {
            s.practice_minutes.increment(actor, 10)?;
            s.completed.insert(3)?;
            s.planned.insert(4, actor)?;
            s.weekly_goal.assign(actor, 90)?;
            s.session_limit.assign(actor, 20)
        })?;
        converge(&left, &right)?;
        a.retain_snapshot(&a.snapshot())?;
        b.retain_snapshot(&b.snapshot())?;
        verify_old(&a, 0);
        verify_old(&b, 1);
        Ok(())
    }
    fn stage(root: &Path, target: &Path, attempt: u8) -> Result<()> {
        // Hold both native writer leases throughout read/review/staging. This
        // prevents another native writer, not an independently running old peer.
        let a = Database::open_file(old_path(root, 0), Limits::default())?;
        let b = Database::open_file(old_path(root, 1), Limits::default())?;
        let seed = review(verify_old(&a, 0), verify_old(&b, 1))?;
        mark_cut("leased");
        fs::create_dir(target).map_err(|_| Error::Io)?;
        let x = Database::create_file(new_path(target, 0), Limits::default())?;
        mark_cut("first_created");
        let y = Database::create_file(new_path(target, 1), Limits::default())?;
        mark_cut("both_created");
        let (left, right) = stage_pair(&x, &y, OLD, policy(attempt), seed)?;
        assert_eq!(left.read()?, right.read()?);
        mark_cut("staged");
        x.retain_snapshot(&x.snapshot())?;
        mark_cut("first_pin");
        y.retain_snapshot(&y.snapshot())?;
        mark_cut("ready");
        Ok(())
    }
    fn verify_new(stage: &Path, index: usize, attempt: u8, pinned: bool) -> Result<()> {
        let p = policy(attempt);
        let db = Database::open_file(new_path(stage, index), Limits::default())?;
        let r = Replica::<ProgressV2>::open(&db, db.collection_id(1), p, p.actors[index])?;
        let state = r.read()?;
        assert_eq!(state.practice_seconds.total(), 1800);
        assert_eq!(state.completed.iter().collect::<Vec<_>>(), [1, 3]);
        assert_eq!(state.planned.iter().collect::<Vec<_>>(), [2, 4]);
        assert_eq!(state.weekly_goal.values().collect::<Vec<_>>(), [90]);
        assert_eq!(state.session_limit.values().collect::<Vec<_>>(), [20]);
        assert_eq!(db.snapshot().revision(), if pinned { 4 } else { 3 });
        if pinned {
            assert_eq!(db.retained_snapshot(1)?.revision(), 3);
            assert_eq!(db.retained_ids()?, [1]);
            assert_eq!(r.read_at(&db.retained_snapshot(1)?)?, state);
        } else {
            assert!(db.retained_ids()?.is_empty());
        }
        assert!(matches!(
            Replica::<ProgressV2>::open(&db, db.collection_id(1), p, OLD.actors[index]),
            Err(Error::ForeignStore)
        ));
        Ok(())
    }

    // Fixture entrypoint in this integration-test executable. Parent children
    // inherit a bounded wait/kill/reap guard. No public migration CLI is added.
    #[test]
    #[ignore = "invoked only by the bounded parent process tests"]
    fn worker() {
        let Ok(mode) = std::env::var("MIGRATION_MODE") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("MIGRATION_ROOT").unwrap());
        let target = PathBuf::from(std::env::var_os("MIGRATION_STAGE").unwrap());
        let attempt = std::env::var("MIGRATION_ATTEMPT").unwrap().parse().unwrap();
        match mode.as_str() {
            "init" => initialize(&root).unwrap(),
            "stage" => stage(&root, &target, attempt).unwrap(),
            "busy" => assert_eq!(stage(&root, &target, attempt), Err(Error::Busy)),
            "new0" => verify_new(&target, 0, attempt, true).unwrap(),
            "new1" => verify_new(&target, 1, attempt, true).unwrap(),
            "staged0" => verify_new(&target, 0, attempt, false).unwrap(),
            "staged1" => verify_new(&target, 1, attempt, false).unwrap(),
            "empty0" | "empty1" => {
                let index = usize::from(mode == "empty1");
                let db = Database::open_file(new_path(&target, index), Limits::default()).unwrap();
                assert_eq!(db.snapshot().revision(), 0);
                assert!(db.retained_ids().unwrap().is_empty());
            }
            "old0" | "old1" => {
                let index = usize::from(mode == "old1");
                let db = Database::open_file(old_path(&root, index), Limits::default()).unwrap();
                verify_old(&db, index);
            }
            "repeat" => {
                let x = Database::open_file(new_path(&target, 0), Limits::default()).unwrap();
                let y = Database::open_file(new_path(&target, 1), Limits::default()).unwrap();
                let p = policy(attempt);
                let left =
                    Replica::<ProgressV2>::open(&x, x.collection_id(1), p, p.actors[0]).unwrap();
                let right =
                    Replica::<ProgressV2>::open(&y, y.collection_id(1), p, p.actors[1]).unwrap();
                let before = revisions(&x, &y);
                converge(&left, &right).unwrap();
                assert_eq!(before, revisions(&x, &y));
            }
            _ => panic!("unknown worker mode"),
        }
        println!("MIGRATION DONE");
        io::stdout().flush().unwrap();
    }

    // Compare every encoded record and revision in the old current/retained
    // roots, rather than relying on a digest or only on converted totals.
    #[derive(Debug, PartialEq, Eq)]
    struct OldImage {
        revision: u64,
        roots: Vec<(u64, Vec<u8>)>,
    }
    fn old_oracle(root: &Path) -> Vec<OldImage> {
        (0..2)
            .map(|index| {
                let db = Database::open_file(old_path(root, index), Limits::default()).unwrap();
                verify_old(&db, index);
                let mut rows = vec![(db.snapshot().revision(), raw::<OldProgress>(&db))];
                for id in db.retained_ids().unwrap() {
                    let s = db.retained_snapshot(id).unwrap();
                    rows.push((
                        s.revision(),
                        s.get(db.collection_id(1), OldProgress::<true>::KEY)
                            .unwrap()
                            .unwrap()
                            .as_bytes()
                            .unwrap()
                            .to_vec(),
                    ));
                }
                OldImage {
                    revision: db.snapshot().revision(),
                    roots: rows,
                }
            })
            .collect()
    }
    fn run(mode: &str, root: &Path, target: &Path, attempt: u8) {
        Process::start(mode, root, target, attempt, "").done();
    }
    fn verify_pair(root: &Path, target: &Path, attempt: u8) {
        for mode in ["new0", "new1", "old0", "old1", "repeat"] {
            run(mode, root, target, attempt);
        }
    }

    #[test]
    fn file_migration_requires_old_leases_and_reopens_each_destination_in_a_new_process() {
        let dir = Directory::new();
        let target = dir.0.join("stage");
        run("init", &dir.0, &target, 0);
        let before = old_oracle(&dir.0);
        for index in 0..2 {
            let held = Database::open_file(old_path(&dir.0, index), Limits::default()).unwrap();
            run("busy", &dir.0, &target, 0);
            assert!(!target.exists());
            drop(held);
        }
        run("stage", &dir.0, &target, 0);
        verify_pair(&dir.0, &target, 0);
        assert_eq!(old_oracle(&dir.0), before);
    }

    #[test]
    fn interrupted_native_staging_is_discarded_and_fresh_retry_preserves_old_history() {
        let dir = Directory::new();
        run("init", &dir.0, &dir.0.join("unused"), 0);
        let before = old_oracle(&dir.0);
        for (index, cut) in [
            "leased",
            "first_created",
            "both_created",
            "staged",
            "first_pin",
            "ready",
        ]
        .into_iter()
        .enumerate()
        {
            let attempt = 1 + index as u8 * 2;
            let target = dir.0.join(format!("partial-{attempt}"));
            let child = Process::start("stage", &dir.0, &target, attempt, cut);
            // While parked, the worker still owns BOTH old-store leases.
            child.marker(&format!("MIGRATION CUT {cut}"));
            for old in 0..2 {
                assert!(matches!(
                    Database::open_file(old_path(&dir.0, old), Limits::default()),
                    Err(Error::Busy)
                ));
            }
            let mut child = child;
            child.child.kill().unwrap();
            assert!(!child.wait().success());
            assert_eq!(old_oracle(&dir.0), before);
            let checks: &[&str] = match cut {
                "leased" => {
                    assert!(!target.exists());
                    &[]
                }
                "first_created" => {
                    assert!(!new_path(&target, 1).exists());
                    &["empty0"]
                }
                "both_created" => &["empty0", "empty1"],
                "staged" => &["staged0", "staged1"],
                "first_pin" => &["new0", "staged1"],
                "ready" => &["new0", "new1"],
                _ => unreachable!(),
            };
            for mode in checks {
                run(mode, &dir.0, &target, attempt);
            }
            if target.exists() {
                fs::remove_dir_all(&target).unwrap();
            }
            let retry = dir.0.join(format!("retry-{attempt}"));
            run("stage", &dir.0, &retry, attempt + 1);
            verify_pair(&dir.0, &retry, attempt + 1);
            assert_eq!(old_oracle(&dir.0), before);
        }
    }
}
