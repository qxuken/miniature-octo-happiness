use super::*;
fn ints(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|n| n.to_le_bytes()).collect()
}
fn stream(
    revision: u64,
    data_revision: u64,
    next_id: u64,
    roots: &[(u64, u64, u64)],
    data: &[(u64, u64)],
) -> Vec<u8> {
    let mut b = MAGIC.to_vec();
    b.extend(ints(&[VERSION]));
    b.extend([1; 16]);
    b.extend(ints(&[
        revision,
        data_revision,
        next_id,
        roots.len() as u64,
    ]));
    for &(id, captured, revision) in roots {
        b.extend(ints(&[id, captured, revision]));
    }
    b.extend(ints(&[data.len() as u64]));
    for &(revision, next) in data {
        b.extend(ints(&[revision, next, 0]));
    }
    b.extend(ints(&[codec::crc(&b)]));
    b
}
fn decoded(bytes: &[u8]) -> Result<()> {
    decode(bytes, &Memory::new(Limits::default()).unwrap()).map(|_| ())
}
fn signed(mut b: Vec<u8>) -> Vec<u8> {
    let n = b.len() - 8;
    let crc = codec::crc(&b[..n]);
    b[n..].copy_from_slice(&crc.to_le_bytes());
    b
}
#[test]
fn header_and_retention_predicates_have_independent_controls() {
    let base = stream(10, 8, 3, &[(1, 7, 6)], &[(6, 2), (8, 2)]);
    assert_eq!(decoded(&base), Ok(()));
    for (offset, value, error) in [
        (0, 0u64, Error::UnsupportedFormat),
        (8, 2, Error::UnsupportedFormat),
        (32, 7, Error::Corrupt),
        (48, 0, Error::Corrupt),
        (56, 65, Error::Limit),
        (64, 0, Error::Corrupt),
        (64, 3, Error::Corrupt),
        (72, 10, Error::Corrupt),
        (72, 5, Error::Corrupt),
        (80, 9, Error::Corrupt),
        (88, 66, Error::Limit),
    ] {
        let mut b = base.clone();
        b[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        // For data>current keep captured >= data, so this OR term acts alone.
        if offset == 80 {
            b[72..80].copy_from_slice(&9u64.to_le_bytes());
        }
        assert_eq!(
            decoded(&signed(b)),
            Err(error),
            "offset={offset} value={value}"
        );
    }
    let mut b = base.clone();
    b[16..32].fill(0);
    assert_eq!(decoded(&signed(b)), Err(Error::Corrupt));
    let mut b = base.clone();
    b[24] ^= 1;
    assert_eq!(decoded(&b), Err(Error::Corrupt));
    assert_eq!(decoded(&base[..79]), Err(Error::Corrupt));
    assert_eq!(
        decoded(&stream(
            10,
            8,
            3,
            &[(1, 7, 6), (1, 7, 6)],
            &[(6, 2), (8, 2)]
        )),
        Err(Error::Corrupt)
    );
}
#[test]
fn data_revision_membership_order_and_next_collection_guards() {
    for (roots, data, expected) in [
        (vec![], vec![(8, 2)], Ok(())),
        (vec![(1, 7, 6)], vec![(6, 2), (8, 2)], Ok(())),
        (vec![], vec![(9, 2)], Err(Error::Corrupt)),
        (vec![(1, 7, 6)], vec![(8, 2), (6, 2)], Err(Error::Corrupt)),
        (
            vec![(1, 7, 6)],
            vec![(6, 2), (6, 2), (8, 2)],
            Err(Error::Corrupt),
        ),
        (vec![], vec![(6, 2), (8, 2)], Err(Error::Corrupt)),
        (vec![(1, 7, 6)], vec![(6, 2)], Err(Error::Corrupt)),
        (vec![(1, 7, 6)], vec![(8, 2)], Err(Error::Corrupt)),
        (vec![(1, 7, 6)], vec![(6, 3), (8, 2)], Err(Error::Corrupt)),
        (vec![(1, 7, 6)], vec![(6, 1), (8, 2)], Ok(())),
    ] {
        assert_eq!(decoded(&stream(10, 8, 3, &roots, &data)), expected);
    }
    let mut trailing = stream(10, 8, 3, &[], &[(8, 2)]);
    let at = trailing.len() - 8;
    trailing.insert(at, 0);
    assert_eq!(decoded(&signed(trailing)), Err(Error::Corrupt));
}
#[test]
fn collection_record_and_map_order_guards() {
    let mem = Memory::new(Limits::default()).unwrap();
    // revision,next,count,id,content,records
    for (id, next, content, expected) in [
        (1, 2, 4, Ok(())),
        (0, 2, 4, Err(Error::Corrupt)),
        (2, 2, 4, Err(Error::Corrupt)),
        (1, 2, 0, Err(Error::Corrupt)),
        (1, 2, 6, Err(Error::Corrupt)),
        (1, 0, 4, Err(Error::Corrupt)),
    ] {
        let b = ints(&[5, next, 1, id, content, 0]);
        let mut r = Reader {
            bytes: &b,
            mem: &mem,
            nodes: 0,
        };
        assert_eq!(r.data().map(|_| ()), expected);
    }
    let b = ints(&[5, 3, 2, 1, 4, 0, 1, 4, 0]);
    assert!(matches!(
        (Reader {
            bytes: &b,
            mem: &mem,
            nodes: 0
        })
        .data(),
        Err(Error::Corrupt)
    ));
    for keys in [*b"ab", *b"aa", *b"ba"] {
        for written in [0, 4, 5] {
            let mut b = ints(&[5, 2, 1, 1, 4, 2]);
            for key in keys {
                b.extend(ints(&[1]));
                b.push(key);
                b.extend(ints(&[written]));
                b.push(0);
                b.extend(ints(&[0]));
            }
            let mut r = Reader {
                bytes: &b,
                mem: &mem,
                nodes: 0,
            };
            assert_eq!(r.data().is_ok(), keys[0] < keys[1] && written == 4);
        }
        let mut b = vec![6];
        b.extend(ints(&[2]));
        for key in keys {
            b.extend(ints(&[1]));
            b.push(key);
            b.push(0);
            b.extend(ints(&[0]));
        }
        assert_eq!(
            (Reader {
                bytes: &b,
                mem: &mem,
                nodes: 0
            })
            .value(1)
            .is_ok(),
            keys[0] < keys[1]
        );
    }
}
#[test]
fn reader_writer_bounds_and_error_mapping() {
    let mem = Memory::new(Limits {
        max_depth: 1,
        ..Limits::default()
    })
    .unwrap();
    let scalar = [0; 9];
    for (depth, nodes, expected) in [
        (1, MAX_NODES - 1, Ok(())),
        (2, MAX_NODES - 1, Err(Error::Limit)),
        (1, MAX_NODES, Err(Error::Limit)),
    ] {
        assert_eq!(
            (Reader {
                bytes: &scalar,
                mem: &mem,
                nodes
            })
            .value(depth)
            .map(|_| ()),
            expected
        );
    }
    for (tag, bits, expected) in [
        (0, 0, Ok(())),
        (0, 1, Err(Error::Corrupt)),
        (1, 1, Ok(())),
        (1, 2, Err(Error::Corrupt)),
        (8, 0, Err(Error::Corrupt)),
    ] {
        let mut b = vec![tag];
        b.extend(ints(&[bits]));
        assert_eq!(
            (Reader {
                bytes: &b,
                mem: &mem,
                nodes: 0
            })
            .value(1)
            .map(|_| ()),
            expected
        );
    }
    let no_memory = Memory::new(Limits {
        max_live_bytes: 0,
        ..Limits::default()
    })
    .unwrap();
    assert_eq!(
        (Reader {
            bytes: &scalar,
            mem: &no_memory,
            nodes: 0
        })
        .value(1)
        .map(|_| ()),
        Err(Error::Limit)
    );
    for (count, expected) in [(2, Ok(2)), (3, Err(Error::Limit))] {
        let b = ints(&[count]);
        assert_eq!(
            (Reader {
                bytes: &b,
                mem: &mem,
                nodes: 0
            })
            .count(2),
            expected
        );
    }
    assert_eq!(
        (Reader {
            bytes: &[],
            mem: &mem,
            nodes: 0
        })
        .take(1),
        Err(Error::Corrupt)
    );
    let value = Value::scalar(&mem, Kind::None, 0).unwrap();
    let mut w = Writer {
        bytes: ChargedVec::new(),
        mem: &mem,
        nodes: MAX_NODES - 1,
    };
    w.value(&value).unwrap();
    assert_eq!(w.value(&value), Err(Error::Limit));
    let big = vec![0; MAX_IMAGE_BYTES + 1];
    assert_eq!(decoded(&big), Err(Error::Limit));
    let mut w = Writer {
        bytes: ChargedVec::new(),
        mem: &mem,
        nodes: 0,
    };
    w.bytes(&big[..MAX_IMAGE_BYTES]).unwrap();
    assert_eq!(w.bytes(&[]), Ok(()));
    assert_eq!(w.bytes(&[0]), Err(Error::Limit));
}

#[test]
fn shared_dag_expansion_checks_nodes_and_physical_restore_size_without_source_writes() {
    use crate::{
        Database,
        storage::{IoKind, SimulatedDisk},
    };
    for depth in [13, 14] {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), [3; 16], Limits::default()).unwrap();
        let mut value = db.none().unwrap();
        for _ in 0..depth {
            value = db
                .list()
                .unwrap()
                .list_push(&value)
                .unwrap()
                .list_push(&value)
                .unwrap();
        }
        let mem = Memory::new(Limits::default()).unwrap();
        let mut writer = Writer {
            bytes: ChargedVec::new(),
            mem: &mem,
            nodes: 0,
        };
        if depth == 13 {
            writer.value(&value).unwrap();
            assert_eq!(writer.nodes, 16383);
        } else {
            assert_eq!(writer.value(&value), Err(Error::Limit));
        }
        let c = db.create_collection().unwrap();
        let mut tx = db.begin();
        tx.put(c, b"shared", &value).unwrap();
        tx.commit().unwrap();
        drop(tx);
        drop(value);
        drop(db);
        let image = disk.durable_image().unwrap();
        assert!(image.len() < 1024 * 1024);
        let source = SimulatedDisk::from_image(&image).unwrap();
        assert_eq!(
            export_logical_on(source.clone(), Limits::default()),
            Err(Error::Limit)
        );
        assert_eq!(source.durable_image().unwrap(), image);
        assert!(
            source
                .trace()
                .unwrap()
                .iter()
                .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
        );
    }
}
