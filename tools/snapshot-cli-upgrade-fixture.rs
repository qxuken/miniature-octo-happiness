use weekend_db::{Database, Limits};

#[cfg(windows)]
fn keygen(directory: &std::path::Path) {
    use std::io::Write;
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(["weekend-replicate".to_owned()]).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("key.der"))
        .unwrap()
        .write_all(&signing_key.serialize_der())
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("cert.der"))
        .unwrap()
        .write_all(cert.der().as_ref())
        .unwrap();
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args[1] == "identity" {
        println!("{}", weekend_db::version::SOURCE_ID);
        return;
    }
    match args[1].as_str() {
        #[cfg(windows)]
        "keygen" => keygen(std::path::Path::new(&args[2])),
        "create" => {
            let db = Database::create_file(&args[2], Limits::default()).unwrap();
            let c = db.create_collection().unwrap();
            let mut tx = db.begin();
            tx.put(c, b"row", &db.bytes(&vec![1; 32768]).unwrap())
                .unwrap();
            assert_eq!(tx.commit().unwrap(), 2);
            assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), 1);
        }
        "advance" => {
            let db = Database::open_file(&args[2], Limits::default()).unwrap();
            let revision: u64 = args[3].parse().unwrap();
            let mut tx = db.begin();
            tx.put(
                db.collection_id(1),
                b"row",
                &db.bytes(&vec![(revision - 2) as u8; 32768]).unwrap(),
            )
            .unwrap();
            assert_eq!(tx.commit().unwrap(), revision);
        }
        "check" => {
            let db = Database::open_file(&args[2], Limits::default()).unwrap();
            let revision: u64 = args[3].parse().unwrap();
            let snapshot = db.snapshot();
            assert_eq!(snapshot.revision(), revision);
            let rows = snapshot.scan(db.collection_id(1)).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, b"row");
            assert_eq!(
                rows[0].1.as_bytes().unwrap(),
                vec![(revision - 2) as u8; 32768]
            );
            assert_eq!(db.retained_ids().unwrap(), vec![1]);
            let old = db.retained_snapshot(1).unwrap();
            assert_eq!(old.revision(), 2);
            let rows = old.scan(db.collection_id(1)).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, b"row");
            assert_eq!(rows[0].1.as_bytes().unwrap(), vec![1; 32768]);
            println!("verified current={revision} retained=1:2 rows=1 bytes=32768");
        }
        _ => panic!("fixture command"),
    }
}
