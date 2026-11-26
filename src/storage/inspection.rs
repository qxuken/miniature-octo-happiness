//! Quiescent inspection and verified copies with an exclusive source lease.
use super::{ByteStore, CompactionStats, Storage, codec};
use crate::{DatabaseOptions, Limits, Result, memory::Memory};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedInfo {
    pub id: u64,
    pub revision: u64,
    pub data_revision: u64,
}

/// Verified metadata from the selected current graph and every retained graph.
/// `witnessed` reports the matching persisted witness. Inspection itself does
/// not stabilize an unwitnessed root or acknowledge a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub format_version: u32,
    pub identity: [u8; 16],
    pub revision: u64,
    pub data_revision: u64,
    pub physical_bytes: u64,
    pub committed_bytes: u64,
    pub witnessed: bool,
    pub retained: Vec<RetainedInfo>,
}

fn information(
    source: &mut Storage,
    recovered: &codec::Recovered,
    mem: &Memory,
) -> Result<ImageInfo> {
    let mut retained = Vec::new();
    mem.reserve_external(&mut retained, recovered.retention.entries.len())?;
    for entry in &recovered.retention.entries {
        retained.push(RetainedInfo {
            id: entry.id,
            revision: entry.head.revision,
            data_revision: entry.head.data_revision,
        });
    }
    Ok(ImageInfo {
        format_version: crate::version::FORMAT_VERSION,
        identity: recovered.identity,
        revision: recovered.head.revision,
        data_revision: recovered.head.data_revision,
        physical_bytes: source.backend.length()?,
        committed_bytes: recovered.slot.end,
        witnessed: recovered.witnessed,
        retained,
    })
}

/// Verify a quiescent image under an exclusive lease. Never writes or syncs the
/// source, including on failure. The lease is released before returning.
pub fn inspect_on<B: ByteStore + 'static>(source: B, limits: Limits) -> Result<ImageInfo> {
    inspect_on_with_options(source, limits.into())
}

/// Verify the selected graph and retained roots using the requested allocator and read mode.
pub fn inspect_on_with_options<B: ByteStore + 'static>(
    backend: B,
    options: DatabaseOptions,
) -> Result<ImageInfo> {
    let mem = Memory::with_allocator(options.limits, options.allocator)?;
    if options.paged_reads {
        let page_source = codec::paging::Source::acquire(backend, &mem)?;
        let mut source = Storage::locked(page_source.clone(), &mem)?;
        let recovered = codec::recover_with_pages(&mut *source.backend, &mem, Some(page_source))?;
        information(&mut source, &recovered, &mem)
    } else {
        let mut source = Storage::locked(backend, &mem)?;
        let recovered = codec::recover(&mut *source.backend, &mem)?;
        information(&mut source, &recovered, &mem)
    }
}

fn copy_verified<B: ByteStore + 'static>(
    source: &mut Storage,
    recovered: codec::Recovered,
    destination: B,
    mem: &Memory,
) -> Result<CompactionStats> {
    let before_bytes = source.backend.length()?;
    let mut destination = {
        let codec::Recovered {
            head,
            next_collection,
            identity,
            retention,
            cache,
            data_cache,
            ..
        } = recovered;
        // File-local offsets belong to the source. The copy encodes its own.
        drop(cache);
        drop(data_cache);
        Storage::create(
            destination,
            mem,
            &head,
            next_collection,
            identity,
            &retention,
        )?
    };
    // The offline copy only returns sizes. Release cached references before
    // reading the independent verification graph.
    destination.release_serialization_caches();
    destination.verify(mem)?;
    Ok(CompactionStats {
        before_bytes,
        after_bytes: destination.end,
    })
}

