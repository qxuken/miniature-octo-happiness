#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn export_bytes(
    file: &mut impl weekend_db::storage::ByteStore,
    bytes: &[u8],
) -> weekend_db::Result<()> {
    file.lock()?;
    let mut offset = 0;
    while offset < bytes.len() {
        let written = file.write(offset as u64, &bytes[offset..])?;
        if written == 0 || written > bytes.len() - offset {
            return Err(weekend_db::Error::Io);
        }
        offset += written;
    }
    file.sync()?;
    file.unlock();
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn run(args: &[std::ffi::OsString]) -> weekend_db::Result<()> {
    use std::io::Read;
    use weekend_db::{Limits, storage};
    match args {
        [command, source, destination] if command == "salvage" => {
            let report = storage::salvage_file(source, destination, Limits::default())?;
            println!(
                "salvaged revision {} data {} retained {} from slot {}",
                report.revision, report.data_revision, report.retained_roots, report.selected_slot
            );
            println!("verified slots {:?}", report.verified_slot_revisions);
            println!("witnesses ignored, acknowledged data loss possible");
            Ok(())
        }
        [command, path] if command == "check" => {
            let info = storage::inspect_file(path, Limits::default())?;
            println!(
                "format {} revision {} data {} witnessed {}",
                info.format_version, info.revision, info.data_revision, info.witnessed
            );
            println!("identity {:02x?}", info.identity);
            println!(
                "physical bytes {} committed bytes {}",
                info.physical_bytes, info.committed_bytes
            );
            for root in info.retained {
                println!(
                    "retained {} revision {} data {}",
                    root.id, root.revision, root.data_revision
                );
            }
            Ok(())
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        [command, path] if command == "compact" => {
            let stats = weekend_db::Database::compact_file(path, Limits::default())?;
            println!(
                "compacted {} bytes to {} bytes",
                stats.before_bytes, stats.after_bytes
            );
            Ok(())
        }
        #[cfg(windows)]
        [command, _] if command == "compact" => Err(weekend_db::Error::UnsupportedPlatform),
        [command, source, destination] if command == "backup" || command == "restore" => {
            let stats = if command == "backup" {
                storage::backup_file(source, destination, Limits::default())?
            } else {
                storage::restore_file(source, destination, Limits::default())?
            };
            println!(
                "verified copy {} bytes from {} bytes",
                stats.after_bytes, stats.before_bytes
            );
            Ok(())
        }
        [command, source, destination] if command == "export" => {
            let bytes = storage::export_logical_file(source, Limits::default())?;
            let mut file = storage::FileStore::create(destination)?;
            export_bytes(&mut file, &bytes)?;
            println!("exported logical format 1");
            Ok(())
        }
        [command, source, destination] if command == "import" => {
            let file = std::fs::File::open(source).map_err(|_| weekend_db::Error::Io)?;
            let mut bytes = Vec::new();
            file.take(storage::MAX_IMAGE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| weekend_db::Error::Io)?;
            storage::import_logical_file(&bytes, destination, Limits::default())?;
            println!("imported and verified logical format 1");
            Ok(())
        }
        _ => Err(weekend_db::Error::Invalid),
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
mod export_decisions {
    use super::export_bytes;
    use std::collections::VecDeque;
    use weekend_db::{Error, Result, storage::ByteStore};

    #[derive(Debug, PartialEq)]
    enum Event {
        Lock,
        Write(u64, Vec<u8>),
        Sync,
        Unlock,
    }
    #[derive(Default)]
    struct Script {
        writes: VecDeque<Result<usize>>,
        lock_error: Option<Error>,
        sync_error: Option<Error>,
        events: Vec<Event>,
    }
    impl ByteStore for Script {
        fn lock(&mut self) -> Result<()> {
            self.events.push(Event::Lock);
            self.lock_error.map_or(Ok(()), Err)
        }
        fn unlock(&mut self) {
            self.events.push(Event::Unlock);
        }
        fn length(&mut self) -> Result<u64> {
            panic!("export must not query length")
        }
        fn read(&mut self, _: u64, _: &mut [u8]) -> Result<usize> {
            panic!("export must not read")
        }
        fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
            self.events.push(Event::Write(offset, bytes.to_vec()));
            self.writes.pop_front().expect("unexpected export write")
        }
        fn sync(&mut self) -> Result<()> {
            self.events.push(Event::Sync);
            self.sync_error.map_or(Ok(()), Err)
        }
    }
    #[test]
    fn short_writes_use_exact_suffixes_before_sync_and_unlock() {
        let mut file = Script {
            writes: [Ok(1), Ok(2), Ok(3)].into(),
            ..Default::default()
        };
        assert_eq!(export_bytes(&mut file, b"abcdef"), Ok(()));
        assert_eq!(
            file.events,
            [
                Event::Lock,
                Event::Write(0, b"abcdef".to_vec()),
                Event::Write(1, b"bcdef".to_vec()),
                Event::Write(3, b"def".to_vec()),
                Event::Sync,
                Event::Unlock,
            ]
        );
        assert!(file.writes.is_empty());
    }
    #[test]
    fn zero_and_excess_progress_stop_before_another_write_or_sync() {
        for progress in [0, 7] {
            let mut file = Script {
                writes: [Ok(progress)].into(),
                ..Default::default()
            };
            assert_eq!(export_bytes(&mut file, b"abcdef"), Err(Error::Io));
            assert_eq!(
                file.events,
                [Event::Lock, Event::Write(0, b"abcdef".to_vec())]
            );
        }
    }
    #[test]
    fn empty_export_syncs_without_writing() {
        let mut file = Script::default();
        assert_eq!(export_bytes(&mut file, b""), Ok(()));
        assert_eq!(file.events, [Event::Lock, Event::Sync, Event::Unlock]);
    }
    #[test]
    fn errors_stop_at_the_first_failed_stage() {
        let mut file = Script {
            lock_error: Some(Error::Busy),
            ..Default::default()
        };
        assert_eq!(export_bytes(&mut file, b"abc"), Err(Error::Busy));
        assert_eq!(file.events, [Event::Lock]);

        let mut file = Script {
            writes: [Err(Error::Limit)].into(),
            ..Default::default()
        };
        assert_eq!(export_bytes(&mut file, b"abc"), Err(Error::Limit));
        assert_eq!(file.events, [Event::Lock, Event::Write(0, b"abc".to_vec())]);

        let mut file = Script {
            writes: [Ok(3)].into(),
            sync_error: Some(Error::Io),
            ..Default::default()
        };
        assert_eq!(export_bytes(&mut file, b"abc"), Err(Error::Io));
        assert_eq!(
            file.events,
            [Event::Lock, Event::Write(0, b"abc".to_vec()), Event::Sync]
        );
    }
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    let result = weekend_db::storage::with_storage_diagnostics(|| run(&args));
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let result: weekend_db::Result<()> = {
        let _ = args;
        Err(weekend_db::Error::UnsupportedPlatform)
    };
    if let Err(error) = result {
        eprintln!("{error}");
        eprintln!(
            "usage: weekend-admin check FILE | compact FILE (macOS/Linux) | backup SOURCE NEW_FILE | restore BACKUP NEW_FILE | salvage SOURCE NEW_FILE | export SOURCE NEW_LOGICAL | import LOGICAL NEW_FILE"
        );
        std::process::exit(1);
    }
}
