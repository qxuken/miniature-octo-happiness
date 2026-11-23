use super::*;
use crate::{Limits, storage::MemoryStore};
use std::io::{self, Cursor};
use wire::Reply;

const STREAM: [u8; 16] = [2; 16];

struct Exchange {
    replies: Cursor<Vec<u8>>,
    sent: Vec<u8>,
}
impl Exchange {
    fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        let mut bytes = Vec::new();
        for reply in replies {
            wire::reply(&mut bytes, Ok(reply)).unwrap();
        }
        Self {
            replies: Cursor::new(bytes),
            sent: Vec::new(),
        }
    }
}
impl Read for Exchange {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.replies.read(bytes)
    }
}
impl Write for Exchange {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.sent.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn exports() -> (Database, [Export; 3]) {
    exports_on(MemoryStore::new().unwrap())
}

fn exports_on(source: impl ByteStore + 'static) -> (Database, [Export; 3]) {
    let mut db = Database::create_on(source, [1; 16], Limits::default()).unwrap();
    let mut primary = Primary::new(&db, STREAM).unwrap();
    let exports = std::array::from_fn(|revision| {
        if revision != 0 {
            assert_eq!(db.begin().commit(), Ok(revision as u64));
        }
        primary
            .export_to(&mut db, MemoryStore::new().unwrap())
            .unwrap()
    });
    (db, exports)
}

fn ack(export: &Export) -> DurableAck {
    export.offer.ack(export.enrollment)
}

fn poll(primary: &mut Primary, reply: Reply) -> Result<Option<DurableAck>> {
    primary.poll(Exchange::new([reply]))
}

#[test]
fn live_poll_refreshes_local_revision_and_keeps_accepted_follower_progress() {
    let (source, [old, _, latest]) = exports();
    let old = ack(&old);
    let latest = ack(&latest);
    let mut primary = Primary::new(&source, STREAM).unwrap();
    primary.verified_revision = old.revision;
    assert_eq!(
        poll(&mut primary, Reply::Installed(latest)),
        Err(Error::ForeignStore)
    );

    let foreign =
        Database::create_on(MemoryStore::new().unwrap(), [9; 16], Limits::default()).unwrap();
    let mut channel = Exchange::new([Reply::Empty]);
    assert_eq!(
        primary.poll_live(&foreign, &mut channel),
        Err(Error::ForeignStore)
    );
    assert!(channel.sent.is_empty());

    let empty = primary
        .poll_live(&source, Exchange::new([Reply::Empty]))
        .unwrap();
    assert_eq!(empty.local_revision, latest.revision);
    assert_eq!(empty.revision_gap(), None);
    let observed = primary
        .poll_live(&source, Exchange::new([Reply::Installed(latest)]))
        .unwrap();
    assert_eq!(observed.follower, Some(latest));
    assert_eq!(observed.revision_gap(), Some(0));
    assert_eq!(source.begin().commit(), Ok(latest.revision + 1));
    assert_eq!(
        primary.poll_live(&source, Exchange::new([Reply::Installed(old)])),
        Err(Error::Conflict)
    );
    let observed = primary
        .poll_live(&source, Exchange::new([Reply::Installed(latest)]))
        .unwrap();
    assert_eq!(observed.local_revision, latest.revision + 1);
    assert_eq!(observed.follower, Some(latest));
    assert_eq!(observed.revision_gap(), Some(1));
}

#[test]
fn live_copy_failures_leave_the_source_healthy_and_the_sender_bound_unchanged() {
    use crate::storage::{APPEND_START, Fault, IoEvent, IoKind, SimulatedDisk};

    let disk = SimulatedDisk::new().unwrap();
    let mut source = Database::create_on(disk.clone(), [4; 16], Limits::default()).unwrap();
    let mut primary = Primary::new(&source, STREAM).unwrap();
    let old = primary
        .export_to(&mut source, MemoryStore::new().unwrap())
        .unwrap();
    let old_ack = ack(&old);
    assert_eq!(
        poll(&mut primary, Reply::Installed(old_ack)),
        Ok(Some(old_ack))
    );
    let collection = source.create_collection().unwrap();
    let write = |source: &Database, value| {
        let mut tx = source.begin();
        tx.put(collection, b"row", &source.unsigned(value).unwrap())
            .unwrap();
        tx.commit().unwrap()
    };
    assert_eq!(write(&source, 1), 2);
    assert_eq!(source.retain_snapshot(&source.snapshot()), Ok(1));
    assert_eq!(write(&source, 2), 4);
    let original = disk.durable_image().unwrap();
    let baseline = source.memory_usage().live_bytes;
    let rejected = SimulatedDisk::new().unwrap();
    assert_eq!(
        primary
            .export_live_to(
                &source,
                rejected.clone(),
                Limits {
                    max_live_bytes: 1,
                    ..Limits::default()
                }
            )
            .err(),
        Some(Error::Limit)
    );
    assert!(rejected.durable_image().unwrap().is_empty());
    assert_eq!(primary.verified_revision, 0);

    let trace = SimulatedDisk::new().unwrap();
    let mut probe = Primary::new(&source, STREAM).unwrap();
    let expected = probe
        .export_live_to(&source, trace.clone(), Limits::default())
        .unwrap();
    let expected_ack = ack(&expected);
    let expected_bytes = trace.durable_image().unwrap();
    let quiescent = probe
        .export_to(&mut source, MemoryStore::new().unwrap())
        .unwrap();
    assert_eq!(expected.descriptor(), quiescent.descriptor());
    drop(quiescent);
    let events = trace.trace().unwrap();
    let after_sync = events
        .iter()
        .rposition(|event| event.kind == IoKind::Sync)
        .unwrap()
        + 1;
    let root_offset = (1 + expected.revision() % 2) * REGION as u64;
    let root_write = events[..after_sync]
        .iter()
        .rposition(|event| event.kind == IoKind::Write && event.offset == root_offset)
        .unwrap();
    // The last length check and contiguous full-image read are the digest pass.
    // Witness and graph reads between the final sync and that pass are separate.
    let mut digest = vec![IoEvent {
        kind: IoKind::Length,
        offset: 0,
        len: 0,
    }];
    digest.extend(
        expected_bytes
            .chunks(REGION)
            .enumerate()
            .map(|(index, chunk)| IoEvent {
                kind: IoKind::Read,
                offset: (index * REGION) as u64,
                len: chunk.len(),
            }),
    );
    assert!(events.ends_with(&digest));
    let verification = &events[after_sync..events.len() - digest.len()];
    let witness = (3 + expected.revision() % 2) * REGION as u64;
    assert!(
        verification
            .iter()
            .any(|event| event.kind == IoKind::Read && event.offset == witness)
    );
    assert!(
        verification
            .iter()
            .any(|event| event.kind == IoKind::Read && event.offset >= APPEND_START as u64)
    );

    for (step, event) in events.iter().enumerate() {
        let mut failed = SimulatedDisk::new().unwrap();
        failed.arm(step, Fault::Error);
        let expected_error = if (root_write..after_sync).contains(&step) {
            Error::OutcomeUnknown
        } else {
            Error::Io
        };
        assert_eq!(
            primary
                .export_live_to(&source, failed.clone(), Limits::default())
                .err(),
            Some(expected_error),
            "step {step}: {event:?}"
        );
        assert_eq!(failed.trace().unwrap(), events[..=step]);
        failed.lock().unwrap();
        failed.unlock();
        assert_eq!(primary.verified_revision, old_ack.revision);
        assert_eq!(
            poll(&mut primary, Reply::Installed(expected_ack)),
            Err(Error::ForeignStore)
        );
        assert_eq!(primary.progress, Some(old_ack));
        assert!(!source.requires_recovery());
        assert_eq!(source.memory_usage().live_bytes, baseline);
        assert_eq!(disk.durable_image().unwrap(), original);
    }
    eprintln!(
        "live capture: {} preparation operations ({} root and witness), {} verification, {} digest",
        after_sync,
        after_sync - root_write,
        verification.len(),
        digest.len()
    );
    let destination = SimulatedDisk::new().unwrap();
    let retry = primary
        .export_live_to(&source, destination.clone(), Limits::default())
        .unwrap();
    assert_eq!(retry.descriptor(), expected.descriptor());
    assert_eq!(destination.durable_image().unwrap(), expected_bytes);
    assert_eq!(primary.progress, Some(old_ack));
    assert_eq!(
        poll(&mut primary, Reply::Installed(expected_ack)),
        Ok(Some(expected_ack))
    );
    let check_roots = |db: &Database| {
        assert_eq!(db.retained_ids().unwrap(), [1]);
        let retained = db.retained_snapshot(1).unwrap();
        let current = db.snapshot();
        let collection = db.collection_id(1);
        assert_eq!(retained.revision(), 2);
        assert_eq!(current.revision(), 4);
        for (snapshot, value) in [(retained, 1), (current, 2)] {
            assert_eq!(
                snapshot.get(collection, b"row").unwrap().unwrap().as_u64(),
                Ok(value)
            );
        }
    };
    check_roots(&source);
    let recovered = Database::open_on(
        SimulatedDisk::from_image(&expected_bytes).unwrap(),
        Limits::default(),
    )
    .unwrap();
    check_roots(&recovered);
    assert_eq!(source.begin().commit(), Ok(5));
}

#[test]
fn live_capture_source_and_limits_are_checked_before_creating_destination() {
    let limits = Limits::default();
    let create =
        |identity| Database::create_on(MemoryStore::new().unwrap(), identity, limits).unwrap();
    let source = create([1; 16]);
    let older = create([1; 16]);
    let foreign = create([9; 16]);
    let volatile = Database::with_limits(limits).unwrap();
    assert_eq!(source.begin().commit(), Ok(1));
    let mut primary = Primary::new(&source, STREAM).unwrap();
    assert_eq!(source.begin().commit(), Ok(2));
    // Vary only identity against the admitted source. The older fixture
    // separately removes revision headroom while keeping identity equal.
    assert_eq!(foreign.begin().commit(), Ok(1));
    assert_eq!(foreign.begin().commit(), Ok(2));
    for (source, capture_limits, expected) in [
        (
            &source,
            Limits {
                max_depth: 0,
                ..limits
            },
            Error::Invalid,
        ),
        (&volatile, limits, Error::Invalid),
        (&older, limits, Error::ForeignStore),
        (&foreign, limits, Error::ForeignStore),
    ] {
        assert_eq!(
            primary
                .capture_live::<MemoryStore>(source, capture_limits, || panic!(
                    "destination created before admission"
                ))
                .err(),
            Some(expected)
        );
        assert_eq!(primary.verified_revision, 1);
    }
    let mut calls = 0;
    assert_eq!(
        primary
            .capture_live::<MemoryStore>(&source, limits, || {
                calls += 1;
                Err(Error::Busy)
            })
            .err(),
        Some(Error::Busy)
    );
    assert_eq!(calls, 1);
    assert_eq!(primary.verified_revision, 1);
    let export = primary
        .capture_live(&source, limits, || {
            calls += 1;
            MemoryStore::new()
        })
        .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(export.revision(), 2);
    assert_eq!(primary.verified_revision, 2);
    assert!(primary.progress.is_none());
}

#[test]
fn live_copy_stages_large_values_within_the_copy_budget() {
    let limits = Limits::default();
    let mut source = Database::create_on(MemoryStore::new().unwrap(), [4; 16], limits).unwrap();
    let collection = source.create_collection().unwrap();
    let mut tx = source.begin();
    for key in 0..6 {
        let value = source.bytes(&vec![key; 98 * 1024]).unwrap();
        tx.put(collection, &[key], &value).unwrap();
    }
    assert_eq!(tx.commit(), Ok(2));
    assert_eq!(source.retain_snapshot(&source.snapshot()), Ok(1));

    let mut primary = Primary::new(&source, STREAM).unwrap();
    let expected = primary
        .export_to(&mut source, MemoryStore::new().unwrap())
        .unwrap()
        .descriptor();
    let budget = Limits {
        max_live_bytes: 800 * 1024,
        ..limits
    };
    let destination = crate::storage::SimulatedDisk::new().unwrap();
    assert_eq!(
        primary
            .export_live_to(
                &source,
                destination.clone(),
                Limits {
                    max_live_bytes: 512 * 1024,
                    ..limits
                },
            )
            .err(),
        Some(Error::Limit)
    );
    assert!(destination.durable_image().unwrap().is_empty());
    let export = primary
        .export_live_to(&source, destination.clone(), budget)
        .unwrap();
    assert_eq!(export.descriptor(), expected);
    drop(export);
    primary
        .reopen_export(destination, expected, budget)
        .unwrap();
}

fn wrong_digest(mut ack: DurableAck) -> DurableAck {
    ack.digest[0] ^= 1;
    ack
}

#[test]
fn reopened_exports_require_the_saved_identity_and_keep_the_original_bytes() {
    use crate::storage::{Fault, IoKind, SimulatedDisk};

    let mut source =
        Database::create_on(MemoryStore::new().unwrap(), [4; 16], Limits::default()).unwrap();
    source.begin().commit().unwrap();
    let mut primary = Primary::new(&source, STREAM).unwrap();
    let disk = SimulatedDisk::new().unwrap();
    let export = primary.export_to(&mut source, disk.clone()).unwrap();
    let descriptor = export.descriptor();
    let expected = ack(&export);
    let original = disk.durable_image().unwrap();
    assert_eq!(
        descriptor.digest,
        <[u8; 32]>::from(Sha256::digest(&original))
    );
    assert_eq!(
        primary
            .reopen_export(disk.clone(), descriptor, Limits::default())
            .err(),
        Some(Error::Busy)
    );
    drop(export);
    source.begin().commit().unwrap();
    let mut primary = Primary::new(&source, STREAM).unwrap();
    disk.arm(usize::MAX, Fault::Error);
    assert_eq!(
        primary
            .reopen_export(
                disk.clone(),
                descriptor,
                Limits {
                    max_depth: 0,
                    ..Limits::default()
                }
            )
            .err(),
        Some(Error::Invalid)
    );
    assert!(disk.trace().unwrap().is_empty());
    for (candidate, error) in [
        (
            ExportDescriptor {
                enrollment: Enrollment {
                    stream_id: [8; 16],
                    ..descriptor.enrollment
                },
                ..descriptor
            },
            Error::ForeignStore,
        ),
        (
            ExportDescriptor {
                revision: 3,
                ..descriptor
            },
            Error::ForeignStore,
        ),
        (
            ExportDescriptor {
                revision: 2,
                ..descriptor
            },
            Error::Corrupt,
        ),
        (
            ExportDescriptor {
                length: 1,
                ..descriptor
            },
            Error::Limit,
        ),
        (
            ExportDescriptor {
                length: descriptor.length + 1,
                ..descriptor
            },
            Error::Corrupt,
        ),
        (
            ExportDescriptor {
                digest: [0; 32],
                ..descriptor
            },
            Error::Corrupt,
        ),
    ] {
        assert_eq!(
            primary
                .reopen_export(disk.clone(), candidate, Limits::default())
                .err(),
            Some(error)
        );
    }
    let mut corrupt = original.clone();
    // Zero all witnesses. A byte-matching descriptor alone cannot admit
    // an unwitnessed candidate.
    corrupt[3 * REGION..5 * REGION].fill(0);
    let unwitnessed = ExportDescriptor {
        digest: Sha256::digest(&corrupt).into(),
        ..descriptor
    };
    assert_eq!(
        primary
            .reopen_export(
                SimulatedDisk::from_image(&corrupt).unwrap(),
                unwitnessed,
                Limits::default()
            )
            .err(),
        Some(Error::Corrupt)
    );
    disk.arm(usize::MAX, Fault::Error);
    let reopened = primary
        .reopen_export(disk.clone(), descriptor, Limits::default())
        .unwrap();
    assert_eq!(reopened.descriptor(), descriptor);
    let events = disk.trace().unwrap();
    assert!(events.iter().any(|event| event.kind == IoKind::Read));
    assert!(
        events
            .iter()
            .all(|event| matches!(event.kind, IoKind::Length | IoKind::Read))
    );
    drop(reopened);
    for (step, event) in events.iter().enumerate() {
        disk.arm(step, Fault::Error);
        assert_eq!(
            primary
                .reopen_export(disk.clone(), descriptor, Limits::default())
                .err(),
            Some(Error::Io),
            "step {step}: {event:?}"
        );
        assert_eq!(disk.trace().unwrap(), events[..=step]);
        assert_eq!(disk.durable_image().unwrap(), original);
        assert_eq!(primary.verified_revision, 2);
        assert!(primary.progress.is_none());
    }
    eprintln!("saved capture: {} read-only operations", events.len());
    disk.arm(usize::MAX, Fault::Error);
    let mut reopened = primary
        .reopen_export(disk.clone(), descriptor, Limits::default())
        .unwrap();
    assert_eq!(reopened.descriptor(), descriptor);
    let mut exchange = Exchange::new([Reply::Ready, Reply::Installed(expected)]);
    assert_eq!(primary.ship(&mut reopened, &mut exchange), Ok(expected));
    assert!(exchange.sent.ends_with(&original));
    drop(reopened);
    assert_eq!(disk.durable_image().unwrap(), original);
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|event| matches!(event.kind, IoKind::Length | IoKind::Read))
    );
}

