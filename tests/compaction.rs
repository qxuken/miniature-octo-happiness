#[path = "support/allocator.rs"]
pub mod allocator;
#[cfg(all(
    feature = "fault-injection",
    any(
        target_os = "macos",
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    )
))]
#[path = "support/descriptor_limit.rs"]
mod descriptor_limit;
use allocator::TestAllocator;
use std::sync::Arc;
use weekend_db as allocator_engine;
mod support;
#[path = "../examples/support/users.rs"]
mod users;
use support::Model;
use weekend_db::{
    Database, Error, Limits,
    storage::{APPEND_START, ByteStore, Crash, Fault, IoKind, MemoryStore, REGION, SimulatedDisk},
};
const ID: [u8; 16] = *b"compaction-test1";
fn fixture() -> Vec<u8> {
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let shared = db
        .list()
        .unwrap()
        .list_push(&db.bytes(b"shared\0payload").unwrap())
        .unwrap();
    let value = db
        .map()
        .unwrap()
        .map_set(b"shared", &shared)
        .unwrap()
        .map_set(b"n", &db.unsigned(0).unwrap())
        .unwrap();
    let mut tx = db.begin();
    for key in [b"a", b"b"] {
        tx.put(c, key, &value).unwrap();
    }
    tx.commit().unwrap();
    drop(tx);
    let old = db.snapshot();
    db.retain_snapshot(&old).unwrap();
    db.retain_snapshot(&old).unwrap();
    drop(old);
    db.release_retained(1).unwrap();
    let discarded = db.create_collection().unwrap();
    db.drop_collection(discarded).unwrap();
    for n in 1..=20 {
        let mut tx = db.begin();
        tx.put(
            c,
            b"a",
            &value.map_set(b"n", &db.unsigned(n).unwrap()).unwrap(),
        )
        .unwrap();
        tx.commit().unwrap();
        drop(tx);
        if n == 10 {
            assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 3);
        }
    }
    db.release_retained(3).unwrap();
    let current = db.snapshot();
    assert_eq!(db.retain_snapshot(&current).unwrap(), 4);
    assert_eq!(db.retain_snapshot(&current).unwrap(), 5);
    disk.durable_image().unwrap()
}
fn rows(s: &weekend_db::Snapshot<'_>, db: &Database) -> Vec<(Vec<u8>, Model)> {
    s.scan(db.collection_id(1))
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), Model::read(v)))
        .collect()
}
fn check(db: &Database, original: &Database) {
    assert_eq!(db.snapshot().revision(), original.snapshot().revision());
    assert_eq!(
        db.snapshot().data_revision(),
        original.snapshot().data_revision()
    );
    assert_eq!(
        rows(&db.snapshot(), db),
        rows(&original.snapshot(), original)
    );
    assert_eq!(db.retained_ids().unwrap(), vec![2, 4, 5]);
    for id in [2, 4, 5] {
        let (a, b) = (
            db.retained_snapshot(id).unwrap(),
            original.retained_snapshot(id).unwrap(),
        );
        assert_eq!(a.revision(), b.revision());
        assert_eq!(a.data_revision(), b.data_revision());
        assert_eq!(rows(&a, db), rows(&b, original));
    }
    assert_eq!(db.retained_snapshot(3).err(), Some(Error::NotFound));
}
#[test]
fn compact_copy_preserves_roots_counters_and_owned_values() {
    let mut image = fixture();
    image.extend_from_slice(b"abandoned unaligned append tail");
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let mut source = Database::open_on(disk.clone(), Limits::default()).unwrap();
    let owned = source
        .snapshot()
        .get(source.collection_id(1), b"a")
        .unwrap()
        .unwrap();
    let target = SimulatedDisk::new().unwrap();
    let before = disk.durable_image().unwrap();
    let stats = source.compact_to(target.clone()).unwrap();
    assert_eq!(stats.before_bytes as usize, before.len());
    assert_eq!(
        stats.after_bytes as usize,
        target.durable_image().unwrap().len()
    );
    assert!(stats.after_bytes < stats.before_bytes / 2);
    assert_eq!(disk.durable_image().unwrap(), before);
    target.crash(Crash::DropAll).unwrap();
    let db = Database::open_on(target.clone(), Limits::default()).unwrap();
    check(&db, &source);
    assert_eq!(db.create_collection().unwrap().as_u64(), 3);
    assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 6);
    let mut tx = source.begin();
    tx.delete(source.collection_id(1), b"a").unwrap();
    tx.commit().unwrap();
    drop(tx);
    assert!(
        source
            .snapshot()
            .get(source.collection_id(1), b"a")
            .unwrap()
            .is_none()
    );
    assert!(
        db.snapshot()
            .get(db.collection_id(1), b"a")
            .unwrap()
            .is_some()
    );
    drop(source);
    assert_eq!(owned.map_get(b"n").unwrap().unwrap().as_u64(), Ok(20));
}
#[test]
fn compaction_rejects_nonempty_or_leased_destinations_and_native_sources() {
    let mut source = Database::open_on(
        SimulatedDisk::from_image(&fixture()).unwrap(),
        Limits::default(),
    )
    .unwrap();
    let target = SimulatedDisk::from_image(b"untouched").unwrap();
    assert_eq!(source.compact_to(target.clone()), Err(Error::Invalid));
    assert_eq!(target.durable_image().unwrap(), b"untouched");
    let store = MemoryStore::new().unwrap();
    let _leased = Database::create_on(store.clone(), ID, Limits::default()).unwrap();
    assert_eq!(source.compact_to(store), Err(Error::Busy));
    let mut native = Database::new().unwrap();
    assert_eq!(
        native.compact_to(MemoryStore::new().unwrap()),
        Err(Error::Invalid)
    );
}

