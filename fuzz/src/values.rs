use std::collections::BTreeMap;
use weekend_db::{Database, Error, Value};

#[path = "../../tests/support/mod.rs"]
mod support;
use support::Model;

fn payload(db: &Database, kind: u8, bits: u8) -> (Value, Model) {
    match kind % 7 {
        0 => (db.none().unwrap(), Model::None),
        1 => (
            db.boolean(bits & 1 != 0).unwrap(),
            Model::Bool(bits & 1 != 0),
        ),
        2 => (
            db.integer(i64::from(bits as i8)).unwrap(),
            Model::Int(i64::from(bits as i8)),
        ),
        3 => (
            db.unsigned(u64::from(bits)).unwrap(),
            Model::UInt(u64::from(bits)),
        ),
        4 => {
            let bits = [
                0,
                1 << 63,
                u64::MAX,
                0x7ff0_0000_0000_0000,
                0x7ff8_0000_0000_0001,
            ][usize::from(bits) % 5];
            (db.float(f64::from_bits(bits)).unwrap(), Model::Float(bits))
        }
        5 => {
            let bytes = vec![bits; usize::from(bits % 64)];
            (db.bytes(&bytes).unwrap(), Model::Bytes(bytes))
        }
        _ => {
            let value = db.integer(i64::from(bits)).unwrap();
            (
                db.map().unwrap().map_set(b"nested", &value).unwrap(),
                Model::Map(BTreeMap::from([(
                    b"nested".to_vec(),
                    Model::Int(i64::from(bits)),
                )])),
            )
        }
    }
}

pub fn value_ops(input: &[u8]) {
    let db = Database::new().unwrap();
    let mut map = db.map().unwrap();
    let mut list = db.list().unwrap();
    let mut expected_map = BTreeMap::new();
    let mut expected_list = Vec::new();
    // Begin across the first radix boundary, then generate insert/remove edits.
    for i in 0..33 {
        list = list.list_push(&db.integer(i).unwrap()).unwrap();
        expected_list.push(Model::Int(i));
    }
    let mut history = Vec::new();
    for &[op, key, bits] in input.as_chunks::<3>().0.iter().take(64) {
        let (value, model) = payload(&db, op >> 3, bits);
        let mut name = vec![0; if key & 0x80 == 0 { 0 } else { 128 }];
        if key != 0 {
            name.push(key & 31);
        }
        let index = usize::from(key) % (expected_list.len() + 2);
        history.push((map.clone(), Model::Map(expected_map.clone())));
        history.push((list.clone(), Model::List(expected_list.clone())));
        match op & 7 {
            0 => {
                map = map.map_set(&name, &value).unwrap();
                expected_map.insert(name, model);
            }
            1 => {
                map = map.map_remove(&name).unwrap();
                expected_map.remove(&name);
            }
            2 => assert_eq!(
                map.map_get(&name).unwrap().map(|v| Model::read(&v)),
                expected_map.get(&name).cloned()
            ),
            3 => {
                list = list.list_push(&value).unwrap();
                expected_list.push(model);
            }
            4 if index <= expected_list.len() => {
                list = list.list_insert(index, &value).unwrap();
                expected_list.insert(index, model);
            }
            5 if index < expected_list.len() => {
                list = list.list_set(index, &value).unwrap();
                expected_list[index] = model;
            }
            6 if index < expected_list.len() => {
                list = list.list_remove(index).unwrap();
                expected_list.remove(index);
            }
            7 if index < expected_list.len() => assert_eq!(
                Model::read(&list.list_get(index).unwrap()),
                expected_list[index]
            ),
            4 => assert_eq!(list.list_insert(index, &value), Err(Error::Bounds)),
            5 => assert_eq!(list.list_set(index, &value), Err(Error::Bounds)),
            6 => assert_eq!(list.list_remove(index), Err(Error::Bounds)),
            7 => assert_eq!(list.list_get(index), Err(Error::Bounds)),
            _ => unreachable!(),
        }
        assert_eq!(Model::read(&map), Model::Map(expected_map.clone()));
        assert_eq!(Model::read(&list), Model::List(expected_list.clone()));
    }
    drop(db);
    for (value, expected) in history {
        assert_eq!(Model::read(&value), expected);
    }
}
