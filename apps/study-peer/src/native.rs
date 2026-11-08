mod protocol;
use protocol::{Enrollment, StudyReplica, command, utf8};
#[cfg(test)]
mod tests;
mod tls;

#[allow(dead_code)] // The shared example also demonstrates manually reviewed edits.
#[path = "../../../examples/support/study.rs"]
mod study;

use std::{
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
use tls::{Credentials, Incoming, Tls};
use weekend_db::{Database, Error, Limits};

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const USAGE: &str = "study-peer keygen <credentials-dir>
study-peer init <file> <a|b> <32-hex-collection-id>
study-peer run <file> <a|b> <collection-id> <listen-addr> <peer-addr> <credentials-dir> <peer-cert.der> [--no-push]
commands: edit <lesson> <minutes> <weekly-goal> <session-limit>, practice <minutes>, show, peer, push on|off, quit";
const COLLECTION_ROW: u64 = 1;
const POLL: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(2);

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
