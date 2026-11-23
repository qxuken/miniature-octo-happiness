#[path = "support/allocator.rs"]
pub mod allocator;
use allocator::TestAllocator;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use weekend_db::{
    AllocatorHandle, Database, Error, Limits, Result,
    storage::{ByteStore, Crash, Fault, IoKind, MAX_IMAGE_BYTES, MemoryStore, SimulatedDisk},
};

fn selected_disk(image: &[u8]) -> (Arc<TestAllocator>, SimulatedDisk) {
    let allocator = Arc::new(TestAllocator::default());
    let disk = SimulatedDisk::from_image_with_allocator(image, allocator.handle()).unwrap();
    (allocator, disk)
}

#[test]
fn storage_identity_is_read_without_io_and_survives_compaction() {
    assert_eq!(
        Database::new().unwrap().storage_identity(),
        Err(Error::Invalid)
    );
    let source = SimulatedDisk::new().unwrap();
    let identity = [3; 16];
    let mut db = Database::create_on(source.clone(), identity, Limits::default()).unwrap();
    source.arm(usize::MAX, Fault::Error);
    assert_eq!(db.storage_identity(), Ok(identity));
    assert!(source.trace().unwrap().is_empty());

    let destination = SimulatedDisk::new().unwrap();
    db.compact_to(destination.clone()).unwrap();
    let reopened = Database::open_on(destination, Limits::default()).unwrap();
    assert_eq!(reopened.storage_identity(), Ok(identity));
}

struct ReportedLength {
    disk: SimulatedDisk,
    reported: Arc<AtomicU64>,
}
impl ByteStore for ReportedLength {
    fn lock(&mut self) -> Result<()> {
        self.disk.lock()
    }
    fn unlock(&mut self) {
        self.disk.unlock();
    }
    fn length(&mut self) -> Result<u64> {
        match self.reported.load(Ordering::Relaxed) {
            u64::MAX => self.disk.length(),
            n => Ok(n),
        }
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.disk.read(offset, out)
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.disk.write(offset, bytes)
    }
    fn sync(&mut self) -> Result<()> {
        self.disk.sync()
    }
}

#[test]
fn source_length_guards_distinguish_short_over_cap_and_valid_images() {
    for action in ["space", "persist", "compact"] {
        for invalid in [None, Some(0), Some(MAX_IMAGE_BYTES as u64 + 1)] {
            let disk = SimulatedDisk::new().unwrap();
            let reported = Arc::new(AtomicU64::new(u64::MAX));
            let mut db = Database::create_on(
                ReportedLength {
                    disk: disk.clone(),
                    reported: reported.clone(),
                },
                [3; 16],
                Limits::default(),
            )
            .unwrap();
            let before = disk.durable_image().unwrap();
            reported.store(invalid.unwrap_or(u64::MAX), Ordering::Relaxed);
            disk.arm(usize::MAX, Fault::Error);
            let result = match action {
                "space" => db.storage_space().map(|_| ()),
                "persist" => db.begin().commit().map(|_| ()),
                "compact" => db.compact_to(MemoryStore::new().unwrap()).map(|_| ()),
                _ => unreachable!(),
            };
            assert_eq!(
                result,
                if invalid.is_some() {
                    Err(Error::Corrupt)
                } else {
                    Ok(())
                },
                "{action} {invalid:?}"
            );
            if invalid.is_some() {
                assert_eq!(disk.durable_image().unwrap(), before);
                assert!(disk.trace().unwrap().is_empty());
                assert_eq!(db.requires_recovery(), action != "compact");
            }
        }
    }
}

#[test]
fn deferred_fault_fires_once_and_read_fault_variants_preserve_output_bounds() {
    let mut disk = SimulatedDisk::from_image(&[5, 6]).unwrap();
    disk.lock().unwrap();
    disk.arm(1, Fault::Error);
    assert_eq!(disk.length(), Ok(2));
    assert_eq!(disk.length(), Err(Error::Io));
    assert_eq!(disk.length(), Ok(2));
    assert_eq!(disk.trace().unwrap().len(), 3);
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|event| event.kind == IoKind::Length)
    );
    for (fault, expected, bytes) in [
        (Fault::Short(0), Ok(0), [9, 9]),
        (Fault::Short(1), Ok(1), [5, 9]),
        (Fault::Short(usize::MAX), Ok(2), [5, 6]),
        (Fault::Error, Err(Error::Io), [9, 9]),
        (
            Fault::Tear {
                prefix: 1,
                durable: true,
            },
            Err(Error::Io),
            [9, 9],
        ),
        (Fault::SyncError(Crash::PersistAll), Err(Error::Io), [9, 9]),
    ] {
        disk.arm(0, fault);
        let mut out = [9, 9];
        assert_eq!(disk.read(0, &mut out), expected);
        assert_eq!(out, bytes);
        assert_eq!(disk.length(), Ok(2));
    }
    assert_eq!(disk.read(2, &mut [9]), Ok(0));
}

