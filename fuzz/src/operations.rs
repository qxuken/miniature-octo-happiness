use crate::{RAW_LIMIT, fixture_image};
use weekend_db::{
    Database, Error, Limits, Result,
    storage::{
        ByteStore, Crash, Fault, SimulatedDisk, backup_on, export_logical_on, import_logical_on,
    },
};

fn disk() -> SimulatedDisk {
    SimulatedDisk::from_image(fixture_image()).unwrap()
}
fn logical(source: SimulatedDisk) -> Vec<u8> {
    export_logical_on(source, Limits::default()).unwrap()
}
fn check(source: SimulatedDisk) {
    let db = Database::open_on(source, Limits::default()).unwrap();
    assert_eq!(db.snapshot().revision(), 4);
    let c = db.collection_id(1);
    assert_eq!(
        db.snapshot().get(c, &[0]).unwrap().unwrap().as_i64(),
        Ok(21)
    );
    assert_eq!(
        db.snapshot().get(c, &[1]).unwrap().unwrap().as_i64(),
        Ok(12)
    );
    assert_eq!(db.retained_ids().unwrap(), [1]);
    let retained = db.retained_snapshot(1).unwrap();
    assert_eq!(retained.revision(), 2);
    assert_eq!(retained.get(c, &[0]).unwrap().unwrap().as_i64(), Ok(11));
}

// Independent bitwise CRC-64/ECMA-182, checked against the standard check string.
fn logical_crc(bytes: &[u8]) -> u64 {
    let mut crc = 0u64;
    for &byte in bytes {
        for bit in (0..8).rev() {
            let high = (crc >> 63) ^ u64::from((byte >> bit) & 1);
            crc <<= 1;
            if high != 0 {
                crc ^= 0x42f0e1eba9ea3693;
            }
        }
    }
    crc
}

/// Mutated logical streams are either rejected before any destination writes,
/// or re-export exactly (the format is canonical). The clean fixture also has
/// an independent record/revision oracle, avoiding a roundtrip-only check.
pub fn logical_import_export(input: &[u8]) {
    let Some((&mode, program)) = input.split_first() else {
        return;
    };
    let source = disk();
    let before = source.durable_image().unwrap();
    let canonical = logical(source.clone());
    let mut bytes = if mode % 5 == 0 {
        program[..program.len().min(RAW_LIMIT)].to_vec()
    } else {
        canonical.clone()
    };
    if matches!(mode % 5, 1 | 4) {
        for edit in program.as_chunks::<3>().0.iter().take(32) {
            let index = usize::from(u16::from_le_bytes([edit[0], edit[1]])) % bytes.len();
            bytes[index] ^= edit[2];
        }
    } else if mode % 5 == 2 && !program.is_empty() {
        bytes.truncate(usize::from(program[0]) % bytes.len());
    }
    if mode % 5 == 4 {
        let end = bytes.len() - 8;
        let crc = logical_crc(&bytes[..end]);
        bytes[end..].copy_from_slice(&crc.to_le_bytes());
    }
    let destination = SimulatedDisk::new().unwrap();
    let result = import_logical_on(&bytes, destination.clone(), Limits::default());
    assert_eq!(source.durable_image().unwrap(), before);
    if result.is_ok() {
        destination.crash(Crash::DropAll).unwrap();
        assert_eq!(logical(destination.clone()), bytes);
        if bytes == canonical {
            check(destination);
        }
    } else {
        assert!(destination.durable_image().unwrap().is_empty());
    }
}

