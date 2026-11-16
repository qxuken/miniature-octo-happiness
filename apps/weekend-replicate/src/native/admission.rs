use super::{
    AppResult,
    tls::{Incoming, Tls},
};
use crate::diagnostics::{self, Diagnostic, Phase};
use std::{
    io,
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc::{self, Receiver},
    },
    thread::{self, ScopedJoinHandle},
    time::{Duration, Instant},
};
use weekend_db::replication::Follower;

#[path = "../../../support/listener_ready.rs"]
mod listener_ready;

const MAX_HANDSHAKES: usize = 4;
const MAX_READY: usize = 4;
const WAIT_MS: i32 = 10;

fn reap(handshakes: &mut Vec<ScopedJoinHandle<'_, ()>>) -> io::Result<()> {
    while let Some(done) = handshakes.iter().position(ScopedJoinHandle::is_finished) {
        handshakes
            .swap_remove(done)
            .join()
            .map_err(|_| io::Error::other("snapshot TLS worker panicked"))?;
    }
    Ok(())
}

pub(super) fn serve(
    listener: TcpListener,
    mut follower: Follower,
    tls: &Tls,
    maximum: usize,
) -> AppResult<()> {
    admit(listener, tls, maximum, MAX_HANDSHAKES, |pending| {
        for channel in pending {
            diagnostics::storage(Phase::Replication, || {
                // Peer errors remain suppressed. The pure progress gate retains
                // any context from this same exchange only when recovery is needed.
                let _ = follower.serve_authenticated(channel);
                follower.progress().map(|_| ())
            })?;
        }
        Ok(())
    })
}

