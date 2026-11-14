use super::*;
use weekend_db::{
    Error, Result,
    storage::{Crash, IoKind},
};

type Revision = (u64, u64);

fn history(db: &Database) -> (Revision, Vec<(u64, Revision)>) {
    let current = db.snapshot();
    let retained = db
        .retained_ids()
        .unwrap()
        .into_iter()
        .map(|id| {
            let view = db.retained_snapshot(id).unwrap();
            (id, (view.revision(), view.data_revision()))
        })
        .collect();
    ((current.revision(), current.data_revision()), retained)
}

fn checked_open(input: &[u8]) -> Result<Database> {
    let disk = SimulatedDisk::from_image(input).unwrap();
    let db = match Database::open_on(disk.clone(), Limits::default()) {
        Ok(db) => db,
        Err(error) => {
            assert!(matches!(
                error,
                Error::Corrupt | Error::UnsupportedFormat | Error::Limit
            ));
            assert!(
                disk.trace()
                    .unwrap()
                    .iter()
                    .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync))
            );
            assert_eq!(disk.durable_image().unwrap(), input);
            return Err(error);
        }
    };
    let expected = history(&db);
    let witness = (3 + (expected.0.0 & 1) as usize) * REGION;
    let durable = disk.durable_image().unwrap();
    assert_eq!(durable.len(), input.len());
    assert_eq!(&durable[..witness], &input[..witness]);
    assert_eq!(&durable[witness + REGION..], &input[witness + REGION..]);
    drop(db);
    disk.crash(Crash::DropAll).unwrap();
    let reopened = Database::open_on(disk, Limits::default()).unwrap();
    assert_eq!(history(&reopened), expected);
    Ok(reopened)
}

pub fn open_image_bytes(input: &[u8]) {
    if input.len() <= RAW_LIMIT {
        let _ = checked_open(input);
    }
}

fn crc(bytes: &[u8]) -> u64 {
    let mut sum = 0u64;
    for &byte in bytes {
        sum ^= u64::from(byte) << 56;
        for _ in 0..8 {
            sum = (sum << 1) ^ (0x42f0_e1eb_a9ea_3693 & 0u64.wrapping_sub(sum >> 63));
        }
    }
    sum
}

fn resign(region: &mut [u8], field: usize) {
    region[field..field + 8].fill(0);
    let sum = crc(region);
    region[field..field + 8].copy_from_slice(&sum.to_le_bytes());
}

pub fn open_structured_image(input: &[u8]) {
    let Some((&mode, rest)) = input.split_first() else {
        return;
    };
    let mut image = fixture_image().to_vec();
    let mode = mode % 20;
    if mode >= 10 {
        // All objects in this fixture fit a single region. Mutate both metadata
        // and graph regions and optionally repair their format-level CRC.
        for patch in rest.as_chunks::<5>().0.iter().take(12) {
            let index = usize::from(patch[0]) % (image.len() / REGION);
            let region = &mut image[index * REGION..(index + 1) * REGION];
            let offset = usize::from(u16::from_le_bytes([patch[1], patch[2]])) % REGION;
            region[offset] ^= patch[3];
            if patch[4] & 1 != 0 {
                resign(region, if index < 5 { 64 } else { 48 });
            }
        }
        let _ = checked_open(&image);
        return;
    }

    // Revision 4 is in the even slot. The expected states below come from
    // the fixture history and the public recovery contract.
    let slot = REGION;
    let witness = 3 * REGION;
    let descriptor = image.len() - REGION;
    let expected = match mode {
        0 => Ok(4),
        1 | 2 => {
            if mode == 1 {
                image[slot..slot + REGION].fill(0);
            } else {
                image[slot + 64] ^= 1;
            }
            if rest.first().is_some_and(|bits| bits & 1 != 0) {
                image[witness..witness + REGION].fill(0);
                Ok(3)
            } else {
                Err(Error::Corrupt)
            }
        }
        3 => {
            image[witness..witness + REGION].fill(0);
            image[slot + 12] ^= 1;
            resign(&mut image[slot..slot + REGION], 64);
            Err(Error::Corrupt)
        }
        4 => {
            image[witness..witness + REGION].fill(0);
            image[descriptor + 64] ^= 1;
            resign(&mut image[descriptor..], 48);
            Err(Error::Corrupt)
        }
        5 => {
            image[witness..witness + REGION].fill(0);
            image[descriptor + 48] ^= 1;
            Err(Error::Corrupt)
        }
        6 => {
            image[witness + 64] ^= 1;
            Ok(4)
        }
        7 => {
            image[witness + 72] ^= 1;
            resign(&mut image[witness..witness + REGION], 64);
            Err(Error::Corrupt)
        }
        8 => {
            image[8] ^= 1;
            resign(&mut image[..REGION], 64);
            Err(Error::UnsupportedFormat)
        }
        9 => {
            image[REGION..3 * REGION].fill(0);
            image[3 * REGION..5 * REGION].fill(0);
            Err(Error::Corrupt)
        }
        _ => unreachable!(),
    };
    let actual = checked_open(&image).map(|db| {
        let view = db.snapshot();
        let collection = db.collection_id(1);
        assert_eq!(
            view.get(collection, &[0])
                .unwrap()
                .unwrap()
                .as_i64()
                .unwrap(),
            if view.revision() == 4 { 21 } else { 11 }
        );
        assert_eq!(
            view.get(collection, &[1]).unwrap().unwrap().as_i64(),
            Ok(12)
        );
        assert_eq!(db.retained_ids().unwrap(), [RETAINED]);
        let retained = db.retained_snapshot(RETAINED).unwrap();
        assert_eq!(retained.revision(), 2);
        assert_eq!(
            retained.get(collection, &[0]).unwrap().unwrap().as_i64(),
            Ok(11)
        );
        assert_eq!(
            retained.get(collection, &[1]).unwrap().unwrap().as_i64(),
            Ok(12)
        );
        view.revision()
    });
    assert_eq!(actual, expected, "structured mode {mode}");
}

#[test]
fn fixture_crc_matches_the_ecma_vector() {
    assert_eq!(crc(b"123456789"), 0x6c40_df5f_0b49_7347);
}
