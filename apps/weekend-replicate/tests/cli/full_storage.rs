use super::*;
use sha2::{Digest, Sha256};
use weekend_db::replication::{Enrollment, Follower};

const STREAM: &str = "11221122112211221122112211221122";
fn receipt(output: &str) -> &str {
    output
        .lines()
        .find(|line| line.starts_with("follower durable_revision="))
        .expect("missing follower receipt")
}

#[test]
#[ignore = "requires a dedicated empty Linux tmpfs no larger than 8 MiB"]
fn authenticated_follower_reopens_at_the_old_receipt_when_tmpfs_is_full() {
    let (mount, empty_space) = test_tmpfs::empty_mount("WEEKEND_TEST_FOLLOWER_TMPFS");

    let dir = directory("follower-full-storage");
    let root = &dir.0;
    let primary = root.join("primary.wdb");
    seed_primary(&primary);
    let source = primary.to_str().unwrap();
    let first_image = root.join("first.wdb");
    let first_ticket = root.join("first-ticket.wdb");
    run(&[
        "capture",
        source,
        STREAM,
        first_image.to_str().unwrap(),
        first_ticket.to_str().unwrap(),
    ]);
    let description = run(&["describe", source, STREAM]);
    let identity = description
        .split_ascii_whitespace()
        .find_map(|part| part.strip_prefix("file_identity="))
        .unwrap();
    let sender = root.join("sender");
    let receiver = root.join("receiver");
    credentials(&sender);
    credentials(&receiver);
    let sender_cert = sender.join("cert.der");
    let receiver_cert = receiver.join("cert.der");
    let next_image = root.join("next.wdb");
    let next_ticket = root.join("next-ticket.wdb");
    let payload = vec![0xa5; 256 * 1024];
    {
        let db = Database::open_file(&primary, Limits::default()).unwrap();
        let mut transaction = db.begin();
        transaction
            .put(
                db.collection_id(1),
                b"payload",
                &db.bytes(&payload).unwrap(),
            )
            .unwrap();
        assert_eq!(transaction.commit(), Ok(5));
    }
    let local = run(&[
        "capture",
        source,
        STREAM,
        next_image.to_str().unwrap(),
        next_ticket.to_str().unwrap(),
    ]);
    assert!(local.starts_with(&format!(
        "local captured_revision=5 stream={STREAM} file_identity={identity}\n"
    )));
    assert!(!local.contains("follower durable_revision="));
    let exact_image = fs::read(&next_image).unwrap();
    let exact_ticket = fs::read(&next_ticket).unwrap();
    let old_digest = Sha256::digest(fs::read(&first_image).unwrap());
    let next_digest = Sha256::digest(&exact_image);

    let serve = |follower: &str| {
        Server::start(
            root,
            &[
                "serve",
                follower,
                STREAM,
                identity,
                "127.0.0.1:0",
                receiver.to_str().unwrap(),
                sender_cert.to_str().unwrap(),
                "--max-exchanges",
                "1",
            ],
        )
    };
    let send = |address: &str, image: &Path, ticket: &Path| {
        Command::new(BIN)
            .args([
                "ship-captured",
                source,
                STREAM,
                image.to_str().unwrap(),
                ticket.to_str().unwrap(),
                address,
                sender.to_str().unwrap(),
                receiver_cert.to_str().unwrap(),
                "--attempts",
                "1",
            ])
            .output()
            .unwrap()
    };

    for selection_failure in [false, true] {
        let label = if selection_failure {
            "selection"
        } else {
            "staging"
        };
        let path = mount.join(label);
        fs::create_dir(&path).unwrap();
        let follower = Directory(path);
        let path = follower.0.to_str().unwrap();
        run(&["follower-init", path, STREAM, identity]);
        let (mut server, address) = serve(path);
        let installed = send(&address, &first_image, &first_ticket);
        server.finished();
        assert!(installed.status.success(), "{installed:?}");
        let installed = String::from_utf8(installed.stdout).unwrap();
        let prior = receipt(&installed);
        assert_eq!(
            prior,
            format!("follower durable_revision=4 stream={STREAM} digest={old_digest:x}")
        );
        let old_files = files(&follower.0);
        assert_eq!(old_files.len(), 2);

        let (block, available) = test_tmpfs::space(&follower.0);
        let length = exact_image.len() as u64;
        assert!(length > 2 * block && length.is_multiple_of(block) && length < available);
        let room = if selection_failure {
            length
        } else {
            length / block / 2 * block
        };
        let ballast = test_tmpfs::fill(&follower.0, room);

        let (mut server, address) = serve(path);
        let failed = send(&address, &next_image, &next_ticket);
        assert_eq!(failed.status.code(), Some(1), "{failed:?}");
        let transcript = String::from_utf8(failed.stdout).unwrap();
        assert!(transcript.starts_with("local captured_revision=5 "));
        assert!(!transcript.contains("follower durable_revision="));
        assert_eq!(server.exit_status().success(), !selection_failure);
        test_tmpfs::assert_full(&follower.0);
        let staged = fs::read(follower.0.join("replica-1.wdb")).unwrap();
        assert_eq!(staged.len() as u64, room);
        assert_eq!(staged, exact_image[..staged.len()]);
        for (file, bytes) in &old_files {
            assert_eq!(fs::read(follower.0.join(file)).unwrap(), *bytes);
        }

        // A new follower process must recover revision 4 even while native writes fail.
        let (mut reopened, address) = serve(path);
        let polled = run(&[
            "poll",
            source,
            STREAM,
            &address,
            sender.to_str().unwrap(),
            receiver_cert.to_str().unwrap(),
        ]);
        reopened.finished();
        assert!(polled.contains("local verified_revision=5"));
        assert_eq!(receipt(&polled), prior);
        verify_follower(&follower.0, STREAM, identity);
        test_tmpfs::assert_full(&follower.0);
        for (file, bytes) in &old_files {
            assert_eq!(fs::read(follower.0.join(file)).unwrap(), *bytes);
        }

        fs::remove_file(&ballast).unwrap();
        let (mut server, address) = serve(path);
        let retried = send(&address, &next_image, &next_ticket);
        server.finished();
        assert!(retried.status.success(), "{retried:?}");
        let retried = String::from_utf8(retried.stdout).unwrap();
        let next = receipt(&retried);
        assert_eq!(
            next,
            format!("follower durable_revision=5 stream={STREAM} digest={next_digest:x}")
        );
        assert_eq!(
            fs::read(follower.0.join("replica-1.wdb")).unwrap(),
            exact_image
        );
        assert_eq!(fs::read(&next_image).unwrap(), exact_image);
        assert_eq!(fs::read(&next_ticket).unwrap(), exact_ticket);
        {
            let follower = Follower::open(
                &follower.0,
                Enrollment {
                    stream_id: parse_id(STREAM),
                    file_identity: parse_id(identity),
                },
                Limits::default(),
            )
            .unwrap();
            let replica = follower.replica().unwrap();
            let current = replica.snapshot();
            let collection = replica.collection_id(1);
            assert_eq!(current.revision(), 5);
            assert_eq!(current.count(collection).unwrap(), 2);
            assert_eq!(
                current.get(collection, b"key").unwrap().unwrap().as_u64(),
                Ok(2)
            );
            assert_eq!(
                current
                    .get(collection, b"payload")
                    .unwrap()
                    .unwrap()
                    .as_bytes(),
                Ok(payload.as_slice())
            );
            assert_eq!(replica.retained_ids().unwrap(), [1]);
            let retained = replica.retained_snapshot(1).unwrap();
            assert_eq!(retained.revision(), 2);
            assert_eq!(retained.count(collection).unwrap(), 1);
            assert_eq!(
                retained.get(collection, b"key").unwrap().unwrap().as_u64(),
                Ok(1)
            );
        }
        let selected = files(&follower.0);
        let (mut server, address) = serve(path);
        let repeated = send(&address, &next_image, &next_ticket);
        server.finished();
        assert!(repeated.status.success(), "{repeated:?}");
        assert_eq!(receipt(&String::from_utf8(repeated.stdout).unwrap()), next);
        assert_eq!(files(&follower.0), selected);
        eprintln!(
            "{label}: ENOSPC after {} of {length} bytes, reopened revision 4, retry/replay revision 5",
            staged.len()
        );
    }
    assert_eq!(test_tmpfs::space(&mount).1, empty_space);
    assert!(fs::read_dir(&mount).unwrap().next().is_none());
}
