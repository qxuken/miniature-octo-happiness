use weekend_db::{Database, Limits};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    match args[1].as_str() {
        "create" => {
            let db = Database::create_file(&args[2], Limits::default()).unwrap();
            let c = db.create_collection().unwrap();
            let mut tx = db.begin();
            tx.put(c, b"value", &db.bytes(&vec![1; 32768]).unwrap())
                .unwrap();
            assert_eq!(tx.commit().unwrap(), 2);
            assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 1);
        }
        "advance" => {
            let db = Database::open_file(&args[2], Limits::default()).unwrap();
            let mut tx = db.begin();
            tx.put(
                db.collection_id(1),
                b"value",
                &db.bytes(&vec![2; 32768]).unwrap(),
            )
            .unwrap();
            assert_eq!(tx.commit().unwrap(), 4);
        }
        "check" => {
            let db = Database::open_file(&args[2], Limits::default()).unwrap();
            let revision: u64 = args[3].parse().unwrap();
            let byte = if revision == 3 {
                1
            } else {
                assert_eq!(revision, 4);
                2
            };
            assert_eq!(db.snapshot().revision(), revision);
            assert_eq!(
                db.snapshot()
                    .get(db.collection_id(1), b"value")
                    .unwrap()
                    .unwrap()
                    .as_bytes()
                    .unwrap(),
                vec![byte; 32768]
            );
            assert_eq!(db.retained_ids().unwrap(), vec![1]);
            let old = db.retained_snapshot(1).unwrap();
            assert_eq!(old.revision(), 2);
            assert_eq!(
                old.get(db.collection_id(1), b"value")
                    .unwrap()
                    .unwrap()
                    .as_bytes()
                    .unwrap(),
                vec![1; 32768]
            );
        }
        _ => panic!("fixture command"),
    }
}
