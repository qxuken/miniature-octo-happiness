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
