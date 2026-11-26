#[path = "support/bytestore_contract.rs"]
mod contract;

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Mutex},
};
use weekend_db::{
    Database, Error, Limits, Result,
    storage::{ByteStore, Crash, Fault, MAX_IMAGE_BYTES, REGION, SimulatedDisk, inspect_on},
};

#[test]
fn lease_clones_crashes_and_stale_unlocks_preserve_exclusion() {
    let mut first = SimulatedDisk::new().unwrap();
    let mut second = first.clone();
    assert_eq!(first.length(), Err(Error::Io));
    first.lock().unwrap();
    assert_eq!(first.length(), Ok(0));
    assert_eq!(second.lock(), Err(Error::Busy));
    assert_eq!(second.length(), Err(Error::Io));
    second.unlock();
    assert_eq!(first.length(), Ok(0));
    first.crash(Crash::DropAll).unwrap();
    assert_eq!(first.length(), Err(Error::Io));
    second.lock().unwrap();
    assert_eq!(first.length(), Err(Error::Io));
    first.unlock();
    assert_eq!(second.length(), Ok(0));
    assert_eq!(first.lock(), Err(Error::Busy));
    second.unlock();
    first.lock().unwrap();
    assert_eq!(first.length(), Ok(0));
}

#[test]
fn crash_policies_cover_writes_beyond_the_mask_word_and_prefix_bounds() {
    for (policy, expected) in [
        (Crash::DropAll, vec![0; 66]),
        (Crash::PersistAll, vec![1; 66]),
        (Crash::Mask(u64::MAX), [vec![1; 64], vec![0; 2]].concat()),
        (
            Crash::Torn {
                write: 64,
                prefix: usize::MAX,
            },
            {
                let mut bytes = vec![0; 66];
                bytes[64] = 1;
                bytes
            },
        ),
        (
            Crash::Torn {
                write: 64,
                prefix: 0,
            },
            vec![0; 66],
        ),
        (
            Crash::Torn {
                write: 66,
                prefix: 1,
            },
            vec![0; 66],
        ),
    ] {
        let mut disk = SimulatedDisk::from_image(&[0; 66]).unwrap();
        disk.lock().unwrap();
        for offset in 0..66 {
            assert_eq!(disk.write(offset, &[1]), Ok(1));
        }
        disk.crash(policy).unwrap();
        assert_eq!(disk.durable_image().unwrap(), expected);
    }
    let mut disk = SimulatedDisk::from_image(&[0]).unwrap();
    disk.lock().unwrap();
    disk.write(0, &[1]).unwrap();
    disk.write(0, &[2]).unwrap();
    disk.crash(Crash::Mask(3)).unwrap();
    assert_eq!(
        disk.durable_image().unwrap(),
        [1],
        "masked writes survive in reverse call order"
    );
}

#[test]
fn empty_persisted_prefix_does_not_extend_the_durable_image() {
    let original = b"stable";
    let tear = Crash::Torn {
        write: 0,
        prefix: 0,
    };
    for during_sync in [false, true] {
        let mut disk = SimulatedDisk::from_image(original).unwrap();
        disk.lock().unwrap();
        assert_eq!(disk.write(32, b"new"), Ok(3));
        assert_eq!(disk.length(), Ok(35));

        let crash = if during_sync {
            disk.arm(0, Fault::SyncError(tear));
            assert_eq!(disk.sync(), Err(Error::Io));
            assert_eq!(disk.durable_image().unwrap(), original);
            Crash::DropAll
        } else {
            tear
        };
        disk.crash(crash).unwrap();
        assert_eq!(disk.durable_image().unwrap(), original);
        disk.lock().unwrap();
        assert_eq!(disk.length(), Ok(original.len() as u64));
    }
}