#[test]
fn primary_poll_rejects_invalid_progress_without_losing_its_last_ack() {
    let (db, [old, current, next]) = exports();
    let (old, current, next) = (ack(&old), ack(&current), ack(&next));
    let installed = Reply::Installed;

    for (case, reply, expected) in [
        ("older revision", installed(old), Err(Error::Conflict)),
        ("advance", installed(next), Ok(Some(next))),
        ("same acknowledgment", installed(current), Ok(Some(current))),
        (
            "wrong stream",
            installed(DurableAck {
                stream_id: [3; 16],
                ..next
            }),
            Err(Error::ForeignStore),
        ),
        (
            "ahead of verified head",
            installed(DurableAck {
                revision: next.revision + 1,
                ..next
            }),
            Err(Error::ForeignStore),
        ),
        (
            "same revision, different digest",
            installed(wrong_digest(current)),
            Err(Error::Conflict),
        ),
        ("empty after progress", Reply::Empty, Err(Error::Conflict)),
    ] {
        let mut primary = Primary::new(&db, STREAM).unwrap();
        assert_eq!(poll(&mut primary, Reply::Empty), Ok(None));
        assert_eq!(poll(&mut primary, installed(current)), Ok(Some(current)));
        assert_eq!(poll(&mut primary, reply), expected, "{case}");
        if expected.is_err() {
            // A lower revision must still conflict, and the exact prior
            // acknowledgment must still be accepted.
            assert_eq!(
                poll(&mut primary, installed(old)),
                Err(Error::Conflict),
                "{case}"
            );
            assert_eq!(
                poll(&mut primary, installed(current)),
                Ok(Some(current)),
                "{case}"
            );
        } else if case == "advance" {
            assert_eq!(poll(&mut primary, installed(current)), Err(Error::Conflict));
        }
    }
}

