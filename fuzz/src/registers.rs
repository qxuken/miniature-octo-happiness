//! Causal oracle stores full assignment events, never joins production registers.
use std::{
    collections::BTreeSet,
    io::{self, Cursor, Read, Write},
};
use weekend_db::{
    Database, Error, Limits,
    crdt::{MaxRegister, MinRegister, MultiValueRegister, Policy, Record, Replica, Resolution},
    storage::{Crash, SimulatedDisk},
};
const POLICY: Policy = Policy {
    collection: [71; 16],
    generation: 1,
    actors: [[1; 16], [2; 16]],
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Record, Resolution)]
#[record(
    key = b"causality",
    format = b"FUZREG01",
    fields(manual, high, low),
    resolve = true
)]
struct Registers {
    manual: MultiValueRegister,
    #[resolve]
    high: MaxRegister,
    #[resolve]
    low: MinRegister,
}
#[derive(Clone, Default)]
struct Knowledge([BTreeSet<usize>; 3]);
struct Event {
    field: usize,
    actor: usize,
    sequence: u64,
    value: u64,
    observed: BTreeSet<usize>,
}
impl Knowledge {
    fn visible(&self, field: usize, events: &[Event]) -> Vec<usize> {
        let retired: BTreeSet<_> = self.0[field]
            .iter()
            .flat_map(|id| events[*id].observed.iter().copied())
            .collect();
        let mut live: Vec<_> = self.0[field].difference(&retired).copied().collect();
        live.sort_by_key(|id| events[*id].actor);
        assert!(live.len() <= 2);
        if live.len() == 2 {
            assert_ne!(events[live[0]].actor, events[live[1]].actor);
        }
        live
    }
    fn values(&self, field: usize, events: &[Event]) -> Vec<u64> {
        let mut values = Vec::new();
        for id in self.visible(field, events) {
            let value = events[id].value;
            if !values.contains(&value) {
                values.push(value);
            }
        }
        values
    }
    fn assign(&mut self, field: usize, actor: usize, value: u64, events: &mut Vec<Event>) {
        let sequence = self.0[field]
            .iter()
            .filter(|id| events[**id].actor == actor)
            .map(|id| events[*id].sequence)
            .max()
            .unwrap_or(0)
            + 1;
        assert!(
            !events
                .iter()
                .any(|e| e.field == field && e.actor == actor && e.sequence == sequence)
        );
        let id = events.len();
        events.push(Event {
            field,
            actor,
            sequence,
            value,
            observed: self.0[field].clone(),
        });
        self.0[field].insert(id);
    }
    fn resolve(&mut self, actor: usize, events: &mut Vec<Event>) {
        for field in [1, 2] {
            let values = self.values(field, events);
            if values.len() == 2 {
                let selected = if field == 1 {
                    *values.iter().max().unwrap()
                } else {
                    *values.iter().min().unwrap()
                };
                self.assign(field, actor, selected, events);
            }
        }
    }
    fn union(&mut self, incoming: &Self) {
        for field in 0..3 {
            self.0[field].extend(&incoming.0[field]);
        }
    }
    fn wire(&self, events: &[Event]) -> Vec<u8> {
        // Independently project event history to the documented 17-byte dot
        // slots; this is not a componentwise implementation of register join.
        let mut bytes = Vec::new();
        for field in 0..3 {
            let visible = self.visible(field, events);
            for actor in 0..2 {
                let latest = self.0[field]
                    .iter()
                    .filter(|id| events[**id].actor == actor)
                    .max_by_key(|id| events[**id].sequence)
                    .copied();
                let live = latest.is_some_and(|id| visible.contains(&id));
                bytes.extend_from_slice(&latest.map_or(0, |id| events[id].sequence).to_le_bytes());
                bytes.push(u8::from(live));
                bytes.extend_from_slice(
                    &latest
                        .filter(|_| live)
                        .map_or(0, |id| events[id].value)
                        .to_le_bytes(),
                );
            }
        }
        bytes
    }
}
struct Channel {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
    lost: bool,
}
impl Read for Channel {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.input.read(b)
    }
}
impl Write for Channel {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.lost {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.output.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn replica(db: &Database, actor: usize) -> Replica<'_, Registers> {
    Replica::open(db, db.collection_id(1), POLICY, POLICY.actors[actor]).unwrap()
}
#[derive(Clone)]
struct Offer {
    actor: usize,
    frame: Vec<u8>,
    knowledge: Knowledge,
}
fn capture(db: &Database, actor: usize, knowledge: &Knowledge) -> Offer {
    let before = db.snapshot().revision();
    let mut channel = Channel {
        input: Cursor::new(Vec::new()),
        output: Vec::new(),
        lost: false,
    };
    assert_eq!(
        replica(db, actor)
            .push_authenticated(&mut channel)
            .unwrap_err(),
        Error::Io
    );
    assert_eq!(db.snapshot().revision(), before);
    Offer {
        actor,
        frame: channel.output,
        knowledge: knowledge.clone(),
    }
}
fn verify(db: &Database, actor: usize, knowledge: &Knowledge, events: &[Event], revision: u64) {
    assert_eq!(db.snapshot().revision(), revision);
    let actual = replica(db, actor).read().unwrap();
    let mut bytes = vec![0; Registers::SIZE];
    actual.encode(&mut bytes).unwrap();
    assert_eq!(bytes, knowledge.wire(events));
    assert_eq!(
        actual.manual.values().collect::<Vec<_>>(),
        knowledge.values(0, events)
    );
    assert_eq!(
        actual.high.values().collect::<Vec<_>>(),
        knowledge.values(1, events)
    );
    assert_eq!(
        actual.low.values().collect::<Vec<_>>(),
        knowledge.values(2, events)
    );
}
fn deliver(
    db: &Database,
    offer: &Offer,
    knowledge: &mut Knowledge,
    events: &mut Vec<Event>,
    revision: &mut u64,
    lost: bool,
) {
    let receiver = 1 - offer.actor;
    let before = knowledge.wire(events);
    knowledge.union(&offer.knowledge);
    if knowledge.wire(events) != before {
        knowledge.resolve(receiver, events);
        *revision += 1;
    }
    let mut channel = Channel {
        input: Cursor::new(offer.frame.clone()),
        output: Vec::new(),
        lost,
    };
    assert_eq!(
        replica(db, receiver).serve_authenticated(&mut channel),
        if lost { Err(Error::Io) } else { Ok(()) }
    );
    if lost {
        assert!(channel.output.is_empty());
    } else {
        assert_eq!(channel.output.len(), 65 + Registers::SIZE);
        assert_eq!(&channel.output[..8], Registers::FORMAT);
        assert_eq!(&channel.output[8..24], &POLICY.collection);
        assert_eq!(&channel.output[24..32], &POLICY.generation.to_le_bytes());
        assert_eq!(&channel.output[32..48], &POLICY.actors[0]);
        assert_eq!(&channel.output[48..64], &POLICY.actors[1]);
        assert_eq!(channel.output[64], receiver as u8);
        assert_eq!(&channel.output[65..], knowledge.wire(events));
    }
    verify(db, receiver, knowledge, events, *revision);
}
fn reopen(databases: &mut [Database; 2], disks: &[SimulatedDisk; 2], actor: usize) {
    drop(std::mem::replace(
        &mut databases[actor],
        Database::new().unwrap(),
    ));
    disks[actor].crash(Crash::DropAll).unwrap();
    databases[actor] = Database::open_on(disks[actor].clone(), Limits::default()).unwrap();
}
pub fn register_causality(input: &[u8]) {
    let disks = std::array::from_fn(|_| SimulatedDisk::new().unwrap());
    let mut databases = std::array::from_fn(|actor| {
        let db = Database::create_on(
            disks[actor].clone(),
            [actor as u8 + 1; 16],
            Limits::default(),
        )
        .unwrap();
        let c = db.create_collection().unwrap();
        Replica::<Registers>::enroll(&db, c, POLICY, POLICY.actors[actor]).unwrap();
        db
    });
    let mut models: [Knowledge; 2] = Default::default();
    let mut revisions = [2u64; 2];
    let mut events = Vec::new();
    let mut delayed = Vec::<Offer>::new();
    for op in input.chunks(3).take(32) {
        let actor = usize::from(op[0] & 1);
        let arg = op.get(1).copied().unwrap_or(0);
        let value = u64::from(op.get(2).copied().unwrap_or(0));
        match (op[0] >> 1) % 7 {
            0 | 1 => {
                // assignment or explicit reviewed choice have the same causal effect
                let field = usize::from(arg % 3);
                replica(&databases[actor], actor)
                    .update(|state, id| match field {
                        0 => state.manual.assign(id, value),
                        1 => state.high.assign(id, value),
                        _ => state.low.assign(id, value),
                    })
                    .unwrap();
                models[actor].assign(field, actor, value, &mut events);
                revisions[actor] += 1;
            }
            2 => {
                let offer = capture(&databases[actor], actor, &models[actor]);
                let receiver = 1 - actor;
                deliver(
                    &databases[receiver],
                    &offer,
                    &mut models[receiver],
                    &mut events,
                    &mut revisions[receiver],
                    arg & 1 != 0,
                );
                if arg & 1 != 0 {
                    reopen(&mut databases, &disks, receiver);
                }
            }
            3 => {
                if delayed.len() < 8 {
                    delayed.push(capture(&databases[actor], actor, &models[actor]));
                }
            }
            4 => {
                if !delayed.is_empty() {
                    let offer = &delayed[usize::from(arg) % delayed.len()];
                    let receiver = 1 - offer.actor;
                    deliver(
                        &databases[receiver],
                        offer,
                        &mut models[receiver],
                        &mut events,
                        &mut revisions[receiver],
                        value & 1 != 0,
                    );
                }
            }
            5 => reopen(&mut databases, &disks, actor),
            _ => {
                replica(&databases[actor], actor)
                    .update(|state, id| state.resolve_fields(id))
                    .unwrap();
                models[actor].resolve(actor, &mut events);
                revisions[actor] += 1;
            }
        }
        assert!(events.len() <= 192);
        for actor in 0..2 {
            verify(
                &databases[actor],
                actor,
                &models[actor],
                &events,
                revisions[actor],
            );
        }
    }
    for actor in [0, 1, 0, 1] {
        let offer = capture(&databases[actor], actor, &models[actor]);
        let receiver = 1 - actor;
        deliver(
            &databases[receiver],
            &offer,
            &mut models[receiver],
            &mut events,
            &mut revisions[receiver],
            false,
        );
    }
    assert_eq!(models[0].wire(&events), models[1].wire(&events));
    for field in [1, 2] {
        assert!(models[0].values(field, &events).len() <= 1);
    }
    let stable = revisions;
    for actor in [0, 1, 0, 1] {
        let offer = capture(&databases[actor], actor, &models[actor]);
        let receiver = 1 - actor;
        deliver(
            &databases[receiver],
            &offer,
            &mut models[receiver],
            &mut events,
            &mut revisions[receiver],
            false,
        );
    }
    assert_eq!(revisions, stable);
    for actor in 0..2 {
        reopen(&mut databases, &disks, actor);
        verify(
            &databases[actor],
            actor,
            &models[actor],
            &events,
            revisions[actor],
        );
    }
}
#[test]
fn replay_register_causality() {
    for input in [
        include_bytes!("../seeds/register_causality/independent-policy-commits").as_slice(),
        include_bytes!("../seeds/register_causality/offline-manual-resolutions").as_slice(),
        include_bytes!("../seeds/register_causality/equal-values-distinct-dots").as_slice(),
        include_bytes!("../seeds/register_causality/retired-dot-replay").as_slice(),
        include_bytes!("../seeds/register_causality/policy-lost-reply-reopen").as_slice(),
        include_bytes!("../seeds/register_causality/empty-policy-updates").as_slice(),
    ] {
        register_causality(input);
    }
    for input in [
        vec![],
        vec![
            0, 1, 60, 0, 2, 30, 1, 1, 90, 1, 2, 20, 6, 0, 0, 7, 0, 0, 8, 1, 0, 8, 0, 0, 8, 1, 1,
            10, 0, 0,
        ],
        vec![
            0, 0, 1, 1, 0, 2, 4, 0, 0, 5, 0, 0, 2, 0, 3, 3, 0, 4, 4, 1, 0, 5, 1, 0, 12, 0, 0,
        ],
    ] {
        register_causality(&input);
    }
    for factor in 0..16u8 {
        register_causality(
            &(0..96u8)
                .map(|b| b.wrapping_mul(factor))
                .collect::<Vec<_>>(),
        );
    }
}
