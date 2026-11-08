//! Accelerated calendar qualification driver, not a wall-clock soak.
#[allow(dead_code)]
#[path = "../examples/support/study.rs"]
mod study;
use std::{
    io::Write,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::Path,
    time::{Duration, Instant},
};
use study::StudyProgress;
use weekend_db::{
    Limits,
    crdt::{Policy, Replica},
    storage,
};
use weekend_db_typed::{Error, Model, Store};
#[derive(Model)]
#[weekend(type_id = 301, version = 1)]
struct Task {
    #[weekend(id = 1, unique)]
    slug: String,
    #[weekend(id = 2)]
    score: u64,
}
#[derive(Model)]
#[weekend(type_id = 302, version = 1)]
struct Receipt {
    #[weekend(id = 1)]
    day: u64,
}
fn limits() -> Limits {
    #[cfg(feature = "live-budget")]
    return Limits {
        max_live_bytes: 16 * 1024 * 1024,
        ..Limits::default()
    };
    #[cfg(not(feature = "live-budget"))]
    Limits::default()
}
fn open(path: impl AsRef<Path>) -> Store {
    Store::from_database(weekend_db::Database::open_file(path, limits()).unwrap()).unwrap()
}
fn memory_json(store: &Store) -> String {
    #[cfg(feature = "live-budget")]
    {
        let usage = store.database().memory_usage();
        assert!(usage.live_bytes <= usage.peak_bytes);
        assert!(usage.peak_bytes <= usage.limit_bytes);
        format!(
            "{{\"live_bytes\":{},\"peak_bytes\":{},\"limit_bytes\":{}}}",
            usage.live_bytes, usage.peak_bytes, usage.limit_bytes
        )
    }
    #[cfg(not(feature = "live-budget"))]
    {
        let _ = store;
        "null".into()
    }
}
fn policy() -> Policy {
    Policy {
        collection: [31; 16],
        generation: 1,
        actors: [[1; 16], [2; 16]],
    }
}
fn stream(s: TcpStream) -> TcpStream {
    s.set_nonblocking(false).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(15))).unwrap();
    s
}
fn verify(store: &mut Store, role: usize, day: u64) {
    let table = store.table::<Task>("tasks").unwrap();
    store
        .read(|r| {
            assert_eq!(r.list(&table)?.len(), 8);
            for id in 1..=8 {
                let task = r.find(&table, Task::SLUG, &format!("task-{id}"))?.unwrap();
                assert_eq!(task.id(), id);
                assert_eq!(task.score, day * 100 + role as u64 * 10 + id);
            }
            assert!(r.check_indexes(&table)?.is_consistent());
            Ok(())
        })
        .unwrap();
    let db = store.database();
    let replica =
        Replica::<StudyProgress>::open(db, db.collection_id(6), policy(), policy().actors[role])
            .unwrap();
    let state = replica.read().unwrap();
    assert_eq!(state.practice_minutes.total(), 30 * day + day * (day + 1));
    assert_eq!(
        state.completed.iter().collect::<Vec<_>>(),
        (0..(day * 2) as u8).collect::<Vec<_>>()
    );
    assert_eq!(state.effective_weekly_goal(), Some(90 + day));
    assert_eq!(
        state.session_limit.values().collect::<Vec<_>>(),
        vec![20 + day % 5]
    );
    // Recheck every daily retained image against a closed-form historical oracle.
    let history = store.history().unwrap();
    assert_eq!(history, (1..=day).collect::<Vec<_>>());
    for retained in history {
        assert!(retained <= day);
        store
            .read_retained(retained, |read| {
                assert_eq!(read.list(&table)?.len(), 8);
                for id in 1..=8 {
                    let row = read
                        .find(&table, Task::SLUG, &format!("task-{id}"))?
                        .unwrap();
                    assert_eq!(row.id(), id);
                    assert_eq!(row.score, retained * 100 + role as u64 * 10 + id);
                }
                assert!(read.check_indexes(&table)?.is_consistent());
                Ok(())
            })
            .unwrap();
        let old = replica
            .read_at(&db.retained_snapshot(retained).unwrap())
            .unwrap();
        assert_eq!(
            old.practice_minutes.total(),
            30 * retained + retained * (retained + 1)
        );
        assert_eq!(
            old.completed.iter().collect::<Vec<_>>(),
            (0..(retained * 2) as u8).collect::<Vec<_>>()
        );
        assert_eq!(old.effective_weekly_goal(), Some(90 + retained));
        assert_eq!(
            old.session_limit.values().collect::<Vec<_>>(),
            vec![20 + retained % 5]
        );
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let phase = &args[0];
    let path = Path::new(&args[1]);
    let role: usize = args[2].parse().unwrap();
    assert!(role < 2);
    let day: u64 = args[3].parse().unwrap();
    assert!(day <= 30);
    let start = Instant::now();
    let mut transaction_us = Vec::<u128>::new();
    let mut commits = 0;
    let mut conflicts = 0;
    let mut retry = 0;
    if phase == "init" {
        let mut store =
            Store::initialize(weekend_db::Database::create_file(path, limits()).unwrap()).unwrap();
        let tasks = store.table::<Task>("tasks").unwrap();
        let receipt = store.table::<Receipt>("receipt").unwrap();
        store
            .write(|w| {
                for id in 1..=8 {
                    w.insert(
                        &tasks,
                        id,
                        &Task {
                            slug: format!("task-{id}"),
                            score: id,
                        },
                    )?;
                }
                w.insert(&receipt, 1, &Receipt { day: 0 })
            })
            .unwrap();
        let db = store.database();
        let c = db.create_collection().unwrap();
        assert_eq!(c.as_u64(), 6);
        Replica::<StudyProgress>::enroll(db, c, policy(), policy().actors[role]).unwrap();
    } else if phase == "backup" {
        storage::backup_file(path, path.with_extension("backup"), limits()).unwrap();
        let backup_us = start.elapsed().as_micros();
        let restore_started = Instant::now();
        storage::restore_file(
            path.with_extension("backup"),
            path.with_extension("restored"),
            limits(),
        )
        .unwrap();
        let restore_us = restore_started.elapsed().as_micros();
        verify(&mut open(path.with_extension("restored")), role, day);
        println!(
            "{{\"phase\":\"backup\",\"role\":{role},\"day\":{day},\"backup_us\":{backup_us},\"restore_us\":{restore_us},\"elapsed_us\":{}}}",
            start.elapsed().as_micros()
        );
        return;
    } else {
        let opened = Instant::now();
        let mut store = open(path);
        let reopen_us = opened.elapsed().as_micros();
        match phase.as_str() {
            "day" => {
                let tasks = store.table::<Task>("tasks").unwrap();
                let receipt = store.table::<Receipt>("receipt").unwrap();
                let previous = store
                    .read(|r| Ok(r.get(&receipt, 1)?.unwrap().day))
                    .unwrap();
                assert!(previous == day || previous + 1 == day);
                if previous != day {
                    // One conventional optimistic conflict; the winner is durable.
                    let result = store.write(|w| {
                        w.update(&tasks, 1, |row| {
                            row.score += 1;
                            Ok(())
                        })?;
                        store.write(|other| {
                            other.update(&tasks, 1, |row| {
                                row.score += 1;
                                Ok(())
                            })
                        })?;
                        Ok(())
                    });
                    assert_eq!(result, Err(Error::Engine(weekend_db::Error::Conflict)));
                    conflicts += 1;
                    commits += 1;
                    retry += 1;
                    let commit_started = Instant::now();
                    store
                        .write(|w| {
                            for id in 1..=8 {
                                w.update(&tasks, id, |row| {
                                    row.score = day * 100 + role as u64 * 10 + id;
                                    Ok(())
                                })?;
                            }
                            w.update(&receipt, 1, |row| {
                                row.day = day;
                                Ok(())
                            })
                        })
                        .unwrap();
                    transaction_us.push(commit_started.elapsed().as_micros());
                    commits += 1;
                    // Deliberately terminate after durable receipt, before CRDT work/response.
                    if args.get(4).is_some_and(|x| x == "interrupt") {
                        std::process::exit(70);
                    }
                }
                let db = store.database();
                let replica = Replica::<StudyProgress>::open(
                    db,
                    db.collection_id(6),
                    policy(),
                    policy().actors[role],
                )
                .unwrap();
                let mark = ((day - 1) * 2 + role as u64) as u8;
                if !replica.read().unwrap().completed.contains(mark) {
                    let commit_started = Instant::now();
                    replica
                        .update(|state, actor| {
                            state.completed.insert(mark)?;
                            state
                                .practice_minutes
                                .increment(actor, 10 + 10 * role as u64 + day)?;
                            state
                                .weekly_goal
                                .assign(actor, 60 + 30 * role as u64 + day)?;
                            state
                                .session_limit
                                .assign(actor, 20 + 10 * role as u64 + day % 5)
                        })
                        .unwrap();
                    transaction_us.push(commit_started.elapsed().as_micros());
                    commits += 1;
                }
            }
            "serve" | "sync" => {
                let db = store.database();
                let replica = Replica::<StudyProgress>::open(
                    db,
                    db.collection_id(6),
                    policy(),
                    policy().actors[role],
                )
                .unwrap();
                let before = db.snapshot().revision();
                if phase == "serve" {
                    let listener = TcpListener::bind(&args[4]).unwrap();
                    println!("READY");
                    std::io::stdout().flush().unwrap();
                    listener.set_nonblocking(true).unwrap();
                    let deadline = Instant::now() + Duration::from_secs(30);
                    let incoming = loop {
                        match listener.accept() {
                            Ok((socket, _)) => break socket,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(Instant::now() < deadline, "accept timed out");
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            Err(error) => panic!("accept failed: {error}"),
                        }
                    };
                    replica.serve_authenticated(stream(incoming)).unwrap();
                } else {
                    replica
                        .push_authenticated(stream(
                            TcpStream::connect_timeout(
                                &args[4].to_socket_addrs().unwrap().next().unwrap(),
                                Duration::from_secs(10),
                            )
                            .unwrap(),
                        ))
                        .unwrap();
                }
                commits = db.snapshot().revision() - before;
            }
            "checkpoint" => {
                assert_eq!(store.checkpoint().unwrap(), day);
                commits = 1;
            }
            "verify" => verify(&mut store, role, day),
            _ => panic!("unknown phase"),
        }
        println!(
            "{{\"phase\":\"{phase}\",\"role\":{role},\"day\":{day},\"revision\":{},\"memory_usage\":{},\"transaction_us\":{transaction_us:?},\"reopen_us\":{reopen_us},\"elapsed_us\":{},\"commits\":{commits},\"conflicts\":{conflicts},\"retries\":{retry},\"file_bytes\":{}}}",
            store.database().snapshot().revision(),
            memory_json(&store),
            start.elapsed().as_micros(),
            std::fs::metadata(path).unwrap().len()
        );
        return;
    }
    println!(
        "{{\"phase\":\"{phase}\",\"role\":{role},\"day\":{day},\"elapsed_us\":{},\"commits\":{commits},\"conflicts\":{conflicts},\"retries\":{retry},\"file_bytes\":{}}}",
        start.elapsed().as_micros(),
        std::fs::metadata(path).unwrap().len()
    );
}
