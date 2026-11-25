use super::*;
use crate::{
    replication::{Export, Primary, visit_image},
    storage::{Crash, Fault, IoKind, MemoryStore, SimulatedDisk, inspect_on},
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
    fail_flush: Option<usize>,
    flushes: usize,
}
impl Exchange {
    fn request(enrollment: Enrollment, offer: Option<Offer>) -> Self {
        let mut bytes = Vec::new();
        wire::request(&mut bytes, enrollment, offer).unwrap();
        Self {
            input: Cursor::new(bytes),
            output: Vec::new(),
            lose_ack: false,
            fail_flush: None,
            flushes: 0,
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
        self.flushes += 1;
        if self.fail_flush == Some(self.flushes) {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
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
    fn exports(identity: [u8; 16], retain: bool) -> (Export, Export) {
        let limits = Limits::default();
        let mut db = Database::create_on(MemoryStore::new().unwrap(), identity, limits).unwrap();
        let mut primary = Primary::new(&db, [2; 16]).unwrap();
        let id = db.create_collection().unwrap();
        let mut tx = db.begin();
        tx.put(id, b"key", &db.unsigned(1).unwrap()).unwrap();
        tx.commit().unwrap();
        drop(tx);
        if retain {
            db.retain_snapshot(&db.snapshot()).unwrap();
        }
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
        Self::with_exports(false)
    }
    fn with_exports(retain: bool) -> Self {
        let (mut first, next) = Self::exports([1; 16], retain);
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

fn check_retained(receiver: &Receiver<Slots>, ack: DurableAck, value: u64) {
    check(receiver, ack, value);
    let view = &receiver.installed.as_ref().unwrap().1;
    assert_eq!(view.retained_ids().unwrap(), [1]);
    let retained = view.retained_snapshot(1).unwrap();
    let collection = view.collection_id(1);
    assert_eq!(view.snapshot().count(collection).unwrap(), 1);
    assert_eq!(retained.revision(), 2);
    assert_eq!(retained.count(collection).unwrap(), 1);
    assert_eq!(
        retained.get(collection, b"key").unwrap().unwrap().as_u64(),
        Ok(1)
    );
}

#[test]
fn replica_decode_preserves_the_first_error_and_both_roots() {
    let mut fixture = Fixture::with_exports(true);
    let (mut receiver, manifest, slots, selected) = fixture.start();
    let ack = Exchange::offer(&mut fixture.next)
        .serve(&mut receiver)
        .unwrap()
        .unwrap();
    check_retained(&receiver, ack, 2);
    drop(receiver);
    let before = selected.durable_image().unwrap();
    let manifest_before = manifest.durable_image().unwrap();
    let defaults = Limits::default();

    // An armed first I/O distinguishes failing database construction from
    // failing recovery. The lease is held just as it is on follower reopen.
    for (limits, expected_error, expects_io) in [
        (
            Limits {
                max_depth: 0,
                ..defaults
            },
            Error::Invalid,
            false,
        ),
        (
            Limits {
                max_live_bytes: 0,
                ..defaults
            },
            Error::Limit,
            false,
        ),
        (defaults, Error::Io, true),
    ] {
        let mut image = Lease::new(selected.clone()).unwrap();
        selected.arm(0, Fault::Error);
        let result = Database::decode_replica(
            &mut image.0,
            fixture.enrollment.file_identity,
            ack.revision,
            before.len() as u64,
            limits,
        );
        assert_eq!(result.err(), Some(expected_error));
        assert_eq!(selected.trace().unwrap().len(), usize::from(expects_io));
        assert_eq!(selected.durable_image().unwrap(), before);
    }
    selected.arm(usize::MAX, Fault::Error);
    let reopened = fixture.reopen(&manifest, &slots);
    check_retained(&reopened, ack, 2);
    assert!(
        selected
            .trace()
            .unwrap()
            .iter()
            .all(|event| matches!(event.kind, IoKind::Read | IoKind::Length))
    );
    assert_eq!(selected.durable_image().unwrap(), before);
    assert_eq!(manifest.durable_image().unwrap(), manifest_before);
}

#[test]
fn torn_staging_sync_keeps_the_exact_old_receipt() {
    let mut fixture = Fixture::with_exports(true);
    let (mut receiver, _, _, candidate) = fixture.start();
    let ack = Exchange::offer(&mut fixture.next)
        .serve(&mut receiver)
        .unwrap()
        .unwrap();
    let complete = candidate.durable_image().unwrap();
    let trace = candidate.trace().unwrap();
    let sync = trace
        .iter()
        .position(|event| event.kind == IoKind::Sync)
        .unwrap();
    let writes: Vec<_> = trace[..sync]
        .iter()
        .filter(|event| event.kind == IoKind::Write)
        .collect();
    assert!(writes.len() >= 3);
    drop(receiver);

    for index in [0, writes.len() / 2, writes.len() - 1] {
        let event = writes[index];
        let offset = usize::try_from(event.offset).unwrap();
        for prefix in [0, 1, event.len / 2, event.len - 1, event.len] {
            let (mut receiver, manifest, slots, candidate) = fixture.start();
            candidate.arm(
                sync,
                Fault::SyncError(Crash::Torn {
                    write: index,
                    prefix,
                }),
            );
            let mut offer = Exchange::offer(&mut fixture.next);
            assert_eq!(offer.serve(&mut receiver), Err(Error::Io));
            assert_eq!(offer.output.first(), Some(&1));
            assert_eq!(offer.terminal_reply(), Err(Error::Io));
            let mut poll = Exchange::request(fixture.enrollment, None);
            assert_eq!(poll.serve(&mut receiver), Ok(Some(fixture.first_ack)));
            assert!(manifest.trace().unwrap().is_empty());
            drop(receiver);
            manifest.crash(Crash::DropAll).unwrap();
            candidate.crash(Crash::DropAll).unwrap();

            let mut expected = Vec::new();
            if prefix != 0 {
                expected.resize(offset + prefix, 0);
                expected[offset..].copy_from_slice(&complete[offset..offset + prefix]);
            }
            assert_eq!(candidate.durable_image().unwrap(), expected);
            assert_eq!(manifest.durable_image().unwrap(), fixture.manifest);
            assert_eq!(
                slots.0[0].as_ref().unwrap().durable_image().unwrap(),
                fixture.image
            );
            let mut reopened = fixture.reopen(&manifest, &slots);
            check_retained(&reopened, fixture.first_ack, 1);
            assert_eq!(
                Exchange::request(fixture.enrollment, None).serve(&mut reopened),
                Ok(Some(fixture.first_ack))
            );
            let retry_disk = SimulatedDisk::new().unwrap();
            reopened.images.0[1] = Some(retry_disk.clone());
            assert_eq!(
                Exchange::offer(&mut fixture.next).serve(&mut reopened),
                Ok(Some(ack))
            );
            assert_eq!(retry_disk.durable_image().unwrap(), complete);
            check_retained(&reopened, ack, 2);
            let installed = reopened.images.clone();
            drop(reopened);
            manifest.crash(Crash::DropAll).unwrap();
            retry_disk.crash(Crash::DropAll).unwrap();
            check_retained(&fixture.reopen(&manifest, &installed), ack, 2);
        }
    }
}

#[test]
fn every_distinct_torn_manifest_prefix_preserves_the_selected_receipt() {
    let mut cases = 0;
    for shift in [false, true] {
        let mut fixture = Fixture::with_exports(true);
        if shift {
            // Reuse the other root and witness without changing the selected image.
            let (receiver, manifest, _, _) = fixture.start();
            receiver.manifest.begin().commit().unwrap();
            drop(receiver);
            fixture.manifest = manifest.durable_image().unwrap();
        }
        let (mut receiver, manifest, _, candidate) = fixture.start();
        let previous = receiver.manifest.snapshot().revision();
        let ack = Exchange::offer(&mut fixture.next)
            .serve(&mut receiver)
            .unwrap()
            .unwrap();
        let staged = candidate.durable_image().unwrap();
        let complete = manifest.durable_image().unwrap();
        let trace = manifest.trace().unwrap();
        let [_, root_sync, witness_sync]: [usize; 3] = trace
            .iter()
            .enumerate()
            .filter_map(|(step, event)| (event.kind == IoKind::Sync).then_some(step))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        drop(receiver);

        let parity = (previous + 1) as usize % 2;
        let root = (1 + parity) * REGION..(2 + parity) * REGION;
        let witness = (3 + parity) * REGION..(4 + parity) * REGION;
        for (sync, region) in [(root_sync, root.clone()), (witness_sync, witness.clone())] {
            let write = sync - 1;
            let event = &trace[write];
            assert_eq!(
                (event.kind, event.offset, event.len),
                (IoKind::Write, region.start as u64, REGION)
            );
            let old = &fixture.manifest[region.clone()];
            let new = &complete[region.clone()];
            assert!(old.iter().any(|&byte| byte != 0));
            let prefixes = std::iter::once(0)
                .chain(
                    old.iter()
                        .zip(new)
                        .enumerate()
                        .filter_map(|(i, (a, b))| (a != b).then_some(i + 1)),
                )
                .chain(std::iter::once(REGION));
            for prefix in prefixes {
                let tear = |durable| Fault::Tear { prefix, durable };
                let sync_tear = Fault::SyncError(Crash::Torn { write: 0, prefix });
                for (step, fault, crash, surviving) in [
                    (write, tear(true), Crash::DropAll, prefix),
                    (write, tear(false), Crash::PersistAll, prefix),
                    (write, tear(false), Crash::DropAll, 0),
                    (sync, sync_tear, Crash::DropAll, prefix),
                    (sync, sync_tear, Crash::PersistAll, REGION),
                ] {
                    let case = (parity, region.start, prefix, fault, crash);
                    let (mut receiver, manifest, slots, candidate) = fixture.start();
                    manifest.arm(step, fault);
                    let mut offer = Exchange::offer(&mut fixture.next);
                    assert_eq!(
                        offer.serve(&mut receiver),
                        Err(Error::OutcomeUnknown),
                        "{case:?}"
                    );
                    assert_eq!(offer.output.first(), Some(&1));
                    assert_eq!(
                        offer.terminal_reply(),
                        Err(Error::RecoveryRequired),
                        "{case:?}"
                    );
                    assert_eq!(
                        receiver.progress(),
                        Err(Error::RecoveryRequired),
                        "{case:?}"
                    );
                    assert_eq!(manifest.trace().unwrap(), trace[..=step], "{case:?}");
                    drop(receiver);
                    manifest.crash(crash).unwrap();
                    candidate.crash(Crash::DropAll).unwrap();

                    let mut expected = complete.clone();
                    expected[region.start + surviving..region.end]
                        .copy_from_slice(&old[surviving..]);
                    if sync == root_sync {
                        expected[witness.clone()]
                            .copy_from_slice(&fixture.manifest[witness.clone()]);
                    }
                    assert!(manifest.durable_image().unwrap() == expected, "{case:?}");
                    assert_eq!(candidate.durable_image().unwrap(), staged);
                    assert_eq!(
                        slots.0[0].as_ref().unwrap().durable_image().unwrap(),
                        fixture.image
                    );

                    let published = expected[root.clone()] == complete[root.clone()];
                    let witnessed =
                        !published || expected[witness.clone()] == complete[witness.clone()];
                    let info = inspect_on(manifest.clone(), Limits::default()).unwrap();
                    assert_eq!(
                        (info.revision, info.witnessed),
                        (previous + u64::from(published), witnessed),
                        "{case:?}"
                    );
                    assert!(manifest.durable_image().unwrap() == expected, "{case:?}");
                    let mut reopened = fixture.reopen(&manifest, &slots);
                    let (selected, value) = if published {
                        (ack, 2)
                    } else {
                        (fixture.first_ack, 1)
                    };
                    check_retained(&reopened, selected, value);
                    assert_eq!(
                        Exchange::request(fixture.enrollment, None).serve(&mut reopened),
                        Ok(Some(selected))
                    );
                    let settled = manifest.durable_image().unwrap();
                    if !published {
                        reopened.images.0[1] = Some(SimulatedDisk::new().unwrap());
                    }
                    let mut retry = Exchange::offer(&mut fixture.next);
                    assert_eq!(retry.serve(&mut reopened), Ok(Some(ack)));
                    assert_eq!(retry.output.first(), Some(&(if published { 2 } else { 1 })));
                    if published {
                        assert_eq!(manifest.durable_image().unwrap(), settled);
                    }
                    assert_eq!(
                        reopened.images.0[1]
                            .as_ref()
                            .unwrap()
                            .durable_image()
                            .unwrap(),
                        staged
                    );
                    assert_eq!(reopened.manifest.snapshot().revision(), previous + 1);
                    check_retained(&reopened, ack, 2);
                    let installed = reopened.images.clone();
                    drop(reopened);
                    manifest.crash(Crash::DropAll).unwrap();
                    installed.0[1]
                        .as_ref()
                        .unwrap()
                        .crash(Crash::DropAll)
                        .unwrap();
                    check_retained(&fixture.reopen(&manifest, &installed), ack, 2);
                    cases += 1;
                }
            }
        }
    }
    eprintln!("manifest prefix cases across both parities: {cases}");
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
    let foreign = Exchange::offer(&mut Fixture::exports([7; 16], false).1);
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
    let mut fixture = Fixture::with_exports(true);
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
        check_retained(&receiver, ack, 2);
    }
    assert_eq!(manifest.durable_image().unwrap(), before);
    drop(receiver);
    let mut reopened = fixture.reopen(&manifest, &slots);
    check_retained(&reopened, ack, 2);
    assert_eq!(
        Exchange::request(fixture.enrollment, None).serve(&mut reopened),
        Ok(Some(ack))
    );
}

#[test]
fn ready_and_terminal_flush_failures_reopen_the_exact_selected_image() {
    let mut fixture = Fixture::with_exports(true);
    let next_ack = fixture.next.offer.ack(fixture.enrollment);
    for failed in [None, Some(1), Some(2)] {
        let (mut receiver, manifest, slots, candidate) = fixture.start();
        let mut transfer = Exchange::offer(&mut fixture.next);
        transfer.fail_flush = failed;
        assert_eq!(
            transfer.serve(&mut receiver),
            failed.map_or(Ok(Some(next_ack)), |_| Err(Error::Io)),
            "failed flush {failed:?}"
        );
        assert_eq!(transfer.flushes, 2);
        let (selected, value) = if failed == Some(1) {
            // Ready's write succeeds before its flush fails. Serve can send
            // the terminal I/O error, but must leave the upload unread.
            assert_eq!(transfer.input.position(), 89);
            assert_eq!(transfer.output, [1, 10]);
            assert!(candidate.durable_image().unwrap().is_empty());
            assert!(manifest.trace().unwrap().is_empty());
            (fixture.first_ack, 1)
        } else {
            assert_eq!(
                transfer.input.position(),
                transfer.input.get_ref().len() as u64
            );
            assert_eq!(transfer.terminal_reply(), Ok(Some(next_ack)));
            (next_ack, 2)
        };
        check_retained(&receiver, selected, value);

        drop(receiver);
        manifest.crash(Crash::DropAll).unwrap();
        candidate.crash(Crash::DropAll).unwrap();
        let mut reopened = fixture.reopen(&manifest, &slots);
        check_retained(&reopened, selected, value);
        assert_eq!(
            Exchange::request(fixture.enrollment, None).serve(&mut reopened),
            Ok(Some(selected))
        );
        let mut retry = Exchange::offer(&mut fixture.next);
        assert_eq!(retry.serve(&mut reopened), Ok(Some(next_ack)));
        assert_eq!(retry.output[0], if failed == Some(1) { 1 } else { 2 });
        check_retained(&reopened, next_ack, 2);
    }
}

#[test]
fn installed_record_length_slot_and_offer_guards() {
    let mut bytes = [0; 49];
    bytes[9..17].copy_from_slice(&(crate::storage::APPEND_START as u64).to_le_bytes());
    assert!(Installed::decode(&bytes).is_ok());
    bytes[0] = 1;
    assert!(Installed::decode(&bytes).is_ok());
    bytes[0] = 2;
    assert!(matches!(Installed::decode(&bytes), Err(Error::Corrupt)));
    assert!(matches!(Installed::decode(&[]), Err(Error::Corrupt)));
    bytes[0] = 0;
    bytes[9..17].fill(0);
    assert!(matches!(Installed::decode(&bytes), Err(Error::Corrupt)));
}

#[test]
fn manifest_enrollment_and_missing_selected_image_reject_without_fallback() {
    let enrollment = Enrollment {
        stream_id: [1; 16],
        file_identity: [2; 16],
    };
    let limits = Limits::default();
    let make = || Database::create_on(MemoryStore::new().unwrap(), [1; 16], limits).unwrap();
    assert!(matches!(
        Receiver::open(make(), Slots::default(), enrollment, limits),
        Err(Error::Corrupt)
    ));
    let db = make();
    db.create_collection().unwrap();
    assert!(matches!(
        Receiver::open(db, Slots::default(), enrollment, limits),
        Err(Error::Corrupt)
    ));
    let mut pair = enrollment.stream_id.to_vec();
    pair.extend_from_slice(&enrollment.file_identity);
    for (bytes, error) in [
        (vec![0; 31], Some(Error::Corrupt)),
        (pair.clone(), None),
        (
            {
                let mut b = pair.clone();
                b[0] ^= 1;
                b
            },
            Some(Error::ForeignStore),
        ),
        (
            {
                let mut b = pair.clone();
                b[16] ^= 1;
                b
            },
            Some(Error::ForeignStore),
        ),
        (
            {
                let mut b = pair.clone();
                b.resize(81, 0);
                b[41..49].copy_from_slice(&(crate::storage::APPEND_START as u64).to_le_bytes());
                b
            },
            Some(Error::Corrupt),
        ),
    ] {
        let db = make();
        let c = db.create_collection().unwrap();
        let mut tx = db.begin();
        tx.put(c, MANIFEST_KEY, &db.bytes(&bytes).unwrap()).unwrap();
        tx.commit().unwrap();
        drop(tx);
        let result = Receiver::open(db, Slots::default(), enrollment, limits);
        match error {
            Some(e) => assert!(matches!(result,Err(actual) if actual==e)),
            None => assert_eq!(result.unwrap().progress(), Ok(None)),
        }
    }
    let db = make();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, MANIFEST_KEY, &db.unsigned(0).unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    assert!(matches!(
        Receiver::open(db, Slots::default(), enrollment, limits),
        Err(Error::Corrupt)
    ));
}
#[test]
fn nonempty_candidate_rejects_before_ready_and_preserves_old_revision() {
    let mut fixture = Fixture::new();
    let (mut receiver, manifest, slots, mut candidate) = fixture.start();
    candidate.lock().unwrap();
    candidate.write(0, &[1]).unwrap();
    candidate.sync().unwrap();
    candidate.unlock();
    let mut exchange = Exchange::offer(&mut fixture.next);
    assert_eq!(exchange.serve(&mut receiver), Err(Error::Corrupt));
    assert_eq!(exchange.output.first(), Some(&7));
    assert!(manifest.trace().unwrap().is_empty());
    drop(receiver);
    manifest.crash(Crash::DropAll).unwrap();
    candidate.crash(Crash::DropAll).unwrap();
    check(&fixture.reopen(&manifest, &slots), fixture.first_ack, 1);
}

#[test]
fn manifest_and_image_open_preserve_non_not_found_errors() {
    let enrollment = Enrollment {
        stream_id: [1; 16],
        file_identity: [2; 16],
    };
    let limits = Limits::default();
    let manifest = Database::with_limits(Limits {
        max_bytes: 0,
        ..limits
    })
    .unwrap();
    manifest.create_collection().unwrap();
    assert!(matches!(
        Receiver::open(manifest, Slots::default(), enrollment, limits),
        Err(Error::Limit)
    ));
    struct Denied;
    impl Images for Denied {
        type Store = SimulatedDisk;
        fn create(&mut self, _: u8) -> Result<Self::Store> {
            Err(Error::Busy)
        }
        fn open(&mut self, _: u8) -> Result<Self::Store> {
            Err(Error::Busy)
        }
    }
    let fixture = Fixture::new();
    let manifest = Database::open_on(
        SimulatedDisk::from_image(&fixture.manifest).unwrap(),
        limits,
    )
    .unwrap();
    assert!(matches!(
        Receiver::open(manifest, Denied, fixture.enrollment, limits),
        Err(Error::Busy)
    ));
}