#[test]
fn write_prefix_failure_and_immediate_durability_have_separate_controls() {
    for (fault, result, volatile, durable) in [
        (None, Ok(2), [7, 8], [5, 5]),
        (Some(Fault::Short(0)), Ok(0), [5, 5], [5, 5]),
        (Some(Fault::Short(1)), Ok(1), [7, 5], [5, 5]),
        (Some(Fault::Short(usize::MAX)), Ok(2), [7, 8], [5, 5]),
        (
            Some(Fault::Tear {
                prefix: 0,
                durable: true,
            }),
            Err(Error::Io),
            [5, 5],
            [5, 5],
        ),
        (
            Some(Fault::Tear {
                prefix: 1,
                durable: false,
            }),
            Err(Error::Io),
            [7, 5],
            [5, 5],
        ),
        (
            Some(Fault::Tear {
                prefix: 1,
                durable: true,
            }),
            Err(Error::Io),
            [7, 5],
            [7, 5],
        ),
        (Some(Fault::Error), Err(Error::Io), [5, 5], [5, 5]),
        (
            Some(Fault::SyncError(Crash::PersistAll)),
            Err(Error::Io),
            [5, 5],
            [5, 5],
        ),
    ] {
        let mut disk = SimulatedDisk::from_image(&[5, 5]).unwrap();
        disk.lock().unwrap();
        if let Some(fault) = fault {
            disk.arm(0, fault);
        }
        assert_eq!(disk.write(0, &[7, 8]), result);
        let mut out = [0; 2];
        assert_eq!(disk.read(0, &mut out), Ok(2));
        assert_eq!(out, volatile);
        assert_eq!(disk.durable_image().unwrap(), durable);
        disk.crash(Crash::DropAll).unwrap();
        assert_eq!(disk.durable_image().unwrap(), durable);
    }
}

#[test]
fn failed_sync_preserves_only_its_selected_pending_writes() {
    for (fault, expected) in [
        (Fault::Short(0), vec![0, 0]),
        (
            Fault::Tear {
                prefix: 1,
                durable: true,
            },
            vec![0, 0],
        ),
        (Fault::SyncError(Crash::DropAll), vec![0, 0]),
        (Fault::SyncError(Crash::PersistAll), vec![1, 2]),
        (Fault::SyncError(Crash::Mask(1)), vec![1, 0]),
        (
            Fault::SyncError(Crash::Torn {
                write: 1,
                prefix: 1,
            }),
            vec![0, 2],
        ),
    ] {
        let mut disk = SimulatedDisk::from_image(&[0, 0]).unwrap();
        disk.lock().unwrap();
        disk.write(0, &[1]).unwrap();
        disk.write(1, &[2]).unwrap();
        disk.arm(0, fault);
        assert_eq!(disk.sync(), Err(Error::Io));
        assert_eq!(disk.durable_image().unwrap(), expected);
        disk.sync().unwrap();
        assert_eq!(disk.durable_image().unwrap(), [1, 2]);
    }
}

#[test]
#[cfg(not(miri))]
fn simulator_image_cap_rejects_before_copying_the_input() {
    let bytes = vec![0; MAX_IMAGE_BYTES + 1];
    assert_eq!(SimulatedDisk::from_image(&bytes).err(), Some(Error::Limit));
    assert!(SimulatedDisk::from_image(&[]).is_ok());
}

#[test]
fn simulator_allocator_owns_both_image_copies_and_releases_partial_construction() {
    let allocator = Arc::new(TestAllocator::default());
    let handle = allocator.handle();
    let live = allocator.live_bytes();
    for allowance in 0..=4 {
        let result = allocator.with_refusal(allowance, || {
            SimulatedDisk::from_image_with_allocator(b"seed", handle.clone())
        });
        if allowance == 4 {
            let mut disk = result.unwrap();
            assert_eq!(disk.durable_image().unwrap(), b"seed");
            disk.lock().unwrap();
            let mut bytes = [0; 4];
            assert_eq!(disk.read(0, &mut bytes), Ok(4));
            assert_eq!(&bytes, b"seed");
        } else {
            assert_eq!(result.err(), Some(Error::OutOfMemory));
        }
        assert_eq!(allocator.live_bytes(), live);
    }
    drop(handle);
    assert_eq!(allocator.live(), 0);
}

#[test]
fn first_event_allocator_refusal_keeps_the_armed_fault() {
    let (allocator, mut disk) = selected_disk(&[]);
    disk.lock().unwrap();
    disk.arm(0, Fault::Short(0));
    assert_eq!(
        allocator.with_refusal(0, || disk.write(0, b"x")),
        Err(Error::OutOfMemory)
    );
    assert!(disk.trace().unwrap().is_empty());
    assert_eq!(disk.write(0, b"x"), Ok(0));
    assert_eq!(disk.length(), Ok(0));
    assert_eq!(disk.write(0, b"x"), Ok(1));
    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), b"x");
}

