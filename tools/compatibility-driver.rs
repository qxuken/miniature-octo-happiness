use weekend_db::{Database, Error, Limits};
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args[0].to_str().unwrap() {
        "write" => {
            let db = Database::create_file(&args[1], Limits::default()).unwrap();
            let c = db.create_collection().unwrap();
            let mut tx = db.begin();
            tx.put(c, b"key", &db.integer(11).unwrap()).unwrap();
            assert_eq!(tx.commit(), Ok(2));
            assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(1));
            let mut tx = db.begin();
            tx.put(c, b"key", &db.integer(21).unwrap()).unwrap();
            tx.put(c, b"new", &db.bytes(b"current").unwrap()).unwrap();
            assert_eq!(tx.commit(), Ok(4));
        }
        "read" => {
            let db = Database::open_file(&args[1], Limits::default()).unwrap();
            let c = db.collection_id(1);
            let now = db.snapshot();
            assert_eq!(now.revision(), 4);
            assert_eq!(now.scan(c).unwrap().len(), 2);
            assert_eq!(now.get(c, b"key").unwrap().unwrap().as_i64(), Ok(21));
            assert_eq!(
                now.get(c, b"new").unwrap().unwrap().as_bytes().unwrap(),
                b"current"
            );
            let old = db.retained_snapshot(1).unwrap();
            assert_eq!(old.revision(), 2);
            assert_eq!(old.scan(c).unwrap().len(), 1);
            assert_eq!(old.get(c, b"key").unwrap().unwrap().as_i64(), Ok(11));
            assert!(old.get(c, b"new").unwrap().is_none());
        }
        "reject" => assert!(matches!(
            Database::open_file(&args[1], Limits::default()),
            Err(Error::UnsupportedFormat)
        )),
        _ => panic!("unknown mode"),
    }
}
