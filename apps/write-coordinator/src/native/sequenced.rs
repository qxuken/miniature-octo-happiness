use super::{KEY, state_in};
use weekend_db::{Database, Error, Result, Validation, Value};

const FORMAT_COLLECTION: u64 = 1;
const DATA_COLLECTION: u64 = 2;
const FORMAT_KEY: &[u8] = b"protocol";
const FORMAT: &[u8] = b"coordinator-sequenced-v1";
const MAX_SLOTS: u64 = 1024;
const INITIAL_REVISION: u64 = 3;

pub(crate) fn initialize(db: &Database) -> Result<()> {
    let format = db.create_collection()?;
    let data = db.create_collection()?;
    assert_eq!(format.as_u64(), FORMAT_COLLECTION);
    assert_eq!(data.as_u64(), DATA_COLLECTION);
    let mut tx = db.begin();
    tx.put(format, FORMAT_KEY, &db.bytes(FORMAT)?)?;
    tx.put(data, KEY, &db.unsigned(0)?)?;
    assert_eq!(tx.commit()?, INITIAL_REVISION);
    Ok(())
}

pub(crate) fn admit(db: &Database) -> Result<()> {
    let marker = db
        .snapshot()
        .get(db.collection_id(FORMAT_COLLECTION), FORMAT_KEY)?
        .ok_or(Error::UnsupportedFormat)?;
    if marker.as_bytes().map_err(|_| Error::Corrupt)? != FORMAT {
        return Err(Error::UnsupportedFormat);
    }
    state_in(db, DATA_COLLECTION)?;
    Ok(())
}

fn number(word: &str) -> Result<u64> {
    word.parse().map_err(|_| Error::Invalid)
}

fn slot_key(slot: &str) -> Result<[u8; 6]> {
    let slot = number(slot)?;
    if slot >= MAX_SLOTS {
        return Err(Error::Invalid);
    }
    let mut key = *b"seq/\0\0";
    key[4..].copy_from_slice(&(slot as u16).to_be_bytes());
    Ok(key)
}

struct Receipt {
    sequence: u64,
    expected: u64,
    value: u64,
    revision: u64,
}

impl Receipt {
    fn decode(value: &Value, head: u64) -> Result<Self> {
        let bytes = value.as_bytes().map_err(|_| Error::Corrupt)?;
        if bytes.len() != 33 || bytes[0] != 1 {
            return Err(Error::Corrupt);
        }
        let field = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        let receipt = Self {
            sequence: field(1),
            expected: field(9),
            value: field(17),
            revision: field(25),
        };
        if receipt.sequence == 0
            || receipt.expected.checked_add(1) != Some(receipt.revision)
            || receipt
                .sequence
                .checked_add(INITIAL_REVISION)
                .is_none_or(|first| first > receipt.revision)
            || receipt.revision > head
        {
            return Err(Error::Corrupt);
        }
        Ok(receipt)
    }

    fn encode(&self) -> [u8; 33] {
        let mut bytes = [0; 33];
        bytes[0] = 1;
        bytes[1..9].copy_from_slice(&self.sequence.to_le_bytes());
        bytes[9..17].copy_from_slice(&self.expected.to_le_bytes());
        bytes[17..25].copy_from_slice(&self.value.to_le_bytes());
        bytes[25..33].copy_from_slice(&self.revision.to_le_bytes());
        bytes
    }
}

pub(crate) fn handle(db: &mut Database, line: &str) -> Result<String> {
    if db.requires_recovery() {
        return Err(Error::RecoveryRequired);
    }
    let words: Vec<_> = line.split_whitespace().collect();
    match words.as_slice() {
        ["get"] => {
            let (revision, value) = state_in(db, DATA_COLLECTION)?;
            Ok(format!("value {revision} {value}"))
        }
        ["receipt", slot] => {
            let snapshot = db.snapshot();
            let key = slot_key(slot)?;
            let Some(bytes) = snapshot.get(db.collection_id(DATA_COLLECTION), &key)? else {
                return Ok("receipt 0".to_owned());
            };
            let r = Receipt::decode(&bytes, snapshot.revision())?;
            Ok(format!(
                "receipt {} {} {} {}",
                r.sequence, r.expected, r.value, r.revision
            ))
        }
        ["set-seq", slot, sequence, expected, value] => submit(
            db,
            &slot_key(slot)?,
            number(sequence)?,
            number(expected)?,
            number(value)?,
        ),
        _ => Err(Error::Invalid),
    }
}

