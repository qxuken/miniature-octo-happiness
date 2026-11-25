use crate::diagnostics::{self, Phase};
mod admission;
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
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread,
    time::Duration,
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
commands: edit <lesson> <minutes> <weekly-goal> <session-limit>, practice <minutes>, show, peer, incoming, push on|off, quit";
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
        [option, path] if option == flag => {
            Ok(Some(diagnostics::at(Phase::ConfigurationLoad, || {
                bounded_file(Path::new(path), 128)
            })?))
        }
        _ => Err(USAGE.into()),
    }
}

pub fn main(args: Vec<OsString>) -> AppResult<()> {
    match args.as_slice() {
        [command] if command == "--help" || command == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [command, dir] if command == "keygen" => diagnostics::at(Phase::CredentialsLoad, || {
            Credentials::create(Path::new(dir))
        }),
        [command, path, actor, id] if command == "init" => {
            initialize(Path::new(path), Enrollment::parse(actor, id)?)
        }
        [command, a, b, policy] if command == "checkpoint-roster" => roster::checkpoint(
            [Path::new(a), Path::new(b)],
            diagnostics::at(Phase::ConfigurationLoad, || {
                roster::policy_file(Path::new(policy))
            })?,
            &mut io::stdout().lock(),
        ),
        [command, a, b, old, target_a, target_b, new, options @ ..]
            if command == "stage-roster" =>
        {
            roster::stage(
                [Path::new(a), Path::new(b)],
                diagnostics::at(Phase::ConfigurationLoad, || {
                    roster::policy_file(Path::new(old))
                })?,
                [Path::new(target_a), Path::new(target_b)],
                diagnostics::at(Phase::ConfigurationLoad, || {
                    roster::policy_file(Path::new(new))
                })?,
                expected_receipt(options, "--expect-old")?.as_deref(),
                &mut io::stdout().lock(),
            )
        }
        [command, a, b, policy, options @ ..] if command == "verify-roster" => roster::verify(
            [Path::new(a), Path::new(b)],
            diagnostics::at(Phase::ConfigurationLoad, || {
                roster::policy_file(Path::new(policy))
            })?,
            expected_receipt(options, "--expect-staged")?.as_deref(),
            &mut io::stdout().lock(),
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
                roster::enrollment(
                    utf8(actor)?,
                    diagnostics::at(Phase::ConfigurationLoad, || {
                        roster::policy_file(Path::new(config))
                    })?,
                )?
            };
            let options = RunOptions::parse(options)?;
            let credentials = diagnostics::at(Phase::CredentialsLoad, || {
                Credentials::load(Path::new(credentials))
            })?;
            let pin = diagnostics::at(Phase::CredentialsLoad, || {
                bounded_file(Path::new(pin), 65536)
            })?;
            let tls = diagnostics::at(Phase::TlsConfig, || Tls::new(&credentials, pin.into()))?;
            let db = diagnostics::storage(Phase::StorageOpen, || {
                Database::open_file(path, Limits::default())
            })?;
            let network = Network {
                listener: {
                    let address = utf8(listen)?.parse::<SocketAddr>()?;
                    diagnostics::at(Phase::Listen, || Ok(TcpListener::bind(address)?))?
                },
                peer: utf8(peer)?.parse()?,
                tls,
            };
            run(&db, enrollment, network, options, |stream| stream)
        }
        _ => Err(USAGE.into()),
    }
}

fn initialize(path: &Path, enrollment: Enrollment) -> AppResult<()> {
    let db = diagnostics::storage(Phase::StorageCreate, || {
        Database::create_file(path, Limits::default())
    })?;
    diagnostics::storage(Phase::StorageCreate, || enroll(&db, enrollment))?;
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
    diagnostic: Option<diagnostics::Diagnostic>,
}

fn observe_local_command<T>(operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    diagnostics::at(Phase::LocalCommand, || {
        let mut result = None;
        // Parsing/retries remain in the reusable protocol, without CLI/TLS imports.
        // Observe its returned engine error, then return the identical boxed result.
        let _ = diagnostics::storage(Phase::LocalCommand, || {
            let response = operation();
            let error = response
                .as_ref()
                .err()
                .and_then(|error| error.downcast_ref::<Error>())
                .copied();
            result = Some(response);
            error.map_or(Ok(()), Err)
        });
        result.expect("command operation is called exactly once")
    })
}