#[test]
fn compact_images_start_at_any_revision_parity_including_empty_and_metadata_only() {
    for commits in 0..6 {
        let store = MemoryStore::new().unwrap();
        let mut db = Database::create_on(store, ID, Limits::default()).unwrap();
        for _ in 0..commits {
            db.begin().commit().unwrap();
        }
        let target = MemoryStore::new().unwrap();
        db.compact_to(target.clone()).unwrap();
        let copy = Database::open_on(target, Limits::default()).unwrap();
        assert_eq!(copy.snapshot().revision(), commits);
        assert_eq!(copy.snapshot().data_revision(), 0);
        assert_eq!(copy.create_collection().unwrap().as_u64(), 1);
        assert_eq!(copy.snapshot().revision(), commits + 1);
    }
}

#[test]
fn compact_copy_requires_the_selected_roots_witness() {
    // Model a device silently losing the witness while reporting successful
    // writes and syncs. The full graph and root slot remain readable.
    struct LoseWitness(SimulatedDisk);
    impl ByteStore for LoseWitness {
        fn lock(&mut self) -> weekend_db::Result<()> {
            self.0.lock()
        }
        fn unlock(&mut self) {
            self.0.unlock();
        }
        fn length(&mut self) -> weekend_db::Result<u64> {
            self.0.length()
        }
        fn read(&mut self, offset: u64, bytes: &mut [u8]) -> weekend_db::Result<usize> {
            self.0.read(offset, bytes)
        }
        fn write(&mut self, offset: u64, bytes: &[u8]) -> weekend_db::Result<usize> {
            if bytes.starts_with(b"WDBACK01") {
                self.0.write(offset, &[0; REGION][..bytes.len()])
            } else {
                self.0.write(offset, bytes)
            }
        }
        fn sync(&mut self) -> weekend_db::Result<()> {
            self.0.sync()
        }
    }
    let disk = SimulatedDisk::new().unwrap();
    let mut db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
    db.create_collection().unwrap();
    let revision = db.snapshot().revision();
    let before = disk.durable_image().unwrap();
    let target = SimulatedDisk::new().unwrap();
    assert_eq!(
        db.compact_to(LoseWitness(target.clone())),
        Err(Error::Corrupt)
    );
    assert_eq!(disk.durable_image().unwrap(), before);
    let reopened = Database::open_on(target.clone(), Limits::default()).unwrap();
    assert_eq!(reopened.snapshot().revision(), revision);
    let witness = (3 + (revision % 2) as usize) * REGION;
    assert_eq!(
        &target.durable_image().unwrap()[witness..witness + 8],
        b"WDBACK01"
    );
}

