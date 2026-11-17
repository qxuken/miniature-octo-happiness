use super::*;

const POLICY: Policy = Policy {
    collection: [9; 16],
    generation: 1,
    actors: [[1; 16], [2; 16]],
};
fn register(sequence: [u64; 2], values: [Option<u64>; 2]) -> MultiValueRegister {
    let mut bytes = [0; MultiValueRegister::SIZE];
    for i in 0..2 {
        bytes[i * 17..i * 17 + 8].copy_from_slice(&sequence[i].to_le_bytes());
        bytes[i * 17 + 8] = u8::from(values[i].is_some());
        bytes[i * 17 + 9..i * 17 + 17].copy_from_slice(&values[i].unwrap_or(0).to_le_bytes());
    }
    MultiValueRegister::decode(&bytes).unwrap()
}
#[test]
fn policy_and_frame_independent_guards() {
    assert_eq!(POLICY.validate(), Ok(()));
    for policy in [
        Policy {
            collection: [0; 16],
            ..POLICY
        },
        Policy {
            generation: 0,
            ..POLICY
        },
        Policy {
            actors: [[0; 16], [2; 16]],
            ..POLICY
        },
        Policy {
            actors: [[2; 16], [1; 16]],
            ..POLICY
        },
        Policy {
            actors: [[1; 16], [1; 16]],
            ..POLICY
        },
    ] {
        assert_eq!(policy.validate(), Err(Error::Invalid));
    }
    assert_eq!(
        POLICY.matches(Policy {
            collection: [8; 16],
            ..POLICY
        }),
        Err(Error::ForeignStore)
    );
    assert_eq!(
        POLICY.matches(Policy {
            generation: 2,
            ..POLICY
        }),
        Err(Error::Conflict)
    );
    let state = State {
        policy: POLICY,
        actor: Actor(0),
        payload: GrowCounter::default(),
    };
    let mut buffer = [0; MAX_FRAME];
    let valid = state.encode(&mut buffer).unwrap().to_vec();
    for (index, value, error) in [
        (0, 0, Error::UnsupportedFormat),
        (8, 0, Error::Corrupt),
        (24, 0, Error::Corrupt),
        (64, 2, Error::Corrupt),
    ] {
        let mut bytes = valid.clone();
        if index == 8 {
            bytes[8..24].fill(0);
        } else {
            bytes[index] = value;
        }
        assert!(matches!(State::<GrowCounter>::decode(&bytes),Err(e) if e==error));
    }
    assert!(matches!(
        State::<GrowCounter>::decode(&valid[..64]),
        Err(Error::Corrupt)
    ));
    assert!(State::<GrowCounter>::decode(&valid).is_ok());
    assert!(matches!(
        State::<GrowCounter>::read(&mut &valid[..7]),
        Err(Error::Io)
    ));
    assert!(matches!(
        State::<GrowCounter>::read(&mut &valid[..8]),
        Err(Error::Io)
    ));
    let mut wrong = valid;
    wrong[0] = 0;
    assert!(matches!(
        State::<GrowCounter>::read(&mut wrong.as_slice()),
        Err(Error::UnsupportedFormat)
    ));
}
#[test]
fn register_decode_and_same_dot_predicate_controls() {
    let base = register([1, 1], [Some(4), Some(5)]);
    // Equal sequences, both live and different payloads reject; independently
    // remove each condition while keeping the other applicable conditions true.
    for (incoming, expected) in [
        (register([1, 1], [Some(9), Some(5)]), Err(Error::Conflict)),
        (register([0, 1], [None, Some(5)]), Ok(())),
        (register([1, 1], [None, Some(5)]), Err(Error::Conflict)),
        (register([1, 1], [Some(4), Some(5)]), Ok(())),
        (register([1, 2], [Some(4), Some(9)]), Ok(())),
        (register([2, 1], [Some(4), Some(5)]), Err(Error::Conflict)),
    ] {
        assert_eq!(base.check_peer(incoming, Actor(0)), expected);
    }
    let mut bytes = [0; MultiValueRegister::SIZE];
    for (seq, flag, value, valid) in [
        (0, 0, 0, true),
        (1, 0, 0, false),
        (0, 0, 1, false),
        (1, 1, 0, true),
        (0, 1, 0, false),
        (1, 2, 0, false),
    ] {
        bytes[..8].copy_from_slice(&u64::to_le_bytes(seq));
        bytes[8] = flag;
        bytes[9..17].copy_from_slice(&u64::to_le_bytes(value));
        assert_eq!(MultiValueRegister::decode(&bytes).is_ok(), valid);
    }
    assert_eq!(base.encode(&mut bytes[..33]), Err(Error::Buffer));
    assert_eq!(
        MultiValueRegister::decode(&bytes[..33]),
        Err(Error::Corrupt)
    );
    let mut exhausted = register([u64::MAX, 0], [Some(4), None]);
    let before = exhausted;
    assert_eq!(exhausted.assign(Actor(0), 7), Err(Error::Exhausted));
    assert_eq!(exhausted, before);
    for (incoming, expected) in [
        (register([0, 0], [None, None]), vec![4, 5]),
        (register([1, 1], [None, Some(5)]), vec![5]),
        (register([2, 1], [Some(7), None]), vec![7]),
    ] {
        let mut joined = base;
        joined.join(incoming);
        assert_eq!(joined.values().collect::<Vec<_>>(), expected);
    }
    assert_eq!(MultiValueRegister::default().resolve_with(u64::max), None);
    assert_eq!(
        register([1, 1], [Some(4), Some(4)])
            .values()
            .collect::<Vec<_>>(),
        vec![4]
    );
    assert_eq!(base.resolve_with(u64::min), Some(4));
}
#[test]
fn container_bounds_and_exhaustion_preserve_state() {
    let mut counter = GrowCounter::default();
    assert_eq!(counter.increment(Actor(0), 0), Err(Error::Invalid));
    counter.increment(Actor(0), MAX_COMPONENT).unwrap();
    let old = counter;
    for n in [1, u64::MAX] {
        assert_eq!(counter.increment(Actor(0), n), Err(Error::Limit));
        assert_eq!(counter, old);
    }
    let mut malformed = counter.to_le_bytes();
    malformed[..8].copy_from_slice(&(MAX_COMPONENT + 1).to_le_bytes());
    assert_eq!(GrowCounter::from_le_bytes(malformed), Err(Error::Corrupt));
    assert_eq!(GrowCounter::decode(&[0; 15]), Err(Error::Corrupt));
    assert_eq!(counter.encode(&mut [0; 15]), Err(Error::Buffer));
    let mut set = GrowSet::default();
    assert_eq!(set.insert(63), Ok(true));
    assert_eq!(set.insert(63), Ok(false));
    assert_eq!(set.insert(64), Err(Error::Bounds));
    assert!(!set.contains(64));
    assert!(!set.contains(62));
    assert!(set.contains(63));
    assert_eq!(set.encode(&mut [0; 7]), Err(Error::Buffer));
    assert_eq!(GrowSet::decode(&[0; 7]), Err(Error::Corrupt));
    let mut set = AddWinsSet::default();
    assert_eq!(set.remove(63), Ok(false));
    set.insert(63, Actor(1)).unwrap();
    assert!(set.contains(63));
    assert_eq!(set.remove(63), Ok(true));
    assert!(!set.contains(63));
    assert!(!set.contains(64));
    assert_eq!(set.insert(64, Actor(0)), Err(Error::Bounds));
    assert_eq!(set.remove(64), Err(Error::Bounds));
    let mut bytes = vec![0; AddWinsSet::SIZE];
    bytes[..8].copy_from_slice(&u64::MAX.to_le_bytes());
    let mut full = AddWinsSet::decode(&bytes).unwrap();
    let old = full;
    assert_eq!(full.insert(0, Actor(0)), Err(Error::Exhausted));
    assert_eq!(full, old);
    assert_eq!(
        AddWinsSet::default().check_peer(full, Actor(0)),
        Err(Error::Conflict)
    );
    assert_eq!(full.check_peer(old, Actor(0)), Ok(()));
    bytes[16..24].copy_from_slice(&1u64.to_le_bytes());
    bytes[24..32].copy_from_slice(&2u64.to_le_bytes());
    assert_eq!(AddWinsSet::decode(&bytes), Err(Error::Corrupt));
    assert_eq!(set.encode(&mut bytes[..1]), Err(Error::Buffer));
    assert_eq!(AddWinsSet::decode(&bytes[..1]), Err(Error::Corrupt));
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct SizedRecord<const N: usize>;
impl<const N: usize> Record for SizedRecord<N> {
    const KEY: &'static [u8] = b"size";
    const FORMAT: &'static [u8; 8] = b"TESTSIZE";
    const SIZE: usize = N;
    fn encode(self, bytes: &mut [u8]) -> Result<()> {
        bytes.fill(0);
        Ok(())
    }
    fn decode(_: &[u8]) -> Result<Self> {
        Ok(Self)
    }
    fn merge(&mut self, _: Self) {}
    fn check_peer(self, _: Self, _: Actor) -> Result<()> {
        Ok(())
    }
}
#[test]
fn replica_size_actor_enrollment_and_local_row_guards() {
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    assert!(matches!(
        Replica::<SizedRecord<4097>>::configured(&db, c, POLICY, [1; 16]),
        Err(Error::Limit)
    ));
    assert!(Replica::<SizedRecord<4096>>::configured(&db, c, POLICY, [1; 16]).is_ok());
    assert!(matches!(
        Replica::<GrowCounter>::configured(&db, c, POLICY, [3; 16]),
        Err(Error::ForeignStore)
    ));
    assert!(matches!(
        Replica::<GrowCounter>::open(&db, c, POLICY, [1; 16]),
        Err(Error::NotFound)
    ));
    let r = Replica::<GrowCounter>::enroll(&db, c, POLICY, [1; 16]).unwrap();
    assert!(matches!(
        Replica::<GrowCounter>::enroll(&db, c, POLICY, [1; 16]),
        Err(Error::AlreadyExists)
    ));
    assert!(matches!(
        Replica::<GrowCounter>::open(&db, c, POLICY, [2; 16]),
        Err(Error::ForeignStore)
    ));
    assert!(matches!(
        r.local_state(Some(db.unsigned(0).unwrap())),
        Err(Error::Corrupt)
    ));
    let local = State {
        policy: POLICY,
        actor: Actor(0),
        payload: GrowCounter::default(),
    };
    assert_eq!(r.check_peer(local, local), Err(Error::ForeignStore));
    assert_eq!(
        r.check_peer(
            State {
                actor: Actor(1),
                ..local
            },
            local
        ),
        Ok(())
    );
    let revision = db.snapshot().revision();
    assert_eq!(r.update(|_, _| Err(Error::Invalid)), Err(Error::Invalid));
    assert_eq!(db.snapshot().revision(), revision);
}

#[test]
fn unhealthy_admission_and_exhausted_policy_resolution() {
    use crate::storage::{Fault, SimulatedDisk};
    let mut bytes = [0; MultiValueRegister::SIZE];
    register([u64::MAX, 1], [Some(4), Some(5)])
        .encode(&mut bytes)
        .unwrap();
    let mut policy = MaxRegister::decode(&bytes).unwrap();
    let previous = policy;
    assert_eq!(policy.resolve(Actor(0)), Err(Error::Exhausted));
    assert_eq!(policy, previous);
    let disk = SimulatedDisk::new().unwrap();
    let db = Database::create_on(disk.clone(), [3; 16], crate::Limits::default()).unwrap();
    let r = Replica::<GrowCounter>::enroll(&db, db.create_collection().unwrap(), POLICY, [1; 16])
        .unwrap();
    assert_eq!(r.require_healthy(), Ok(()));
    disk.arm(0, Fault::Error);
    assert_eq!(r.update(|c, a| c.increment(a, 1)), Err(Error::Io));
    assert_eq!(r.require_healthy(), Err(Error::RecoveryRequired));
    assert_eq!(
        r.update(|_, _| panic!("unhealthy callback")),
        Err(Error::RecoveryRequired)
    );
}

#[test]
fn register_rejects_empty_history_and_unwitnessed_retirements() {
    let mut bytes = [0; MultiValueRegister::SIZE];
    assert_eq!(
        MultiValueRegister::decode(&bytes),
        Ok(MultiValueRegister::default())
    );
    for sequences in [[1u64, 0], [0, 1], [1, 1]] {
        bytes[..8].copy_from_slice(&sequences[0].to_le_bytes());
        bytes[17..25].copy_from_slice(&sequences[1].to_le_bytes());
        assert_eq!(MultiValueRegister::decode(&bytes), Err(Error::Corrupt));
    }
    let live = register([1, 1], [Some(4), Some(5)]);
    for (incoming, expected) in [
        (register([1, 1], [None, Some(5)]), Err(Error::Conflict)),
        (register([1, 1], [Some(4), None]), Err(Error::Conflict)),
        (register([1, 2], [Some(4), None]), Err(Error::Conflict)),
        (register([1, 2], [None, Some(6)]), Ok(())),
        (register([0, 1], [None, Some(5)]), Ok(())),
        (live, Ok(())),
    ] {
        assert_eq!(live.check_peer(incoming, Actor(0)), expected);
    }
    // Each history is individually valid, but each says its assignment followed
    // the other's. Such a causal cycle cannot belong to one enrolled history.
    let left = register([1, 1], [Some(4), None]);
    let right = register([1, 1], [None, Some(5)]);
    assert_eq!(left.check_peer(right, Actor(0)), Err(Error::Conflict));
    assert_eq!(right.check_peer(left, Actor(1)), Err(Error::Conflict));
}

#[test]
fn register_causal_event_oracle_accepts_bounded_honest_schedules() {
    #[derive(Clone, Copy, Default)]
    struct Model {
        peers: [MultiValueRegister; 2],
        knowledge: [u16; 2],
        observed: [u16; 6],
    }
    fn walk(model: Model, depth: usize, visited: &mut usize) {
        *visited += 1;
        for actor in 0..2 {
            let state = model.peers[actor];
            assert_eq!(
                state.check_peer(model.peers[1 - actor], Actor(actor)),
                Ok(())
            );
            let known = model.knowledge[actor];
            let retired = (0..depth)
                .filter(|i| known & (1 << i) != 0)
                .fold(0, |all, i| all | model.observed[i]);
            let mut expected: Vec<_> = (0..depth)
                .filter(|i| known & !retired & (1 << i) != 0)
                .map(|i| i as u64 + 1)
                .collect();
            let mut actual: Vec<_> = state.values().collect();
            expected.sort_unstable();
            actual.sort_unstable();
            assert_eq!(actual, expected);
            let mut bytes = [0; MultiValueRegister::SIZE];
            state.encode(&mut bytes).unwrap();
            assert_eq!(MultiValueRegister::decode(&bytes), Ok(state));
        }
        if depth == 6 {
            return;
        }
        for actor in 0..2 {
            let mut next = model;
            next.observed[depth] = next.knowledge[actor];
            next.knowledge[actor] |= 1 << depth;
            next.peers[actor]
                .assign(Actor(actor), depth as u64 + 1)
                .unwrap();
            assert_eq!(
                model.peers[actor].check_peer(next.peers[actor], Actor(1 - actor)),
                Ok(())
            );
            walk(next, depth + 1, visited);
            let mut next = model;
            next.knowledge[actor] |= next.knowledge[1 - actor];
            next.peers[actor].join(next.peers[1 - actor]);
            walk(next, depth + 1, visited);
        }
    }
    let mut visited = 0;
    walk(Model::default(), 0, &mut visited);
    assert_eq!(visited, 5461);
}

#[test]
fn register_forged_retirement_frames_never_publish_or_acknowledge() {
    struct Channel {
        input: std::io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl std::io::Read for Channel {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            std::io::Read::read(&mut self.input, bytes)
        }
    }
    impl std::io::Write for Channel {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.output.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for case in 0..5 {
        let db = Database::new().unwrap();
        let collection = db.create_collection().unwrap();
        let replica =
            Replica::<MultiValueRegister>::enroll(&db, collection, POLICY, [1; 16]).unwrap();
        let local = if case == 3 {
            register([1, 1], [Some(4), None])
        } else {
            register([1, 1], [Some(4), Some(5)])
        };
        let mut tx = db.begin();
        tx.put(
            collection,
            MultiValueRegister::KEY,
            &replica
                .value(State {
                    policy: POLICY,
                    actor: Actor(0),
                    payload: local,
                })
                .unwrap(),
        )
        .unwrap();
        tx.commit().unwrap();
        drop(tx);
        let incoming = match case {
            2 => register([1, 2], [Some(4), None]),
            4 => register([1, 2], [None, Some(6)]),
            _ => register([1, 1], [None, Some(5)]),
        };
        let mut buffer = [0; MAX_FRAME];
        let mut bytes = State {
            policy: POLICY,
            actor: Actor(1),
            payload: incoming,
        }
        .encode(&mut buffer)
        .unwrap()
        .to_vec();
        if case == 0 {
            // Turn the last visible choice into a canonical tombstone.
            bytes[HEADER + 17 + 8..HEADER + 34].fill(0);
        }
        let before = db.snapshot().revision();
        let row = db
            .snapshot()
            .get(collection, MultiValueRegister::KEY)
            .unwrap();
        let mut channel = Channel {
            input: std::io::Cursor::new(bytes),
            output: Vec::new(),
        };
        let result = replica.serve_authenticated(&mut channel);
        if case == 4 {
            assert_eq!(result, Ok(()));
            assert_eq!(db.snapshot().revision(), before + 1);
            assert!(!channel.output.is_empty());
            assert_eq!(
                replica
                    .state_at(&db.snapshot())
                    .unwrap()
                    .payload
                    .values()
                    .collect::<Vec<_>>(),
                vec![6]
            );
        } else {
            assert_eq!(
                result,
                Err(if case == 0 {
                    Error::Corrupt
                } else {
                    Error::Conflict
                })
            );
            assert_eq!(db.snapshot().revision(), before);
            assert!(channel.output.is_empty());
            assert_eq!(
                db.snapshot()
                    .get(collection, MultiValueRegister::KEY)
                    .unwrap(),
                row
            );
        }
    }
}
