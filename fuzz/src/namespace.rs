#[path = "../../src/storage/replacement.rs"]
mod replacement;
use std::{collections::BTreeMap, sync::OnceLock};
use weekend_db::{Database, Error, Limits, storage::SimulatedDisk};
type Names = BTreeMap<String, usize>;
fn images() -> &'static [Vec<u8>; 2] {
    static IMAGES: OnceLock<[Vec<u8>; 2]> = OnceLock::new();
    IMAGES.get_or_init(|| {
        let original = crate::fixture_image().to_vec();
        let mut db = Database::open_on(
            SimulatedDisk::from_image(&original).unwrap(),
            Limits::default(),
        )
        .unwrap();
        let compact = SimulatedDisk::new().unwrap();
        db.compact_to(compact.clone()).unwrap();
        [original, compact.durable_image().unwrap()]
    })
}
fn check_image(id: usize) {
    let db = Database::open_on(
        SimulatedDisk::from_image(&images()[usize::from(id != 0)]).unwrap(),
        Limits::default(),
    )
    .unwrap();
    let c = db.collection_id(1);
    let now = db.snapshot();
    let old = db.retained_snapshot(1).unwrap();
    assert_eq!((now.revision(), old.revision()), (4, 2));
    for (snapshot, values) in [(&now, [21, 12]), (&old, [11, 12])] {
        assert_eq!(snapshot.scan(c).unwrap().len(), 2);
        for (key, value) in values.into_iter().enumerate() {
            assert_eq!(
                snapshot.get(c, &[key as u8]).unwrap().unwrap().as_i64(),
                Ok(value)
            );
        }
    }
}

pub fn replacement_namespace(input: &[u8]) {
    let mut names = Names::from([("canonical".into(), 0), ("unrelated".into(), usize::MAX)]);
    for (round, op) in input.chunks(2).take(8).enumerate() {
        let fail = usize::from(op[0] % 14);
        let policy = usize::from(op.get(1).copied().unwrap_or(0) % 3);
        let busy = op[0] & 128 != 0;
        let candidate = format!("candidate-{round}");
        let new_id = round + 1;
        // Lease acquisition is adapter-owned. A refused lease must not invoke
        // publication or create a candidate in this namespace model.
        let initial = names.clone();
        let mut visited = Vec::new();
        if busy {
            assert_eq!(names, initial);
            continue;
        }
        names.insert(candidate.clone(), new_id);
        let initial = names.clone();
        let mut states = [initial.clone(), initial.clone(), initial.clone()]; // durable/flushed/volatile
        let mut boundary = 0;
        let result = replacement::publish(|step| {
            let index = match step {
                replacement::Step::Exchange => 0,
                replacement::Step::SyncDirectory => 1,
                replacement::Step::SyncReplacement => 2,
                replacement::Step::RemoveOld => 3,
                replacement::Step::SyncCleanupDirectory => 4,
                replacement::Step::SyncCleanupFile => 5,
            };
            visited.push(index);
            if boundary == fail {
                return Err(Error::Io);
            }
            boundary += 1;
            match step {
                replacement::Step::Exchange => {
                    let previous = states[2]["canonical"];
                    states[2].insert("canonical".into(), new_id);
                    states[2].insert(candidate.clone(), previous);
                }
                replacement::Step::SyncDirectory | replacement::Step::SyncCleanupDirectory => {
                    states[1] = states[2].clone()
                }
                replacement::Step::SyncReplacement | replacement::Step::SyncCleanupFile => {
                    states[0] = states[1].clone()
                }
                replacement::Step::RemoveOld => {
                    states[2].remove(&candidate);
                }
            }
            if boundary == fail {
                return Err(Error::Io);
            }
            boundary += 1;
            Ok(())
        });
        let completed = fail.div_ceil(2).min(6);
        assert_eq!(result, if fail < 12 { Err(Error::Io) } else { Ok(()) });
        assert_eq!(visited, (0..(fail / 2 + 1).min(6)).collect::<Vec<_>>());
        // Independently derive observable state from completed barrier count,
        // rather than invoking the implementation's step callback again.
        for (layer, actual) in states.iter().enumerate() {
            let visible = match layer {
                0 => {
                    if completed >= 6 {
                        6
                    } else if completed >= 3 {
                        3
                    } else {
                        0
                    }
                }
                1 => {
                    if completed >= 5 {
                        5
                    } else if completed >= 2 {
                        2
                    } else {
                        0
                    }
                }
                _ => completed,
            };
            let mut expected = initial.clone();
            if visible >= 1 {
                expected.insert("canonical".into(), new_id);
                expected.insert(candidate.clone(), initial["canonical"]);
            }
            if visible >= 4 {
                expected.remove(&candidate);
            }
            assert_eq!(actual, &expected);
            assert_eq!(actual["unrelated"], usize::MAX);
            check_image(actual["canonical"]);
        }
        names = states[policy].clone(); // crash chooses one modeled persistence layer
    }
}
#[test]
fn replay_namespace() {
    for fail in 0..14 {
        for policy in 0..3 {
            replacement_namespace(&[fail, policy, 0, 2, 13, 0, 128, 0]);
        }
    }
}