#[test]
fn all_retained_slots_and_dropped_collections_survive_compaction() {
    let mut db = Database::create_on(MemoryStore::new().unwrap(), ID, Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"kept", &db.unsigned(42).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    for _ in 0..weekend_db::MAX_RETAINED_SNAPSHOTS {
        db.retain_snapshot(&db.snapshot()).unwrap();
    }
    db.drop_collection(c).unwrap();
    let target = MemoryStore::new().unwrap();
    db.compact_to(target.clone()).unwrap();
    let db = Database::open_on(target, Limits::default()).unwrap();
    let c = db.collection_id(c.as_u64());
    assert_eq!(db.snapshot().scan(c), Err(Error::NotFound));
    for id in db.retained_ids().unwrap() {
        assert_eq!(
            db.retained_snapshot(id)
                .unwrap()
                .get(c, b"kept")
                .unwrap()
                .unwrap()
                .as_u64(),
            Ok(42)
        );
    }
    db.release_retained(1).unwrap();
    assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 65);
    assert_eq!(db.create_collection().unwrap().as_u64(), 2);
}
#[test]
fn copy_faults_at_every_io_leave_source_unchanged_and_only_whole_copies_recover() {
    let image = fixture();
    let mut source = Database::open_on(
        SimulatedDisk::from_image(&image).unwrap(),
        Limits::default(),
    )
    .unwrap();
    let target = SimulatedDisk::new().unwrap();
    source.compact_to(target.clone()).unwrap();
    let trace = target.trace().unwrap();
    let compacted = target.durable_image().unwrap();
    let [append_sync, root_sync, witness_sync]: [usize; 3] = trace
        .iter()
        .enumerate()
        .filter_map(|(i, event)| (event.kind == IoKind::Sync).then_some(i))
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    assert_eq!(trace[witness_sync + 1].kind, IoKind::Length);
    assert!(
        trace[witness_sync + 2..]
            .iter()
            .all(|event| event.kind == IoKind::Read)
    );
    for (step, event) in trace.iter().enumerate() {
        let faults = match event.kind {
            IoKind::Write => vec![
                Fault::Error,
                Fault::Short(1),
                Fault::Tear {
                    prefix: 1,
                    durable: true,
                },
            ],
            IoKind::Sync => vec![
                Fault::Error,
                Fault::SyncError(Crash::DropAll),
                Fault::SyncError(Crash::PersistAll),
            ],
            _ => vec![Fault::Error, Fault::Short(1)],
        };
        for fault in faults {
            for crash in [Crash::DropAll, Crash::PersistAll] {
                let disk = SimulatedDisk::from_image(&image).unwrap();
                let mut source = Database::open_on(disk.clone(), Limits::default()).unwrap();
                disk.arm(usize::MAX, Fault::Error);
                let target = SimulatedDisk::new().unwrap();
                target.arm(step, fault);
                let result = source.compact_to(target.clone());
                let expected = if matches!(fault, Fault::Short(1))
                    && matches!(event.kind, IoKind::Read | IoKind::Write)
                {
                    None
                } else if step <= append_sync || step > witness_sync {
                    Some(Error::Io)
                } else {
                    Some(Error::OutcomeUnknown)
                };
                assert_eq!(result.err(), expected, "step={step}, fault={fault:?}");
                if expected.is_some() {
                    assert_eq!(target.trace().unwrap(), trace[..=step]);
                }
                assert_eq!(disk.durable_image().unwrap(), image);
                assert_eq!(
                    disk.trace()
                        .unwrap()
                        .iter()
                        .map(|e| e.kind)
                        .collect::<Vec<_>>(),
                    [IoKind::Length]
                );
                assert!(!source.requires_recovery());
                let mut probe = target.clone();
                probe.lock().expect("copy released the destination lease");
                probe.unlock();
                target.clear_fault();
                target.crash(crash).unwrap();

                let reopened = Database::open_on(target.clone(), Limits::default());
                let root_survives = expected.is_none()
                    || step > root_sync
                    || step == root_sync
                        && matches!(
                            (crash, fault),
                            (Crash::PersistAll, _) | (_, Fault::SyncError(Crash::PersistAll))
                        );
                if root_survives {
                    check(&reopened.unwrap(), &source);
                    assert_eq!(target.durable_image().unwrap(), compacted);
                } else {
                    // A durable append prefix can extend the sparse image past
                    // the minimum length while the header is still volatile.
                    let error = if step < append_sync
                        && event.offset >= APPEND_START as u64
                        && matches!((fault, crash), (Fault::Tear { .. }, Crash::DropAll))
                    {
                        Error::UnsupportedFormat
                    } else {
                        Error::Corrupt
                    };
                    assert_eq!(
                        reopened.err(),
                        Some(error),
                        "step={step}, fault={fault:?}, crash={crash:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn compact_source_length_failure_stops_before_destination_and_allows_retry() {
    let image = fixture();
    let disk = SimulatedDisk::from_image(&image).unwrap();
    let mut source = Database::open_on(disk.clone(), Limits::default()).unwrap();
    let target = SimulatedDisk::new().unwrap();
    disk.arm(0, Fault::Error);
    target.arm(usize::MAX, Fault::Error);
    assert_eq!(source.compact_to(target.clone()), Err(Error::Io));
    assert_eq!(
        disk.trace()
            .unwrap()
            .iter()
            .map(|e| e.kind)
            .collect::<Vec<_>>(),
        [IoKind::Length]
    );
    assert!(target.trace().unwrap().is_empty());
    assert!(target.durable_image().unwrap().is_empty());
    assert_eq!(disk.durable_image().unwrap(), image);
    assert!(!source.requires_recovery());
    disk.clear_fault();
    source.compact_to(target.clone()).unwrap();
    check(
        &Database::open_on(target, Limits::default()).unwrap(),
        &source,
    );
}
#[test]
fn allocation_failures_during_copy_and_verification_never_change_the_source() {
    let image = fixture();
    let mut success = false;
    for budget in 0..1024 {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let allocator = Arc::new(TestAllocator::default());
        let mut source = Database::open_on_with_options(disk.clone(), allocator.options()).unwrap();
        let before = disk.durable_image().unwrap();
        let target = SimulatedDisk::new().unwrap();
        let live = allocator.live_bytes();
        let result = allocator.with_refusal(budget, || source.compact_to(target.clone()));
        assert_eq!(disk.durable_image().unwrap(), before);
        if result.is_ok() {
            assert!(budget > 0);
            check(
                &Database::open_on(target, Limits::default()).unwrap(),
                &source,
            );
            success = true;
            break;
        }
        assert_eq!(result, Err(Error::OutOfMemory), "budget={budget}");
        assert_eq!(allocator.live_bytes(), live);
    }
    assert!(success);
}
#[test]
fn typed_schemas_unknown_fields_and_secondary_lookups_survive_compaction() {
    use users::{Age, NicknameUpdate, Users};
    let store = MemoryStore::new().unwrap();
    let mut db = Database::create_on(store, ID, Limits::default()).unwrap();
    let users = Users::create(&db).unwrap();
    let ids = users.ids();
    let mut tx = db.begin();
    users.install_v1(&db, &mut tx).unwrap();
    users
        .insert_v1(&db, &mut tx, 42, "Alice", "old@example.test", 25)
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    users.migrate_to_v2(&db, &mut tx).unwrap();
    users
        .set_nickname(&db, &mut tx, 42, NicknameUpdate::Text("Al"))
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    let mut record = weekend_db::schema::Record::from_value(
        db.snapshot()
            .get(users.records, &42u64.to_le_bytes())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    record = record.set(777, &db.bytes(b"unknown").unwrap()).unwrap();
    let mut tx = db.begin();
    tx.put(users.records, &42u64.to_le_bytes(), record.as_value())
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    let mut tx = db.begin();
    users.migrate_to_v3(&db, &mut tx).unwrap();
    users
        .set_email(&db, &mut tx, 42, "new@example.test")
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    let copy = MemoryStore::new().unwrap();
    db.compact_to(copy.clone()).unwrap();
    drop(db);
    let db = Database::open_on(copy, Limits::default()).unwrap();
    let users = Users::from_ids(&db, ids);
    for (id, version) in [(1, 1), (2, 2)] {
        let user = users
            .find_by_email(&mut db.retained_snapshot(id).unwrap(), "old@example.test")
            .unwrap()
            .unwrap();
        assert_eq!(user.version(), version);
        assert_eq!(user.age(), Age::Years(25));
    }
    let user = users
        .find_by_email(&mut db.snapshot(), "new@example.test")
        .unwrap()
        .unwrap();
    assert_eq!(user.version(), 3);
    assert_eq!(user.age(), Age::Months(300));
    let record = weekend_db::schema::Record::from_value(
        db.snapshot()
            .get(users.records, &42u64.to_le_bytes())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        record.field(777).unwrap().unwrap().as_bytes(),
        Ok(b"unknown".as_slice())
    );
}

#[cfg(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]
mod files {
    use super::*;
    #[cfg(feature = "fault-injection")]
    use std::sync::{Arc, Mutex};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "weekend-compact-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn seed(path: &Path) {
        fs::write(path, fixture()).unwrap();
    }
    #[test]
    fn admin_compaction_respects_the_lease_and_publishes_the_same_bytes_as_the_api() {
        let dir = Directory::new();
        let path = dir.path("admin space λ.db");
        let reference = dir.path("reference.db");
        seed(&path);
        fs::copy(&path, &reference).unwrap();
        let before = fs::read(&path).unwrap();
        let compact = || {
            std::process::Command::new(env!("CARGO_BIN_EXE_weekend-admin"))
                .arg("compact")
                .arg(&path)
                .output()
                .unwrap()
        };
        let db = Database::open_file(&path, Limits::default()).unwrap();
        let busy = compact();
        assert!(!busy.status.success());
        assert!(busy.stdout.is_empty());
        assert!(busy.stderr.starts_with(b"Busy"));
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(db);

        let stats = Database::compact_file(&reference, Limits::default()).unwrap();
        let output = compact();
        assert!(output.status.success(), "{:?}", output.stderr);
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "compacted {} bytes to {} bytes\n",
                stats.before_bytes, stats.after_bytes
            ),
        );
        assert!(stats.after_bytes < stats.before_bytes);
        assert_eq!(fs::read(&path).unwrap(), fs::read(reference).unwrap());
        let original = Database::open_on(
            SimulatedDisk::from_image(&before).unwrap(),
            Limits::default(),
        )
        .unwrap();
        let reopened = Database::open_file(&path, Limits::default()).unwrap();
        check(&reopened, &original);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
    }
    #[test]
    fn offline_file_compaction_preserves_metadata_and_enforces_quiescence() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        #[cfg(target_os = "macos")]
        use std::process::Command;
        let dir = Directory::new();
        assert_eq!(
            Database::compact_file(Path::new(""), Limits::default()),
            Err(Error::Invalid)
        );
        assert_eq!(
            Database::compact_file(dir.path("missing.db"), Limits::default()),
            Err(Error::NotFound)
        );
        let path = dir.path("space λ.db");
        seed(&path);
        let db = Database::open_file(&path, Limits::default()).unwrap();
        assert_eq!(
            Database::compact_file(&path, Limits::default()),
            Err(Error::Busy)
        );
        drop(db);
        let alias = dir.path("alias");
        fs::hard_link(&path, &alias).unwrap();
        let before = fs::read(&path).unwrap();
        assert_eq!(
            Database::compact_file(&path, Limits::default()),
            Err(Error::Invalid)
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
        fs::remove_file(alias).unwrap();
        let link = dir.path("link");
        symlink(&path, &link).unwrap();
        assert_eq!(
            Database::compact_file(&link, Limits::default()),
            Err(Error::Invalid)
        );
        fs::remove_file(link).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        #[cfg(target_os = "macos")]
        {
            assert!(
                Command::new("/usr/bin/xattr")
                    .args(["-w", "weekend.test", "preserved"])
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(
                Command::new("/bin/chmod")
                    .args(["+a", "everyone allow read"])
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        #[cfg(target_os = "linux")]
        linux_metadata::set(&path);
        let stats = Database::compact_file(&path, Limits::default()).unwrap();
        assert!(stats.after_bytes < stats.before_bytes / 2);
        assert_eq!(fs::metadata(&path).unwrap().len(), stats.after_bytes);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        #[cfg(target_os = "macos")]
        {
            let xattr = Command::new("/usr/bin/xattr")
                .args(["-p", "weekend.test"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(xattr.status.success());
            assert_eq!(xattr.stdout, b"preserved\n");
            let acl = Command::new("/bin/ls")
                .arg("-le")
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                String::from_utf8(acl.stdout)
                    .unwrap()
                    .contains("everyone allow read")
            );
        }
        #[cfg(target_os = "linux")]
        linux_metadata::check(&path);
        let original = Database::open_on(
            SimulatedDisk::from_image(&before).unwrap(),
            Limits::default(),
        )
        .unwrap();
        let reopened = Database::open_file(&path, Limits::default()).unwrap();
        check(&reopened, &original);
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn replacement_drops_inherited_acl_when_source_has_none() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = Directory::new();
        let path = dir.path("source-without-acl");
        seed(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let before = fs::metadata(&path).unwrap();
        linux_metadata::set_default_acl(&dir.0);
        Database::compact_file(&path, Limits::default()).unwrap();
        linux_metadata::check_no_acl(&path);
        let after = fs::metadata(&path).unwrap();
        assert_eq!(after.mode() & 0o7777, before.mode() & 0o7777);
        assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
    }
    #[cfg(feature = "fault-injection")]
    #[test]
    fn file_errors_at_every_observation_preserve_a_whole_canonical_store() {
        use std::os::unix::fs::MetadataExt;
        use weekend_db::storage::{CompactionEvent, CompactionStep as S, compact_file_with_hook};
        let dir = Directory::new();
        let base = dir.path("base");
        seed(&base);
        let image = fs::read(&base).unwrap();
        let original = Database::open_on(
            SimulatedDisk::from_image(&image).unwrap(),
            Limits::default(),
        )
        .unwrap();
        let events = Arc::new(Mutex::new(Vec::<CompactionEvent>::new()));
        let log = events.clone();
        let trace = dir.path("trace");
        fs::copy(&base, &trace).unwrap();
        let traced_path = trace.clone();
        compact_file_with_hook(&trace, Limits::default(), move |event| {
            assert!(
                matches!(
                    Database::open_file(&traced_path, Limits::default()),
                    Err(Error::Busy)
                ),
                "lease gap at {event:?}"
            );
            log.lock().unwrap().push(event);
            Ok(())
        })
        .unwrap();
        let compacted = fs::read(&trace).unwrap();
        assert_ne!(compacted, image);
        let events: Arc<[CompactionEvent]> = events.lock().unwrap().clone().into();
        let position = |step, after| {
            events
                .iter()
                .position(|event| *event == CompactionEvent { step, after })
                .unwrap()
        };
        let scratch_created = events[..position(S::TargetLength, false)]
            .iter()
            .rposition(|event| event.step == S::SourceLength && !event.after)
            .unwrap();
        let metadata = position(S::CopyMetadata, false);
        for (range, operations) in [
            (
                0..scratch_created + 2,
                [
                    S::SourceLength,
                    S::SourceRead,
                    S::SourceWrite,
                    S::SourceSync,
                ],
            ),
            (
                scratch_created + 2..metadata,
                [
                    S::TargetLength,
                    S::TargetRead,
                    S::TargetWrite,
                    S::TargetSync,
                ],
            ),
        ] {
            let phase = &events[range];
            assert!(phase.iter().all(|e| operations.contains(&e.step)));
            assert!(
                operations
                    .iter()
                    .all(|step| phase.iter().any(|e| e.step == *step))
            );
        }
        let first_exchange = position(S::Exchange, false);
        let exchanged = position(S::Exchange, true);
        let removed = position(S::RemoveOld, true);
        assert_eq!(
            events[first_exchange..],
            [
                S::Exchange,
                S::SyncDirectory,
                S::SyncReplacement,
                S::RemoveOld,
                S::SyncCleanupDirectory,
                S::SyncCleanupFile,
            ]
            .into_iter()
            .flat_map(|step| [false, true].map(|after| CompactionEvent { step, after }))
            .collect::<Vec<_>>()
        );
        for fail in 0..events.len() {
            let attempt = Directory::new();
            let path = attempt.path("store.db");
            let unrelated = attempt.path(".wdb-compact-00000000000000000000000000000000.tmp");
            fs::write(&unrelated, b"unrelated").unwrap();
            fs::copy(&base, &path).unwrap();
            let old_inode = fs::metadata(&path).unwrap().ino();
            let expected = events.clone();
            let mut position = 0;
            let result = compact_file_with_hook(&path, Limits::default(), move |event| {
                let here = position;
                position += 1;
                assert!(here <= fail, "continued after hook failure");
                assert_eq!(event, expected[here]);
                if here == fail { Err(Error::Io) } else { Ok(()) }
            });
            assert_eq!(
                result,
                Err(if fail < first_exchange {
                    Error::Io
                } else {
                    Error::OutcomeUnknown
                }),
                "event={:?}",
                events[fail]
            );
            let canonical = fs::read(&path).unwrap();
            assert_eq!(
                &canonical,
                if fail < exchanged { &image } else { &compacted }
            );
            assert_eq!(
                fs::metadata(&path).unwrap().ino() == old_inode,
                fail < exchanged
            );
            assert_eq!(fs::read(&unrelated).unwrap(), b"unrelated");
            let mut extra = fs::read_dir(&attempt.0)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|entry| entry != &path && entry != &unrelated);
            if (scratch_created..removed).contains(&fail) {
                let scratch = extra.next().expect("preserve the operation's scratch file");
                assert!(
                    scratch
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with(".wdb-compact-")
                );
                assert_eq!(
                    fs::metadata(&scratch).unwrap().ino() == old_inode,
                    fail >= exchanged
                );
                if (metadata..exchanged).contains(&fail) {
                    assert_eq!(fs::read(scratch).unwrap(), compacted);
                } else if fail >= exchanged {
                    assert_eq!(fs::read(scratch).unwrap(), image);
                }
            }
            assert!(extra.next().is_none(), "event={:?}", events[fail]);
            check(
                &Database::open_file(&path, Limits::default()).unwrap(),
                &original,
            );
        }
    }

    #[cfg(feature = "fault-injection")]
    mod descriptors {
        use super::*;
        use crate::descriptor_limit::{EMFILE, fill_table, run_worker};
        use std::os::unix::fs::MetadataExt;
        use weekend_db::storage::{
            CompactionStep as Step, NativeIoOperation as Operation, compact_file_with_hook,
            with_storage_diagnostics,
        };

        const CHILD_CASE: &str = "WEEKEND_TEST_COMPACTION_DESCRIPTOR_CASE";
        const TEST: &str =
            "files::descriptors::descriptor_exhaustion_preserves_current_and_retained_roots";

        #[derive(Clone, Copy)]
        enum Scratch {
            Absent,
            Empty,
            Complete,
        }

        struct Case {
            name: &'static str,
            free: usize,
            checkpoint: Option<Step>,
            operation: Operation,
            scratch: Scratch,
        }

        const CASES: &[Case] = &[
            Case {
                name: "source parent",
                free: 0,
                checkpoint: None,
                operation: Operation::OpenDirectory,
                scratch: Scratch::Absent,
            },
            Case {
                name: "source directory duplicate",
                free: 1,
                checkpoint: None,
                operation: Operation::DuplicateHandle,
                scratch: Scratch::Absent,
            },
            Case {
                name: "source open",
                free: 2,
                checkpoint: None,
                operation: Operation::Open,
                scratch: Scratch::Absent,
            },
            Case {
                name: "source lease check",
                free: 3,
                checkpoint: None,
                operation: Operation::Open,
                scratch: Scratch::Absent,
            },
            Case {
                name: "source claim check",
                free: 4,
                checkpoint: None,
                operation: Operation::Open,
                scratch: Scratch::Absent,
            },
            Case {
                name: "scratch directory duplicate",
                free: 16,
                checkpoint: Some(Step::SourceLength),
                operation: Operation::DuplicateHandle,
                scratch: Scratch::Absent,
            },
            Case {
                name: "target directory duplicate",
                free: 5,
                checkpoint: None,
                operation: Operation::DuplicateHandle,
                scratch: Scratch::Absent,
            },
            Case {
                name: "target create",
                free: 6,
                checkpoint: None,
                operation: Operation::Create,
                scratch: Scratch::Absent,
            },
            Case {
                name: "target lease check",
                free: 7,
                checkpoint: None,
                operation: Operation::Open,
                scratch: Scratch::Empty,
            },
            Case {
                name: "target claim check",
                free: 8,
                checkpoint: None,
                operation: Operation::Open,
                scratch: Scratch::Empty,
            },
            Case {
                name: "pre-exchange name check",
                free: 16,
                checkpoint: Some(Step::CheckNames),
                operation: Operation::Open,
                scratch: Scratch::Complete,
            },
        ];

        fn run_child(case: &Case) {
            let limits = Limits::default();
            let source = Path::new("store.db");
            let unrelated = Path::new(".wdb-compact-unrelated.tmp");
            let before = fs::read(source).unwrap();
            let inode = fs::metadata(source).unwrap().ino();
            let mut original =
                Database::open_on(SimulatedDisk::from_image(&before).unwrap(), limits).unwrap();
            let reference = SimulatedDisk::new().unwrap();
            original.compact_to(reference.clone()).unwrap();
            let compacted = reference.durable_image().unwrap();

            let mut held = fill_table();
            assert!(held.len() > case.free);
            held.truncate(held.len() - case.free);
            let checkpoint = case.checkpoint;
            let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let observed = reached.clone();
            let mut checkpoint_fds = Vec::new();
            let failure = with_storage_diagnostics(|| {
                compact_file_with_hook(source, limits, move |event| {
                    assert_ne!(event.step, Step::Exchange, "failure passed the name check");
                    if checkpoint == Some(event.step)
                        && !event.after
                        && !observed.swap(true, Ordering::Relaxed)
                    {
                        checkpoint_fds.extend(fill_table());
                        assert!(!checkpoint_fds.is_empty());
                    }
                    Ok(())
                })
            })
            .unwrap_err();
            assert_eq!(failure.error, Error::Io, "{}", case.name);
            let native = failure.native.unwrap();
            assert_eq!(native.operation, case.operation, "{}", case.name);
            assert_eq!(native.raw_os_error, Some(EMFILE), "{}", case.name);
            assert_eq!(reached.load(Ordering::Relaxed), checkpoint.is_some());
            assert_eq!(
                fill_table().len(),
                case.free,
                "descriptor leak at {}",
                case.name
            );
            drop(held);

            assert_eq!(fs::metadata(source).unwrap().ino(), inode);
            assert_eq!(fs::read(source).unwrap(), before);
            assert_eq!(fs::read(unrelated).unwrap(), b"unrelated");
            check(&Database::open_file(source, limits).unwrap(), &original);

            let mut scratch = fs::read_dir(".")
                .unwrap()
                .map(|entry| PathBuf::from(entry.unwrap().file_name()))
                .filter(|entry| entry != source && entry != unrelated);
            let saved = match case.scratch {
                Scratch::Absent => None,
                Scratch::Empty | Scratch::Complete => {
                    let path = scratch.next().expect("operation's scratch file");
                    assert!(
                        path.as_os_str()
                            .to_str()
                            .unwrap()
                            .starts_with(".wdb-compact-")
                    );
                    let bytes = fs::read(&path).unwrap();
                    match case.scratch {
                        Scratch::Empty => assert!(bytes.is_empty()),
                        Scratch::Complete => assert_eq!(bytes, compacted),
                        Scratch::Absent => unreachable!(),
                    }
                    Some((path, bytes))
                }
            };
            assert!(scratch.next().is_none(), "{}", case.name);

            Database::compact_file(source, limits).unwrap();
            assert_ne!(fs::metadata(source).unwrap().ino(), inode);
            assert_eq!(fs::read(source).unwrap(), compacted);
            check(&Database::open_file(source, limits).unwrap(), &original);
            assert_eq!(fs::read(unrelated).unwrap(), b"unrelated");
            let expected_files = 2 + usize::from(saved.is_some());
            if let Some((path, bytes)) = saved {
                assert_eq!(fs::read(path).unwrap(), bytes);
            }
            assert_eq!(fs::read_dir(".").unwrap().count(), expected_files);
            println!("verified descriptor exhaustion at {}", case.name);
        }

        #[test]
        fn descriptor_exhaustion_preserves_current_and_retained_roots() {
            if let Ok(case) = std::env::var(CHILD_CASE) {
                run_child(&CASES[case.parse::<usize>().unwrap()]);
                return;
            }
            for (index, case) in CASES.iter().enumerate() {
                let dir = Directory::new();
                seed(&dir.path("store.db"));
                fs::write(dir.path(".wdb-compact-unrelated.tmp"), b"unrelated").unwrap();
                run_worker(
                    TEST,
                    (CHILD_CASE, &index.to_string()),
                    &dir.0,
                    &format!("verified descriptor exhaustion at {}", case.name),
                );
            }
        }
    }

    #[cfg(feature = "fault-injection")]
    #[test]
    fn recheck_rejects_replaced_canonical_or_scratch_inode_before_exchange() {
        use std::os::unix::fs::MetadataExt;
        use weekend_db::storage::{CompactionStep as S, compact_file_with_hook};

        for replace_original in [true, false] {
            let dir = Directory::new();
            let path = dir.path("store.db");
            seed(&path);
            let observed_path = path.clone();
            let held = dir.path("held.db");
            let held_name = held.clone();
            let result = compact_file_with_hook(&path, Limits::default(), move |event| {
                assert_ne!(event.step, S::Exchange);
                if event.step == S::CheckNames && !event.after {
                    let affected = if replace_original {
                        observed_path.clone()
                    } else {
                        fs::read_dir(observed_path.parent().unwrap())
                            .unwrap()
                            .map(|entry| entry.unwrap().path())
                            .find(|entry| {
                                entry
                                    .file_name()
                                    .unwrap()
                                    .to_str()
                                    .unwrap()
                                    .starts_with(".wdb-compact-")
                            })
                            .unwrap()
                    };
                    fs::rename(&affected, &held_name).unwrap();
                    fs::copy(&held_name, &affected).unwrap();
                    assert_ne!(
                        fs::metadata(&affected).unwrap().ino(),
                        fs::metadata(&held_name).unwrap().ino()
                    );
                }
                Ok(())
            });
            assert_eq!(result, Err(Error::Invalid));
            assert!(held.exists());
            assert_eq!(fs::read(&path).unwrap(), fixture());
        }
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod linux_metadata {
    use std::{
        ffi::{CString, c_char, c_int, c_void},
        os::unix::ffi::OsStrExt,
        path::Path,
    };
    unsafe extern "C" {
        fn setxattr(
            path: *const c_char,
            name: *const c_char,
            value: *const c_void,
            size: usize,
            flags: c_int,
        ) -> c_int;
        fn getxattr(
            path: *const c_char,
            name: *const c_char,
            value: *mut c_void,
            size: usize,
        ) -> isize;
    }
    fn attributes() -> Vec<(&'static std::ffi::CStr, Vec<u8>)> {
        // Linux POSIX ACL xattr v2: user_obj, named user, group_obj, mask, other.
        let mut acl = 2u32.to_le_bytes().to_vec();
        for (tag, permissions, id) in [
            (1u16, 6u16, u32::MAX),
            (2, 4, 12345),
            (4, 4, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            acl.extend_from_slice(&tag.to_le_bytes());
            acl.extend_from_slice(&permissions.to_le_bytes());
            acl.extend_from_slice(&id.to_le_bytes());
        }
        vec![
            (c"user.weekend.test", b"preserved\0binary".to_vec()),
            (c"user.weekend.empty", vec![]),
            (c"system.posix_acl_access", acl),
        ]
    }
    pub fn set_default_acl(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let value = attributes().pop().unwrap().1;
        assert_eq!(
            unsafe {
                setxattr(
                    path.as_ptr(),
                    c"system.posix_acl_default".as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                )
            },
            0,
            "{}",
            std::io::Error::last_os_error()
        );
    }
    pub fn check_no_acl(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            unsafe {
                getxattr(
                    path.as_ptr(),
                    c"system.posix_acl_access".as_ptr(),
                    std::ptr::null_mut(),
                    0,
                )
            },
            -1
        );
        assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(61)); // ENODATA
    }
    pub fn set(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        for (name, value) in attributes() {
            assert_eq!(
                unsafe {
                    setxattr(
                        path.as_ptr(),
                        name.as_ptr(),
                        value.as_ptr().cast(),
                        value.len(),
                        0,
                    )
                },
                0,
                "{}",
                std::io::Error::last_os_error()
            );
        }
    }
    pub fn check(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        for (name, value) in attributes() {
            let mut bytes = vec![0; value.len() + 1];
            let n = unsafe {
                getxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                )
            };
            assert_eq!(
                n,
                value.len() as isize,
                "{}",
                std::io::Error::last_os_error()
            );
            assert_eq!(&bytes[..n as usize], value);
        }
    }
}
