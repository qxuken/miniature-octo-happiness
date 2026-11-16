mod protocol;
use protocol::{Enrollment, StudyReplica, command, utf8};
mod roster;
#[cfg(test)]
mod tests;
mod tls;

#[allow(dead_code)] // The shared example also demonstrates manually reviewed edits.
#[path = "../../../examples/support/study.rs"]
mod study;

use std::{
    ffi::OsString,
    io::{self, BufRead, Write},
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread,
    time::{Duration, Instant},
};
use tls::{Credentials, Incoming, Tls, bounded_file};
use weekend_db::{Database, Error, Limits};

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const USAGE: &str = "study-peer keygen <credentials-dir>
study-peer init <file> <a|b> <32-hex-collection-id>
study-peer run <file> <a|b> <collection-id> <listen-addr> <peer-addr> <credentials-dir> <peer-cert.der> [--no-push] [--sync-only]
study-peer run-policy <file> <a|b> <policy-file> <listen-addr> <peer-addr> <credentials-dir> <peer-cert.der> [--no-push] [--sync-only]
study-peer checkpoint-roster <old-a> <old-b> <old-policy-file>
study-peer stage-roster <old-a> <old-b> <old-policy-file> <new-a> <new-b> <new-policy-file> [--expect-old <checkpoint>]
study-peer verify-roster <a> <b> <policy-file> [--expect-staged <receipt>]
commands: edit <lesson> <minutes> <weekly-goal> <session-limit>, practice <minutes>, show, peer, push on|off, quit";
const COLLECTION_ROW: u64 = 1;
const POLL: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(2);

struct Network {
    listener: TcpListener,
    peer: SocketAddr,
    tls: Tls,
}

struct RunOptions {
    pushing: bool,
    sync_only: bool,
}

impl RunOptions {
    fn parse(args: &[OsString]) -> AppResult<Self> {
        let mut options = Self {
            pushing: true,
            sync_only: false,
        };
        for option in args {
            if option == "--no-push" && options.pushing {
                options.pushing = false;
            } else if option == "--sync-only" && !options.sync_only {
                options.sync_only = true;
            } else {
                return Err(USAGE.into());
            }
        }
        Ok(options)
    }
}

fn expected_receipt(args: &[OsString], flag: &str) -> AppResult<Option<Vec<u8>>> {
    match args {
        [] => Ok(None),
        [option, path] if option == flag => Ok(Some(bounded_file(Path::new(path), 128)?)),
        _ => Err(USAGE.into()),
    }
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
        [command, a, b, policy] if command == "checkpoint-roster" => roster::checkpoint(
            [Path::new(a), Path::new(b)],
            roster::policy_file(Path::new(policy))?,
        ),
        [command, a, b, old, target_a, target_b, new, options @ ..]
            if command == "stage-roster" =>
        {
            roster::stage(
                [Path::new(a), Path::new(b)],
                roster::policy_file(Path::new(old))?,
                [Path::new(target_a), Path::new(target_b)],
                roster::policy_file(Path::new(new))?,
                expected_receipt(options, "--expect-old")?.as_deref(),
            )
        }
        [command, a, b, policy, options @ ..] if command == "verify-roster" => roster::verify(
            [Path::new(a), Path::new(b)],
            roster::policy_file(Path::new(policy))?,
            expected_receipt(options, "--expect-staged")?.as_deref(),
        ),
        [
            command,
            path,
            actor,
            config,
            listen,
            peer,
            credentials,
            pin,
            options @ ..,
        ] if command == "run" || command == "run-policy" => {
            let enrollment = if command == "run" {
                Enrollment::parse(actor, config)?
            } else {
                roster::enrollment(utf8(actor)?, roster::policy_file(Path::new(config))?)?
            };
            let options = RunOptions::parse(options)?;
            let tls = Tls::new(
                &Credentials::load(Path::new(credentials))?,
                bounded_file(Path::new(pin), 65536)?.into(),
            )?;
            let db = Database::open_file(path, Limits::default())?;
            let network = Network {
                listener: TcpListener::bind(utf8(listen)?.parse::<SocketAddr>()?)?,
                peer: utf8(peer)?.parse()?,
                tls,
            };
            run(&db, enrollment, network, options, |stream| stream)
        }
        _ => Err(USAGE.into()),
    }
}

fn initialize(path: &Path, enrollment: Enrollment) -> AppResult<()> {
    let db = Database::create_file(path, Limits::default())?;
    enroll(&db, enrollment)?;
    Ok(())
}

fn enroll(db: &Database, enrollment: Enrollment) -> weekend_db::Result<StudyReplica<'_>> {
    let collection = db.create_collection()?;
    assert_eq!(collection.as_u64(), COLLECTION_ROW);
    StudyReplica::enroll(db, collection, enrollment.policy, enrollment.actor)
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
    options: RunOptions,
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
    let pushing = AtomicBool::new(options.pushing);
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
                        let result =
                            server
                                .accept_at(socket, Instant::now(), None)
                                .and_then(|stream| {
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
                    ["edit" | "practice", ..] if options.sync_only => {
                        writeln!(
                            stdout.lock(),
                            "error local edits are disabled during staging"
                        )?;
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
