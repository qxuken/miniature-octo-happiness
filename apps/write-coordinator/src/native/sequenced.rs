use super::{KEY, counter_at, state_in};
use weekend_db::{CollectionId, Database, Error, Result, Snapshot, Validation, Value};

const FORMAT_COLLECTION: u64 = 1;
const DATA_COLLECTION: u64 = 2;
const FORMAT_KEY: &[u8] = b"protocol";
const FORMAT: &[u8] = b"coordinator-sequenced-v1";
pub(crate) const MAX_SLOTS: u64 = 1024;
pub(crate) const INITIAL_REVISION: u64 = 3;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Receipt {
    pub(crate) sequence: u64,
    pub(crate) expected: u64,
    pub(crate) value: u64,
    pub(crate) revision: u64,
}

impl Receipt {
    fn decode(value: &Value, head: u64) -> Result<Self> {
        Self::from_bytes(value.as_bytes().map_err(|_| Error::Corrupt)?, head)
    }

    pub(crate) fn from_bytes(bytes: &[u8], head: u64) -> Result<Self> {
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

    pub(crate) fn encode(&self) -> [u8; 33] {
        let mut bytes = [0; 33];
        bytes[0] = 1;
        bytes[1..9].copy_from_slice(&self.sequence.to_le_bytes());
        bytes[9..17].copy_from_slice(&self.expected.to_le_bytes());
        bytes[17..25].copy_from_slice(&self.value.to_le_bytes());
        bytes[25..33].copy_from_slice(&self.revision.to_le_bytes());
        bytes
    }

    pub(crate) fn reply(&self) -> String {
        format!(
            "receipt {} {} {} {}",
            self.sequence, self.expected, self.value, self.revision
        )
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(2 * bytes.len());
    for &b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}

pub(crate) fn unhex<const N: usize>(word: &str) -> Result<[u8; N]> {
    if word.len() != 2 * N {
        return Err(Error::Invalid);
    }
    fn digit(b: u8) -> Result<u8> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err(Error::Invalid),
        }
    }
    let mut decoded = [0; N];
    for (output, pair) in decoded.iter_mut().zip(word.as_bytes().as_chunks::<2>().0) {
        *output = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Ok(decoded)
}

fn require_identity(db: &Database, identity: &str) -> Result<()> {
    if unhex::<16>(identity)? != db.storage_identity()? {
        return Err(Error::ForeignStore);
    }
    Ok(())
}

fn value_reply(db: &Database) -> Result<String> {
    let (revision, value) = state_in(db, DATA_COLLECTION)?;
    Ok(format!("value {revision} {value}"))
}

fn receipt_at(
    snapshot: &Snapshot<'_>,
    collection: CollectionId,
    slot: &str,
) -> Result<Option<Receipt>> {
    let key = slot_key(slot)?;
    snapshot
        .get(collection, &key)?
        .as_ref()
        .map(|bytes| Receipt::decode(bytes, snapshot.revision()))
        .transpose()
}

fn receipt_reply(db: &Database, slot: &str) -> Result<String> {
    let receipt = receipt_at(&db.snapshot(), db.collection_id(DATA_COLLECTION), slot)?;
    Ok(receipt.map_or_else(|| "receipt 0".to_owned(), |receipt| receipt.reply()))
}

fn checkpoint_reply(db: &Database, slot: &str) -> Result<String> {
    let snapshot = db.snapshot();
    let collection = db.collection_id(DATA_COLLECTION);
    let revision = snapshot.revision();
    let value = counter_at(&snapshot, collection)?;
    let receipt = receipt_at(&snapshot, collection, slot)?
        .map_or_else(|| "-".to_owned(), |receipt| hex(&receipt.encode()));
    Ok(format!("checkpoint {revision} {value} {receipt}"))
}

enum Predecessor {
    Unchecked,
    Exact(Option<Receipt>),
}

pub(crate) fn handle(db: &mut Database, line: &str) -> Result<String> {
    if db.requires_recovery() {
        return Err(Error::RecoveryRequired);
    }
    let words: Vec<_> = line.split_whitespace().collect();
    match words.as_slice() {
        ["get"] => value_reply(db),
        ["receipt", slot] => receipt_reply(db, slot),
        ["identity"] => Ok(format!("identity {}", hex(&db.storage_identity()?))),
        ["journal-get", identity] => {
            require_identity(db, identity)?;
            value_reply(db)
        }
        ["journal-receipt", identity, slot] => {
            require_identity(db, identity)?;
            receipt_reply(db, slot)
        }
        ["journal-check", identity, slot] => {
            require_identity(db, identity)?;
            checkpoint_reply(db, slot)
        }
        [
            "journal-set",
            identity,
            slot,
            sequence,
            previous,
            expected,
            value,
        ] => {
            require_identity(db, identity)?;
            let previous = if *previous == "-" {
                None
            } else {
                let bytes = unhex::<33>(previous)?;
                Some(Receipt::from_bytes(&bytes, u64::MAX).map_err(|_| Error::Invalid)?)
            };
            let sequence = number(sequence)?;
            let expected = number(expected)?;
            if previous.map_or(0, |r| r.sequence).checked_add(1) != Some(sequence)
                || previous.is_some_and(|r| r.revision > expected)
            {
                return Err(Error::Invalid);
            }
            submit(
                db,
                &slot_key(slot)?,
                sequence,
                expected,
                number(value)?,
                Predecessor::Exact(previous),
            )
        }
        ["set-seq", slot, sequence, expected, value] => submit(
            db,
            &slot_key(slot)?,
            number(sequence)?,
            number(expected)?,
            number(value)?,
            Predecessor::Unchecked,
        ),
        _ => Err(Error::Invalid),
    }
}

fn submit(
    db: &Database,
    key: &[u8],
    sequence: u64,
    expected: u64,
    value: u64,
    predecessor: Predecessor,
) -> Result<String> {
    if sequence == 0 {
        return Err(Error::Invalid);
    }
    let collection = db.collection_id(DATA_COLLECTION);
    let mut tx = db.begin_with_validation(Validation::WholeDatabase);
    let (revision, current) = state_in(db, DATA_COLLECTION)?;
    let previous = tx
        .get(collection, key)?
        .as_ref()
        .map(|bytes| Receipt::decode(bytes, revision))
        .transpose()?;
    let last = previous.map_or(0, |receipt| receipt.sequence);
    if let Some(receipt) = previous {
        match sequence.cmp(&last) {
            std::cmp::Ordering::Less => return Ok(format!("retired {}", receipt.sequence - 1)),
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
    if let Predecessor::Exact(expected) = predecessor
        && expected != previous
    {
        return Err(Error::Conflict);
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
use weekend_db as allocator_engine;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../tests/support/allocator.rs"]
mod allocator;
#[cfg(test)]
#[path = "../../../../tests/support/logical_checksum.rs"]
mod logical_checksum;

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
    fn sequenced_reads_preserve_dispatch_and_storage_errors() {
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
        let before = disk.durable_image().unwrap();
        assert_eq!(handle(&mut db, "receipt 0"), Err(Error::NotFound));
        assert_eq!(disk.durable_image().unwrap(), before);
        initialize(&db).unwrap();
        let identity = hex(&[7; 16]);
        let before = disk.durable_image().unwrap();
        for (input, reply) in [
            ("get".to_owned(), "value 3 0".to_owned()),
            ("receipt 0".to_owned(), "receipt 0".to_owned()),
            ("identity".to_owned(), format!("identity {identity}")),
            (format!("journal-get {identity}"), "value 3 0".to_owned()),
            (
                format!("journal-receipt {identity} 0"),
                "receipt 0".to_owned(),
            ),
            (
                format!("journal-check {identity} 0"),
                "checkpoint 3 0 -".to_owned(),
            ),
        ] {
            assert_eq!(handle(&mut db, &input), Ok(reply), "{input}");
        }
        for input in [
            "",
            "get extra",
            "unknown",
            "receipt invalid",
            "receipt 1024",
        ] {
            assert_eq!(handle(&mut db, input), Err(Error::Invalid), "{input}");
        }
        assert_eq!(disk.durable_image().unwrap(), before);

        let collection = db.collection_id(DATA_COLLECTION);
        let mut tx = db.begin();
        tx.put(
            collection,
            &slot_key("0").unwrap(),
            &db.unsigned(1).unwrap(),
        )
        .unwrap();
        assert_eq!(tx.commit(), Ok(4));
        drop(tx);
        let before = disk.durable_image().unwrap();
        for input in [
            "receipt 0".to_owned(),
            format!("journal-receipt {identity} 0"),
            format!("journal-check {identity} 0"),
        ] {
            assert_eq!(handle(&mut db, &input), Err(Error::Corrupt), "{input}");
        }
        assert_eq!(disk.durable_image().unwrap(), before);

        let mut tx = db.begin();
        tx.delete(collection, KEY).unwrap();
        assert_eq!(tx.commit(), Ok(5));
        drop(tx);
        let before = disk.durable_image().unwrap();
        assert_eq!(
            handle(&mut db, &format!("journal-check {identity} 0")),
            Err(Error::NotFound)
        );
        assert_eq!(db.snapshot().revision(), 5);
        assert_eq!(disk.durable_image().unwrap(), before);
    }

    #[test]
    fn sequenced_requests_check_store_identity_before_operands() {
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        let before = disk.durable_image().unwrap();
        let foreign = hex(&[8; 16]);
        for (identity, error) in [
            (foreign.as_str(), Error::ForeignStore),
            ("bad", Error::Invalid),
        ] {
            for input in [
                format!("journal-get {identity}"),
                format!("journal-receipt {identity} invalid"),
                format!("journal-check {identity} invalid"),
                format!("journal-set {identity} invalid bad - bad bad"),
            ] {
                assert_eq!(handle(&mut db, &input), Err(error), "{input}");
            }
        }
        assert_eq!(db.snapshot().revision(), 3);
        assert_eq!(disk.durable_image().unwrap(), before);

        let mut without_storage = Database::new().unwrap();
        initialize(&without_storage).unwrap();
        let input = format!("journal-get {}", hex(&[7; 16]));
        assert_eq!(handle(&mut without_storage, &input), Err(Error::Invalid));
        assert_eq!(handle(&mut without_storage, "get"), Ok("value 3 0".into()));
    }

    #[test]
    fn sequenced_write_fields_reject_invalid_operands_before_submission() {
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        let identity = hex(&[7; 16]);
        assert_eq!(
            handle(&mut db, &format!("journal-set {identity} 0 1 - 3 12")),
            Ok("committed 4".into())
        );
        let receipt = Receipt {
            sequence: 1,
            expected: 3,
            value: 12,
            revision: 4,
        };
        let previous = hex(&receipt.encode());
        let mut wrong_version = receipt.encode();
        wrong_version[0] = 2;
        let wrong_version = hex(&wrong_version);
        let journal = ["journal-set", &identity, "0", "2", &previous, "4", "30"];
        let plain = ["set-seq", "0", "2", "4", "30"];
        let before = disk.durable_image().unwrap();
        for (index, replacement) in [
            (2, "bad-slot"),
            (2, "1024"),
            (3, "bad-sequence"),
            (4, "bad-predecessor"),
            (4, wrong_version.as_str()),
            (5, "bad-revision"),
            (6, "18446744073709551616"),
        ] {
            let mut input = journal;
            input[index] = replacement;
            let input = input.join(" ");
            assert_eq!(handle(&mut db, &input), Err(Error::Invalid), "{input}");
        }
        for (index, replacement) in [
            (1, "bad-slot"),
            (1, "1024"),
            (2, "bad-sequence"),
            (3, "bad-revision"),
            (4, "18446744073709551616"),
        ] {
            let mut input = plain;
            input[index] = replacement;
            let input = input.join(" ");
            assert_eq!(handle(&mut db, &input), Err(Error::Invalid), "{input}");
        }
        assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 1 3 12 4".into()));
        assert_eq!(handle(&mut db, "get"), Ok("value 4 12".into()));
        assert_eq!(disk.durable_image().unwrap(), before);
        assert_eq!(
            handle(&mut db, &journal.join(" ")),
            Ok("committed 5".into())
        );
        assert_eq!(
            handle(&mut db, "set-seq 0 3 5 40"),
            Ok("committed 6".into())
        );
        let latest = Receipt {
            sequence: 3,
            expected: 5,
            value: 40,
            revision: 6,
        };
        assert_eq!(
            handle(&mut db, &format!("journal-check {identity} 0")),
            Ok(format!("checkpoint 6 40 {}", hex(&latest.encode())))
        );
    }

    #[test]
    fn sequenced_hex_checks_width_and_both_digits() {
        let bytes = [
            0x00, 0x01, 0x02, 0x03, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x10, 0x20, 0xf1, 0xfe,
            0xff, 0x00,
        ];
        let encoded = "00010203456789abcdef1020f1feff00";
        assert_eq!(hex(&[]), "");
        assert_eq!(hex(&bytes), encoded);
        assert_eq!(unhex::<16>(encoded), Ok(bytes));
        for input in [
            encoded[..30].to_owned(),
            format!("{encoded}00"),
            format!("X{}", &encoded[1..]),
            format!("0X{}", &encoded[2..]),
        ] {
            assert_eq!(unhex::<16>(&input), Err(Error::Invalid), "{input}");
        }
    }

    #[test]
    fn initialization_counter_allocation_refusal_keeps_the_seed_unpublished() {
        use super::allocator::TestAllocator;
        use std::sync::Arc;

        let probe_allocator = Arc::new(TestAllocator::default());
        let probe = Database::create_on_with_options(
            SimulatedDisk::new().unwrap(),
            [7; 16],
            probe_allocator.options(),
        )
        .unwrap();
        let start = probe_allocator.requests();
        let format = probe.create_collection().unwrap();
        probe.create_collection().unwrap();
        let mut tx = probe.begin();
        tx.put(format, FORMAT_KEY, &probe.bytes(FORMAT).unwrap())
            .unwrap();
        let allowed = probe_allocator.requests() - start;
        drop(tx);

        let allocator = Arc::new(TestAllocator::default());
        let disk = SimulatedDisk::new().unwrap();
        let db =
            Database::create_on_with_options(disk.clone(), [7; 16], allocator.options()).unwrap();
        assert_eq!(
            allocator.with_refusal(allowed, || initialize(&db)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(db.snapshot().revision(), 2);
        assert_eq!(allocator.live_bytes(), probe_allocator.live_bytes());
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        let reopened = open(&disk);
        assert_eq!(admit(&reopened), Err(Error::UnsupportedFormat));
        assert_eq!(
            reopened
                .snapshot()
                .count(reopened.collection_id(FORMAT_COLLECTION)),
            Ok(0)
        );
        assert_eq!(
            reopened
                .snapshot()
                .count(reopened.collection_id(DATA_COLLECTION)),
            Ok(0)
        );
    }

    #[test]
    fn sequenced_submit_preserves_counter_read_errors() {
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        let collection = db.collection_id(DATA_COLLECTION);
        for (counter, error) in [
            (Some(db.bytes(b"wrong type").unwrap()), Error::Type),
            (None, Error::NotFound),
        ] {
            let mut tx = db.begin();
            match counter {
                Some(value) => tx.put(collection, KEY, &value).unwrap(),
                None => tx.delete(collection, KEY).unwrap(),
            }
            let revision = tx.commit().unwrap();
            drop(tx);
            let image = disk.durable_image().unwrap();
            let command = format!("set-seq 0 1 {revision} 7");
            assert_eq!(handle(&mut db, &command), Err(error));
            assert_eq!(db.snapshot().revision(), revision);
            assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 0".into()));
            assert_eq!(disk.durable_image().unwrap(), image);
        }
        let mut tx = db.begin();
        tx.put(collection, KEY, &db.unsigned(0).unwrap()).unwrap();
        assert_eq!(tx.commit(), Ok(6));
        drop(tx);
        assert_eq!(handle(&mut db, "set-seq 0 1 6 7"), Ok("committed 7".into()));
    }

    #[test]
    fn sequenced_draft_limits_leave_current_and_retained_rows_unchanged() {
        let seed = SimulatedDisk::new().unwrap();
        let db = Database::create_on(seed.clone(), [7; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        let retained = db.retain_snapshot(&db.snapshot()).unwrap();
        drop(db);
        let initial = seed.durable_image().unwrap();
        for limits in [
            Limits {
                max_observations: 0,
                ..Limits::default()
            },
            Limits {
                max_writes: 0,
                ..Limits::default()
            },
            Limits {
                max_bytes: 32,
                ..Limits::default()
            },
            Limits {
                max_writes: 1,
                ..Limits::default()
            },
        ] {
            let disk = SimulatedDisk::from_image(&initial).unwrap();
            let mut db = Database::open_on(disk.clone(), limits).unwrap();
            let before = disk.durable_image().unwrap();
            let command = "set-seq 0 1 4 30";
            assert_eq!(handle(&mut db, command), Err(Error::Limit), "{limits:?}");
            assert_eq!(handle(&mut db, "get"), Ok("value 4 0".into()));
            assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 0".into()));
            assert_eq!(disk.durable_image().unwrap(), before);
            drop(db);
            disk.crash(Crash::DropAll).unwrap();
            let mut db = open(&disk);
            assert_eq!(handle(&mut db, "get"), Ok("value 4 0".into()));
            assert_eq!(handle(&mut db, command), Ok("committed 5".into()));
            assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 1 4 30 5".into()));
            let committed = disk.durable_image().unwrap();
            assert_eq!(handle(&mut db, command), Ok("committed 5".into()));
            assert_eq!(disk.durable_image().unwrap(), committed);
            let snapshot = db.retained_snapshot(retained).unwrap();
            let collection = db.collection_id(DATA_COLLECTION);
            assert_eq!(snapshot.revision(), INITIAL_REVISION);
            assert_eq!(counter_at(&snapshot, collection), Ok(0));
            assert_eq!(receipt_at(&snapshot, collection, "0"), Ok(None));
        }
    }

    #[test]
    fn sequenced_counter_allocation_failure_preserves_prior_receipt() {
        use super::allocator::TestAllocator;
        use std::sync::Arc;

        let allocator = Arc::new(TestAllocator::default());
        let disk = SimulatedDisk::new().unwrap();
        let mut db =
            Database::create_on_with_options(disk.clone(), [7; 16], allocator.options()).unwrap();
        initialize(&db).unwrap();
        assert_eq!(
            handle(&mut db, "set-seq 0 1 3 12"),
            Ok("committed 4".into())
        );
        let collection = db.collection_id(DATA_COLLECTION);
        let start = allocator.requests();
        let mut probe = db.begin_with_validation(Validation::WholeDatabase);
        assert!(
            probe
                .get(collection, &slot_key("0").unwrap())
                .unwrap()
                .is_some()
        );
        let observation_requests = allocator.requests() - start;
        drop(probe);
        let before = disk.durable_image().unwrap();
        let live_bytes = allocator.live_bytes();
        let next = "set-seq 0 2 4 30";
        assert_eq!(
            allocator.with_refusal(observation_requests, || handle(&mut db, next)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(allocator.live_bytes(), live_bytes);
        assert_eq!(handle(&mut db, "get"), Ok("value 4 12".into()));
        assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 1 3 12 4".into()));
        assert_eq!(disk.durable_image().unwrap(), before);
        assert_eq!(handle(&mut db, next), Ok("committed 5".into()));
    }

    #[test]
    fn sequenced_submit_refuses_an_exhausted_imported_revision() {
        use weekend_db::storage::{export_logical_on, import_logical_on};

        let limits = Limits::default();
        let seed = SimulatedDisk::new().unwrap();
        let db = Database::create_on(seed.clone(), [7; 16], limits).unwrap();
        initialize(&db).unwrap();
        drop(db);
        let mut logical = export_logical_on(seed, limits).unwrap();
        // The logical header records the acknowledged revision before the DATA
        // revision. The two seeded collections keep their original revisions.
        assert_eq!(logical[..8], *b"WDBLOG01");
        assert_eq!(logical[32..40], INITIAL_REVISION.to_le_bytes());
        logical[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
        logical_checksum::sign(&mut logical);
        let disk = SimulatedDisk::new().unwrap();
        import_logical_on(&logical, disk.clone(), limits).unwrap();
        let mut db = open(&disk);
        admit(&db).unwrap();
        let before = disk.durable_image().unwrap();
        assert_eq!(db.snapshot().revision(), u64::MAX);
        let request = format!("set-seq 0 1 {} 30", u64::MAX);
        assert_eq!(handle(&mut db, &request), Err(Error::Exhausted));
        assert_eq!(db.snapshot().revision(), u64::MAX);
        assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 0".into()));
        assert_eq!(disk.durable_image().unwrap(), before);
    }

    #[test]
    fn sequenced_predecessor_modes_from_the_same_committed_image() {
        let seed = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(seed.clone(), [7; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        let identity = hex(&[7; 16]);
        let first = format!("journal-set {identity} 0 1 - 3 12");
        assert_eq!(handle(&mut db, &first), Ok("committed 4".into()));
        drop(db);
        let image = seed.durable_image().unwrap();
        let receipt = Receipt {
            sequence: 1,
            expected: 3,
            value: 12,
            revision: 4,
        };
        for (command, expected, published) in [
            (
                "set-seq 0 2 4 30".to_owned(),
                Ok("committed 5".into()),
                true,
            ),
            (
                format!("journal-set {identity} 0 2 {} 4 30", hex(&receipt.encode())),
                Ok("committed 5".into()),
                true,
            ),
            (
                format!(
                    "journal-set {identity} 0 2 {} 4 30",
                    hex(&Receipt {
                        value: 13,
                        ..receipt
                    }
                    .encode())
                ),
                Err(Error::Conflict),
                false,
            ),
        ] {
            let disk = SimulatedDisk::from_image(&image).unwrap();
            let mut db = open(&disk);
            let before = disk.durable_image().unwrap();
            assert_eq!(handle(&mut db, &command), expected, "{command}");
            if published {
                assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 2 4 30 5".into()));
                assert_eq!(handle(&mut db, "get"), Ok("value 5 30".into()));
            } else {
                assert_eq!(handle(&mut db, "receipt 0"), Ok("receipt 1 3 12 4".into()));
                assert_eq!(handle(&mut db, "get"), Ok("value 4 12".into()));
                assert_eq!(disk.durable_image().unwrap(), before);
            }
        }
    }

    #[test]
    fn initialization_io_faults_reopen_at_each_publication_boundary() {
        let original = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(original.clone(), [7; 16], Limits::default()).unwrap();
        let initial = original.durable_image().unwrap();
        original.arm(usize::MAX, Fault::Error);
        initialize(&db).unwrap();
        let trace = original.trace().unwrap();
        let syncs: Vec<_> = trace
            .iter()
            .enumerate()
            .filter_map(|(step, event)| (event.kind == IoKind::Sync).then_some(step))
            .collect();
        let [
            format_append,
            format_root,
            format_witness,
            data_append,
            data_root,
            data_witness,
            seed_append,
            seed_root,
            _,
        ]: [usize; 9] = syncs.try_into().unwrap();
        assert_eq!(admit(&db), Ok(()));
        assert_eq!(handle(&mut db, "get"), Ok("value 3 0".into()));

        for step in 0..trace.len() {
            for crash in [Crash::DropAll, Crash::PersistAll] {
                let disk = SimulatedDisk::from_image(&initial).unwrap();
                let mut db = open(&disk);
                disk.arm(step, Fault::Error);
                let (revision, append) = if step <= format_witness {
                    (0, format_append)
                } else if step <= data_witness {
                    (1, data_append)
                } else {
                    (2, seed_append)
                };
                let expected = if step <= append {
                    Error::Io
                } else {
                    Error::OutcomeUnknown
                };
                assert_eq!(initialize(&db), Err(expected), "step {step}");
                assert_eq!(db.snapshot().revision(), revision, "step {step}");
                assert_eq!(disk.trace().unwrap(), trace[..=step]);
                assert_eq!(handle(&mut db, "get"), Err(Error::RecoveryRequired));
                drop(db);
                disk.clear_fault();
                disk.crash(crash).unwrap();

                let reopened = open(&disk);
                let persisted =
                    |root| step > root || (step == root && matches!(crash, Crash::PersistAll));
                let revision = [format_root, data_root, seed_root]
                    .into_iter()
                    .filter(|&root| persisted(root))
                    .count() as u64;
                assert_eq!(reopened.snapshot().revision(), revision, "step {step}");
                let expected = match revision {
                    0 => Err(Error::NotFound),
                    1 | 2 => Err(Error::UnsupportedFormat),
                    INITIAL_REVISION => Ok(()),
                    _ => unreachable!(),
                };
                let image = disk.durable_image().unwrap();
                assert_eq!(admit(&reopened), expected, "step {step}");
                assert_eq!(disk.durable_image().unwrap(), image);
                assert_eq!(
                    state_in(&reopened, DATA_COLLECTION),
                    if revision == INITIAL_REVISION {
                        Ok((INITIAL_REVISION, 0))
                    } else {
                        Err(Error::NotFound)
                    }
                );
            }
        }
    }

    #[test]
    fn initialization_limits_leave_both_collections_unseeded() {
        for limits in [
            Limits {
                max_bytes: FORMAT.len() - 1,
                ..Limits::default()
            },
            Limits {
                max_writes: 0,
                ..Limits::default()
            },
            Limits {
                max_writes: 1,
                ..Limits::default()
            },
        ] {
            let disk = SimulatedDisk::new().unwrap();
            let db = Database::create_on(disk.clone(), [7; 16], limits).unwrap();
            assert_eq!(initialize(&db), Err(Error::Limit), "{limits:?}");
            assert_eq!(db.snapshot().revision(), 2);
            assert!(!db.requires_recovery());
            drop(db);
            disk.crash(Crash::DropAll).unwrap();
            let reopened = open(&disk);
            let snapshot = reopened.snapshot();
            assert_eq!(snapshot.revision(), 2);
            assert_eq!(
                snapshot.count(reopened.collection_id(FORMAT_COLLECTION)),
                Ok(0)
            );
            assert_eq!(
                snapshot.count(reopened.collection_id(DATA_COLLECTION)),
                Ok(0)
            );
            assert_eq!(admit(&reopened), Err(Error::UnsupportedFormat));
        }
    }

    #[test]
    fn format_admission_preserves_read_errors_and_leaves_the_image_unchanged() {
        #[derive(Clone, Copy, Debug)]
        enum Field {
            Missing,
            Bytes(&'static [u8]),
            Unsigned(u64),
        }
        use Field::{Bytes, Missing, Unsigned};

        for (marker, counter, expected) in [
            (Missing, Unsigned(0), Err(Error::UnsupportedFormat)),
            (Bytes(b"other"), Unsigned(0), Err(Error::UnsupportedFormat)),
            (Unsigned(0), Unsigned(0), Err(Error::Corrupt)),
            (Bytes(FORMAT), Missing, Err(Error::NotFound)),
            (Bytes(FORMAT), Bytes(b"wrong type"), Err(Error::Type)),
            (Bytes(FORMAT), Unsigned(0), Ok(())),
        ] {
            let disk = SimulatedDisk::new().unwrap();
            let db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
            let format = db.create_collection().unwrap();
            let data = db.create_collection().unwrap();
            let mut tx = db.begin();
            for (collection, key, field) in [(format, FORMAT_KEY, marker), (data, KEY, counter)] {
                let value = match field {
                    Missing => continue,
                    Bytes(bytes) => db.bytes(bytes).unwrap(),
                    Unsigned(number) => db.unsigned(number).unwrap(),
                };
                tx.put(collection, key, &value).unwrap();
            }
            assert_eq!(tx.commit(), Ok(INITIAL_REVISION));
            drop(tx);
            drop(db);

            let mut db = open(&disk);
            let image = disk.durable_image().unwrap();
            assert_eq!(
                admit(&db),
                expected,
                "marker {marker:?}, counter {counter:?}"
            );
            assert_eq!(
                handle(&mut db, "get"),
                match counter {
                    Missing => Err(Error::NotFound),
                    Bytes(_) => Err(Error::Type),
                    Unsigned(number) => Ok(format!("value 3 {number}")),
                }
            );
            assert_eq!(disk.durable_image().unwrap(), image);
        }
    }

    #[test]
    fn malformed_receipts_fail_closed_for_lookup_and_retry() {
        for (sequence, expected, revision, len, version) in [
            (1, 3, 4, 34, 1),        // otherwise valid, with trailing bytes
            (1, 3, 4, 32, 1),        // length
            (1, 3, 4, 33, 2),        // version
            (0, 3, 4, 33, 1),        // reserved sequence
            (1, 4, 4, 33, 1),        // expected revision mismatch
            (2, 3, 4, 33, 1),        // sequence exceeds the committed prefix
            (1, 4, 5, 33, 1),        // receipt claims a future revision
            (1, u64::MAX, 0, 33, 1), // wrapped revision
        ] {
            let mut db =
                Database::create_on(MemoryStore::new().unwrap(), [7; 16], Limits::default())
                    .unwrap();
            initialize(&db).unwrap();
            let mut encoded = Receipt {
                sequence,
                expected,
                value: 20,
                revision,
            }
            .encode();
            encoded[0] = version;
            let encoded = [encoded.as_slice(), &[0]].concat();
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
            let identity = hex(&[7; 16]);
            assert_eq!(
                handle(&mut db, &format!("journal-check {identity} 0")),
                Err(Error::Corrupt)
            );
            assert_eq!(handle(&mut db, "set-seq 0 1 4 30"), Err(Error::Corrupt));
            assert_eq!(handle(&mut db, "get").unwrap(), "value 4 0");
        }
    }

    #[test]
    fn reopened_replay_preserves_current_and_retained_receipts() {
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
        initialize(&db).unwrap();
        assert_eq!(handle(&mut db, "set-seq 0 1 3 12").unwrap(), "committed 4");
        let retained_id = db.retain_snapshot(&db.snapshot()).unwrap();
        let identity = hex(&[7; 16]);
        let first = Receipt {
            sequence: 1,
            expected: 3,
            value: 12,
            revision: 4,
        };
        let previous = hex(&first.encode());
        let next = format!("journal-set {identity} 0 2 {previous} 5 30");
        assert_eq!(handle(&mut db, &next).unwrap(), "committed 6");
        drop(db);
        let before = disk.durable_image().unwrap();
        disk.crash(Crash::DropAll).unwrap();
        let mut db = open(&disk);

        assert_eq!(handle(&mut db, &next).unwrap(), "committed 6");
        for invalid in [
            format!("journal-set {identity} 0 2 {previous} 4 30"),
            format!("journal-set {identity} 0 2 {previous} 5 31"),
        ] {
            assert_eq!(handle(&mut db, &invalid), Err(Error::Invalid), "{invalid}");
        }
        for invalid in [
            "set-seq 0 0 6 70",
            "set-seq 0 4 6 70",
            "set-seq 1024 1 6 70",
        ] {
            assert_eq!(handle(&mut db, invalid), Err(Error::Invalid), "{invalid}");
        }
        assert_eq!(handle(&mut db, "set-seq 0 1 3 12").unwrap(), "retired 1");
        assert_eq!(
            handle(&mut db, "set-seq 0 3 5 70").unwrap(),
            "conflict 6 30"
        );
        let second = Receipt {
            sequence: 2,
            expected: 5,
            value: 30,
            revision: 6,
        };
        let wrong_predecessor = hex(&Receipt {
            value: 31,
            ..second
        }
        .encode());
        assert_eq!(
            handle(
                &mut db,
                &format!("journal-set {identity} 0 3 {wrong_predecessor} 6 70")
            ),
            Err(Error::Conflict)
        );
        let previous = hex(&second.encode());
        for invalid in [
            format!("journal-set {identity} 0 4 {previous} 6 70"),
            format!("journal-set {identity} 0 3 {} 6 70", hex(&first.encode())),
            format!("journal-set {identity} 0 3 {previous} 5 70"),
        ] {
            assert_eq!(handle(&mut db, &invalid), Err(Error::Invalid), "{invalid}");
        }
        assert_eq!(
            handle(&mut db, &format!("journal-get {}", hex(&[8; 16]))),
            Err(Error::ForeignStore)
        );

        let collection = db.collection_id(DATA_COLLECTION);
        let head = db.snapshot();
        assert_eq!(head.revision(), 6);
        assert_eq!(counter_at(&head, collection), Ok(30));
        assert_eq!(receipt_at(&head, collection, "0"), Ok(Some(second)));
        let retained = db.retained_snapshot(retained_id).unwrap();
        assert_eq!(retained.revision(), 4);
        assert_eq!(counter_at(&retained, collection), Ok(12));
        assert_eq!(receipt_at(&retained, collection, "0"), Ok(Some(first)));
        assert_eq!(disk.durable_image().unwrap(), before);
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