/// Offline backup/restore share backup_on. Faults cover copy publication and a
/// second independent failure while reopening the resulting destination.
pub fn backup_restore(input: &[u8]) {
    let &[step, mode, power, recovery, ..] = input else {
        return;
    };
    let source = disk();
    let before = source.durable_image().unwrap();
    let destination = SimulatedDisk::new().unwrap();
    destination.arm(
        usize::from(step),
        match mode % 3 {
            0 => Fault::Error,
            1 => Fault::Short(0),
            _ => Fault::SyncError(Crash::PersistAll),
        },
    );
    let result = backup_on(source.clone(), destination.clone(), Limits::default());
    assert_eq!(source.durable_image().unwrap(), before);
    destination
        .crash(if power & 1 == 0 {
            Crash::DropAll
        } else {
            Crash::PersistAll
        })
        .unwrap();
    destination.arm(usize::from(recovery), Fault::Error);
    drop(Database::open_on(destination.clone(), Limits::default()));
    destination.clear_fault();
    destination.crash(Crash::DropAll).unwrap();
    let reopened = Database::open_on(destination.clone(), Limits::default());
    if result.is_ok() {
        assert!(reopened.is_ok());
    }
    if reopened.is_ok() {
        drop(reopened);
        check(destination.clone());
        assert_eq!(logical(destination), logical(source));
    }
    // Invalid source is rejected before touching an existing destination.
    let invalid = SimulatedDisk::from_image(b"invalid image").unwrap();
    let occupied = disk();
    assert!(backup_on(invalid.clone(), occupied.clone(), Limits::default()).is_err());
    assert_eq!(invalid.durable_image().unwrap(), b"invalid image");
    assert_eq!(occupied.durable_image().unwrap(), before);
}

struct Progress {
    disk: SimulatedDisk,
    read: usize,
    write: usize,
    writes: usize,
    reads: usize,
}
impl ByteStore for Progress {
    fn lock(&mut self) -> Result<()> {
        self.disk.lock()
    }
    fn unlock(&mut self) {
        self.disk.unlock()
    }
    fn length(&mut self) -> Result<u64> {
        self.disk.length()
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.reads += 1;
        let n = if self.reads <= 64 {
            out.len().min(self.read)
        } else {
            out.len()
        };
        self.disk.read(offset, &mut out[..n])
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.writes += 1;
        let n = if self.writes <= 64 {
            bytes.len().min(self.write)
        } else {
            bytes.len()
        };
        self.disk.write(offset, &bytes[..n])
    }
    fn sync(&mut self) -> Result<()> {
        self.disk.sync()
    }
}
/// Compliant short-progress adapter: zero must terminate; positive reads and
/// writes must preserve exact bytes. This does not fabricate unsafe buffers.
pub fn bytestore_contract(input: &[u8]) {
    let &[read, write, ..] = input else { return };
    let sizes = [0, 1, 63, 4095, 4096, 8192];
    let source = disk();
    let before = source.durable_image().unwrap();
    let destination = SimulatedDisk::new().unwrap();
    let r = sizes[usize::from(read) % sizes.len()];
    let w = sizes[usize::from(write) % sizes.len()];
    let result = backup_on(
        Progress {
            disk: source.clone(),
            read: r,
            write: 8192,
            writes: 0,
            reads: 0,
        },
        Progress {
            disk: destination.clone(),
            read: 8192,
            write: w,
            writes: 0,
            reads: 0,
        },
        Limits::default(),
    );
    assert_eq!(source.durable_image().unwrap(), before);
    if r == 0 || w == 0 {
        assert!(matches!(result, Err(Error::Io) | Err(Error::Corrupt)));
    } else {
        result.unwrap_or_else(|e| panic!("r={r} w={w}: {e:?}"));
        destination.crash(Crash::DropAll).unwrap();
        check(destination);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_operations() {
        assert_eq!(logical_crc(b"123456789"), 0x6c40df5f0b497347);
        logical_import_export(&[4, 48, 0, 2]);
        let mut invalid = logical(disk());
        invalid[48..56].fill(0); // next retained ID may never be zero.
        let end = invalid.len() - 8;
        let crc = logical_crc(&invalid[..end]);
        invalid[end..].copy_from_slice(&crc.to_le_bytes());
        let destination = SimulatedDisk::new().unwrap();
        assert_eq!(
            import_logical_on(&invalid, destination.clone(), Limits::default()),
            Err(Error::Corrupt)
        );
        assert!(destination.durable_image().unwrap().is_empty());
        for mode in 0..5 {
            logical_import_export(&[mode, 0, 0, 1]);
        }
        for r in (0..6).rev() {
            for w in (0..6).rev() {
                bytestore_contract(&[r, w]);
            }
        }
        for step in 0..24 {
            for mode in 0..3 {
                backup_restore(&[step, mode, step % 2, step]);
            }
        }
    }
}