#[test]
fn upload_flush_failure_preserves_progress_and_leaves_the_terminal_reply_unread() {
    struct FlushControl {
        exchange: Exchange,
        fail_second: bool,
        flushes: usize,
    }
    impl Read for FlushControl {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.exchange.read(bytes)
        }
    }
    impl Write for FlushControl {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.exchange.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_second && self.flushes == 2 {
                Err(io::ErrorKind::BrokenPipe.into())
            } else {
                self.exchange.flush()
            }
        }
    }
    fn image(export: &mut Export) -> Vec<u8> {
        let store = export.storage.image_bytes();
        let mut bytes = vec![0; store.length().unwrap() as usize];
        read_all(store, 0, &mut bytes).unwrap();
        bytes
    }

    for fail_second in [true, false] {
        let disk = crate::storage::SimulatedDisk::new().unwrap();
        let (source, [old, mut offered, _]) = exports_on(disk.clone());
        let previous = ack(&old);
        let expected = ack(&offered);
        let descriptor = offered.descriptor();
        let bytes = image(&mut offered);
        assert_eq!(bytes.len() as u64, descriptor.length);
        let source_revision = source.snapshot().revision();
        let source_bytes = disk.durable_image().unwrap();
        let mut primary = Primary::new(&source, STREAM).unwrap();
        assert_eq!(
            poll(&mut primary, Reply::Installed(previous)),
            Ok(Some(previous))
        );
        let mut channel = FlushControl {
            exchange: Exchange::new([Reply::Ready, Reply::Installed(expected)]),
            fail_second,
            flushes: 0,
        };
        let result = primary.ship(&mut offered, &mut channel);

        // Independent wire header followed by every captured image byte. Only
        // the upload flush can fail; header flush and every write succeed.
        let mut sent = b"WDBREPL\x01\x01".to_vec();
        sent.extend_from_slice(&STREAM);
        sent.extend_from_slice(&[1; 16]);
        sent.extend_from_slice(&descriptor.revision.to_le_bytes());
        sent.extend_from_slice(&descriptor.length.to_le_bytes());
        sent.extend_from_slice(&descriptor.digest);
        assert_eq!(sent.len(), 89);
        sent.extend_from_slice(&bytes);
        assert_eq!(channel.exchange.sent, sent);
        assert_eq!(channel.flushes, 2);
        if fail_second {
            assert_eq!(result, Err(Error::Io));
            assert_eq!(channel.exchange.replies.position(), 1);
            assert_eq!(channel.exchange.replies.get_ref().len(), 58);
            assert_eq!(primary.progress, Some(previous));
            assert_eq!(
                poll(&mut primary, Reply::Installed(previous)),
                Ok(Some(previous))
            );
        } else {
            assert_eq!(result, Ok(expected));
            assert_eq!(channel.exchange.replies.position(), 58);
            assert_eq!(primary.progress, Some(expected));
            assert_eq!(
                poll(&mut primary, Reply::Installed(previous)),
                Err(Error::Conflict)
            );
        }
        assert_eq!(source.snapshot().revision(), source_revision);
        assert!(!source.requires_recovery());
        assert_eq!(disk.durable_image().unwrap(), source_bytes);
        assert_eq!(offered.descriptor(), descriptor);
        assert_eq!(image(&mut offered), bytes);

        // A queued receipt may represent an already durable follower despite
        // the local flush error. Reconcile over a fresh healthy channel rather
        // than inferring that the remote did not commit.
        let mut retry = Exchange::new([Reply::Installed(expected)]);
        assert_eq!(primary.ship(&mut offered, &mut retry), Ok(expected));
        assert_eq!(retry.replies.position(), 57);
        assert_eq!(primary.progress, Some(expected));
        assert_eq!(
            poll(&mut primary, Reply::Installed(previous)),
            Err(Error::Conflict)
        );
        assert_eq!(source.snapshot().revision(), source_revision);
        assert_eq!(disk.durable_image().unwrap(), source_bytes);
        assert_eq!(offered.descriptor(), descriptor);
        assert_eq!(image(&mut offered), bytes);
    }
}

