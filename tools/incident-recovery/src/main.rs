//! A fresh core consumer, launched through xtask's shared bounded supervisor.
use weekend_db::{Database, Limits};

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
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
