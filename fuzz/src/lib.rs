mod crash;
mod foreign;
mod history;
mod image;
mod schedule;
mod values;

pub use crash::commit_crash;
pub use foreign::ffi_ops;
pub use history::{image_and_ops, retention_compact_crash};
pub use image::{open_image_bytes, open_structured_image};
pub use schedule::txn_schedule;
pub use values::value_ops;

use std::sync::OnceLock;
use weekend_db::{
    Database, Limits,
    storage::{REGION, SimulatedDisk},
};

pub const RAW_LIMIT: usize = 256 * 1024;
const ID: [u8; 16] = *b"weekend-fuzz-001";
const RETAINED: u64 = 1;

/// Shared fixture with known states at each stage. The latest root is 4,
/// its predecessor is 3, and both retain revision 2.
pub fn fixture_image() -> &'static [u8] {
    static IMAGE: OnceLock<Vec<u8>> = OnceLock::new();
    IMAGE.get_or_init(|| {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        assert_eq!(collection.as_u64(), 1);
        let mut tx = db.begin();
        for (key, value) in [(0, 11), (1, 12)] {
            tx.put(collection, &[key], &db.integer(value).unwrap())
                .unwrap();
        }
        assert_eq!(tx.commit(), Ok(2));
        assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(RETAINED));
        let mut tx = db.begin();
        tx.put(collection, &[0], &db.integer(21).unwrap()).unwrap();
        assert_eq!(tx.commit(), Ok(4));
        let image = disk.durable_image().unwrap();
        assert!(image.len() <= RAW_LIMIT);
        image
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_inputs_replay_without_the_fuzz_runtime() {
        open_image_bytes(b"WEEKDB01");
        open_image_bytes(fixture_image());
        for mode in 0..10 {
            for flags in [0, 1] {
                open_structured_image(&[mode, flags]);
            }
        }
        open_structured_image(&[10, 0, 0, 0, 1, 1, 5, 8, 0, 1, 1]);
        for step in 0..12 {
            for (mode, power) in [(0, 0), (1, 1), (2, 2), (4, 3)] {
                commit_crash(&[step, mode, power, 1, mode & 1, 0, 7, 0x81, 0, 4, 9]);
            }
        }
    }

    #[test]
    fn generated_values_and_history_replay() {
        ffi_ops(&[0, 24, 9, 1, 0, 1, 2, 1, 0, 2, 1, 2, 2, 1, 3, 4, 0, 0]);
        ffi_ops(&[
            0, 16, 7, 128, 0, 42, 3, 0, 42, 130, 17, 8, 3, 1, 43, 128, 1, 43,
        ]);
        value_ops(&[48, 128, 7, 4, 32, 0, 6, 31, 0, 35, 0, 2, 7, 255, 0]);
        let commands = [2, 0, 0, 0, 0, 7, 2, 0, 0, 3, 0, 1, 5, 0, 0, 6, 0, 0];
        for mode in 0..5 {
            let mut input = vec![mode];
            input.extend_from_slice(&commands);
            image_and_ops(&input);
        }
        for at in 0..24 {
            let mut input = vec![at, at % 4, at % 2];
            input.extend_from_slice(&commands);
            retention_compact_crash(&input);
        }
    }
}

mod crdt;
pub use crdt::{crdt_frames, crdt_schedule};

mod snapshot;
pub use snapshot::{snapshot_frames, snapshot_install_schedule};