#[derive(Clone, Copy)]
enum TransportFault {
    HeaderFlush,
    PartialUploadWrite,
}

fn assert_transport_failure_stops_before_receipt(fault: TransportFault) {
    struct Channel {
        exchange: Exchange,
        fault: TransportFault,
        fail: bool,
        header_failed: bool,
        write_failed: bool,
        writes: usize,
        reads: usize,
        flushes: usize,
    }
    impl Read for Channel {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            assert!(!self.header_failed, "reply read after failed header flush");
            assert!(!self.write_failed, "reply read after failed upload write");
            self.reads += 1;
            self.exchange.read(bytes)
        }
    }
    impl Write for Channel {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            assert!(!self.header_failed, "write after failed header flush");
            assert!(!self.write_failed, "write after failed upload write");
            self.writes += 1;
            if self.fail && matches!(self.fault, TransportFault::PartialUploadWrite) {
                if self.exchange.sent.len() == 89 {
                    return self.exchange.write(&bytes[..1]);
                }
                if self.exchange.sent.len() == 90 {
                    self.write_failed = true;
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
            }
            self.exchange.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            assert!(!self.header_failed, "flush after failed header flush");
            assert!(!self.write_failed, "flush after failed upload write");
            self.flushes += 1;
            if self.fail
                && matches!(self.fault, TransportFault::HeaderFlush)
                && self.exchange.sent.len() == 89
            {
                self.header_failed = true;
                Err(io::ErrorKind::BrokenPipe.into())
            } else {
                self.exchange.flush()
            }
        }
    }
    fn image(export: &mut Export) -> Vec<u8> {
        let store = export.storage.image_bytes();
        let mut bytes = vec![0; store.length().unwrap() as usize];
        read_all(store, 0, &mut bytes).unwrap();
        bytes
    }

    for fail in [true, false] {
        let disk = crate::storage::SimulatedDisk::new().unwrap();
        let (source, [old, mut offered, _]) = exports_on(disk.clone());
        let previous = ack(&old);
        let expected = ack(&offered);
        let descriptor = offered.descriptor();
        let bytes = image(&mut offered);
        // The inner-callback omission must reach a later image chunk rather
        // than relying on an eventual terminal EOF to reveal continuation.
        assert!(bytes.len() > REGION);
        assert_eq!(bytes.len() as u64, descriptor.length);
        let source_revision = source.snapshot().revision();
        let source_bytes = disk.durable_image().unwrap();
        let mut primary = Primary::new(&source, STREAM).unwrap();
        assert_eq!(
            poll(&mut primary, Reply::Installed(previous)),
            Ok(Some(previous))
        );
        let mut channel = Channel {
            exchange: Exchange::new([Reply::Ready, Reply::Installed(expected)]),
            fault,
            fail,
            header_failed: false,
            write_failed: false,
            writes: 0,
            reads: 0,
            flushes: 0,
        };
        let result = primary.ship(&mut offered, &mut channel);
        let mut sent = b"WDBREPL\x01\x01".to_vec();
        sent.extend_from_slice(&STREAM);
        sent.extend_from_slice(&[1; 16]);
        sent.extend_from_slice(&descriptor.revision.to_le_bytes());
        sent.extend_from_slice(&descriptor.length.to_le_bytes());
        sent.extend_from_slice(&descriptor.digest);
        assert_eq!(sent.len(), 89);
        sent.extend_from_slice(&bytes);
        if fail {
            assert_eq!(result, Err(Error::Io));
            let (sent_bytes, writes, reads, consumed) = match fault {
                TransportFault::HeaderFlush => (89, 1, 0, 0),
                TransportFault::PartialUploadWrite => (90, 3, 1, 1),
            };
            assert_eq!(channel.exchange.sent, sent[..sent_bytes]);
            assert_eq!(channel.writes, writes);
            assert_eq!(channel.reads, reads);
            assert_eq!(channel.flushes, 1);
            assert_eq!(channel.exchange.replies.position(), consumed);
            assert_eq!(channel.exchange.replies.get_ref().len(), 58);
            assert_eq!(primary.progress, Some(previous));
            assert_eq!(
                poll(&mut primary, Reply::Installed(previous)),
                Ok(Some(previous))
            );
        } else {
            assert_eq!(result, Ok(expected));
            assert_eq!(channel.exchange.sent, sent);
            assert_eq!(channel.writes, 1 + bytes.len().div_ceil(REGION));
            assert_eq!(channel.reads, 3);
            assert_eq!(channel.flushes, 2);
            assert_eq!(channel.exchange.replies.position(), 58);
            assert_eq!(primary.progress, Some(expected));
            assert_eq!(
                poll(&mut primary, Reply::Installed(previous)),
                Err(Error::Conflict)
            );
        }
        assert!(!source.requires_recovery());
        assert_eq!(source.snapshot().revision(), source_revision);
        assert_eq!(disk.durable_image().unwrap(), source_bytes);
        assert_eq!(offered.descriptor(), descriptor);
        assert_eq!(image(&mut offered), bytes);

        // Scripted receipt bytes detect forbidden continuation, not a real
        // follower's state. Reconciliation uses a fresh healthy channel; a
        // local transport error makes no remote-nonpublication promise.
        let mut retry = Exchange::new([Reply::Installed(expected)]);
        assert_eq!(primary.ship(&mut offered, &mut retry), Ok(expected));
        assert_eq!(retry.replies.position(), 57);
        assert_eq!(primary.progress, Some(expected));
        assert_eq!(
            poll(&mut primary, Reply::Installed(previous)),
            Err(Error::Conflict)
        );
        assert_eq!(source.snapshot().revision(), source_revision);
        assert_eq!(disk.durable_image().unwrap(), source_bytes);
        assert_eq!(offered.descriptor(), descriptor);
        assert_eq!(image(&mut offered), bytes);
    }
}

