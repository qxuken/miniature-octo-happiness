use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use weekend_db::{
    Database, Error, Limits, Result,
    storage::{ByteStore, Crash, Fault, IoKind, MAX_IMAGE_BYTES, MemoryStore, SimulatedDisk},
};

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
#[cfg(feature = "fault-injection")]
fn crash_and_sync_allocation_failures_keep_prior_images_and_lease() {
    for budget in 0..=2 {
        let mut disk = SimulatedDisk::from_image(&[0]).unwrap();
        disk.lock().unwrap();
        disk.write(0, &[1]).unwrap();
        disk.fail_allocations_after(Some(budget));
        let result = disk.crash(Crash::PersistAll);
        disk.fail_allocations_after(None);
        if budget < 2 {
            assert_eq!(result, Err(Error::OutOfMemory));
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
    }
    let mut disk = SimulatedDisk::from_image(&[0]).unwrap();
    disk.lock().unwrap();
    disk.write(0, &[1]).unwrap();
    disk.arm(usize::MAX, Fault::Error); // reuse trace capacity, isolate sync copy
    disk.fail_allocations_after(Some(0));
    assert_eq!(disk.sync(), Err(Error::OutOfMemory));
    disk.fail_allocations_after(None);
    assert_eq!(disk.durable_image().unwrap(), [0]);
    assert_eq!(disk.length(), Ok(1));
    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), [1]);
}

#[test]
#[cfg(feature = "fault-injection")]
fn trace_copy_and_first_event_failures_leave_io_state_unchanged() {
    let mut disk = SimulatedDisk::new().unwrap();
    disk.lock().unwrap();
    disk.fail_allocations_after(Some(0));
    assert_eq!(disk.trace(), Ok(vec![])); // empty output needs no allocation
    assert_eq!(disk.write(0, &[1]), Err(Error::OutOfMemory)); // event allocation
    disk.fail_allocations_after(None);
    assert!(disk.trace().unwrap().is_empty());
    assert_eq!(disk.length(), Ok(0));
    disk.fail_allocations_after(Some(0));
    assert_eq!(disk.trace(), Err(Error::OutOfMemory)); // nonempty output copy
    disk.fail_allocations_after(None);
    assert_eq!(disk.trace().unwrap().len(), 1);
    disk.write(0, &[1]).unwrap();
    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), [1]);
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
