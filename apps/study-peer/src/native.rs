#[cfg(test)]
mod tests;
mod tls;

#[allow(dead_code)] // The shared example also demonstrates manually reviewed edits.
#[path = "../../../examples/support/study.rs"]
mod study;

use std::{
    ffi::OsStr,
    fs,
    io::{self, BufRead, Write},
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread,
    time::Duration,
};
use study::StudyProgress;
use tls::{Credentials, Incoming, Tls};
use weekend_db::{
    Database, Error, Limits,
    crdt::{Actor, Policy, Replica},
};

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type StudyReplica<'db> = Replica<'db, StudyProgress>;

const USAGE: &str = "study-peer keygen <credentials-dir>
study-peer init <file> <a|b> <32-hex-collection-id>
study-peer run <file> <a|b> <collection-id> <listen-addr> <peer-addr> <credentials-dir> <peer-cert.der> [--no-push]
commands: edit <lesson> <minutes> <weekly-goal> <session-limit>, practice <minutes>, show, peer, push on|off, quit";
const COLLECTION_ROW: u64 = 1;
const POLL: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
struct Enrollment {
    policy: Policy,
    actor: [u8; 16],
}

impl Enrollment {
    fn new(actor: &str, collection: [u8; 16]) -> AppResult<Self> {
        let policy = Policy {
            collection,
            generation: 7,
            actors: [[1; 16], [2; 16]],
        };
        let actor = match actor {
            "a" => policy.actors[0],
            "b" => policy.actors[1],
            _ => return Err("actor must be a or b".into()),
        };
        if collection == [0; 16] {
            return Err("collection ID must be nonzero".into());
        }
        Ok(Self { policy, actor })
    }

    fn parse(actor: &OsStr, id: &OsStr) -> AppResult<Self> {
        let id = utf8(id)?;
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("collection ID must be 32 hexadecimal digits".into());
        }
        Self::new(utf8(actor)?, u128::from_str_radix(id, 16)?.to_be_bytes())
    }
}

struct Network {
    listener: TcpListener,
    peer: SocketAddr,
    tls: Tls,
}

pub fn main() -> AppResult<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [command] if command == "--help" || command == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [command, dir] if command == "keygen" => Credentials::create(Path::new(dir)),
        [command, path, actor, id] if command == "init" => {
            initialize(Path::new(path), Enrollment::parse(actor, id)?)
        }
        [
            command,
            path,
            actor,
            id,
            listen,
            peer,
            credentials,
            pin,
            options @ ..,
        ] if command == "run" && (options.is_empty() || options == ["--no-push"]) => {
            let enrollment = Enrollment::parse(actor, id)?;
            let db = Database::open_file(path, Limits::default())?;
            let network = Network {
                listener: TcpListener::bind(utf8(listen)?.parse::<SocketAddr>()?)?,
                peer: utf8(peer)?.parse()?,
                tls: Tls::new(
                    &Credentials::load(Path::new(credentials))?,
                    fs::read(pin)?.into(),
                )?,
            };
            run(&db, enrollment, network, options.is_empty(), |stream| {
                stream
            })
        }
        _ => Err(USAGE.into()),
    }
}

fn utf8(value: &OsStr) -> AppResult<&str> {
    value
        .to_str()
        .ok_or_else(|| "expected a UTF-8 argument".into())
}

fn initialize(path: &Path, enrollment: Enrollment) -> AppResult<()> {
    let db = Database::create_file(path, Limits::default())?;
    let collection = db.create_collection()?;
    assert_eq!(collection.as_u64(), COLLECTION_ROW);
    StudyReplica::enroll(&db, collection, enrollment.policy, enrollment.actor)?;
    Ok(())
}

#[derive(Default)]
struct PeerStatus {
    acknowledgments: u64,
    latest: Option<AppResult<u64>>,
}