#[test]
fn transport_header_flush_error_stops_before_receipt_read() {
    assert_transport_failure_stops_before_receipt(TransportFault::HeaderFlush);
}

#[test]
fn transport_partial_upload_write_error_stops_before_flushing() {
    assert_transport_failure_stops_before_receipt(TransportFault::PartialUploadWrite);
}

#[test]
fn poll_header_flush_error_stops_before_receipt_read() {
    struct Channel {
        exchange: Exchange,
        fail: bool,
        failed: bool,
        reads: usize,
        writes: usize,
        flushes: usize,
    }
    impl Read for Channel {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            assert!(!self.failed, "poll reply read after failed request flush");
            self.reads += 1;
            self.exchange.read(bytes)
        }
    }
    impl Write for Channel {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            assert!(!self.failed, "poll write after failed request flush");
            self.writes += 1;
            self.exchange.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            assert!(!self.failed, "poll flush after failed request flush");
            self.flushes += 1;
            assert_eq!(self.exchange.sent.len(), 41);
            if self.fail {
                self.failed = true;
                Err(io::ErrorKind::BrokenPipe.into())
            } else {
                self.exchange.flush()
            }
        }
    }
    fn image(export: &mut Export) -> Vec<u8> {
        let store = export.storage.image_bytes();
        let mut bytes = vec![0; store.length().unwrap() as usize];
        read_all(store, 0, &mut bytes).unwrap();
        bytes
    }

    for fail in [true, false] {
        let disk = crate::storage::SimulatedDisk::new().unwrap();
        let (source, [old, mut captured, _]) = exports_on(disk.clone());
        let previous = ack(&old);
        let expected = ack(&captured);
        let descriptor = captured.descriptor();
        let bytes = image(&mut captured);
        let source_revision = source.snapshot().revision();
        let source_bytes = disk.durable_image().unwrap();
        let assert_stable = |export: &mut Export| {
            assert_eq!(source.snapshot().revision(), source_revision);
            assert!(!source.requires_recovery());
            assert_eq!(disk.durable_image().unwrap(), source_bytes);
            assert_eq!(export.descriptor(), descriptor);
            assert_eq!(image(export), bytes);
        };
        let mut primary = Primary::new(&source, STREAM).unwrap();
        assert_eq!(
            poll(&mut primary, Reply::Installed(previous)),
            Ok(Some(previous))
        );
        let mut channel = Channel {
            exchange: Exchange::new([Reply::Installed(expected)]),
            fail,
            failed: false,
            reads: 0,
            writes: 0,
            flushes: 0,
        };
        let result = primary.poll(&mut channel);
        let mut query = b"WDBREPL\x01\x00".to_vec();
        query.extend_from_slice(&STREAM);
        query.extend_from_slice(&[1; 16]);
        assert_eq!(query.len(), 41);
        assert_eq!(channel.exchange.sent, query);
        assert_eq!(channel.writes, 1);
        assert_eq!(channel.flushes, 1);
        if fail {
            assert_eq!(result, Err(Error::Io));
            assert_eq!(channel.reads, 0);
            assert_eq!(channel.exchange.replies.position(), 0);
            assert_eq!(channel.exchange.replies.get_ref().len(), 57);
            assert_eq!(primary.progress, Some(previous));
            assert_eq!(
                poll(&mut primary, Reply::Installed(previous)),
                Ok(Some(previous))
            );
        } else {
            assert_eq!(result, Ok(Some(expected)));
            assert_eq!(channel.reads, 2);
            assert_eq!(channel.exchange.replies.position(), 57);
            assert_eq!(primary.progress, Some(expected));
            assert_eq!(
                poll(&mut primary, Reply::Installed(previous)),
                Err(Error::Conflict)
            );
        }
        assert_stable(&mut captured);

        // Reconcile a potentially already durable follower receipt over a
        // fresh healthy channel; a failed local query proves no remote absence.
        assert_eq!(
            poll(&mut primary, Reply::Installed(expected)),
            Ok(Some(expected))
        );
        assert_eq!(primary.progress, Some(expected));
        assert_eq!(
            poll(&mut primary, Reply::Installed(previous)),
            Err(Error::Conflict)
        );
        assert_stable(&mut captured);
    }
}

