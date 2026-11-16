use super::{AppResult, Profile, Secret, TIMEOUT, protocol, read_line, time_remaining};
use std::{
    io::{self, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::Instant,
};
use weekend_db::{Database, Limits, Result as DbResult};

const MAX_CONNECTIONS: usize = 8;
const MAX_QUEUED_COMMANDS: usize = 8;

struct Admission(Arc<AtomicUsize>);
impl Admission {
    fn acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        active
            .try_update(Relaxed, Relaxed, |count| {
                (count < MAX_CONNECTIONS).then_some(count + 1)
            })
            .ok()?;
        Some(Self(active.clone()))
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Relaxed);
    }
}

struct Request {
    command: String,
    deadline: Instant,
    admitted: SyncSender<Receiver<String>>,
}

fn dispatch_requests(
    requests: Receiver<Request>,
    mut dispatch: impl FnMut(&str) -> DbResult<String>,
) {
    for request in requests {
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

pub(super) fn serve(
    path: &Path,
    address: SocketAddr,
    secret: Option<Secret>,
    profile: Profile,
) -> AppResult<()> {
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
    let (requests, pending) = mpsc::sync_channel(MAX_QUEUED_COMMANDS);
    thread::Builder::new()
        .name("coordinator-database".into())
        .spawn(move || {
            dispatch_requests(pending, |command| dispatch(&mut db, command));
        })?;
    let active = Arc::new(AtomicUsize::new(0));
    let secret = secret.map(Arc::new);
    writeln!(io::stdout().lock(), "ready {}", listener.local_addr()?)?;
    for incoming in listener.incoming() {
        let mut stream = incoming?;
        let deadline = Instant::now() + TIMEOUT;
        let Some(admission) = Admission::acquire(&active) else {
            // Saturation must never make the listener wait on the peer.
            if stream.set_nonblocking(true).is_ok() {
                let _ = stream.write_all(b"error Busy\n");
            }
            continue;
        };
        let requests = requests.clone();
        let secret = secret.clone();
        thread::Builder::new()
            .name("coordinator-connection".into())
            .spawn(move || {
                let _admission = admission;
                if let Err(error) = serve_connection(stream, deadline, secret.as_deref(), &requests)
                {
                    eprintln!("client connection: {error}");
                }
            })?;
    }
    Ok(())
}

fn serve_connection(
    mut stream: TcpStream,
    deadline: Instant,
    secret: Option<&Secret>,
    requests: &SyncSender<Request>,
) -> io::Result<()> {
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
        let active = Arc::new(AtomicUsize::new(0));
        let mut held: Vec<_> = (0..MAX_CONNECTIONS)
            .map(|_| Admission::acquire(&active).unwrap())
            .collect();
        assert!(Admission::acquire(&active).is_none());
        drop(held.pop());
        held.push(Admission::acquire(&active).unwrap());
        assert!(Admission::acquire(&active).is_none());
        drop(held);
        assert_eq!(active.load(Relaxed), 0);
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
            let worker = scope.spawn(move || {
                dispatch_requests(pending, |command| {
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
}