#[test]
fn byte_ranges_and_one_shot_faults_leave_a_usable_lease() {
    let mut disk = SimulatedDisk::new().unwrap();
    disk.lock().unwrap();
    assert_eq!(disk.read(MAX_IMAGE_BYTES as u64, &mut []), Ok(0));
    assert_eq!(
        disk.read(MAX_IMAGE_BYTES as u64, &mut [0]),
        Err(Error::Limit)
    );
    assert_eq!(disk.write(u64::MAX, &[1]), Err(Error::Limit));
    assert_eq!(
        disk.write(MAX_IMAGE_BYTES as u64 + 1, &[]),
        Err(Error::Limit)
    );
    assert_eq!(disk.write(0, &[1, 2]), Ok(2));
    disk.arm(0, Fault::Short(1));
    assert_eq!(disk.read(0, &mut [0; 2]), Ok(1));
    assert_eq!(disk.read(0, &mut [0; 2]), Ok(2));
    disk.arm(0, Fault::Error);
    assert_eq!(disk.sync(), Err(Error::Io));
    assert_eq!(disk.durable_image().unwrap(), []);
    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), [1, 2]);
}

#[test]
fn consecutive_failed_syncs_preserve_pending_writes_until_successful_retry() {
    for (mask, expected) in [(0, b"-c--"), (1, b"ab--"), (2, b"-cd-"), (3, b"abd-")] {
        let mut disk = SimulatedDisk::from_image(b"----").unwrap();
        disk.lock().unwrap();
        assert_eq!(disk.write(0, b"ab"), Ok(2));
        assert_eq!(disk.write(1, b"cd"), Ok(2));

        disk.arm(
            0,
            Fault::SyncError(Crash::Torn {
                write: 1,
                prefix: 1,
            }),
        );
        assert_eq!(disk.sync(), Err(Error::Io));
        assert_eq!(disk.durable_image().unwrap(), b"-c--");

        disk.arm(0, Fault::SyncError(Crash::Mask(mask)));
        assert_eq!(disk.sync(), Err(Error::Io));
        assert_eq!(disk.durable_image().unwrap(), expected, "mask {mask}");
        let mut visible = [0; 4];
        assert_eq!(disk.read(0, &mut visible), Ok(4));
        assert_eq!(&visible, b"acd-");

        disk.sync().unwrap();
        assert_eq!(disk.durable_image().unwrap(), b"acd-");
        assert_eq!(disk.write(3, b"!"), Ok(1));
        disk.crash(Crash::Mask(1)).unwrap();
        disk.lock().unwrap();
        assert_eq!(disk.read(0, &mut visible), Ok(4));
        assert_eq!(&visible, b"acd!");
    }
}

#[derive(Clone, Copy)]
struct SyncFault {
    barrier: u8,
    persist: bool,
}

#[derive(Default)]
struct Image {
    leased: bool,
    visible: Vec<u8>,
    durable: Vec<u8>,
    syncs: u8,
    fault: Option<SyncFault>,
}

// Independent ByteStore implementation with short transfers. A restart loads
// only the durable bytes into a new set of handles.
struct PageDevice {
    image: Arc<Mutex<Image>>,
    leased: bool,
}

const MAX_TRANSFER: usize = 251;

impl PageDevice {
    fn handle(image: &Arc<Mutex<Image>>) -> Self {
        Self {
            image: image.clone(),
            leased: false,
        }
    }
}

impl Drop for PageDevice {
    fn drop(&mut self) {
        self.unlock();
    }
}

impl ByteStore for PageDevice {
    fn lock(&mut self) -> Result<()> {
        let mut image = self.image.lock().unwrap();
        if image.leased {
            return Err(Error::Busy);
        }
        image.leased = true;
        self.leased = true;
        Ok(())
    }

    fn unlock(&mut self) {
        if std::mem::take(&mut self.leased) {
            self.image.lock().unwrap().leased = false;
        }
    }