#[test]
fn primary_ship_accepts_newer_progress_only_before_uploading() {
    let (db, [old, mut offered, newer]) = exports();
    let (old, current, newer) = (ack(&old), ack(&offered), ack(&newer));
    let length = offered.offer.length as usize;

    for (case, reply, immediate, after_upload) in [
        ("exact", current, Ok(current), Ok(current)),
        ("newer", newer, Ok(newer), Err(Error::Conflict)),
        ("older", old, Err(Error::Conflict), Err(Error::Conflict)),
        (
            "wrong digest",
            wrong_digest(current),
            Err(Error::Conflict),
            Err(Error::Conflict),
        ),
        (
            "wrong stream",
            DurableAck {
                stream_id: [3; 16],
                ..current
            },
            Err(Error::Conflict),
            Err(Error::Conflict),
        ),
    ] {
        for (ready, expected) in [(false, immediate), (true, after_upload)] {
            let mut primary = Primary::new(&db, STREAM).unwrap();
            assert_eq!(poll(&mut primary, Reply::Installed(old)), Ok(Some(old)));
            let mut exchange = Exchange::new(
                ready
                    .then_some(Reply::Ready)
                    .into_iter()
                    .chain([Reply::Installed(reply)]),
            );
            assert_eq!(
                primary.ship(&mut offered, &mut exchange),
                expected,
                "{case}, ready={ready}"
            );
            let mut sent = exchange.sent.as_slice();
            let offer = wire::read_request(&mut sent, primary.enrollment())
                .unwrap()
                .unwrap();
            assert_eq!(offer.revision, current.revision);
            assert_eq!(sent.len(), if ready { length } else { 0 }, "{case}");
            let old_reply = if expected.is_ok() {
                Err(Error::Conflict)
            } else {
                Ok(Some(old))
            };
            assert_eq!(
                poll(&mut primary, Reply::Installed(old)),
                old_reply,
                "{case}, ready={ready}"
            );
            if let Ok(accepted) = expected {
                assert_eq!(
                    poll(&mut primary, Reply::Installed(accepted)),
                    Ok(Some(accepted))
                );
            }
        }
    }
}

