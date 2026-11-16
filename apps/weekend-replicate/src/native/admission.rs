use super::{
    AppResult, SessionError,
    tls::{Incoming, Tls},
};
use std::{
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver},
    thread::{self, ScopedJoinHandle},
    time::{Duration, Instant},
};
use weekend_db::replication::Follower;

#[path = "../../../support/listener_ready.rs"]
mod listener_ready;

const MAX_HANDSHAKES: usize = 4;
const MAX_READY: usize = 4;
const WAIT_MS: i32 = 10;

struct Handshake<'scope> {
    socket: TcpStream,
    worker: ScopedJoinHandle<'scope, ()>,
}

fn reap(handshakes: &mut Vec<Handshake<'_>>) -> io::Result<()> {
    while let Some(done) = handshakes.iter().position(|h| h.worker.is_finished()) {
        handshakes
            .swap_remove(done)
            .worker
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
            if let Err(error) = follower.serve_authenticated(channel) {
                eprintln!("{}", SessionError::Replication(error));
            }
            // Stop before touching another channel if manifest publication failed.
            follower.progress()?;
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
    thread::scope(|scope| {
        let (ready, pending) = mpsc::sync_channel(MAX_READY);
        let follower = thread::Builder::new()
            .name("snapshot-follower".into())
            .spawn_scoped(scope, move || consume(pending))?;
        let mut handshakes = Vec::with_capacity(handshake_limit);
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
                let cancel = socket.try_clone()?;
                let ready = ready.clone();
                let worker = thread::Builder::new()
                    .name("snapshot-tls".into())
                    .spawn_scoped(scope, move || match tls.accept_at(socket, started) {
                        Ok(channel) => {
                            // An overloaded connection closes without a protocol reply.
                            let _ = ready.try_send(channel);
                        }
                        Err(error) => eprintln!("{}", SessionError::Authentication(error)),
                    })?;
                handshakes.push(Handshake {
                    socket: cancel,
                    worker,
                });
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
            for handshake in &handshakes {
                let _ = handshake.socket.shutdown(Shutdown::Both);
            }
        }
        for handshake in handshakes {
            if handshake.worker.join().is_err() && result.is_ok() {
                result = Err(io::Error::other(
                    "snapshot TLS worker panicked during shutdown",
                ));
            }
        }
        drop(ready);
        let serviced = follower
            .join()
            .map_err(|_| io::Error::other("snapshot follower worker panicked"))?;
        result?;
        serviced
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::tls::Credentials;
    use std::io::Read;
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
