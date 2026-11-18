//! A fresh core consumer, launched through xtask's shared bounded supervisor.
use std::path::Path;
use weekend_db::{Database, Limits};

// Fixed version-1 wire rows, deliberately independent of the app's codecs.
fn receipt(fields: [u64; 4]) -> Vec<u8> {
    let mut bytes = vec![1];
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes
}

fn journals(server: &Path, paths: [&Path; 2], continued: bool) {
    let db = Database::open_file(server, Limits::default()).unwrap();
    let identity = db.storage_identity().unwrap();
    let head = db.snapshot();
    let (revision, value, receipts, journal_revisions) = if continued {
        (7, 111, [[2, 5, 99, 6], [2, 6, 111, 7]], [6, 6])
    } else {
        (5, 77, [[1, 3, 42, 4], [1, 4, 77, 5]], [4, 4])
    };
    assert_eq!(head.revision(), revision, "coordinator revision mismatch");
    assert!(db.retained_ids().unwrap().is_empty());
    let protocol = head.scan(db.collection_id(1)).unwrap();
    assert_eq!(protocol.len(), 1);
    assert_eq!(protocol[0].0, b"protocol");
    assert_eq!(
        protocol[0].1.as_bytes().unwrap(),
        b"coordinator-sequenced-v1"
    );
    assert_eq!(head.count(db.collection_id(2)).unwrap(), 3);
    assert_eq!(
        head.get(db.collection_id(2), b"counter")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(value)
    );
    for (index, slot) in [0u16, 1023].into_iter().enumerate() {
        let mut key = b"seq/".to_vec();
        key.extend_from_slice(&slot.to_be_bytes());
        let row = head.get(db.collection_id(2), &key).unwrap().unwrap();
        let expected = receipt(receipts[index]);
        assert_eq!(row.as_bytes().unwrap(), expected);
        let journal = Database::open_file(paths[index], Limits::default()).unwrap();
        assert!(journal.retained_ids().unwrap().is_empty());
        let snapshot = journal.snapshot();
        assert_eq!(
            snapshot.revision(),
            journal_revisions[index],
            "journal revision mismatch"
        );
        let rows = snapshot.scan(journal.collection_id(1)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, b"coordinator-journal");
        let bytes = rows[0].1.as_bytes().unwrap();
        assert_eq!(bytes.len(), 76);
        assert_eq!(&bytes[..8], b"WDBJRN01");
        assert_eq!(&bytes[8..24], identity);
        assert_eq!(&bytes[24..26], slot.to_le_bytes());
        assert_eq!(bytes[26], 1, "journal pending or missing receipt");
        assert_eq!(&bytes[27..60], expected, "journal receipt mismatch");
        assert_eq!(&bytes[60..76], [0; 16]);
    }
    println!("coordinator {revision} {value} journals 2 slots 0 1023");
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let [action, phase, server, first, second] = args.as_slice()
        && action == "journals"
    {
        assert!(phase == "checkpoint" || phase == "continued");
        journals(
            Path::new(server),
            [Path::new(first), Path::new(second)],
            phase == "continued",
        );
        return;
    }
    let [path] = args.as_slice() else {
        panic!("usage: incident-recovery-consumer RESTORED_FIXTURE");
    };
    let db = Database::open_file(path, Limits::default()).unwrap();
    let collection = db.collection_id(1);
    let current = db.snapshot();
    let retained = db.retained_snapshot(1).unwrap();
    assert_eq!(current.revision(), 4);
    assert_eq!(retained.revision(), 2);
    assert_eq!(
        current.get(collection, &[0]).unwrap().unwrap().as_i64(),
        Ok(21)
    );
    assert_eq!(
        retained.get(collection, &[0]).unwrap().unwrap().as_i64(),
        Ok(11)
    );
    println!("current 4 21 retained 1 2 11");
}
