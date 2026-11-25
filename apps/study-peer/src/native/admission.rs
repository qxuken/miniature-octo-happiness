use super::{
    AppResult,
    tls::{Incoming, Tls},
};
use crate::diagnostics::{self, Diagnostic, Phase, Settings};
use std::{
    io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread::{self, ScopedJoinHandle},
    time::{Duration, Instant},
};
use weekend_db::Error;

const MAX_SESSIONS: usize = 4;

#[derive(Default)]
pub(super) struct Status {
    served: u64,
    failed: u64,
    busy: u64,
    last_error: Option<Box<dyn std::error::Error + Send + Sync>>,
    last_diagnostic: Option<Diagnostic>,
}

impl Status {
    // Workers publish in-memory diagnostics. Only the local command writes
    // the report, so a blocked stderr cannot hold network admission.
    pub(super) fn report(&self) -> String {
        let status = format!(
            "incoming served={} failed={} busy={}",
            self.served, self.failed, self.busy
        );
        match &self.last_error {
            Some(error) => format!("{status} error {error}"),
            None => status,
        }
    }

    pub(super) fn structured(&self, settings: Settings) -> diagnostics::JsonLine {
        diagnostics::incoming(
            settings,
            self.served,
            self.failed,
            self.busy,
            self.last_diagnostic,
        )
    }

    fn record(&mut self, result: AppResult<()>, diagnostic: Option<Diagnostic>) {
        match result {
            Ok(()) => self.served = self.served.saturating_add(1),
            Err(error) => {
                self.failed = self.failed.saturating_add(1);
                self.last_error = Some(error);
                self.last_diagnostic = diagnostic;
            }
        }
    }

    fn refuse(&mut self) {
        self.busy = self.busy.saturating_add(1);
    }
}

pub(super) fn receive(
    listener: TcpListener,
    tls: &Tls,
    stop: &Arc<AtomicBool>,
    status: &Mutex<Status>,
    serve: impl Fn(Incoming) -> AppResult<()> + Sync,
) -> AppResult<()> {
    receive_with(
        listener,
        tls,
        stop,
        status,
        serve,
        TcpListener::accept,
        || Ok(thread::Builder::new()),
    )
}

