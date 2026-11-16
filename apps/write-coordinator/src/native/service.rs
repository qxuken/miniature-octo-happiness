use super::stop::{Stop, wait_for_connection};
use super::{AppResult, Profile, Secret, TIMEOUT, protocol, read_line, time_remaining};
use std::{
    io::{self, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, ScopedJoinHandle},
    time::Instant,
};
use weekend_db::{Database, Limits, Result as DbResult};

const MAX_CONNECTIONS: usize = 8;
const MAX_QUEUED_COMMANDS: usize = 8;

struct StopOnExit<'a>(&'a AtomicBool);
impl Drop for StopOnExit<'_> {
    fn drop(&mut self) {
        self.0.store(true, Relaxed);
    }
}

struct Request {
    command: String,
    deadline: Instant,
    admitted: SyncSender<Receiver<String>>,
}

fn dispatch_requests(
    requests: Receiver<Request>,
    stopping: &AtomicBool,
    mut dispatch: impl FnMut(&str) -> DbResult<String>,
) {
    for request in requests {
        if stopping.load(Relaxed) {
            break;
        }
        if Instant::now() >= request.deadline {
            continue;
        }
        let (respond, response) = mpsc::sync_channel(1);
        if request.admitted.send(response).is_err() {
            continue;
        }
        let reply = dispatch(&request.command).unwrap_or_else(|error| format!("error {error}"));
        let _ = respond.send(reply);
    }
}

fn reap_connections(connections: &mut Vec<ScopedJoinHandle<'_, ()>>) -> io::Result<()> {
    while let Some(finished) = connections.iter().position(|worker| worker.is_finished()) {
        connections
            .swap_remove(finished)
            .join()
            .map_err(|_| io::Error::other("coordinator connection worker panicked"))?;
    }
    Ok(())
}

pub(super) fn serve(
    path: &Path,
    address: SocketAddr,
    secret: Option<Secret>,
    profile: Profile,
) -> AppResult<()> {
    let stop = Stop::register()?;
    let stopping = stop.requested();
    let mut db = Database::open_file(path, Limits::default())?;
    let dispatch: fn(&mut Database, &str) -> DbResult<String> = match profile {
        Profile::Legacy => {
            protocol::state(&db)?;
            protocol::handle
        }
        Profile::Sequenced => {
            protocol::sequenced::admit(&db)?;
            protocol::sequenced::handle
        }
    };
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    let secret = secret.as_ref();
    thread::scope(|scope| {
        let (requests, pending) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
        let database = thread::Builder::new()
            .name("coordinator-database".into())
            .spawn_scoped(scope, move || {
                let _stop_on_exit = StopOnExit(stopping);
                dispatch_requests(pending, stopping, |command| dispatch(&mut db, command));
            })?;
        let mut connections = Vec::with_capacity(MAX_CONNECTIONS);
        let mut result: AppResult<()> = (|| {
            writeln!(io::stdout().lock(), "ready {}", listener.local_addr()?)?;
            while !stopping.load(Relaxed) {
                match wait_for_connection(&listener) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
                if stopping.load(Relaxed) {
                    break;
                }
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                let deadline = Instant::now() + TIMEOUT;
                reap_connections(&mut connections)?;
                if connections.len() == MAX_CONNECTIONS {
                    // Saturation must never make the listener wait on the peer.
                    if stream.set_nonblocking(true).is_ok() {
                        let _ = stream.write_all(b"error Busy\n");
                    }
                    continue;
                }
                let requests = requests.clone();
                connections.push(
                    thread::Builder::new()
                        .name("coordinator-connection".into())
                        .spawn_scoped(scope, move || {
                            if let Err(error) =
                                serve_connection(stream, deadline, secret, &requests)
                            {
                                eprintln!("client connection: {error}");
                            }
                        })?,
                );
            }
            Ok(())
        })();
        stopping.store(true, Relaxed);
        drop(listener);
        drop(requests);
        for worker in connections.into_iter().chain([database]) {
            if worker.join().is_err() && result.is_ok() {
                result =
                    Err(io::Error::other("coordinator worker panicked during shutdown").into());
            }
        }
        result
    })
}

