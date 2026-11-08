use std::collections::BTreeMap;
use weekend_db::{Database, Value};
fn entries(map: &Value) -> BTreeMap<Vec<u8>, u64> {
    let rows = map.map_entries().unwrap();
    let count = rows.len();
    assert_eq!(map.len().unwrap(), count);
    let values: BTreeMap<_, _> = rows
        .into_iter()
        .map(|(key, value)| {
            let bits = value.as_f64().unwrap().to_bits();
            assert_eq!(
                map.map_get(&key)
                    .unwrap()
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                bits
            );
            (key, bits)
        })
        .collect();
    assert_eq!(values.len(), count, "duplicate map entries");
    values
}
fn check_list(list: &Value, expected: &[u64]) {
    assert_eq!(list.len().unwrap(), expected.len());
    for (i, &bits) in expected.iter().enumerate() {
        assert_eq!(list.list_get(i).unwrap().as_f64().unwrap().to_bits(), bits);
    }
}
pub fn tree_adversarial_keys(input: &[u8]) {
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let lengths = [0, 1, 31, 32, 33, 1023, 1024, 1025, 4096];
    let count =
        [31, 32, 33, 1023, 1024, 1025][usize::from(input.first().copied().unwrap_or(0)) % 6];
    let patterns = [
        0,
        1 << 63,
        1,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff8000000000001,
        u64::MAX,
    ];
    let mut list = db.list().unwrap();
    let mut expected_list = Vec::new();
    for i in 0..count {
        let bits = patterns[i % patterns.len()];
        list = list
            .list_push(&db.float(f64::from_bits(bits)).unwrap())
            .unwrap();
        expected_list.push(bits);
    }
    let mut map = db.map().unwrap();
    let mut expected = BTreeMap::new();
    let mut snapshots = Vec::new();
    for (i, op) in input.chunks(4).take(32).enumerate() {
        let key_id = op.get(1).copied().unwrap_or(0);
        let len = lengths[usize::from(key_id) % lengths.len()];
        let mut key = vec![if key_id & 128 == 0 { 0 } else { 255 }; len];
        if let Some(last) = key.last_mut() {
            *last = key_id % 4;
        }
        let bits = patterns[usize::from(op.get(2).copied().unwrap_or(0)) % patterns.len()];
        let value = db.float(f64::from_bits(bits)).unwrap();
        let index = usize::from(op.get(3).copied().unwrap_or(0)) * 4 % (expected_list.len() + 1);
        let old = (
            map.clone(),
            list.clone(),
            expected.clone(),
            expected_list.clone(),
        );
        if op[0] & 1 == 0 {
            map = map.map_set(&key, &value).unwrap();
            expected.insert(key, bits);
        } else {
            map = map.map_remove(&key).unwrap();
            expected.remove(&key);
        }
        match op[0] % 3 {
            0 => {
                list = list.list_insert(index, &value).unwrap();
                expected_list.insert(index, bits);
            }
            1 if index < expected_list.len() => {
                list = list.list_remove(index).unwrap();
                expected_list.remove(index);
            }
            _ if index < expected_list.len() => {
                list = list.list_set(index, &value).unwrap();
                expected_list[index] = bits;
            }
            _ => {}
        }
        assert_eq!(entries(&map), expected);
        check_list(&list, &expected_list);
        assert_eq!(entries(&old.0), old.2);
        check_list(&old.1, &old.3);
        let mut tx = db.begin();
        tx.put(c, b"map", &map).unwrap();
        tx.put(c, b"list", &list).unwrap();
        tx.commit().unwrap();
        if i % 8 == 0 {
            snapshots.push((db.snapshot(), expected.clone(), expected_list.clone()));
        }
    }
    for (snapshot, expected, items) in snapshots {
        assert_eq!(
            entries(&snapshot.get(c, b"map").unwrap().unwrap()),
            expected
        );
        check_list(&snapshot.get(c, b"list").unwrap().unwrap(), &items);
    }
}
#[test]
fn replay_tree_boundaries() {
    tree_adversarial_keys(&[0, 242, 6, 255, 0, 233, 5, 255, 1, 242, 0, 0]);
    for boundary in 0..6 {
        let mut input = vec![boundary, 8, 6, 255];
        input.extend(0..128u8);
        tree_adversarial_keys(&input);
    }
}
