mod support;
#[path = "../examples/support/users.rs"]
mod users;
use support::Model;
use weekend_db::{
    Database, Error, Limits,
    storage::{Crash, Fault, IoKind, MemoryStore, SimulatedDisk},
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
            let disk = SimulatedDisk::from_image(&image).unwrap();
            let mut source = Database::open_on(disk.clone(), Limits::default()).unwrap();
            let before = disk.durable_image().unwrap();
            let target = SimulatedDisk::new().unwrap();
            target.arm(step, fault);
            let result = source.compact_to(target.clone());
            assert_eq!(
                disk.durable_image().unwrap(),
                before,
                "step={step}, fault={fault:?}"
            );
            target.clear_fault();
            target.crash(Crash::PersistAll).unwrap();
            match Database::open_on(target, Limits::default()) {
                Ok(copy) => check(&copy, &source),
                Err(e) => {
                    assert!(result.is_err());
                    assert!(
                        matches!(e, Error::Corrupt | Error::UnsupportedFormat),
                        "step={step}, {e:?}"
                    );
                }
            }
        }
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn allocation_failures_during_copy_and_verification_never_change_the_source() {
    let image = fixture();
    let mut success = false;
    for budget in 0..1024 {
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let mut source = Database::open_on(disk.clone(), Limits::default()).unwrap();
        let before = disk.durable_image().unwrap();
        let target = SimulatedDisk::new().unwrap();
        source.fail_allocations_after(Some(budget));
        let result = source.compact_to(target.clone());
        source.fail_allocations_after(None);
        assert_eq!(disk.durable_image().unwrap(), before);
        if result.is_ok() {
            check(
                &Database::open_on(target, Limits::default()).unwrap(),
                &source,
            );
            success = true;
            break;
        }
        assert_eq!(result, Err(Error::OutOfMemory), "budget={budget}");
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
    fn offline_file_compaction_preserves_metadata_and_enforces_quiescence() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        #[cfg(target_os = "macos")]
        use std::process::Command;
        let dir = Directory::new();
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
        let events = events.lock().unwrap();
        let first_exchange = events.iter().position(|e| e.step == S::Exchange).unwrap();
        for fail in 0..events.len() {
            let path = dir.path(&format!("error-{fail}"));
            fs::copy(&base, &path).unwrap();
            let mut position = 0;
            let result = compact_file_with_hook(&path, Limits::default(), move |_| {
                let here = position;
                position += 1;
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
            check(
                &Database::open_file(&path, Limits::default()).unwrap(),
                &original,
            );
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