    fn length(&mut self) -> Result<u64> {
        assert!(self.leased);
        Ok(self.image.lock().unwrap().visible.len() as u64)
    }

    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        assert!(self.leased);
        let image = self.image.lock().unwrap();
        let offset = usize::try_from(offset).map_err(|_| Error::Limit)?;
        let available = image.visible.get(offset..).unwrap_or_default();
        let count = available.len().min(out.len()).min(MAX_TRANSFER);
        out[..count].copy_from_slice(&available[..count]);
        Ok(count)
    }

    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        assert!(self.leased);
        let offset = usize::try_from(offset).map_err(|_| Error::Limit)?;
        assert!(bytes.len() <= REGION - offset % REGION);
        let count = bytes.len().min(MAX_TRANSFER);
        let end = offset
            .checked_add(count)
            .filter(|end| *end <= MAX_IMAGE_BYTES)
            .ok_or(Error::Limit)?;
        if count != 0 {
            let mut image = self.image.lock().unwrap();
            if end > image.visible.len() {
                image.visible.resize(end, 0);
            }
            image.visible[offset..end].copy_from_slice(&bytes[..count]);
        }
        Ok(count)
    }

    fn sync(&mut self) -> Result<()> {
        assert!(self.leased);
        let image = &mut *self.image.lock().unwrap();
        image.syncs += 1;
        let failure = image.fault.take_if(|fault| fault.barrier == image.syncs);
        if failure.is_none_or(|fault| fault.persist) {
            image.durable.clone_from(&image.visible);
        }
        failure.map_or(Ok(()), |_| Err(Error::Io))
    }
}

#[test]
fn independent_short_transfer_backend_passes_public_lifecycle_contract() {
    let images = RefCell::new(HashMap::<String, Arc<Mutex<Image>>>::new());
    contract::check_lifecycle(
        |name| {
            let image = Arc::new(Mutex::new(Image::default()));
            assert!(
                images
                    .borrow_mut()
                    .insert(name.to_owned(), image.clone())
                    .is_none()
            );
            Ok(PageDevice::handle(&image))
        },
        |name| Ok(PageDevice::handle(&images.borrow()[name])),
    );
}

#[test]
fn independent_backend_reopens_durable_bytes_after_commit_and_barrier_failures() {
    let limits = Limits::default();
    for (barrier, persist, error, revision, witnessed) in [
        (1, false, Some(Error::Io), 3, true),
        (1, true, Some(Error::Io), 3, true),
        (2, false, Some(Error::OutcomeUnknown), 3, true),
        (2, true, Some(Error::OutcomeUnknown), 4, false),
        (3, false, Some(Error::OutcomeUnknown), 4, false),
        (3, true, Some(Error::OutcomeUnknown), 4, true),
        (3, true, None, 4, true),
    ] {
        let image = Arc::new(Mutex::new(Image::default()));
        let db = Database::create_on(PageDevice::handle(&image), contract::ID, limits).unwrap();
        contract::seed(&db);
        {
            let mut image = image.lock().unwrap();
            image.syncs = 0;
            image.fault = error.map(|_| SyncFault { barrier, persist });
        }
        assert_eq!(
            contract::write_value(&db, 2),
            error.map_or(Ok(4), Err),
            "barrier {barrier}, persist {persist}"
        );
        assert_eq!(db.requires_recovery(), error.is_some());
        if error.is_some() {
            contract::check_views(&db, 3, 2, 1);
            assert_eq!(contract::write_value(&db, 2), Err(Error::RecoveryRequired));
        } else {
            contract::check_views(&db, 4, 4, 2);
        }
        drop(db);
        let durable = {
            let image = image.lock().unwrap();
            assert!(!image.leased);
            assert!(image.fault.is_none());
            assert_eq!(image.syncs, barrier);
            image.durable.clone()
        };

        let restarted = Arc::new(Mutex::new(Image {
            visible: durable.clone(),
            durable: durable.clone(),
            ..Image::default()
        }));
        let inspected = inspect_on(PageDevice::handle(&restarted), limits).unwrap();
        assert_eq!(
            (inspected.revision, inspected.witnessed),
            (revision, witnessed)
        );
        assert_eq!(restarted.lock().unwrap().durable, durable);
        let db = Database::open_on(PageDevice::handle(&restarted), limits).unwrap();
        let (data_revision, value) = if revision == 3 { (2, 1) } else { (4, 2) };
        contract::check_views(&db, revision, data_revision, value);
        drop(db);
        assert!(
            inspect_on(PageDevice::handle(&restarted), limits)
                .unwrap()
                .witnessed
        );
    }
}
