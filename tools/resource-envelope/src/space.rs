//! Kernel ENOSPC/EDQUOT maintenance fixture. Only the controller fills storage.
use super::*;
use std::{collections::BTreeSet, path::PathBuf};
use storage::{NativeIoCategory, NativeIoOperation};
#[cfg(windows)]
#[path = "quota_token.rs"]
mod quota_token;
const COPY: usize = 256 * MIB;
const COMPACTED: u64 = 63_074_304;
fn replaces_source(op: &str) -> bool {
    cfg!(unix) && op == "compact"
}
fn names(dir: &Path) -> BTreeSet<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect()
}
fn partial(dir: &Path, op: &str) -> PathBuf {
    if !replaces_source(op) {
        return dir.join(format!("failed-{op}.wdb"));
    }
    let paths: Vec<_> = names(dir)
        .into_iter()
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(".wdb-compact-")
        })
        .collect();
    assert_eq!(paths.len(), 1);
    paths[0].clone()
}
fn operation(
    dir: &Path,
    source: &Path,
    op: &str,
    target: &Path,
) -> weekend_db::Result<storage::CompactionStats> {
    match op {
        "backup" => storage::backup_file(source, target, limits(COPY)),
        "restore" => storage::restore_file(dir.join("backup.wdb"), target, limits(COPY)),
        "compact" => {
            #[cfg(unix)]
            {
                Database::compact_file(source, limits(COPY))
            }
            #[cfg(windows)]
            {
                compact_copy(source, target)
            }
        }
        _ => panic!("unknown maintenance operation"),
    }
}
#[cfg(windows)]
pub(super) fn compact_copy(
    source: &Path,
    target: &Path,
) -> weekend_db::Result<storage::CompactionStats> {
    let mut database = Database::open_file(source, limits(COPY))?;
    database.compact_to(storage::FileStore::create(target)?)
}
pub(super) fn run(dir: &Path, source: &Path, mode: &str, arguments: &[std::ffi::OsString]) {
    let kind = std::env::var("WEEKEND_PRIVATE_MAINTENANCE").unwrap();
    #[cfg(unix)]
    assert!(matches!(
        kind.as_str(),
        "enospc"
            | "quota"
            | "inode-quota"
            | "inode-enospc"
            | "writeback"
            | "writeback-slot"
            | "writeback-witness"
            | "writeback-file-metadata"
    ));
    #[cfg(windows)]
    assert!(matches!(kind.as_str(), "ntfs-enospc" | "ntfs-quota"));
    #[cfg(windows)]
    if kind == "ntfs-quota" {
        quota_token::set_copy_owner().expect("set quota owner in the fixture token");
    }
    let inode = matches!(kind.as_str(), "inode-quota" | "inode-enospc");
    let copy_metadata = matches!(kind.as_str(), "writeback-slot" | "writeback-witness");
    let op = arguments[0].to_str().unwrap();
    assert!(op.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
    let pid = std::process::id();
    if mode == "space-quota-probe" {
        assert_eq!(kind, "ntfs-quota");
        assert!(
            fs::metadata(dir)
                .expect("read private quota directory")
                .is_dir()
        );
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(source)
            .expect("create file with the selected quota owner");
    } else if mode == "space-check" {
        let changed = arguments.get(1).is_some_and(|v| v == "changed");
        let db = Database::open_file(source, limits(CORE)).unwrap();
        event(
            &format!("space_{op}"),
            &db,
            FINAL + u64::from(changed),
            ROUNDS,
            changed,
            0,
            "{\"fresh_process\":true}",
            true,
        );
    } else if mode == "space-advance" {
        let db = Database::open_file(source, limits(CORE)).unwrap();
        assert_eq!(probe(&db).commit(), Ok(FINAL + 1));
        event(
            "space_advanced",
            &db,
            FINAL + 1,
            ROUNDS,
            true,
            0,
            "{\"fresh_process\":true}",
            true,
        );
    } else {
        assert!(matches!(op, "backup" | "restore" | "compact"));
        assert_eq!(source, dir.join("source.wdb"));
        let backup = dir.join("backup.wdb");
        if mode == "space-fail" {
            let before = fs::read(source).unwrap();
            let good = fs::read(&backup).unwrap();
            let entries = names(dir);
            let failure = storage::with_storage_diagnostics(|| {
                operation(dir, source, op, &dir.join(format!("failed-{op}.wdb")))
            })
            .unwrap_err();
            let expected = if copy_metadata && !replaces_source(op) {
                Error::OutcomeUnknown
            } else {
                Error::Io
            };
            assert_eq!(failure.error, expected);
            let native = failure.native.unwrap();
            let errno = match kind.as_str() {
                "quota" | "inode-quota" => 122,
                "enospc" | "inode-enospc" => 28,
                "writeback"
                | "writeback-slot"
                | "writeback-witness"
                | "writeback-file-metadata" => 5,
                "ntfs-enospc" => 112,
                "ntfs-quota" => std::env::var("WEEKEND_PRIVATE_QUOTA_ERRNO")
                    .unwrap()
                    .parse()
                    .unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(
                native.category,
                match errno {
                    5 => NativeIoCategory::Other,
                    122 | 1295 => NativeIoCategory::Quota,
                    _ => NativeIoCategory::DiskFull,
                }
            );
            assert_eq!(native.raw_os_error, Some(errno));
            assert_eq!(fs::read(source).unwrap(), before);
            assert_eq!(fs::read(&backup).unwrap(), good);
            let now = names(dir);
            let (size, inventory) = if inode {
                assert_eq!(native.operation, NativeIoOperation::Create);
                assert_eq!(now, entries);
                (0, ",\"inventory_unchanged\":true")
            } else {
                assert!(matches!(
                    native.operation,
                    NativeIoOperation::Write | NativeIoOperation::Sync
                ));
                let partial = partial(dir, op);
                assert!(now.difference(&entries).eq([&partial]));
                let size = fs::metadata(partial).unwrap().len();
                if errno == 5 {
                    assert_eq!(native.operation, NativeIoOperation::Sync);
                    assert_eq!(size, COMPACTED);
                } else {
                    assert!((MIB as u64..COMPACTED).contains(&size));
                }
                (size, "")
            };
            println!(
                "{{\"stage\":\"fail\",\"operation\":\"{op}\",\"pid\":{pid},\"error\":{},\"category\":\"{:?}\",\"raw\":{errno},\"native_operation\":\"{:?}\",\"partial_bytes\":{size},\"source_unchanged\":true,\"backup_unchanged\":true{inventory}}}",
                failure.error as i32, native.category, native.operation
            );
        } else if mode == "space-retry" && inode {
            assert_eq!(names(dir), [source.to_path_buf(), backup.clone()].into());
            let destination = dir.join(format!("failed-{op}.wdb"));
            operation(dir, source, op, &destination).unwrap();
            let actual = if replaces_source(op) {
                source
            } else {
                &destination
            };
            assert_eq!(fs::read(actual).unwrap(), fs::read(backup).unwrap());
            println!(
                "{{\"stage\":\"retry\",\"operation\":\"{op}\",\"pid\":{pid},\"no_partial\":true,\"matches_backup\":true}}"
            );
        } else if mode == "space-retry" {
            let partial = partial(dir, op);
            let bytes = fs::read(&partial).unwrap();
            let inspection = storage::inspect_file(&partial, limits(COPY));
            let disposition = if kind == "writeback-file-metadata" {
                assert_eq!(op, "compact");
                let info = inspection.unwrap();
                assert_eq!(info.revision, FINAL);
                assert!(info.witnessed);
                assert_eq!(info.retained.len(), 64);
                assert_eq!(info.committed_bytes, COMPACTED);
                assert_eq!(bytes, fs::read(&backup).unwrap());
                "partial_verified_witnessed"
            } else if kind == "writeback-witness" {
                let info = inspection.unwrap();
                assert_eq!(info.revision, FINAL);
                assert!(!info.witnessed);
                assert_eq!(info.retained.len(), 64);
                assert_eq!(info.committed_bytes, COMPACTED);
                let good = fs::read(&backup).unwrap();
                let start = 3 * storage::REGION;
                let end = 4 * storage::REGION;
                assert_eq!(bytes[..start], good[..start]);
                assert_eq!(bytes[end..], good[end..]);
                assert!(bytes[start..end].iter().all(|b| *b == 0));
                "partial_verified_unwitnessed"
            } else {
                assert!(matches!(
                    inspection,
                    Err(Error::Corrupt | Error::UnsupportedFormat)
                ));
                if kind == "writeback-slot" {
                    assert_eq!(inspection, Err(Error::Corrupt));
                }
                "partial_rejected"
            };
            let existing = if replaces_source(op) {
                false
            } else {
                assert_eq!(
                    operation(dir, source, op, &partial),
                    Err(Error::AlreadyExists)
                );
                assert_eq!(fs::read(&partial).unwrap(), bytes);
                true
            };
            let destination = dir.join(format!("retry-{op}.wdb"));
            operation(dir, source, op, &destination).unwrap();
            let actual = if replaces_source(op) {
                source
            } else {
                &destination
            };
            assert_eq!(fs::read(actual).unwrap(), fs::read(backup).unwrap());
            assert_eq!(fs::read(&partial).unwrap(), bytes);
            println!(
                "{{\"stage\":\"retry\",\"operation\":\"{op}\",\"pid\":{pid},\"{disposition}\":true,\"existing_rejected\":{existing},\"partial_preserved\":true,\"matches_backup\":true}}"
            );
        } else if mode == "space-retire" {
            if !inode {
                fs::remove_file(partial(dir, op)).unwrap();
            }
            if !replaces_source(op) {
                let prefix = if inode { "failed" } else { "retry" };
                fs::remove_file(dir.join(format!("{prefix}-{op}.wdb"))).unwrap();
            }
            assert_eq!(
                names(dir),
                [source.to_path_buf(), backup].into_iter().collect()
            );
            println!(
                "{{\"stage\":\"retire\",\"operation\":\"{op}\",\"pid\":{pid},\"owned_outputs_removed\":true}}"
            );
        } else {
            panic!("unknown maintenance mode");
        }
    }
    complete(mode);
}
