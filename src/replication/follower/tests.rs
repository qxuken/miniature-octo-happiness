use super::*;
use crate::{
    replication::{Export, Primary, visit_image},
    storage::{Crash, Fault, IoKind, MemoryStore, SimulatedDisk},
};
use std::io::Cursor;

#[derive(Clone, Default)]
struct Slots([Option<SimulatedDisk>; 2]);
impl Images for Slots {
    type Store = SimulatedDisk;
    fn create(&mut self, slot: u8) -> Result<Self::Store> {
        // Tests supply fresh media for this installation. This gives them
        // handles for fault injection and simulated power loss.
        self.open(slot)
    }
    fn open(&mut self, slot: u8) -> Result<Self::Store> {
        self.0[slot as usize].clone().ok_or(Error::NotFound)
    }
}
struct Exchange {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
    lose_ack: bool,
}
impl Exchange {
    fn request(enrollment: Enrollment, offer: Option<Offer>) -> Self {
        let mut bytes = Vec::new();
        wire::request(&mut bytes, enrollment, offer).unwrap();
        Self {
            input: Cursor::new(bytes),
            output: Vec::new(),
            lose_ack: false,
        }
    }
    fn offer(export: &mut Export) -> Self {
        let mut exchange = Self::request(export.enrollment, Some(export.offer));
        visit_image(export.storage.image_bytes(), export.offer.length, |chunk| {
            exchange.input.get_mut().extend_from_slice(chunk);
            Ok(())
        })
        .unwrap();
        exchange
    }
    fn serve(&mut self, follower: &mut Receiver<Slots>) -> Result<Option<DurableAck>> {
        follower.serve(&mut *self)?;
        self.terminal_reply()
    }
    fn terminal_reply(&self) -> Result<Option<DurableAck>> {
        let mut replies = self.output.as_slice();
        let mut reply = wire::read_reply(&mut replies);
        if matches!(reply, Ok(wire::Reply::Ready)) {
            reply = wire::read_reply(&mut replies);
        }
        assert!(replies.is_empty(), "extra bytes after terminal reply");
        match reply? {
            wire::Reply::Installed(ack) => Ok(Some(ack)),
            wire::Reply::Empty => Ok(None),
            wire::Reply::Ready => panic!("missing terminal reply"),
        }
    }
}
impl Read for Exchange {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.input.read(bytes)
    }
}
impl Write for Exchange {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.lose_ack && !self.output.is_empty() {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        self.output.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Fixture {
    enrollment: Enrollment,
    manifest: Vec<u8>,
    image: Vec<u8>,
    first: Export,
    next: Export,
    first_ack: DurableAck,
}
impl Fixture {
    fn exports(identity: [u8; 16]) -> (Export, Export) {
        let limits = Limits::default();
        let mut db = Database::create_on(MemoryStore::new().unwrap(), identity, limits).unwrap();
        let mut primary = Primary::new(&db, [2; 16]).unwrap();
        let id = db.create_collection().unwrap();
        let mut tx = db.begin();
        tx.put(id, b"key", &db.unsigned(1).unwrap()).unwrap();
        tx.commit().unwrap();
        drop(tx);
        let first = primary
            .export_to(&mut db, MemoryStore::new().unwrap())
            .unwrap();
        let mut tx = db.begin();
        tx.put(id, b"key", &db.unsigned(2).unwrap()).unwrap();
        tx.commit().unwrap();
        drop(tx);
        let next = primary
            .export_to(&mut db, MemoryStore::new().unwrap())
            .unwrap();
        (first, next)
    }
    fn new() -> Self {
        let (mut first, next) = Self::exports([1; 16]);
        let enrollment = first.enrollment;
        let limits = Limits::default();
        let manifest = SimulatedDisk::new().unwrap();
        let image = SimulatedDisk::new().unwrap();
        let mut receiver = Receiver::create(
            Database::create_on(manifest.clone(), enrollment.stream_id, limits).unwrap(),
            Slots([Some(image.clone()), None]),
            enrollment,
            limits,
        )
        .unwrap();
        let first_ack = Exchange::offer(&mut first)
            .serve(&mut receiver)
            .unwrap()
            .unwrap();
        Self {
            enrollment,
            manifest: manifest.durable_image().unwrap(),
            image: image.durable_image().unwrap(),
            first,
            next,
            first_ack,
        }
    }
    fn reopen(&self, manifest: &SimulatedDisk, slots: &Slots) -> Receiver<Slots> {
        Receiver::open(
            Database::open_on(manifest.clone(), Limits::default()).unwrap(),
            slots.clone(),
            self.enrollment,
            Limits::default(),
        )
        .unwrap()
    }
    fn start(&self) -> (Receiver<Slots>, SimulatedDisk, Slots, SimulatedDisk) {
        let manifest = SimulatedDisk::from_image(&self.manifest).unwrap();
        let candidate = SimulatedDisk::new().unwrap();
        let slots = Slots([
            Some(SimulatedDisk::from_image(&self.image).unwrap()),
            Some(candidate.clone()),
        ]);
        let receiver = self.reopen(&manifest, &slots);
        manifest.arm(usize::MAX, Fault::Error);
        (receiver, manifest, slots, candidate)
    }
}

fn check(receiver: &Receiver<Slots>, ack: DurableAck, value: u64) {
    assert_eq!(receiver.progress(), Ok(Some(ack)));
    let view = &receiver.installed.as_ref().unwrap().1;
    let snapshot = view.snapshot();
    assert_eq!(snapshot.revision(), ack.revision);
    assert_eq!(
        snapshot
            .get(view.collection_id(1), b"key")
            .unwrap()
            .unwrap()
            .as_u64(),
        Ok(value)
    );
}

#[test]
fn interrupted_image_and_manifest_barriers_recover_exactly_one_image() {
    let mut fixture = Fixture::new();
    let (mut receiver, manifest, slots, candidate) = fixture.start();
    let ack = Exchange::offer(&mut fixture.next)
        .serve(&mut receiver)
        .unwrap()
        .unwrap();
    assert_eq!(ack, fixture.next.offer.ack(fixture.enrollment));
    check(&receiver, ack, 2);
    let sync_steps = |disk: &SimulatedDisk| {
        disk.trace()
            .unwrap()
            .iter()
            .enumerate()
            .filter_map(|(step, event)| (event.kind == IoKind::Sync).then_some(step))
            .collect::<Vec<_>>()
    };
    let stage_sync = sync_steps(&candidate);
    let manifest_sync = sync_steps(&manifest);
    assert_eq!(stage_sync.len(), 1);
    assert_eq!(manifest_sync.len(), 3);
    drop(receiver);
    manifest.crash(Crash::DropAll).unwrap();
    candidate.crash(Crash::DropAll).unwrap();
    let mut recovered = fixture.reopen(&manifest, &slots);
    assert_eq!(
        Exchange::request(fixture.enrollment, None).serve(&mut recovered),
        Ok(Some(ack))
    );

    for (step, stage, phase) in stage_sync.into_iter().map(|step| (step, true, 0)).chain(
        manifest_sync
            .into_iter()
            .enumerate()
            .map(|(phase, step)| (step, false, phase)),
    ) {
        for survive in [Crash::DropAll, Crash::PersistAll] {
            let (mut receiver, manifest, slots, candidate) = fixture.start();
            let disk = if stage { &candidate } else { &manifest };
            disk.arm(step, Fault::SyncError(survive));
            let mut exchange = Exchange::offer(&mut fixture.next);
            let (error, wire_error) = match (stage, phase) {
                (true, _) | (false, 0) => (Error::Io, Error::Io),
                _ => (Error::OutcomeUnknown, Error::RecoveryRequired),
            };
            assert_eq!(exchange.serve(&mut receiver), Err(error), "step={step}");
            assert_eq!(exchange.output.first(), Some(&1));
            assert_eq!(exchange.terminal_reply(), Err(wire_error));
            let mut query = Exchange::request(fixture.enrollment, None);
            if stage {
                assert!(manifest.trace().unwrap().is_empty());
                check(&receiver, fixture.first_ack, 1);
                assert_eq!(query.serve(&mut receiver), Ok(Some(fixture.first_ack)));
            } else {
                assert_eq!(receiver.progress(), Err(Error::RecoveryRequired));
                assert_eq!(query.serve(&mut receiver), Err(Error::RecoveryRequired));
                assert_eq!(query.terminal_reply(), Err(Error::RecoveryRequired));
            }

            drop(receiver);
            manifest.crash(Crash::DropAll).unwrap();
            candidate.crash(Crash::DropAll).unwrap();
            let mut recovered = fixture.reopen(&manifest, &slots);
            // Only a surviving publication root can select the new image.
            let published =
                !stage && (phase == 2 || (phase == 1 && matches!(survive, Crash::PersistAll)));
            let (expected, value) = if published {
                (ack, 2)
            } else {
                (fixture.first_ack, 1)
            };
            check(&recovered, expected, value);
            assert_eq!(
                Exchange::request(fixture.enrollment, None).serve(&mut recovered),
                Ok(Some(expected))
            );
            if !published {
                recovered.images.0[1] = Some(SimulatedDisk::new().unwrap());
            }
            let mut retry = Exchange::offer(&mut fixture.next);
            assert_eq!(retry.serve(&mut recovered), Ok(Some(ack)));
            assert_eq!(retry.output.first(), Some(&(if published { 2 } else { 1 })));
            check(&recovered, ack, 2);
        }
    }
}

#[test]
fn transport_rejects_bad_images_and_envelopes_without_advancing() {
    let mut fixture = Fixture::new();
    let (mut receiver, manifest, _, _) = fixture.start();
    let manifest_before = manifest.durable_image().unwrap();
    // The scripted transport splits input and output, so malformed bodies can
    // be tested without a timing dependency on the sender.
    for (change, expected) in [
        (0, Error::UnsupportedFormat),
        (9, Error::ForeignStore),
        (25, Error::ForeignStore),
        (55, Error::Limit),
        (41, Error::Corrupt),
        (89, Error::Corrupt),
        (usize::MAX, Error::Io),
    ] {
        let mut transfer = Exchange::offer(&mut fixture.next);
        let bytes = transfer.input.get_mut();
        if change == usize::MAX {
            bytes.pop();
        } else {
            bytes[change] ^= 128;
        }
        receiver.images.0[1] = Some(SimulatedDisk::new().unwrap());
        assert_eq!(
            transfer.serve(&mut receiver),
            Err(expected),
            "offset {change}"
        );
        check(&receiver, fixture.first_ack, 1);
        assert_eq!(manifest.durable_image().unwrap(), manifest_before);
    }
}

#[test]
fn image_verification_failures_preserve_the_reported_revision() {
    use sha2::{Digest, Sha256};

    let mut fixture = Fixture::new();
    let good = Exchange::offer(&mut fixture.next);
    let image = &good.input.get_ref()[89..];
    let foreign = Exchange::offer(&mut Fixture::exports([7; 16]).1);
    let foreign = &foreign.input.get_ref()[89..];
    assert_eq!(image.len(), foreign.len());
    let mut trailing = image.to_vec();
    trailing.resize(image.len() + REGION, 0);
    let mut unwitnessed = image.to_vec();
    let witness = (3 + (fixture.next.revision() % 2) as usize) * REGION;
    unwitnessed[witness] ^= 1;
    let next = fixture.next.offer;
    let next_ack = next.ack(fixture.enrollment);

    for (case, body, offer, expected) in [
        ("verified", image, next, Ok(Some(next_ack))),
        (
            "digest",
            image,
            Offer {
                digest: fixture.first.offer.digest,
                ..next
            },
            Err(Error::Corrupt),
        ),
        (
            "identity",
            foreign,
            Offer {
                digest: Sha256::digest(foreign).into(),
                ..next
            },
            Err(Error::Corrupt),
        ),
        (
            "revision",
            image,
            Offer {
                revision: next.revision + 1,
                ..next
            },
            Err(Error::Corrupt),
        ),
        (
            "committed end",
            trailing.as_slice(),
            Offer {
                length: trailing.len() as u64,
                digest: Sha256::digest(&trailing).into(),
                ..next
            },
            Err(Error::Corrupt),
        ),
        (
            "witness",
            unwitnessed.as_slice(),
            Offer {
                digest: Sha256::digest(&unwitnessed).into(),
                ..next
            },
            Err(Error::Corrupt),
        ),
    ] {
        let (mut receiver, manifest, slots, candidate) = fixture.start();
        let mut transfer = Exchange::request(fixture.enrollment, Some(offer));
        transfer.input.get_mut().extend_from_slice(body);
        assert_eq!(transfer.serve(&mut receiver), expected, "{case}");
        assert_eq!(transfer.output.first(), Some(&1), "{case}");
        assert_eq!(transfer.terminal_reply(), expected, "{case}");
        let (ack, value) = if expected.is_ok() {
            (next_ack, 2)
        } else {
            // The candidate was fully persisted, but verification stopped the
            // installation before the first manifest operation.
            assert_eq!(candidate.durable_image().unwrap(), body, "{case}");
            assert!(manifest.trace().unwrap().is_empty(), "{case}");
            (fixture.first_ack, 1)
        };
        drop(receiver);
        manifest.crash(Crash::DropAll).unwrap();
        candidate.crash(Crash::DropAll).unwrap();
        let mut reopened = fixture.reopen(&manifest, &slots);
        check(&reopened, ack, value);
        assert_eq!(
            Exchange::request(fixture.enrollment, None).serve(&mut reopened),
            Ok(Some(ack)),
            "{case}",
        );
    }
}

#[test]
fn lost_ack_is_recovered_and_a_delayed_offer_never_rolls_back_progress() {
    let mut fixture = Fixture::new();
    let (mut receiver, manifest, slots, candidate) = fixture.start();
    let mut lost = Exchange::offer(&mut fixture.next);
    lost.lose_ack = true;
    assert_eq!(lost.serve(&mut receiver), Err(Error::Io));
    assert_eq!(lost.output, [1]);
    let ack = receiver.progress().unwrap().unwrap();
    drop(receiver);
    manifest.crash(Crash::DropAll).unwrap();
    candidate.crash(Crash::DropAll).unwrap();
    let mut receiver = fixture.reopen(&manifest, &slots);

    let mut query = Exchange::request(fixture.enrollment, None);
    assert_eq!(query.serve(&mut receiver), Ok(Some(ack)));
    assert_eq!(ack.revision, fixture.next.revision());
    let delayed = Exchange::offer(&mut fixture.first);
    let duplicate = Exchange::offer(&mut fixture.next);
    let mut conflict = Exchange::offer(&mut fixture.next);
    conflict.input.get_mut()[57] ^= 1;
    let before = manifest.durable_image().unwrap();
    for (case, mut transfer, expected) in [
        ("older offer", delayed, Ok(Some(ack))),
        ("duplicate offer", duplicate, Ok(Some(ack))),
        ("conflicting offer", conflict, Err(Error::Conflict)),
    ] {
        assert_eq!(transfer.serve(&mut receiver), expected, "{case}");
        assert_eq!(transfer.terminal_reply(), expected, "{case}");
        assert_eq!(transfer.input.position(), 89, "{case}");
        check(&receiver, ack, 2);
    }
    assert_eq!(manifest.durable_image().unwrap(), before);
    drop(receiver);
    let mut reopened = fixture.reopen(&manifest, &slots);
    assert_eq!(
        Exchange::request(fixture.enrollment, None).serve(&mut reopened),
        Ok(Some(ack))
    );
}