#[test]
fn crash_and_sync_allocation_failures_keep_prior_images_and_lease() {
    for budget in 0..=2 {
        let (allocator, mut disk) = selected_disk(&[0]);
        disk.lock().unwrap();
        disk.write(0, &[1]).unwrap();
        let live = allocator.live_bytes();
        let result = allocator.with_refusal(budget, || disk.crash(Crash::PersistAll));
        if budget < 2 {
            assert_eq!(result, Err(Error::OutOfMemory));
            assert_eq!(allocator.live_bytes(), live);
            assert_eq!(disk.length(), Ok(1));
            let mut byte = [0];
            disk.read(0, &mut byte).unwrap();
            assert_eq!(byte, [1]);
            assert_eq!(disk.durable_image().unwrap(), [0]);
            disk.sync().unwrap();
        } else {
            assert_eq!(result, Ok(()));
            assert_eq!(disk.length(), Err(Error::Io));
        }
        assert_eq!(disk.durable_image().unwrap(), [1]);
        drop(disk);
        assert_eq!(allocator.live(), 0);
    }
    let (allocator, mut disk) = selected_disk(&[0]);
    disk.lock().unwrap();
    disk.write(0, &[1]).unwrap();
    disk.arm(usize::MAX, Fault::Error); // reuse trace capacity, isolate sync copy
    assert_eq!(
        allocator.with_refusal(0, || disk.sync()),
        Err(Error::OutOfMemory)
    );
    assert_eq!(disk.durable_image().unwrap(), [0]);
    assert_eq!(disk.length(), Ok(1));
    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), [1]);
}

#[test]
fn durable_tear_growth_allocation_failure_leaves_both_images_and_pending_queue_unchanged() {
    for (durable, budget, expected_result) in [
        (false, 3, Err(Error::Io)),
        (true, 0, Err(Error::OutOfMemory)),
        (true, 1, Err(Error::OutOfMemory)),
        (true, 2, Err(Error::OutOfMemory)),
        (true, 3, Err(Error::OutOfMemory)),
        (true, 4, Err(Error::Io)),
    ] {
        let (allocator, mut disk) = selected_disk(&[]);
        disk.lock().unwrap();
        disk.length().unwrap(); // Reserve trace capacity before installing the budget.
        disk.arm(0, Fault::Tear { prefix: 1, durable });
        // Copy, pending capacity and volatile capacity use three allocations.
        // An immediately durable sparse tear also needs durable-image capacity.
        let result = allocator.with_refusal(budget, || disk.write(32, &[9]));
        assert_eq!(
            result, expected_result,
            "durable={durable}, budget={budget}"
        );
        let trace = disk.trace().unwrap();
        assert_eq!(trace.len(), 1);
        assert_eq!(
            (trace[0].kind, trace[0].offset, trace[0].len),
            (IoKind::Write, 32, 1)
        );

        let failed = result == Err(Error::OutOfMemory);
        let mut written = [0; 33];
        written[32] = 9;
        let volatile = if failed { &[][..] } else { &written[..] };
        assert_eq!(disk.length(), Ok(volatile.len() as u64));
        let mut out = vec![7; volatile.len()];
        assert_eq!(disk.read(0, &mut out), Ok(volatile.len()));
        assert_eq!(out, volatile);
        assert_eq!(
            disk.durable_image().unwrap(),
            if durable && !failed {
                &written[..]
            } else {
                &[][..]
            }
        );
        let mut competing = disk.clone();
        assert_eq!(competing.lock(), Err(Error::Busy));

        // PersistAll exposes an accidentally queued write after failed admission.
        disk.crash(Crash::PersistAll).unwrap();
        assert_eq!(disk.durable_image().unwrap(), volatile);
    }
}

#[test]
fn creation_identity_and_empty_destination_guards_release_failed_leases() {
    for (identity, image) in [([0; 16], &b""[..]), ([1; 16], &b"occupied"[..])] {
        let disk = SimulatedDisk::from_image(image).unwrap();
        assert_eq!(
            Database::create_on(disk.clone(), identity, Limits::default()).err(),
            Some(Error::Invalid)
        );
        assert_eq!(disk.durable_image().unwrap(), image);
        let mut probe = disk.clone();
        probe.lock().unwrap();
        probe.unlock();
        assert!(
            disk.trace()
                .unwrap()
                .iter()
                .all(|event| event.kind == IoKind::Length)
        );
    }
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [1; 16], Limits::default()).unwrap();
    assert_eq!(db.snapshot().revision(), 0);
    drop(db);
    let mut probe = disk.clone();
    probe.lock().unwrap();
    probe.unlock();
}
