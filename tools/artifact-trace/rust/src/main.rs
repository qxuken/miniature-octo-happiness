use weekend_db::{Database, Error, Kind, Limits, Rows, Snapshot, Value};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn value(v: &Value) -> String {
    match v.kind() {
        Kind::Unsigned => format!("u{}", v.as_u64().unwrap()),
        Kind::Bytes => format!("b{}", hex(v.as_bytes().unwrap())),
        Kind::Map => format!(
            "m{{{}}}",
            v.map_entries()
                .unwrap()
                .iter()
                .map(|(k, v)| format!("{}:{}", hex(k), value(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Kind::List => format!(
            "l[{}]",
            (0..v.len().unwrap())
                .map(|i| value(&v.list_get(i).unwrap()))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => panic!("unexpected value kind"),
    }
}
fn rows(rows: Rows) -> String {
    assert!(rows.len() <= 16);
    format!(
        "[{}]",
        rows.iter()
            .map(|(k, v)| format!("{}={}", hex(k), value(v)))
            .collect::<Vec<_>>()
            .join(";")
    )
}
fn make(db: &Database, n: u32) -> Value {
    let list = db
        .list()
        .unwrap()
        .list_push(&db.bytes(&[0, n as u8, 255]).unwrap())
        .unwrap()
        .list_push(&db.unsigned(u64::from(n) + 1).unwrap())
        .unwrap();
    db.map()
        .unwrap()
        .map_set(b"n", &db.unsigned(n.into()).unwrap())
        .unwrap()
        .map_set(b"v", &list)
        .unwrap()
}
fn segment(db: &Database, cells: &[[u32; 4]], start: usize) -> usize {
    let collection = db.collection_id(1);
    let mut txs: [_; 4] = std::array::from_fn(|_| None);
    let mut held: Option<Snapshot<'_>> = None;
    for i in start..cells.len() {
        let [op, slot, k, n] = cells[i];
        let slot = slot as usize;
        // A REOPEN cell is emitted by the caller after all borrowed handles drop.
        if op == 12 {
            assert!(held.is_none() && txs.iter().all(Option::is_none));
            return i;
        }
        let mut result = String::from("-");
        let action: Result<(), Error> = match op {
            1 => {
                assert!(txs[slot].is_none());
                txs[slot] = Some(db.begin());
                Ok(())
            }
            2 => txs[slot]
                .as_mut()
                .unwrap()
                .put(collection, &[k as u8], &make(db, n)),
            3 => match txs[slot].as_mut().unwrap().get(collection, &[k as u8]) {
                Ok(Some(v)) => {
                    result = value(&v);
                    Ok(())
                }
                Ok(None) => Err(Error::NotFound),
                Err(e) => Err(e),
            },
            4 => txs[slot]
                .as_mut()
                .unwrap()
                .scan(collection)
                .map(|r| result = rows(r)),
            5 => txs[slot]
                .as_mut()
                .unwrap()
                .commit()
                .map(|r| result = r.to_string()),
            6 => txs[slot].as_mut().unwrap().delete(collection, &[k as u8]),
            7 => {
                assert!(txs[slot].take().is_some());
                Ok(())
            }
            8 => {
                assert!(held.is_none());
                held = Some(db.snapshot());
                Ok(())
            }
            9 => {
                assert!(held.take().is_some());
                Ok(())
            }
            10 => db
                .retain_snapshot(held.as_ref().unwrap())
                .map(|id| result = id.to_string()),
            11 => {
                assert!(held.is_none());
                db.retained_snapshot(k.into()).map(|s| held = Some(s))
            }
            13 => db.release_retained(k.into()),
            _ => panic!("bad opcode"),
        };
        emit(
            db,
            held.as_ref(),
            i,
            op,
            action.err().map_or(0, |e| e as i32),
            &result,
        );
    }
    assert!(held.is_none() && txs.iter().all(Option::is_none));
    cells.len()
}
fn emit(db: &Database, held: Option<&Snapshot<'_>>, i: usize, op: u32, status: i32, result: &str) {
    let current = db.snapshot();
    let collection = db.collection_id(1);
    let h = held.map_or_else(
        || "-".into(),
        |s| format!("{}{}", s.revision(), rows(s.scan(collection).unwrap())),
    );
    let ids = db.retained_ids().unwrap();
    let r = if ids.is_empty() {
        "-".into()
    } else {
        ids.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    println!(
        "{i} {op} {status} {} {} H={h} R={r} X={result}",
        current.revision(),
        rows(current.scan(collection).unwrap())
    );
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 4);
    assert_eq!(weekend_db::version::SOURCE_ID, args[3]);
    let bytes = std::fs::read(&args[1]).unwrap();
    assert!(bytes.len() >= 12 && bytes.len() <= 4108);
    assert_eq!(&bytes[..8], b"WDBTRC1\n");
    let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    assert!(count <= 256);
    assert_eq!(bytes.len(), 12 + 16 * count);
    let cells = bytes[12..]
        .chunks_exact(16)
        .map(|c| {
            std::array::from_fn(|i| u32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect::<Vec<[u32; 4]>>();
    for [op, slot, k, n] in &cells {
        assert!((1..=13).contains(op) && *slot < 4 && *k <= 255 && *n <= 254);
    }
    let mut db = Database::create_file(&args[2], Limits::default()).unwrap();
    db.create_collection().unwrap();
    let mut pos = 0;
    while pos < cells.len() {
        pos = segment(&db, &cells, pos);
        if pos < cells.len() {
            drop(db);
            db = Database::open_file(&args[2], Limits::default()).unwrap();
            emit(&db, None, pos, 12, 0, "-");
            pos += 1;
        }
    }
}