fn run<S: io::Read + Write>(
    db: &Database,
    enrollment: Enrollment,
    network: Network,
    start_pushing: bool,
    incoming: impl Fn(Incoming) -> S + Send,
) -> AppResult<()> {
    let replica = StudyReplica::open(
        db,
        db.collection_id(COLLECTION_ROW),
        enrollment.policy,
        enrollment.actor,
    )?;
    let Network {
        listener,
        peer,
        tls,
    } = network;
    listener.set_nonblocking(true)?;
    let pushing = AtomicBool::new(start_pushing);
    let stop = AtomicBool::new(false);
    let status = Mutex::new(PeerStatus::default());
    let stdout = io::stdout();
    writeln!(stdout.lock(), "ready {}", listener.local_addr()?)?;

    thread::scope(|scope| {
        let stop_receiving = &stop;
        let receiving_replica = &replica;
        let server = &tls;
        let receiving = scope.spawn(move || {
            while !stop_receiving.load(Relaxed) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        let result = server.accept(socket).and_then(|stream| {
                            Ok(receiving_replica.serve_authenticated(incoming(stream))?)
                        });
                        if let Err(error) = result {
                            eprintln!("incoming sync: {error}");
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::park_timeout(Duration::from_millis(25));
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        });
        let outgoing = scope.spawn(|| {
            let mut delay = POLL;
            while !stop.load(Relaxed) {
                if !pushing.load(Relaxed) {
                    thread::park();
                    continue;
                }
                let result = tls.connect(peer).and_then(|stream| {
                    let ack = replica.push_authenticated(stream)?;
                    Ok(ack.state.practice_minutes.total())
                });
                {
                    let mut status = status.lock().unwrap();
                    if result.is_ok() {
                        status.acknowledgments += 1;
                        delay = POLL;
                    } else {
                        delay = (delay * 2).min(MAX_BACKOFF);
                    }
                    status.latest = Some(result);
                }
                thread::park_timeout(delay);
            }
        });

        let result = (|| -> AppResult<()> {
            for line in io::stdin().lock().lines() {
                let line = line?;
                let words: Vec<_> = line.split_whitespace().collect();
                match words.as_slice() {
                    [] => {}
                    ["quit"] => break,
                    ["push", value @ ("on" | "off")] => {
                        pushing.store(*value == "on", Relaxed);
                        outgoing.thread().unpark();
                        writeln!(stdout.lock(), "push {value}")?;
                    }
                    ["peer"] => {
                        let reply = {
                            let status = status.lock().unwrap();
                            let n = status.acknowledgments;
                            match &status.latest {
                                Some(Ok(total)) => format!("peer {n} ack {total}"),
                                Some(Err(error)) => format!("peer {n} error {error}"),
                                None => format!("peer {n} idle"),
                            }
                        };
                        writeln!(stdout.lock(), "{reply}")?;
                    }
                    _ => match command(&words, db, &replica) {
                        Ok(reply) => writeln!(stdout.lock(), "{reply}")?,
                        Err(error) => writeln!(stdout.lock(), "error {error}")?,
                    },
                }
            }
            Ok(())
        })();
        stop.store(true, Relaxed);
        receiving.thread().unpark();
        outgoing.thread().unpark();
        receiving.join().map_err(|_| Error::Panic)??;
        outgoing.join().map_err(|_| Error::Panic)?;
        result
    })
}

fn command(words: &[&str], db: &Database, replica: &StudyReplica<'_>) -> AppResult<String> {
    let revision = match words {
        ["show"] => {
            let snapshot = db.snapshot();
            let state = replica.read_at(&snapshot)?;
            return Ok(format!(
                "state {} {} {:?} {:?} {:?}",
                snapshot.revision(),
                state.practice_minutes.total(),
                state.weekly_goal.values().collect::<Vec<_>>(),
                state.session_limit.values().collect::<Vec<_>>(),
                state.completed.iter().collect::<Vec<_>>(),
            ));
        }
        ["practice", minutes] => {
            let minutes = minutes.parse()?;
            edit(replica, |state, actor| {
                state.practice_minutes.increment(actor, minutes)
            })?
        }
        ["edit", lesson, minutes, goal, limit] => {
            let (lesson, minutes, goal, limit) = (
                lesson.parse()?,
                minutes.parse()?,
                goal.parse()?,
                limit.parse()?,
            );
            edit(replica, |state, actor| {
                state.completed.insert(lesson)?;
                state.practice_minutes.increment(actor, minutes)?;
                state.weekly_goal.assign(actor, goal)?;
                state.session_limit.assign(actor, limit)
            })?
        }
        _ => return Err("unknown command (see study-peer --help)".into()),
    };
    Ok(format!("local {revision}"))
}

fn edit(
    replica: &StudyReplica<'_>,
    edit: impl Fn(&mut StudyProgress, Actor) -> weekend_db::Result<()>,
) -> weekend_db::Result<u64> {
    // These commands only edit the draft. A definite conflict can run again
    // against a fresh snapshot. OutcomeUnknown must reach the caller.
    for _ in 0..2 {
        let result = replica.update(&edit);
        if result != Err(Error::Conflict) {
            return result;
        }
    }
    replica.update(edit)
}
