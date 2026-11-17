//! Real certificate-chain controls: the configured certificate is a trust anchor,
//! but a different valid leaf signed by it must still fail the exact-leaf pin.
use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "snapshot-leaf-pin-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn signed_alternate_leaf_is_rejected_after_chain_trust_on_both_endpoints() {
    let (a, alternate_a) = Credentials::anchor_and_signed_alternate();
    let (b, alternate_b) = Credentials::anchor_and_signed_alternate();
    for (alternate_client, alternate_server) in [(false, false), (true, false), (false, true)] {
        let dir = Directory::new();
        let source = dir.0.join("source");
        let db = Database::create_file(&source, Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        let mut tx = db.begin();
        tx.put(collection, b"key", &db.unsigned(42).unwrap())
            .unwrap();
        tx.commit().unwrap();
        drop(tx);
        drop(db);
        let mut prepared = prepare(&source, [1; 16], &dir.0.join("export")).unwrap();
        let enrollment = prepared.primary.enrollment();
        let folder = dir.0.join("follower");
        fs::create_dir(&folder).unwrap();
        drop(Follower::create(&folder, enrollment, Limits::default()).unwrap());
        let before = fs::read(folder.join("replica-manifest.wdb")).unwrap();
        let mut follower = Follower::open(&folder, enrollment, Limits::default()).unwrap();
        let client = Tls::new(
            if alternate_client { &alternate_a } else { &a },
            b.certificate(),
        )
        .unwrap();
        let server = Tls::new(
            if alternate_server { &alternate_b } else { &b },
            a.certificate(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, served) = thread::scope(|scope| {
            let receiving =
                scope.spawn(|| serve_one(&mut follower, &server, listener.accept().unwrap().0));
            let sent = ship(&mut prepared, &client, address, 1);
            (sent, receiving.join().unwrap())
        });
        if !alternate_client && !alternate_server {
            served.unwrap();
            assert_eq!(sent.unwrap().revision, 2);
            assert_eq!(follower.progress().unwrap().unwrap().revision, 2);
        } else {
            assert!(sent.is_err());
            if alternate_client {
                match served {
                    Err(SessionError::Authentication(error)) => assert_eq!(
                        error.to_string(),
                        "peer certificate does not match the configured pin"
                    ),
                    other => panic!("expected post-chain pin rejection, got {other:?}"),
                }
            } else {
                assert_eq!(
                    sent.err().unwrap().to_string(),
                    "peer certificate does not match the configured pin"
                );
            }
            assert_eq!(follower.progress().unwrap(), None);
            drop(follower);
            // Native Windows excludes raw file reads while the lease is held.
            assert_eq!(
                fs::read(folder.join("replica-manifest.wdb")).unwrap(),
                before
            );
            assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
            assert_eq!(
                Follower::open(&folder, enrollment, Limits::default())
                    .unwrap()
                    .progress()
                    .unwrap(),
                None
            );
        }
    }
}

#[test]
fn cli_identity_option_and_permanent_retry_controls() {
    use std::{
        ffi::{OsStr, OsString},
        io::{Read, Write},
    };
    assert!(identity(OsStr::new("ABCDEFabcdefABCDEFabcdefABCDEFab")).is_ok());
    for id in [
        "x",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "00000000000000000000000000000000",
    ] {
        assert!(identity(OsStr::new(id)).is_err());
    }
    assert_eq!(option(&[], "--attempts", 3, 3).unwrap(), 3);
    for (flag, n, valid) in [
        ("--attempts", "1", true),
        ("--attempts", "3", true),
        ("--attempts", "0", false),
        ("--attempts", "4", false),
        ("--wrong", "1", false),
        ("--attempts", "bad", false),
    ] {
        assert_eq!(
            option(
                &[OsString::from(flag), OsString::from(n)],
                "--attempts",
                3,
                3
            )
            .is_ok(),
            valid
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert!(text(OsStr::from_bytes(&[255])).is_err());
    }
    let dir = Directory::new();
    let source = dir.0.join("source");
    drop(Database::create_file(&source, Limits::default()).unwrap());
    let mut prepared = prepare(&source, [1; 16], &dir.0.join("export")).unwrap();
    let a = Credentials::generate().unwrap();
    let b = Credentials::generate().unwrap();
    let client = Tls::new(&a, b.certificate()).unwrap();
    let server = Tls::new(&b, a.certificate()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::scope(|scope| {
        let receiver = scope.spawn(move || {
            let mut channel = server.accept(listener.accept().unwrap().0).unwrap();
            let mut request = [0; 89];
            channel.read_exact(&mut request).unwrap();
            channel.write_all(&[3]).unwrap();
            channel.flush().unwrap();
        });
        // UnsupportedFormat must stop on attempt1 even when attempt2 is allowed.
        let error = ship(&mut prepared, &client, address, 2).unwrap_err();
        assert_eq!(
            error.downcast_ref::<Error>(),
            Some(&Error::UnsupportedFormat)
        );
        receiver.join().unwrap();
    });
}
