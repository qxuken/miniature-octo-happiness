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
    let mut db =
        Database::create_on(MemoryStore::new().unwrap(), [1; 16], Limits::default()).unwrap();
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

fn wrong_digest(mut ack: DurableAck) -> DurableAck {
    ack.digest[0] ^= 1;
    ack
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