fn submit(db: &Database, key: &[u8], sequence: u64, expected: u64, value: u64) -> Result<String> {
    if sequence == 0 {
        return Err(Error::Invalid);
    }
    let collection = db.collection_id(DATA_COLLECTION);
    let mut tx = db.begin_with_validation(Validation::WholeDatabase);
    let (revision, current) = state_in(db, DATA_COLLECTION)?;
    let mut last = 0;
    if let Some(bytes) = tx.get(collection, key)? {
        let receipt = Receipt::decode(&bytes, revision)?;
        last = receipt.sequence;
        match sequence.cmp(&last) {
            std::cmp::Ordering::Less => return Ok(format!("retired {}", last - 1)),
            std::cmp::Ordering::Equal => {
                if (expected, value) != (receipt.expected, receipt.value) {
                    return Err(Error::Invalid);
                }
                return Ok(format!("committed {}", receipt.revision));
            }
            std::cmp::Ordering::Greater => {}
        }
    }
    if last.checked_add(1) != Some(sequence) {
        return Err(Error::Invalid);
    }
    if revision != expected {
        return Ok(format!("conflict {revision} {current}"));
    }
    let receipt = Receipt {
        sequence,
        expected,
        value,
        revision: revision.checked_add(1).ok_or(Error::Exhausted)?,
    };
    tx.put(collection, KEY, &db.unsigned(value)?)?;
    tx.put(collection, key, &db.bytes(&receipt.encode())?)?;
    assert_eq!(tx.commit()?, receipt.revision);
    Ok(format!("committed {}", receipt.revision))
}

#[cfg(test)]
mod tests {
    use super::*;
    use weekend_db::{
        Limits,
        storage::{Crash, Fault, IoKind, MemoryStore, SimulatedDisk},
    };

    fn open(disk: &SimulatedDisk) -> Database {
        Database::open_on(disk.clone(), Limits::default()).unwrap()
    }

    #[test]
    fn malformed_receipts_fail_closed_for_lookup_and_retry() {
        for (sequence, expected, revision, len, version) in [
            (1, 3, 4, 32, 1),        // length
            (1, 3, 4, 33, 2),        // version
            (0, 3, 4, 33, 1),        // reserved sequence
            (1, 4, 4, 33, 1),        // expected revision mismatch
            (2, 3, 4, 33, 1),        // sequence exceeds the committed prefix
            (1, 4, 5, 33, 1),        // receipt claims a future revision
            (1, u64::MAX, 0, 33, 1), // wrapped revision
        ] {
            let mut db = Database::new().unwrap();
            initialize(&db).unwrap();
            let mut encoded = Receipt {
                sequence,
                expected,
                value: 20,
                revision,
            }
            .encode();
            encoded[0] = version;
            let mut tx = db.begin();
            tx.put(
                db.collection_id(DATA_COLLECTION),
                b"seq/\0\0",
                &db.bytes(&encoded[..len]).unwrap(),
            )
            .unwrap();
            tx.commit().unwrap();
            drop(tx);
            assert_eq!(handle(&mut db, "receipt 0"), Err(Error::Corrupt));
            assert_eq!(handle(&mut db, "set-seq 0 1 4 30"), Err(Error::Corrupt));
            assert_eq!(handle(&mut db, "get").unwrap(), "value 4 0");
        }
    }

