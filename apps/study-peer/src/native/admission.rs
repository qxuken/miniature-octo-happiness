use super::{
    AppResult,
    tls::{Incoming, Tls},
};
use crate::diagnostics::{self, Diagnostic, Phase, Settings};
use std::{
    io,
    net::TcpListener,
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
                let (socket, _) = match listener.accept() {
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
                workers.push(thread::Builder::new().spawn_scoped(scope, move || {
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
        net::TcpStream,
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