#[test]
fn enrollment_offer_and_wire_dispatch_boundaries() {
    use std::io::Cursor;
    let enrollment = Enrollment {
        stream_id: [1; 16],
        file_identity: [2; 16],
    };
    for (s, f, ok) in [(1, 2, true), (0, 2, false), (1, 0, false)] {
        assert_eq!(
            Enrollment {
                stream_id: [s; 16],
                file_identity: [f; 16]
            }
            .validate()
            .is_ok(),
            ok
        );
    }
    for (length, ok) in [
        (APPEND_START as u64 - 1, false),
        (APPEND_START as u64, true),
        (MAX_IMAGE_BYTES as u64, true),
        (MAX_IMAGE_BYTES as u64 + 1, false),
    ] {
        assert_eq!(
            Offer {
                revision: 1,
                length,
                digest: [0; 32]
            }
            .validate()
            .is_ok(),
            ok
        );
    }
    let mut request = Vec::new();
    wire::request(&mut request, enrollment, None).unwrap();
    assert!(
        wire::read_request(&mut request.as_slice(), enrollment)
            .unwrap()
            .is_none()
    );
    for (at, error) in [
        (0, Error::UnsupportedFormat),
        (9, Error::ForeignStore),
        (25, Error::ForeignStore),
        (8, Error::Invalid),
    ] {
        let mut bad = request.clone();
        bad[at] = 255;
        assert!(matches!(wire::read_request(&mut bad.as_slice(),enrollment),Err(e) if e==error));
    }
    for end in [0, 40] {
        assert!(matches!(
            wire::read_request(&mut &request[..end], enrollment),
            Err(Error::Io)
        ));
    }
    for (error, expected) in [
        (Error::UnsupportedFormat, Error::UnsupportedFormat),
        (Error::ForeignStore, Error::ForeignStore),
        (Error::Conflict, Error::Conflict),
        (Error::Limit, Error::Limit),
        (Error::Exhausted, Error::Limit),
        (Error::Corrupt, Error::Corrupt),
        (Error::OutcomeUnknown, Error::RecoveryRequired),
        (Error::RecoveryRequired, Error::RecoveryRequired),
        (Error::Invalid, Error::Invalid),
        (Error::Io, Error::Io),
    ] {
        let mut bytes = Vec::new();
        wire::reply(&mut bytes, Err(error)).unwrap();
        assert!(matches!(wire::read_reply(&mut bytes.as_slice()),Err(e) if e==expected));
    }
    assert!(matches!(
        wire::read_reply(&mut &[255][..]),
        Err(Error::UnsupportedFormat)
    ));
    assert!(matches!(wire::read_reply(&mut &[2][..]), Err(Error::Io)));
    assert!(wire::request(&mut Cursor::new(&mut [0; 0][..]), enrollment, None).is_err());
}

