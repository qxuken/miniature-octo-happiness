//! Exercise the authenticated-channel entry point over a test-owned loopback pair.
use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};
use weekend_db::{
    CollectionId, Database, Limits, ReadOnlyDatabase, Snapshot,
    replication::{Follower, Primary},
    storage,
};

pub fn roundtrip(
    source: &Path,
    area: &Path,
    limits: Limits,
    retained: u64,
    max_transfer: u64,
    verify: impl Fn(&Snapshot<'_>, CollectionId, bool),
) {
    fs::create_dir(area).unwrap();
    let replica_dir = area.join("follower");
    fs::create_dir(&replica_dir).unwrap();
    let mut db = Database::open_file(source, limits).unwrap();
    let local = db.snapshot().revision();
    let mut primary = Primary::new(&db, [111; 16]).unwrap();
    let enroll = primary.enrollment();
    let export_path = area.join("capture");
    let export = primary.export_file(&mut db, &export_path).unwrap();
    let descriptor = export.descriptor();
    drop(export);
    let mut export = primary
        .reopen_export_file(&export_path, descriptor, limits)
        .unwrap();
    let mut follower = Follower::create(&replica_dir, enroll, limits).unwrap();
    assert_eq!(follower.progress().unwrap(), None);
    let mut ship = |follower: &mut Follower| {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::scope(|s| {
            let receive = s.spawn(|| {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(120)))
                    .unwrap();
                follower.serve_authenticated(stream)
            });
            let client = TcpStream::connect(addr).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(120)))
                .unwrap();
            let ack = primary.ship(&mut export, client);
            (ack, receive.join().unwrap())
        })
    };
    if descriptor.length > storage::MAX_IMAGE_BYTES as u64 {
        let (send, recv) = ship(&mut follower);
        assert!(send.is_err());
        assert!(recv.is_err());
        assert_eq!(follower.progress().unwrap(), None);
        assert!(!replica_dir.join("replica-0.wdb").exists());
    }
    follower.set_max_transfer_bytes(max_transfer).unwrap();
    let (ack, recv) = ship(&mut follower);
    recv.unwrap();
    let ack = ack.unwrap();
    assert_eq!(ack.revision, local);
    drop(follower);
    drop(export);
    drop(db);
    let mut reopened = Follower::open(&replica_dir, enroll, limits).unwrap();
    assert_eq!(reopened.progress().unwrap(), Some(ack));
    let replica = reopened.replica().unwrap();
    let c = replica.collection_id(1);
    verify(&replica.snapshot(), c, true);
    verify(&replica.retained_snapshot(retained).unwrap(), c, false);
    reopened.cleanup_inactive().unwrap();
    let slot = reopened.status().unwrap().selected_slot.unwrap();
    drop(reopened);
    let backup = area.join("backup");
    let restore = area.join("restore");
    storage::backup_file(
        replica_dir.join(format!("replica-{slot}.wdb")),
        &backup,
        limits,
    )
    .unwrap();
    storage::restore_file(&backup, &restore, limits).unwrap();
    for path in [&backup, &restore] {
        let view = ReadOnlyDatabase::open_file(path, limits.into()).unwrap();
        view.check().unwrap();
        let c = view.collection_id(1);
        verify(&view.snapshot(), c, true);
        verify(&view.retained_snapshot(retained).unwrap(), c, false);
    }
}