fn run<S: io::Read + Write>(
    db: &Database,
    enrollment: Enrollment,
    network: Network,
    options: RunOptions,
    incoming: impl Fn(Incoming) -> S + Send + Sync,
) -> AppResult<()> {
    let replica = diagnostics::storage(Phase::StorageOpen, || {
        StudyReplica::open(
            db,
            db.collection_id(COLLECTION_ROW),
            enrollment.policy,
            enrollment.actor,
        )
    })?;
    let Network {
        listener,
        peer,
        tls,
    } = network;
    listener.set_nonblocking(true)?;
    let pushing = AtomicBool::new(options.pushing);
    let stop = Arc::new(AtomicBool::new(false));
    let status = Mutex::new(PeerStatus::default());
    let incoming_status = Mutex::new(admission::Status::default());
    let stdout = io::stdout();
    writeln!(stdout.lock(), "ready {}", listener.local_addr()?)?;

    let settings = diagnostics::settings();
    thread::scope(|scope| {
        let stop_receiving = &stop;
        let receiving_replica = &replica;
        let server = &tls;
        let received = &incoming_status;
        let receiving = scope.spawn(move || {
            diagnostics::capture(settings, Phase::Incoming, || {
                admission::receive(listener, server, stop_receiving, received, |stream| {
                    Ok(diagnostics::storage(Phase::Replication, || {
                        receiving_replica.serve_authenticated(incoming(stream))
                    })?)
                })
            })
        });
        let outgoing = scope.spawn(|| {
            let mut delay = POLL;
            while !stop.load(Relaxed) {
                if !pushing.load(Relaxed) {
                    thread::park();
                    continue;
                }
                let (result, diagnostic) = diagnostics::capture(settings, Phase::Connect, || {
                    let stream = diagnostics::at(Phase::Connect, || tls.connect(peer))?;
                    let ack = diagnostics::storage(Phase::Replication, || {
                        replica.push_authenticated(stream)
                    })?;
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
                    status.diagnostic = diagnostic;
                }
                thread::park_timeout(delay);
            }
        });

        let (result, local_diagnostic) =
            diagnostics::capture(settings, Phase::LocalCommand, || {
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
                                    Some(Err(error)) if settings.enabled => status
                                        .diagnostic
                                        .unwrap_or_else(|| {
                                            diagnostics::take(Phase::Connect, error.as_ref())
                                        })
                                        .json(settings)
                                        .to_string(),
                                    Some(Err(error)) => format!("peer {n} error {error}"),
                                    None => format!("peer {n} idle"),
                                }
                            };
                            writeln!(stdout.lock(), "{reply}")?;
                        }
                        ["incoming"] => {
                            let report = {
                                let status = incoming_status.lock().unwrap();
                                if settings.enabled {
                                    status.structured(settings).to_string()
                                } else {
                                    status.report()
                                }
                            };
                            writeln!(stdout.lock(), "{report}")?;
                        }
                        ["edit" | "practice", ..] if options.sync_only => {
                            if settings.enabled {
                                let diagnostic =
                                    diagnostics::take(Phase::LocalCommand, &Error::Invalid);
                                writeln!(stdout.lock(), "{}", diagnostic.json(settings))?;
                            } else {
                                writeln!(
                                    stdout.lock(),
                                    "error local edits are disabled during staging"
                                )?;
                            }
                        }
                        _ => match observe_local_command(|| command(&words, db, &replica)) {
                            Ok(reply) => writeln!(stdout.lock(), "{reply}")?,
                            Err(error) if settings.enabled => {
                                let diagnostic =
                                    diagnostics::take(Phase::LocalCommand, error.as_ref());
                                writeln!(stdout.lock(), "{}", diagnostic.json(settings))?;
                            }
                            Err(error) => writeln!(stdout.lock(), "error {error}")?,
                        },
                    }
                }
                Ok(())
            });
        stop.store(true, Relaxed);
        receiving.thread().unpark();
        outgoing.thread().unpark();
        let (received, diagnostic) = diagnostics::at(Phase::Shutdown, || {
            Ok(receiving.join().map_err(|_| Error::Panic)?)
        })?;
        diagnostics::propagate(received, diagnostic)?;
        diagnostics::at(Phase::Shutdown, || {
            outgoing.join().map_err(|_| Error::Panic)?;
            Ok(())
        })?;
        diagnostics::propagate(result, local_diagnostic)
    })
}