#[test]
fn source_and_export_identity_revision_guards_are_independent() {
    let (db, [_, mut export, _]) = exports();
    let mut primary = Primary::new(&db, STREAM).unwrap();
    assert!(primary.check_source(&db).is_ok());
    primary.enrollment.file_identity[0] ^= 1;
    assert_eq!(primary.check_source(&db), Err(Error::ForeignStore));
    primary.enrollment.file_identity[0] ^= 1;
    primary.verified_revision += 1;
    assert_eq!(primary.check_source(&db), Err(Error::ForeignStore));
    primary = Primary::new(&db, STREAM).unwrap();
    let mut channel = Exchange::new([Reply::Installed(ack(&export))]);
    export.enrollment.stream_id[0] ^= 1;
    assert_eq!(
        primary.ship(&mut export, &mut channel),
        Err(Error::ForeignStore)
    );
    assert!(channel.sent.is_empty());
    export.enrollment.stream_id[0] ^= 1;
    primary.verified_revision = export.offer.revision - 1;
    assert_eq!(
        primary.ship(&mut export, &mut channel),
        Err(Error::ForeignStore)
    );
    assert!(channel.sent.is_empty());
    primary.verified_revision = export.offer.revision;
    let expected = ack(&export);
    assert_eq!(primary.ship(&mut export, &mut channel), Ok(expected));
}

#[test]
fn visit_image_length_and_callback_errors_stop_before_more_chunks() {
    let mut store = crate::storage::MemoryStore::new().unwrap();
    store.lock().unwrap();
    assert_eq!(
        visit_image(&mut store, 1, |_| panic!("invalid length visited")),
        Err(Error::Corrupt)
    );
    assert_eq!(
        visit_image(&mut store, 0, |_| panic!("empty image visited")),
        Ok(())
    );
    store.write(0, &vec![0; REGION * 2]).unwrap();
    let mut calls = 0;
    assert_eq!(
        visit_image(&mut store, (REGION * 2) as u64, |_| {
            calls += 1;
            Err(Error::Invalid)
        }),
        Err(Error::Invalid)
    );
    assert_eq!(calls, 1);
}