    #[test]
    fn failed_retirement_reopens_one_complete_receipt_and_value() {
        let original = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(original.clone(), [7; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        assert_eq!(handle(&mut db, "set-seq 0 1 3 12").unwrap(), "committed 4");
        drop(db);
        let image = original.durable_image().unwrap();

        let mut control = open(&original);
        original.arm(usize::MAX, Fault::Error);
        let next = "set-seq 0 2 4 30";
        assert_eq!(handle(&mut control, next).unwrap(), "committed 5");
        let trace = original.trace().unwrap();
        let syncs: Vec<_> = trace
            .iter()
            .enumerate()
            .filter_map(|(i, event)| (event.kind == IoKind::Sync).then_some(i))
            .collect();
        let [append, root, _]: [usize; 3] = syncs.try_into().unwrap();
        for step in 0..trace.len() {
            for crash in [Crash::DropAll, Crash::PersistAll] {
                let disk = SimulatedDisk::from_image(&image).unwrap();
                let mut db = open(&disk);
                disk.arm(step, Fault::Error);
                let failure = handle(&mut db, next);
                assert_eq!(
                    failure,
                    Err(if step <= append {
                        Error::Io
                    } else {
                        Error::OutcomeUnknown
                    })
                );
                assert_eq!(disk.trace().unwrap(), trace[..=step]);
                assert_eq!(handle(&mut db, "receipt 0"), Err(Error::RecoveryRequired));
                drop(db);
                disk.clear_fault();
                disk.crash(crash).unwrap();

                let mut reopened = open(&disk);
                admit(&reopened).unwrap();
                let published = step > root || (step == root && matches!(crash, Crash::PersistAll));
                let (state, latest, old) = if published {
                    ("value 5 30", "receipt 2 4 30 5", "retired 1")
                } else {
                    ("value 4 12", "receipt 1 3 12 4", "committed 4")
                };
                assert_eq!(handle(&mut reopened, "get").unwrap(), state);
                assert_eq!(handle(&mut reopened, "receipt 0").unwrap(), latest);
                assert_eq!(handle(&mut reopened, "set-seq 0 1 3 12").unwrap(), old);
                assert_eq!(handle(&mut reopened, next).unwrap(), "committed 5");
                drop(reopened);
                disk.crash(Crash::DropAll).unwrap();
                let mut reopened = open(&disk);
                assert_eq!(handle(&mut reopened, "get").unwrap(), "value 5 30");
                assert_eq!(
                    handle(&mut reopened, "receipt 0").unwrap(),
                    "receipt 2 4 30 5"
                );
                assert_eq!(
                    handle(&mut reopened, "set-seq 0 1 5 99").unwrap(),
                    "retired 1"
                );
                assert_eq!(handle(&mut reopened, next).unwrap(), "committed 5");
                assert_eq!(reopened.snapshot().revision(), 5);
            }
        }
    }

    #[test]
    fn one_slot_accepts_more_than_a_thousand_commits_with_a_single_receipt() {
        let mut db =
            Database::create_on(MemoryStore::new().unwrap(), [8; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        for seq in 1..=1050 {
            let expected = seq + 2;
            let command = format!("set-seq 1023 {seq} {expected} {seq}");
            assert_eq!(
                handle(&mut db, &command).unwrap(),
                format!("committed {}", expected + 1)
            );
            if seq % 64 == 0 {
                let copy = MemoryStore::new().unwrap();
                db.compact_to(copy.clone()).unwrap();
                db = Database::open_on(copy, Limits::default()).unwrap();
                admit(&db).unwrap();
                assert_eq!(
                    handle(&mut db, &command).unwrap(),
                    format!("committed {}", expected + 1)
                );
            }
        }
        assert_eq!(handle(&mut db, "get").unwrap(), "value 1053 1050");
        assert_eq!(
            handle(&mut db, "receipt 1023").unwrap(),
            "receipt 1050 1052 1050 1053"
        );
        assert_eq!(
            handle(&mut db, "set-seq 1023 1 1053 1").unwrap(),
            "retired 1049"
        );
        assert_eq!(
            db.snapshot()
                .scan(db.collection_id(DATA_COLLECTION))
                .unwrap()
                .len(),
            2
        );
    }
}