/// Verify the source and durably copy its current and retained roots into an
/// empty destination. Preserves store identity. Neither writes nor syncs source.
/// On error the destination may contain a partial image and must be discarded
/// or independently inspected. Existing nonempty destinations are rejected.
pub fn backup_on<S: ByteStore + 'static, D: ByteStore + 'static>(
    source: S,
    destination: D,
    limits: Limits,
) -> Result<CompactionStats> {
    let mem = Memory::new(limits)?;
    let mut source = Storage::locked(source, &mem)?;
    let recovered = codec::recover(&mut *source.backend, &mem)?;
    copy_verified(&mut source, recovered, destination, &mem)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn inspect_file(path: impl AsRef<std::path::Path>, limits: Limits) -> Result<ImageInfo> {
    inspect_on(super::FileStore::open_read_only(path)?, limits)
}

/// Read-only full verification with the same read-mode policy as database open.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn inspect_file_with_options(
    path: impl AsRef<std::path::Path>,
    options: DatabaseOptions,
) -> Result<ImageInfo> {
    options.limits.validate()?;
    let file = super::FileStore::open_read_only_in(path, options.allocator.clone())?;
    inspect_on_with_options(file, options)
}
/// Offline backup to a new file. Close source database handles first. Validates
/// the source before creating the destination. An existing destination is never
/// overwritten. A failed copy can leave a partial destination for inspection.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn backup_file(
    source: impl AsRef<std::path::Path>,
    destination: impl AsRef<std::path::Path>,
    limits: Limits,
) -> Result<CompactionStats> {
    let mem = Memory::new(limits)?;
    let mut source = Storage::locked(super::FileStore::open_read_only(source)?, &mem)?;
    let recovered = codec::recover(&mut *source.backend, &mem)?;
    let destination = super::FileStore::create(destination)?;
    copy_verified(&mut source, recovered, destination, &mem)
}

/// Restore a verified backup to a new file, preserving identity and all roots.
/// The restored file is a replacement for the source lineage. Independent
/// replicated forks require a separately designed identity/enrollment workflow.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn restore_file(
    backup: impl AsRef<std::path::Path>,
    destination: impl AsRef<std::path::Path>,
    limits: Limits,
) -> Result<CompactionStats> {
    backup_file(backup, destination, limits)
}

#[cfg(test)]
mod decisions {
    use super::*;
    use crate::{
        Database, Error,
        storage::{IoKind, SimulatedDisk},
    };

    struct VerifyCopy {
        source: SimulatedDisk,
        destination: SimulatedDisk,
        memory: Memory,
    }

    impl ByteStore for VerifyCopy {
        fn lock(&mut self) -> Result<()> {
            self.destination.lock()
        }
        fn unlock(&mut self) {
            self.destination.unlock();
        }
        fn length(&mut self) -> Result<u64> {
            self.destination.length()
        }
        fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
            if offset == 0 {
                assert_eq!(self.source.lock(), Err(Error::Busy));
                assert_eq!(self.memory.usage().0, 0);
            }
            self.destination.read(offset, out)
        }
        fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
            self.destination.write(offset, bytes)
        }
        fn sync(&mut self) -> Result<()> {
            self.destination.sync()
        }
    }

    #[test]
    fn backup_releases_copy_graph_before_verification_but_keeps_source_leased() {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), [4; 16], Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        let mut transaction = db.begin();
        transaction
            .put(collection, b"a", &db.integer(1).unwrap())
            .unwrap();
        transaction.commit().unwrap();
        db.retain_snapshot(&db.snapshot()).unwrap();
        drop(db);

        let memory = Memory::new(Limits::default()).unwrap();
        let mut source = Storage::locked(disk.clone(), &memory).unwrap();
        let recovered = codec::recover(&mut *source.backend, &memory).unwrap();
        let destination = SimulatedDisk::new().unwrap();
        copy_verified(
            &mut source,
            recovered,
            VerifyCopy {
                source: disk,
                destination: destination.clone(),
                memory: memory.clone(),
            },
            &memory,
        )
        .unwrap();
        assert!(
            destination
                .trace()
                .unwrap()
                .iter()
                .any(|event| event.kind == IoKind::Read && event.offset == 0)
        );
    }

    #[test]
    fn empty_inspection_reports_no_retained_roots_and_no_source_writes() {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), [4; 16], Limits::default()).unwrap();
        drop(db);
        let original = disk.durable_image().unwrap();
        let source = SimulatedDisk::from_image(&original).unwrap();
        let info = inspect_on(source.clone(), Limits::default()).unwrap();
        assert_eq!(info.revision, 0);
        assert_eq!(info.data_revision, 0);
        assert!(info.retained.is_empty());
        assert!(info.witnessed);
        assert_eq!(info.physical_bytes, original.len() as u64);
        assert_eq!(source.durable_image().unwrap(), original);
        assert!(
            source
                .trace()
                .unwrap()
                .iter()
                .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
        );
    }
}
