mod support;
use std::collections::BTreeMap;
use support::Rng;
use weekend_db::{Database, Error};

#[derive(Clone, Debug)]
enum Op {
    Read(u8),
    Put(u8, i64),
    Delete(u8),
    Scan,
}
#[derive(Debug, PartialEq, Eq)]
enum Event {
    Read(u8, Option<i64>),
    Scan(Vec<(Vec<u8>, i64)>),
}
type Records = BTreeMap<Vec<u8>, i64>;
fn serial(program: &[Op], records: &mut Records) -> Vec<Event> {
    let mut last = 0;
    let mut events = Vec::new();
    for op in program {
        match *op {
            Op::Read(k) => {
                let v = records.get(&vec![k]).copied();
                last = v.unwrap_or(-1);
                events.push(Event::Read(k, v));
            }
            Op::Put(k, salt) => {
                records.insert(vec![k], last + salt);
            }
            Op::Delete(k) => {
                records.remove(&vec![k]);
            }
            Op::Scan => {
                last = records.len() as i64;
                events.push(Event::Scan(
                    records.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                ));
            }
        }
    }
    events
}

#[test]
fn randomized_thread_programs_match_serial_commit_order() {
    use std::{sync::Barrier, thread};
    for seed in 1..=256 {
        let mut rng = Rng(seed ^ 0xbad5eed);
        let db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let mut model = Records::new();
        let mut tx = db.begin();
        for k in 0..3 {
            tx.put(c, &[k], &db.integer(i64::from(k)).unwrap()).unwrap();
            model.insert(vec![k], i64::from(k));
        }
        tx.commit().unwrap();
        let historical = db.snapshot();
        let old = model.clone();
        let programs: Vec<Vec<Op>> = (0..4)
            .map(|_| {
                (0..1 + rng.n(8))
                    .map(|_| {
                        let k = rng.n(6) as u8;
                        match rng.n(4) {
                            0 => Op::Read(k),
                            1 => Op::Put(k, rng.n(7) as i64),
                            2 => Op::Delete(k),
                            _ => Op::Scan,
                        }
                    })
                    .collect()
            })
            .collect();
        let prepared = Barrier::new(4);
        let mut outcomes = thread::scope(|s| {
            let workers: Vec<_> = programs
                .iter()
                .enumerate()
                .map(|(i, program)| {
                    s.spawn({
                        let (db, prepared) = (&db, &prepared);
                        move || {
                            let mut tx = db.begin();
                            let mut events = Vec::new();
                            let mut last = 0;
                            for op in program {
                                match *op {
                                    Op::Read(k) => {
                                        let v =
                                            tx.get(c, &[k]).unwrap().map(|v| v.as_i64().unwrap());
                                        last = v.unwrap_or(-1);
                                        events.push(Event::Read(k, v));
                                    }
                                    Op::Put(k, salt) => {
                                        tx.put(c, &[k], &db.integer(last + salt).unwrap()).unwrap()
                                    }
                                    Op::Delete(k) => tx.delete(c, &[k]).unwrap(),
                                    Op::Scan => {
                                        let rows: Vec<_> = tx
                                            .scan(c)
                                            .unwrap()
                                            .into_iter()
                                            .map(|(k, v)| (k, v.as_i64().unwrap()))
                                            .collect();
                                        last = rows.len() as i64;
                                        events.push(Event::Scan(rows));
                                    }
                                }
                                thread::yield_now();
                            }
                            prepared.wait();
                            (tx.commit(), i, events)
                        }
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|w| w.join().unwrap())
                .collect::<Vec<_>>()
        });
        outcomes.sort_by_key(|(result, _, _)| result.unwrap_or(u64::MAX));
        for (result, i, observed) in outcomes {
            match result {
                Ok(_) => assert_eq!(
                    serial(&programs[i], &mut model),
                    observed,
                    "seed={seed}, tx={i}, programs={programs:?}"
                ),
                Err(e) => assert_eq!(e, Error::Conflict, "seed={seed}"),
            }
        }
        let read = |snapshot: &weekend_db::Snapshot<'_>| {
            snapshot
                .scan(c)
                .unwrap()
                .into_iter()
                .map(|(k, v)| (k, v.as_i64().unwrap()))
                .collect::<Records>()
        };
        assert_eq!(
            read(&db.snapshot()),
            model,
            "seed={seed}, programs={programs:?}"
        );
        assert_eq!(read(&historical), old, "historical seed={seed}");
    }
}

#[test]
fn randomized_short_schedules_match_serial_commit_order() {
    for seed in 1..=600 {
        let mut rng = Rng(seed);
        let db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let mut model = Records::new();
        let mut initial = db.begin();
        for k in 0..2u8 {
            if seed % 4 != 0 {
                initial
                    .put(c, &[k], &db.integer(i64::from(k)).unwrap())
                    .unwrap();
                model.insert(vec![k], i64::from(k));
            }
        }
        initial.commit().unwrap();
        let historical = db.snapshot();
        let old = model.clone();
        let programs: [Vec<Op>; 2] = std::array::from_fn(|_| {
            (0..1 + rng.n(5))
                .map(|_| {
                    let k = rng.n(4) as u8;
                    match rng.n(4) {
                        0 => Op::Read(k),
                        1 => Op::Put(k, rng.n(7) as i64),
                        2 => Op::Delete(k),
                        _ => Op::Scan,
                    }
                })
                .collect()
        });
        let mut txs = [db.begin(), db.begin()];
        let mut next = [0, 0];
        let mut done = [false, false];
        let mut last = [0, 0];
        let mut observed: [Vec<Event>; 2] = std::array::from_fn(|_| Vec::new());
        let mut commits = Vec::new();
        while !done.iter().all(|d| *d) {
            let mut i = rng.n(2);
            if done[i] {
                i = 1 - i;
            }
            if next[i] == programs[i].len() {
                match txs[i].commit() {
                    Ok(_) => commits.push(i),
                    Err(Error::Conflict) => {}
                    Err(e) => panic!("seed={seed}: {e}"),
                }
                done[i] = true;
                continue;
            }
            match programs[i][next[i]] {
                Op::Read(k) => {
                    let v = txs[i].get(c, &[k]).unwrap().map(|v| v.as_i64().unwrap());
                    last[i] = v.unwrap_or(-1);
                    observed[i].push(Event::Read(k, v));
                }
                Op::Put(k, salt) => txs[i]
                    .put(c, &[k], &db.integer(last[i] + salt).unwrap())
                    .unwrap(),
                Op::Delete(k) => txs[i].delete(c, &[k]).unwrap(),
                Op::Scan => {
                    let rows: Vec<_> = txs[i]
                        .scan(c)
                        .unwrap()
                        .into_iter()
                        .map(|(k, v)| (k, v.as_i64().unwrap()))
                        .collect();
                    last[i] = rows.len() as i64;
                    observed[i].push(Event::Scan(rows));
                }
            }
            next[i] += 1;
        }
        for i in commits {
            assert_eq!(
                serial(&programs[i], &mut model),
                observed[i],
                "seed={seed}, programs={programs:?}, tx={i}"
            );
        }
        let actual: Records = db
            .snapshot()
            .scan(c)
            .unwrap()
            .into_iter()
            .map(|(k, v)| (k, v.as_i64().unwrap()))
            .collect();
        assert_eq!(actual, model, "seed={seed}, programs={programs:?}");
        let actual: Records = historical
            .scan(c)
            .unwrap()
            .into_iter()
            .map(|(k, v)| (k, v.as_i64().unwrap()))
            .collect();
        assert_eq!(actual, old, "historical seed={seed}");
    }
}
