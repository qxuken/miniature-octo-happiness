use std::io::{self, Cursor, Read, Write};
use weekend_db::{
    Database, Error, Limits,
    crdt::{GrowCounter, GrowSet, Policy, Record, Replica},
    storage::{Crash, SimulatedDisk},
};
const POLICY: Policy = Policy {
    collection: [9; 16],
    generation: 1,
    actors: [[1; 16], [2; 16]],
};
#[derive(Clone, Copy, Default, PartialEq, Eq, Record)]
#[record(key = b"progress", format = b"FUZCRDT1", fields(count, completed))]
struct Progress {
    count: GrowCounter,
    completed: GrowSet,
}
struct Channel {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
    fail_reply: bool,
}
impl Channel {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            input: Cursor::new(bytes),
            output: Vec::new(),
            fail_reply: false,
        }
    }
}
impl Read for Channel {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.input.read(bytes)
    }
}
impl Write for Channel {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.fail_reply {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn offer<P: Record>(replica: &Replica<'_, P>) -> Vec<u8> {
    let mut channel = Channel::new(Vec::new());
    assert!(matches!(
        replica.push_authenticated(&mut channel),
        Err(Error::Io)
    ));
    channel.output
}

pub fn crdt_frames(input: &[u8]) {
    let Some((&mode, rest)) = input.split_first() else {
        return;
    };
    let db = Database::new().unwrap();
    let id = db.create_collection().unwrap();
    let replica = Replica::<GrowCounter>::enroll(&db, id, POLICY, POLICY.actors[0]).unwrap();
    replica
        .update(|count, actor| count.increment(actor, 5))
        .unwrap();
    let before = db.snapshot().revision();
    let mut frame = if mode & 1 == 0 {
        rest[..rest.len().min(256)].to_vec()
    } else {
        let mut bytes = vec![0; 81];
        bytes[..8].copy_from_slice(b"WDBGCTR\x01");
        bytes[8..24].copy_from_slice(&POLICY.collection);
        bytes[24..32].copy_from_slice(&1u64.to_le_bytes());
        bytes[32..48].copy_from_slice(&POLICY.actors[0]);
        bytes[48..64].copy_from_slice(&POLICY.actors[1]);
        bytes[64] = 1;
        bytes[73..81].copy_from_slice(&7u64.to_le_bytes());
        bytes
    };
    if mode & 1 != 0 {
        for patch in rest.as_chunks::<2>().0.iter().take(16) {
            let offset = usize::from(patch[0]) % 81;
            frame[offset] ^= patch[1];
        }
    }
    let mut reply = Channel::new(frame.clone());
    match replica.push_authenticated(&mut reply) {
        Ok(ack) => {
            assert_eq!(ack.actor, POLICY.actors[1]);
            assert_eq!(ack.policy, POLICY);
            assert_eq!(&ack.state.to_le_bytes()[..8], &5u64.to_le_bytes());
        }
        Err(error) => assert!(matches!(
            error,
            Error::Io
                | Error::Corrupt
                | Error::UnsupportedFormat
                | Error::ForeignStore
                | Error::Conflict
        )),
    }
    assert_eq!(db.snapshot().revision(), before);
    let mut channel = Channel::new(std::mem::take(&mut frame));
    match replica.serve_authenticated(&mut channel) {
        Ok(()) => {
            let bytes = channel.input.get_ref();
            assert_eq!(&bytes[..8], b"WDBGCTR\x01");
            assert_eq!(&bytes[8..24], &POLICY.collection);
            assert_eq!(&bytes[24..32], &1u64.to_le_bytes());
            assert_eq!(
                &bytes[32..64],
                &[POLICY.actors[0], POLICY.actors[1]].concat()
            );
            assert_eq!(bytes[64], 1);
            let echoed = u64::from_le_bytes(bytes[65..73].try_into().unwrap());
            let incoming = u64::from_le_bytes(bytes[73..81].try_into().unwrap());
            assert!(echoed <= 5);
            assert!(incoming <= u64::MAX / 2);
            assert_eq!(replica.read().unwrap().total(), 5 + incoming);
            assert_eq!(db.snapshot().revision(), before + u64::from(incoming != 0));
            let revision = db.snapshot().revision();
            let mut replay = Channel::new(bytes.clone());
            replica.serve_authenticated(&mut replay).unwrap();
            assert_eq!(db.snapshot().revision(), revision);
        }
        Err(error) => {
            assert!(matches!(
                error,
                Error::Io
                    | Error::Corrupt
                    | Error::UnsupportedFormat
                    | Error::ForeignStore
                    | Error::Conflict
            ));
            assert_eq!(db.snapshot().revision(), before);
            assert_eq!(replica.read().unwrap().total(), 5);
            assert!(channel.output.is_empty());
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Model {
    increments: [u64; 2],
    completed: u64,
}
impl Model {
    fn merge(&mut self, incoming: Self) {
        for i in 0..2 {
            self.increments[i] = self.increments[i].max(incoming.increments[i]);
        }
        self.completed |= incoming.completed;
    }
}
fn read(db: &Database, index: usize) -> Replica<'_, Progress> {
    Replica::open(db, db.collection_id(1), POLICY, POLICY.actors[index]).unwrap()
}
fn verify(db: &Database, index: usize, model: Model) {
    let actual = read(db, index).read().unwrap();
    assert_eq!(actual.count.total(), model.increments.iter().sum());
    let bytes = actual.count.to_le_bytes();
    for (component, expected) in bytes.as_chunks::<8>().0.iter().zip(model.increments) {
        assert_eq!(u64::from_le_bytes(*component), expected);
    }
    for id in 0..64 {
        assert_eq!(
            actual.completed.contains(id),
            model.completed & (1u64 << id) != 0
        );
    }
}
fn deliver(db: &Database, receiver: usize, frame: Vec<u8>, lost: bool) {
    let mut channel = Channel::new(frame);
    channel.fail_reply = lost;
    let result = read(db, receiver).serve_authenticated(&mut channel);
    assert_eq!(result, if lost { Err(Error::Io) } else { Ok(()) });
}
pub fn crdt_schedule(input: &[u8]) {
    if input.is_empty() {
        return;
    }
    let disks = std::array::from_fn::<_, 2, _>(|_| SimulatedDisk::new().unwrap());
    let mut databases: [Database; 2] = std::array::from_fn(|i| {
        let db =
            Database::create_on(disks[i].clone(), [i as u8 + 1; 16], Limits::default()).unwrap();
        let id = db.create_collection().unwrap();
        Replica::<Progress>::enroll(&db, id, POLICY, POLICY.actors[i]).unwrap();
        db
    });
    let mut models = [Model::default(); 2];
    let mut delayed = Vec::new();
    for &[command, arg] in input.as_chunks::<2>().0.iter().take(24) {
        let actor = usize::from(command & 1);
        let other = 1 - actor;
        match (command >> 1) % 6 {
            0 => {
                let amount = u64::from(arg) + 1;
                read(&databases[actor], actor)
                    .update(|state, id| state.count.increment(id, amount))
                    .unwrap();
                models[actor].increments[actor] += amount;
            }
            1 => {
                let id = arg % 64;
                read(&databases[actor], actor)
                    .update(|state, _| state.completed.insert(id).map(|_| ()))
                    .unwrap();
                models[actor].completed |= 1u64 << id;
            }
            2 => {
                let frame = offer(&read(&databases[actor], actor));
                deliver(&databases[other], other, frame, arg & 1 != 0);
                let incoming = models[actor];
                models[other].merge(incoming);
            }
            3 => {
                if delayed.len() < 8 {
                    delayed.push((actor, offer(&read(&databases[actor], actor)), models[actor]));
                }
            }
            4 => {
                if !delayed.is_empty() {
                    let (source, frame, model) = &delayed[usize::from(arg) % delayed.len()];
                    let receiver = 1 - source;
                    deliver(&databases[receiver], receiver, frame.clone(), false);
                    models[receiver].merge(*model);
                }
            }
            _ => {
                let placeholder = Database::new().unwrap();
                drop(std::mem::replace(&mut databases[actor], placeholder));
                disks[actor].crash(Crash::DropAll).unwrap();
                databases[actor] =
                    Database::open_on(disks[actor].clone(), Limits::default()).unwrap();
            }
        }
        for i in 0..2 {
            verify(&databases[i], i, models[i]);
        }
    }
    for actor in [0, 1, 0, 1] {
        let other = 1 - actor;
        let frame = offer(&read(&databases[actor], actor));
        deliver(&databases[other], other, frame, false);
        let incoming = models[actor];
        models[other].merge(incoming);
    }
    let revisions = databases.each_ref().map(|db| db.snapshot().revision());
    for actor in [0, 1] {
        let frame = offer(&read(&databases[actor], actor));
        deliver(&databases[1 - actor], 1 - actor, frame, false);
        verify(&databases[actor], actor, models[actor]);
    }
    assert_eq!(
        databases.each_ref().map(|db| db.snapshot().revision()),
        revisions
    );
    assert!(read(&databases[0], 0).read().unwrap() == read(&databases[1], 1).read().unwrap());
}
#[test]
fn deterministic_crdt_inputs_replay() {
    for frame in [
        vec![1],
        vec![1, 64, 1],
        vec![1, 65, 6],
        vec![0],
        vec![1, 24, 3],
    ] {
        crdt_frames(&frame);
    }
    crdt_schedule(&[
        0, 10, 1, 20, 2, 3, 3, 4, 6, 0, 0, 7, 4, 1, 8, 0, 10, 0, 5, 0, 11, 0,
    ]);
}