fn admit(
    listener: TcpListener,
    tls: &Tls,
    maximum: usize,
    handshake_limit: usize,
    consume: impl FnOnce(Receiver<Incoming>) -> AppResult<()> + Send,
) -> AppResult<()> {
    let settings = diagnostics::settings();
    thread::scope(|scope| {
        let (ready, pending) = mpsc::sync_channel(MAX_READY);
        let follower = thread::Builder::new()
            .name("snapshot-follower".into())
            .spawn_scoped(scope, move || {
                diagnostics::capture(settings, Phase::Replication, || consume(pending))
            })?;
        let mut handshakes = Vec::with_capacity(handshake_limit);
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut result = (|| -> io::Result<()> {
            let mut accepted = 0;
            while accepted < maximum && !follower.is_finished() {
                reap(&mut handshakes)?;
                match listener_ready::wait_for_connection(&listener, WAIT_MS) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
                if follower.is_finished() {
                    break;
                }
                let (socket, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let started = Instant::now();
                accepted += 1;
                if handshakes.len() == handshake_limit {
                    continue;
                }
                let ready = ready.clone();
                let cancel = cancellation.clone();
                let worker = thread::Builder::new()
                    .name("snapshot-tls".into())
                    .spawn_scoped(scope, move || {
                        if let Ok(channel) = tls.accept_at(socket, started, Some(cancel)) {
                            // An overloaded connection closes without a protocol reply.
                            let _ = ready.try_send(channel);
                        }
                    })?;
                handshakes.push(worker);
            }
            drop(listener);

            // A finite accept limit drains admitted work, unless the follower
            // stops first. Keep checking for that while handshakes finish.
            while !handshakes.is_empty() && !follower.is_finished() {
                reap(&mut handshakes)?;
                if !handshakes.is_empty() {
                    thread::sleep(Duration::from_millis(WAIT_MS as u64));
                }
            }
            Ok(())
        })();
        if result.is_err() || follower.is_finished() {
            cancellation.store(true, Relaxed);
        }
        for handshake in handshakes {
            if handshake.join().is_err() && result.is_ok() {
                result = Err(io::Error::other(
                    "snapshot TLS worker panicked during shutdown",
                ));
            }
        }
        drop(ready);
        finish(result, follower.join())
    })
}

fn finish(
    admission: io::Result<()>,
    worker: thread::Result<(AppResult<()>, Option<Diagnostic>)>,
) -> AppResult<()> {
    // Preserve the existing precedence: worker panic, admission, worker error.
    let (serviced, diagnostic) =
        worker.map_err(|_| io::Error::other("snapshot follower worker panicked"))?;
    admission?;
    diagnostics::propagate(serviced, diagnostic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::tls::Credentials;
    use std::{
        fs,
        io::{Read, Write},
        net::TcpStream,
        path::PathBuf,
    };
    use weekend_db::Error;

    fn listener() -> (TcpListener, std::net::SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        (listener, address)
    }

    fn pair() -> (Tls, Tls) {
        let client = Credentials::generate().unwrap();
        let server = Credentials::generate().unwrap();
        (
            Tls::new(&client, server.certificate()).unwrap(),
            Tls::new(&server, client.certificate()).unwrap(),
        )
    }

    fn settings(enabled: bool) -> diagnostics::Settings {
        let mut args = vec!["serve".into()];
        if enabled {
            args.insert(0, "--diagnostics=json".into());
        }
        diagnostics::settings_from_args(diagnostics::App::Snapshot, &mut args)
    }

    struct TruncatedImage(PathBuf);
    impl TruncatedImage {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let serial = NEXT.fetch_add(1, Relaxed);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "snapshot-carrier-PRIVATE_CANARY-{}-{nonce}-{serial}",
                std::process::id()
            ));
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .unwrap();
            let image = Self(path);
            file.write_all(b"PRIVATE_CANARY").unwrap();
            image
        }
    }
    impl Drop for TruncatedImage {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    fn inspect_truncated(image: &TruncatedImage) -> AppResult<()> {
        diagnostics::storage(Phase::StorageVerify, || {
            weekend_db::storage::inspect_file(&image.0, weekend_db::Limits::default()).map(|_| ())
        })?;
        Ok(())
    }

    // Tests the existing worker-result carrier with real inspection context;
    // the consumer is a fixture, not evidence of follower installation failure.
    #[test]
    fn worker_join_preserves_real_verification_context_and_original_error() {
        for enabled in [false, true] {
            let image = TruncatedImage::new();
            let config = settings(enabled);
            let (listener, address) = listener();
            let (client, server) = pair();
            let (finished, outcome) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(|| {
                    finished
                        .send(diagnostics::capture(config, Phase::Replication, || {
                            admit(listener, &server, 1, MAX_HANDSHAKES, |pending| {
                                let mut channel = pending.recv_timeout(Duration::from_secs(5))?;
                                channel.read_exact(&mut [0])?;
                                inspect_truncated(&image)
                            })
                        }))
                        .unwrap()
                });
                let mut channel = client.connect(address).unwrap();
                channel.write_all(&[0]).unwrap();
                channel.flush().unwrap();
                let (result, diagnostic) = outcome.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(
                    result.as_ref().err().unwrap().downcast_ref::<Error>(),
                    Some(&Error::Corrupt)
                );
                if enabled {
                    let json = diagnostic.unwrap().json(config).to_string();
                    assert!(json.len() < 1024 && !json.contains("PRIVATE_CANARY"));
                    assert!(json.contains(
                        "\"phase\":\"storage_verify\",\"category\":\"engine\",\"code\":\"Corrupt\""
                    ));
                    assert!(
                        json.contains(
                            "\"verification\":{\"stage\":\"image_length\",\"offset\":null}"
                        )
                    );
                } else {
                    assert!(diagnostic.is_none());
                }
            });
            assert_eq!(fs::read(&image.0).unwrap(), b"PRIVATE_CANARY");
        }
    }

    #[test]
    fn nonfatal_and_successful_channels_do_not_leak_context_to_a_later_failure() {
        let image = TruncatedImage::new();
        let config = settings(true);
        let (listener, address) = listener();
        let (client, server) = pair();
        let (finished, outcome) = mpsc::channel();
        thread::scope(|scope| {
            scope.spawn(|| {
                finished
                    .send(diagnostics::capture(config, Phase::Replication, || {
                        admit(listener, &server, 3, MAX_HANDSHAKES, |pending| {
                            for step in 0..3 {
                                let mut channel = pending.recv_timeout(Duration::from_secs(5))?;
                                channel.set_test_deadline(Instant::now() + Duration::from_secs(5));
                                let mut marker = [0];
                                channel.read_exact(&mut marker)?;
                                assert_eq!(marker[0], step);
                                diagnostics::storage(Phase::Replication, || {
                                    if step == 0 {
                                        // A failed peer operation with a healthy progress gate.
                                        assert_eq!(
                                            weekend_db::storage::inspect_file(
                                                &image.0,
                                                weekend_db::Limits::default()
                                            ),
                                            Err(Error::Corrupt)
                                        );
                                    }
                                    if step == 2 { Err(Error::Limit) } else { Ok(()) }
                                })?;
                                channel.write_all(&marker)?;
                                channel.flush()?;
                            }
                            Ok(())
                        })
                    }))
                    .unwrap()
            });
            let mut channels = Vec::new();
            for marker in 0..3 {
                let mut channel = client.connect(address).unwrap();
                channel.set_test_deadline(Instant::now() + Duration::from_secs(5));
                channel.write_all(&[marker]).unwrap();
                channel.flush().unwrap();
                if marker < 2 {
                    let mut acknowledged = [0];
                    channel.read_exact(&mut acknowledged).unwrap();
                    assert_eq!(acknowledged, [marker]);
                }
                channels.push(channel);
            }
            let (result, diagnostic) = outcome.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(
                result.as_ref().err().unwrap().downcast_ref::<Error>(),
                Some(&Error::Limit)
            );
            let json = diagnostic.unwrap().json(config).to_string();
            assert!(json.contains("\"code\":\"Limit\",\"native\":null,\"verification\":null"));
        });
    }

    #[test]
    fn admission_error_and_worker_panic_keep_their_existing_precedence() {
        for enabled in [false, true] {
            let image = TruncatedImage::new();
            let config = settings(enabled);
            let worker = thread::scope(|scope| {
                scope
                    .spawn(|| {
                        diagnostics::capture(config, Phase::Replication, || {
                            inspect_truncated(&image)
                        })
                    })
                    .join()
            });
            let (result, diagnostic) = diagnostics::capture(config, Phase::Replication, || {
                finish(Err(io::Error::from_raw_os_error(22)), worker)
            });
            assert_eq!(
                result
                    .as_ref()
                    .err()
                    .unwrap()
                    .downcast_ref::<io::Error>()
                    .unwrap()
                    .raw_os_error(),
                Some(22)
            );
            if enabled {
                let json = diagnostic.unwrap().json(config).to_string();
                assert!(json.contains("\"os_code\":22"));
                assert!(json.contains("\"verification\":null"));
                assert!(!json.contains("image_length") && !json.contains("Corrupt"));
            } else {
                assert!(diagnostic.is_none());
            }

            let (result, diagnostic) = diagnostics::capture(config, Phase::Replication, || {
                finish(
                    Err(io::Error::from_raw_os_error(22)),
                    Err(Box::new("PRIVATE_PANIC_CANARY")),
                )
            });
            let error = result
                .as_ref()
                .err()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert_eq!(error.to_string(), "snapshot follower worker panicked");
            if enabled {
                let json = diagnostic.unwrap().json(config).to_string();
                assert!(!json.contains("PRIVATE_PANIC_CANARY") && !json.contains("os_code\":22"));
                assert!(json.contains("\"native\":null,\"verification\":null"));
            } else {
                assert!(diagnostic.is_none());
            }
        }
    }

    fn assert_closed(mut socket: TcpStream) {
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        match socket.read(&mut [0]) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            result => panic!("expected a closed socket, got {result:?}"),
        }
    }

    #[test]
    fn excess_handshake_closes_while_admitted_sockets_are_stalled() {
        let (listener, address) = listener();
        let (_, server) = pair();
        thread::scope(|scope| {
            let run = scope.spawn(|| {
                admit(
                    listener,
                    &server,
                    MAX_HANDSHAKES + 1,
                    MAX_HANDSHAKES,
                    |pending| {
                        assert!(pending.into_iter().next().is_none());
                        Ok(())
                    },
                )
            });
            let stalled: Vec<_> = (0..MAX_HANDSHAKES)
                .map(|_| TcpStream::connect(address).unwrap())
                .collect();
            assert_closed(TcpStream::connect(address).unwrap());
            drop(stalled);
            run.join().unwrap().unwrap();
        });
    }

    #[test]
    fn follower_failure_cancels_stalled_handshakes() {
        for maximum in [2, usize::MAX] {
            let (listener, address) = listener();
            let (client, server) = pair();
            let (finished, result) = mpsc::channel();
            let (connected, fail) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(|| {
                    finished
                        .send(admit(
                            listener,
                            &server,
                            maximum,
                            MAX_HANDSHAKES,
                            move |pending| {
                                let _channel = pending.recv_timeout(Duration::from_secs(5))?;
                                fail.recv_timeout(Duration::from_secs(5))?;
                                Err(Error::RecoveryRequired.into())
                            },
                        ))
                        .unwrap();
                });
                let stalled = TcpStream::connect(address).unwrap();
                let _authenticated = client.connect(address).unwrap();
                connected.send(()).unwrap();
                let error = result
                    .recv_timeout(Duration::from_secs(5))
                    .expect("admission waited for a stalled handshake after follower failure")
                    .unwrap_err();
                assert_eq!(
                    error.downcast_ref::<Error>(),
                    Some(&Error::RecoveryRequired)
                );
                assert_closed(stalled);
            });
        }
    }

    #[test]
    fn excess_authenticated_channel_closes_when_the_ready_queue_is_full() {
        let (listener, address) = listener();
        let (client, server) = pair();
        let (release, blocked) = mpsc::channel();
        thread::scope(|scope| {
            let attempts = MAX_READY + 1;
            let run = scope.spawn(move || {
                // Leave room for every handshake so saturation can occur
                // only at the authenticated queue.
                admit(listener, &server, attempts, attempts, move |pending| {
                    blocked.recv_timeout(Duration::from_secs(8))?;
                    assert_eq!(pending.try_iter().count(), MAX_READY);
                    Ok(())
                })
            });
            let mut connected = Vec::new();
            let mut closed_on_connect = 0;
            for _ in 0..attempts {
                match client.connect(address) {
                    Ok(channel) => connected.push(channel),
                    // The server may close immediately after accepting a
                    // handshake, before the client observes it as complete.
                    Err(_) => closed_on_connect += 1,
                }
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            let closed = thread::scope(|probes| {
                connected
                    .into_iter()
                    .map(|mut channel| {
                        probes.spawn(move || {
                            channel.set_test_deadline(deadline);
                            match channel.read(&mut [0]) {
                                Ok(0) => true,
                                Err(error) => match error.kind() {
                                    io::ErrorKind::UnexpectedEof
                                    | io::ErrorKind::ConnectionReset
                                    | io::ErrorKind::ConnectionAborted => true,
                                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => false,
                                    _ => panic!("unexpected probe error: {error}"),
                                },
                                result => panic!("unexpected probe result: {result:?}"),
                            }
                        })
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|probe| probe.join().unwrap())
                    .filter(|closed| *closed)
                    .count()
            });
            assert_eq!(closed_on_connect + closed, 1);
            release.send(()).unwrap();
            run.join().unwrap().unwrap();
        });
    }
}