fn serve_connection(
    mut stream: TcpStream,
    deadline: Instant,
    secret: Option<&Secret>,
    requests: &SyncSender<Request>,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let request = read_line(&stream, deadline)?;
    let command = match secret {
        Some(secret) => secret.authorize(&request),
        None => Some(request.as_str()),
    };
    let Some(command) = command else {
        return stream.write_all(b"error Unauthorized\n");
    };
    let (admitted, start) = mpsc::sync_channel(1);
    let request = Request {
        command: command.to_owned(),
        deadline,
        admitted,
    };
    match requests.try_send(request) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => return stream.write_all(b"error Busy\n"),
        Err(TrySendError::Disconnected(_)) => return Err(io::ErrorKind::BrokenPipe.into()),
    }
    let response = start
        .recv_timeout(time_remaining(deadline)?)
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => io::ErrorKind::TimedOut,
            mpsc::RecvTimeoutError::Disconnected => io::ErrorKind::ConnectionAborted,
        })?;
    // Once dispatched, a storage operation can finish after the client deadline.
    let reply = response
        .recv()
        .map_err(|_| io::ErrorKind::ConnectionAborted)?;
    stream.write_all(format!("{reply}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc::TryRecvError;
    use std::time::Duration;

    #[test]
    fn a_released_connection_slot_can_be_reused() {
        thread::scope(|scope| {
            let mut connections = vec![scope.spawn(|| {})];
            let deadline = Instant::now() + Duration::from_secs(5);
            while !connections[0].is_finished() {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            reap_connections(&mut connections).unwrap();
            assert!(connections.is_empty());
        });
    }

    #[test]
    fn a_full_dispatch_queue_rejects_a_complete_request() {
        let (requests, pending) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
        for _ in 0..MAX_QUEUED_COMMANDS {
            let (admitted, _) = mpsc::sync_channel(1);
            assert!(
                requests
                    .try_send(Request {
                        command: "queued".into(),
                        deadline: Instant::now(),
                        admitted,
                    })
                    .is_ok()
            );
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(TIMEOUT)).unwrap();
        peer.write_all(b"get\n").unwrap();
        let (connection, _) = listener.accept().unwrap();
        serve_connection(connection, Instant::now() + TIMEOUT, None, &requests).unwrap();
        let mut reply = String::new();
        BufReader::new(peer).read_line(&mut reply).unwrap();
        assert_eq!(reply, "error Busy\n");
        assert_eq!(
            pending.try_iter().filter(|r| r.command == "queued").count(),
            MAX_QUEUED_COMMANDS
        );
    }

    #[test]
    fn expired_queued_commands_are_skipped_before_dispatch() {
        let (requests, pending) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
        let (started, in_progress) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let mut db = Database::new().unwrap();
        protocol::sequenced::initialize(&db).unwrap();
        let wait = Duration::from_secs(5);
        let stopping = AtomicBool::new(false);
        thread::scope(|scope| {
            let enqueue = |command: &str, deadline| {
                let (admitted, start) = mpsc::sync_channel(1);
                requests
                    .send(Request {
                        command: command.to_owned(),
                        deadline,
                        admitted,
                    })
                    .unwrap();
                start
            };
            let database = &mut db;
            let stopping = &stopping;
            let worker = scope.spawn(move || {
                dispatch_requests(pending, stopping, |command| {
                    if command == "set-seq 0 1 3 11" {
                        started.send(()).unwrap();
                        resume.recv_timeout(wait).unwrap();
                    }
                    protocol::sequenced::handle(database, command)
                });
            });
            let first = enqueue("set-seq 0 1 3 11", Instant::now() + wait);
            in_progress.recv_timeout(wait).unwrap();
            let expired = enqueue("set-seq 1 1 4 99", Instant::now());
            drop(enqueue("set-seq 3 1 4 77", Instant::now() + wait));
            let fresh = enqueue("set-seq 2 1 4 42", Instant::now() + wait);
            release.send(()).unwrap();
            assert_eq!(
                first
                    .recv_timeout(wait)
                    .unwrap()
                    .recv_timeout(wait)
                    .unwrap(),
                "committed 4"
            );
            assert_eq!(
                fresh
                    .recv_timeout(wait)
                    .unwrap()
                    .recv_timeout(wait)
                    .unwrap(),
                "committed 5"
            );
            assert!(matches!(
                expired.try_recv(),
                Err(TryRecvError::Disconnected)
            ));
            drop(requests);
            worker.join().unwrap();
        });
        assert_eq!(
            protocol::sequenced::handle(&mut db, "get").unwrap(),
            "value 5 42"
        );
        assert_eq!(
            protocol::sequenced::handle(&mut db, "receipt 1").unwrap(),
            "receipt 0"
        );
        assert_eq!(
            protocol::sequenced::handle(&mut db, "receipt 3").unwrap(),
            "receipt 0"
        );
    }

    #[test]
    fn shutdown_completes_dispatched_write_and_drops_queued_write() {
        use weekend_db::storage::{Crash, SimulatedDisk};

        let (requests, pending) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
        let (release, resume) = mpsc::channel();
        let stopping = AtomicBool::new(false);
        let disk = SimulatedDisk::new().unwrap();
        let limits = Limits::default();
        let mut db = Database::create_on(disk.clone(), [7; 16], limits).unwrap();
        protocol::sequenced::initialize(&db).unwrap();
        let wait = Duration::from_secs(5);
        thread::scope(|scope| {
            let database = &mut db;
            let stop = &stopping;
            let worker = scope.spawn(move || {
                dispatch_requests(pending, stop, |command| {
                    resume.recv_timeout(wait).unwrap();
                    protocol::sequenced::handle(database, command)
                });
            });
            let submit = |command: &str| {
                let (admitted, start) = mpsc::sync_channel(1);
                requests
                    .send(Request {
                        command: command.into(),
                        deadline: Instant::now() + wait,
                        admitted,
                    })
                    .unwrap();
                start
            };
            let running = submit("set-seq 0 1 3 42").recv_timeout(wait).unwrap();
            let queued = submit("set-seq 1 1 4 99");
            drop(requests);
            stopping.store(true, Relaxed);
            release.send(()).unwrap();
            assert_eq!(running.recv_timeout(wait).unwrap(), "committed 4");
            assert!(matches!(
                queued.recv_timeout(wait),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ));
            worker.join().unwrap();
        });
        drop(db);
        disk.crash(Crash::DropAll).unwrap();
        let mut db = Database::open_on(disk, limits).unwrap();
        assert_eq!(
            protocol::sequenced::handle(&mut db, "set-seq 0 1 3 42").unwrap(),
            "committed 4"
        );
        assert_eq!(
            protocol::sequenced::handle(&mut db, "get").unwrap(),
            "value 4 42"
        );
        assert_eq!(
            protocol::sequenced::handle(&mut db, "receipt 1").unwrap(),
            "receipt 0"
        );
    }

    #[test]
    fn dispatcher_unwind_requests_shutdown() {
        let stopping = AtomicBool::new(false);
        thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let _stop_on_exit = StopOnExit(&stopping);
                panic!("dispatcher failure");
            });
            assert!(worker.join().is_err());
        });
        assert!(stopping.load(Relaxed));
    }
}