fn receive_with(
    listener: TcpListener,
    tls: &Tls,
    stop: &Arc<AtomicBool>,
    status: &Mutex<Status>,
    serve: impl Fn(Incoming) -> AppResult<()> + Sync,
    mut accept: impl FnMut(&TcpListener) -> io::Result<(TcpStream, SocketAddr)>,
    mut new_worker: impl FnMut() -> io::Result<thread::Builder>,
) -> AppResult<()> {
    let merge = Mutex::new(());
    let serve = &serve;
    let settings = diagnostics::settings();
    thread::scope(|scope| {
        let mut workers = Vec::with_capacity(MAX_SESSIONS);
        let mut result = (|| -> AppResult<()> {
            while !stop.load(Relaxed) {
                while let Some(done) = workers.iter().position(ScopedJoinHandle::is_finished) {
                    workers.swap_remove(done).join().map_err(|_| Error::Panic)?;
                }
                let (socket, _) = match accept(&listener) {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::park_timeout(Duration::from_millis(25));
                        continue;
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::ConnectionAborted | io::ErrorKind::Interrupted
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                if workers.len() == MAX_SESSIONS {
                    status.lock().unwrap().refuse();
                    continue;
                }
                let accepted = Instant::now();
                let cancellation = Arc::clone(stop);
                let merge = &merge;
                workers.push(new_worker().and_then(|builder| {
                    builder.spawn_scoped(scope, move || {
                        let (result, diagnostic) =
                            diagnostics::capture(settings, Phase::TlsHandshake, || {
                                tls.accept_at(socket, accepted, Some(cancellation))
                            });
                        match result {
                            Ok(channel) => {
                                if let Ok(_guard) = merge.try_lock() {
                                    let (outcome, diagnostic) =
                                        diagnostics::capture(settings, Phase::Replication, || {
                                            serve(channel)
                                        });
                                    status.lock().unwrap().record(outcome, diagnostic);
                                } else {
                                    status.lock().unwrap().refuse();
                                }
                            }
                            Err(error) => status.lock().unwrap().record(Err(error), diagnostic),
                        }
                    })
                })?);
            }
            Ok(())
        })();
        // Close admission before joining workers. Pending TLS handshakes
        // observe the stop flag between bounded socket waits.
        drop(listener);
        stop.store(true, Relaxed);
        for worker in workers {
            if worker.join().is_err() && result.is_ok() {
                result = Err(Error::Panic.into());
            }
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::tls::Credentials;
    use std::{
        io::{Read, Write},
        sync::mpsc,
    };

    struct Stop<'a>(&'a AtomicBool);

    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            self.0.store(true, Relaxed);
        }
    }

    fn await_status(status: &Mutex<Status>, ready: impl Fn(&Status) -> bool) {
        let wait = Instant::now() + Duration::from_secs(2);
        while !ready(&status.lock().unwrap()) {
            assert!(Instant::now() < wait, "{}", status.lock().unwrap().report());
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn listener_retries_transient_errors_and_preserves_fatal_error() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let status = Mutex::new(Status::default());
        let stop = Arc::new(AtomicBool::new(false));
        let mut errors = [
            io::ErrorKind::WouldBlock,
            io::ErrorKind::Interrupted,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::PermissionDenied,
        ]
        .into_iter();
        let result = receive_with(
            listener,
            &server,
            &stop,
            &status,
            |_| panic!("a refused listener call must not serve a session"),
            |_| {
                Err(errors
                    .next()
                    .expect("listener continued after fatal error")
                    .into())
            },
            || Ok(thread::Builder::new()),
        );
        assert_eq!(
            result
                .unwrap_err()
                .downcast_ref::<io::Error>()
                .map(io::Error::kind),
            Some(io::ErrorKind::PermissionDenied)
        );
        assert!(errors.next().is_none());
        assert!(stop.load(Relaxed));
        assert_eq!(
            status.lock().unwrap().report(),
            "incoming served=0 failed=0 busy=0"
        );
    }

    #[test]
    fn receiver_stop_closes_a_queued_connection() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();

        // Stop before accept leaves this connection queued on this listener.
        receive(
            listener,
            &server,
            &Arc::new(AtomicBool::new(true)),
            &Mutex::new(Status::default()),
            |_| panic!("a stopped receiver must not serve a session"),
        )
        .unwrap();

        let mut byte = [0];
        match peer.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            result => panic!("stopped receiver left the queued connection open: {result:?}"),
        }
    }

    #[test]
    fn shutdown_joins_active_workers_and_preserves_the_first_failure() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();

        for (panic_worker, listener_error) in [(false, false), (true, false), (true, true)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let status = Mutex::new(Status::default());
            let stop = Arc::new(AtomicBool::new(false));
            let (entered, enter) = mpsc::channel();
            thread::scope(|scope| {
                let stop_worker = &stop;
                let status_worker = &status;
                let server = &server;
                let run = scope.spawn(move || {
                    let mut admitted = false;
                    receive_with(
                        listener,
                        server,
                        stop_worker,
                        status_worker,
                        |_| {
                            entered.send(())?;
                            let deadline = Instant::now() + Duration::from_secs(4);
                            while !stop_worker.load(Relaxed) {
                                assert!(Instant::now() < deadline, "worker did not observe stop");
                                thread::sleep(Duration::from_millis(5));
                            }
                            assert!(!panic_worker, "controlled worker panic");
                            Ok(())
                        },
                        |listener| {
                            if !admitted {
                                let accepted = listener.accept();
                                admitted = accepted.is_ok();
                                return accepted;
                            }
                            enter.recv_timeout(Duration::from_secs(4)).unwrap();
                            if listener_error {
                                Err(io::ErrorKind::PermissionDenied.into())
                            } else {
                                stop_worker.store(true, Relaxed);
                                Err(io::ErrorKind::WouldBlock.into())
                            }
                        },
                        || Ok(thread::Builder::new()),
                    )
                });
                let _shutdown = Stop(&stop);
                let connection = client.connect(address).unwrap();
                let result = run.join().unwrap();
                drop(connection);
                let status = status.lock().unwrap();
                assert_eq!(status.served, u64::from(!panic_worker));
                assert_eq!((status.failed, status.busy), (0, 0));
                assert!(stop.load(Relaxed));
                match (panic_worker, listener_error) {
                    (false, _) => result.unwrap(),
                    (true, false) => {
                        assert_eq!(
                            result.unwrap_err().downcast_ref::<Error>(),
                            Some(&Error::Panic)
                        );
                    }
                    (true, true) => {
                        assert_eq!(
                            result
                                .unwrap_err()
                                .downcast_ref::<io::Error>()
                                .map(io::Error::kind),
                            Some(io::ErrorKind::PermissionDenied)
                        );
                    }
                }
            });
        }
    }

    #[test]
    fn failed_worker_launch_closes_the_new_socket_and_joins_the_active_worker() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let status = Mutex::new(Status::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (entered, enter) = mpsc::channel();
        let (finished, finish) = mpsc::channel();
        thread::scope(|scope| {
            let run = scope.spawn(|| {
                let mut launches = 0;
                let result = receive_with(
                    listener,
                    &server,
                    &stop,
                    &status,
                    |_| {
                        entered.send(())?;
                        let deadline = Instant::now() + Duration::from_secs(4);
                        while !stop.load(Relaxed) {
                            assert!(Instant::now() < deadline, "worker did not observe stop");
                            thread::sleep(Duration::from_millis(5));
                        }
                        Ok(())
                    },
                    TcpListener::accept,
                    || {
                        launches += 1;
                        if launches == 1 {
                            Ok(thread::Builder::new())
                        } else {
                            Err(io::Error::new(
                                io::ErrorKind::WouldBlock,
                                "worker launch refused",
                            ))
                        }
                    },
                );
                finished.send((result, launches)).unwrap();
            });
            let _shutdown = Stop(&stop);
            let active = client.connect(address).unwrap();
            enter.recv_timeout(Duration::from_secs(4)).unwrap();
            let mut refused = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
            refused
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            let (result, launches) = finish.recv_timeout(Duration::from_secs(4)).unwrap();
            run.join().unwrap();
            assert_eq!(launches, 2);
            let error = result.unwrap_err();
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(io::ErrorKind::WouldBlock)
            );
            assert_eq!(error.to_string(), "worker launch refused");
            match refused.read(&mut [0]) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                    ) => {}
                result => panic!("refused session socket stayed open: {result:?}"),
            }
            drop(active);
            assert!(stop.load(Relaxed));
            assert_eq!(
                status.lock().unwrap().report(),
                "incoming served=1 failed=0 busy=0"
            );
        });
    }

    #[test]
    fn completed_worker_panic_is_propagated_by_the_listener() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let status = Mutex::new(Status::default());
        let stop = Arc::new(AtomicBool::new(false));

        thread::scope(|scope| {
            let run = scope.spawn(|| {
                receive(listener, &server, &stop, &status, |_| {
                    panic!("controlled worker panic");
                })
            });
            let _shutdown = Stop(&stop);
            let connection = client.connect(address).unwrap();
            let deadline = Instant::now() + Duration::from_secs(4);
            while !stop.load(Relaxed) {
                assert!(
                    Instant::now() < deadline,
                    "listener did not reap failed worker"
                );
                thread::sleep(Duration::from_millis(5));
            }
            let error = run.join().unwrap().unwrap_err();
            assert_eq!(error.downcast_ref::<Error>(), Some(&Error::Panic));
            drop(connection);
        });
    }

    #[test]
    fn stalled_sessions_are_bounded_and_the_authenticated_merge_is_serial() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let status = Mutex::new(Status::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (entered, enter) = mpsc::channel();
        thread::scope(|scope| {
            let run = scope.spawn(|| {
                receive(listener, &server, &stop, &status, |mut stream| {
                    entered.send(())?;
                    let mut command = [0];
                    stream.read_exact(&mut command)?;
                    stream.write_all(&command)?;
                    stream.flush()?;
                    Ok(())
                })
            });
            let _shutdown = Stop(&stop);
            let mut stalled: Vec<_> = (0..MAX_SESSIONS)
                .map(|_| TcpStream::connect(address).unwrap())
                .collect();
            let mut excess = TcpStream::connect(address).unwrap();
            excess
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            match excess.read(&mut [0]) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                    ) => {}
                result => panic!("excess connection stayed open: {result:?}"),
            }
            await_status(&status, |state| state.busy == 1 && state.failed == 0);
            drop(stalled.split_off(2));
            await_status(&status, |state| state.failed == 2);

            let mut legitimate = client.connect(address).unwrap();
            enter.recv_timeout(Duration::from_secs(2)).unwrap();
            // A second valid certificate reaches the busy merge gate.
            let second = client.connect(address).ok();
            await_status(&status, |state| state.busy == 2);
            assert!(
                enter.try_recv().is_err(),
                "second connection reached the merge"
            );
            drop(second);
            legitimate.write_all(&[42]).unwrap();
            legitimate.flush().unwrap();
            let mut reply = [0];
            legitimate.read_exact(&mut reply).unwrap();
            assert_eq!(reply, [42]);
            await_status(&status, |state| state.served == 1 && state.failed == 2);

            let stopping = Instant::now();
            stop.store(true, Relaxed);
            run.thread().unpark();
            run.join().unwrap().unwrap();
            assert!(stopping.elapsed() < Duration::from_secs(2));
            assert_eq!(status.lock().unwrap().failed, 4);
        });
    }
}
