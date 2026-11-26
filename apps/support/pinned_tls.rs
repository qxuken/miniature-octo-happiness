use super::{ALPN, ALPN_ERROR, AppResult, SERVER_NAME, TIMEOUT};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, ServerConfig, ServerConnection, StreamOwned,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
    server::WebPkiClientVerifier,
};
use std::{
    fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    time::{Duration, Instant},
};

pub(crate) struct Credentials {
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

impl Credentials {
    #[cfg(any(unix, test))]
    pub(crate) fn generate() -> AppResult<Self> {
        Self::from_generated(rcgen::generate_simple_self_signed([SERVER_NAME.to_owned()]))
    }

    #[cfg(any(unix, test))]
    fn from_generated(
        generated: Result<rcgen::CertifiedKey<rcgen::KeyPair>, rcgen::Error>,
    ) -> AppResult<Self> {
        let rcgen::CertifiedKey { cert, signing_key } = generated?;
        Ok(Self {
            cert: cert.der().clone(),
            key: PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        })
    }

    #[cfg(unix)]
    pub(crate) fn create(dir: &Path) -> AppResult<()> {
        Self::create_with(
            dir,
            Self::generate,
            |options, path| options.open(path),
            |path, bytes| fs::write(path, bytes),
        )
    }

    #[cfg(unix)]
    fn create_with<W: Write>(
        dir: &Path,
        generate: impl FnOnce() -> AppResult<Self>,
        open_key: impl FnOnce(&fs::OpenOptions, &Path) -> io::Result<W>,
        write_cert: impl FnOnce(&Path, &[u8]) -> io::Result<()>,
    ) -> AppResult<()> {
        let creds = generate()?;
        let mut key = fs::OpenOptions::new();
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        fs::DirBuilder::new().mode(0o700).create(dir)?;
        key.mode(0o600);
        key.write(true).create_new(true);
        open_key(&key, &dir.join("key.der"))?.write_all(creds.key.secret_der())?;
        write_cert(&dir.join("cert.der"), creds.cert.as_ref())?;
        Ok(())
    }

    #[cfg(not(unix))]
    pub(crate) fn create(_: &Path) -> AppResult<()> {
        Err("keygen requires Unix private-file modes; provision PKCS#8 key.der and cert.der in an ACL-protected directory on Windows".into())
    }

    pub(crate) fn load(dir: &Path) -> AppResult<Self> {
        Ok(Self {
            cert: bounded_file(&dir.join("cert.der"), 65536)?.into(),
            key: PrivatePkcs8KeyDer::from(bounded(&dir.join("key.der"), 16384, true)?).into(),
        })
    }
}

pub(crate) struct Tls {
    client: Arc<ClientConfig>,
    server: Arc<ServerConfig>,
    peer: CertificateDer<'static>,
}

pub(crate) type Incoming = DeadlineStream<ServerConnection>;

impl Tls {
    pub(crate) fn new(local: &Credentials, peer: CertificateDer<'static>) -> AppResult<Self> {
        Self::configure(local, peer, |verifier, local| {
            ServerConfig::builder()
                .with_client_cert_verifier(verifier)
                .with_single_cert(vec![local.cert.clone()], local.key.clone_key())
        })
    }

    fn configure(
        local: &Credentials,
        peer: CertificateDer<'static>,
        configure_server: impl FnOnce(
            Arc<dyn rustls::server::danger::ClientCertVerifier>,
            &Credentials,
        ) -> Result<ServerConfig, rustls::Error>,
    ) -> AppResult<Self> {
        if peer == local.cert {
            return Err("configure the other peer's certificate".into());
        }
        let mut roots = RootCertStore::empty();
        roots.add(peer.clone())?;
        let mut client = ClientConfig::builder()
            .with_root_certificates(roots.clone())
            .with_client_auth_cert(vec![local.cert.clone()], local.key.clone_key())?;
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
        let mut server = configure_server(verifier, local)?;
        client.alpn_protocols = vec![ALPN.to_vec()];
        server.alpn_protocols = vec![ALPN.to_vec()];
        Ok(Self {
            client: Arc::new(client),
            server: Arc::new(server),
            peer,
        })
    }

    pub(crate) fn connect(
        &self,
        address: SocketAddr,
    ) -> AppResult<DeadlineStream<ClientConnection>> {
        let deadline = Instant::now() + TIMEOUT;
        let stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
        self.client_session(DeadlineSocket {
            stream,
            deadline,
            cancellation: None,
        })
    }

    fn client_session(
        &self,
        socket: DeadlineSocket,
    ) -> AppResult<DeadlineStream<ClientConnection>> {
        self.client_session_with(socket, DeadlineStream::new)
    }

    fn client_session_with(
        &self,
        mut socket: DeadlineSocket,
        finish: impl FnOnce(
            StreamOwned<ClientConnection, DeadlineSocket>,
        ) -> io::Result<DeadlineStream<ClientConnection>>,
    ) -> AppResult<DeadlineStream<ClientConnection>> {
        let name = ServerName::try_from(SERVER_NAME)?;
        let mut tls = ClientConnection::new(self.client.clone(), name)?;
        self.client_handshake(&mut tls, &mut socket)?;
        Ok(finish(StreamOwned::new(tls, socket))?)
    }

    fn client_handshake<T: Read + Write>(
        &self,
        tls: &mut ClientConnection,
        socket: &mut T,
    ) -> AppResult<()> {
        tls.complete_io(socket)?;
        // complete_io may return Ok after partial progress followed by WouldBlock.
        // A parsed certificate/ALPN alone is not proof of a completed handshake.
        if tls.is_handshaking() || tls.wants_write() {
            return Err("TLS handshake incomplete".into());
        }
        self.check_pin(tls.peer_certificates())?;
        if tls.alpn_protocol() != Some(ALPN) {
            return Err(ALPN_ERROR.into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn accept(&self, socket: TcpStream) -> AppResult<Incoming> {
        self.accept_at(socket, Instant::now(), None)
    }

    pub(crate) fn accept_at(
        &self,
        socket: TcpStream,
        accepted: Instant,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> AppResult<Incoming> {
        self.accept_at_with(socket, accepted, cancellation, TcpStream::set_nonblocking)
    }

    fn accept_at_with(
        &self,
        socket: TcpStream,
        accepted: Instant,
        cancellation: Option<Arc<AtomicBool>>,
        set_nonblocking: impl FnOnce(&TcpStream, bool) -> io::Result<()>,
    ) -> AppResult<Incoming> {
        let deadline = accepted + TIMEOUT;
        set_nonblocking(&socket, false)?;
        self.server_session(DeadlineSocket {
            stream: socket,
            deadline,
            cancellation,
        })
    }

    fn server_session(&self, socket: DeadlineSocket) -> AppResult<Incoming> {
        self.server_session_with(socket, DeadlineStream::new)
    }

    fn server_session_with(
        &self,
        mut socket: DeadlineSocket,
        finish: impl FnOnce(StreamOwned<ServerConnection, DeadlineSocket>) -> io::Result<Incoming>,
    ) -> AppResult<Incoming> {
        let mut tls = ServerConnection::new(self.server.clone())?;
        self.server_handshake(&mut tls, &mut socket)?;
        socket.cancellation = None;
        Ok(finish(StreamOwned::new(tls, socket))?)
    }

    fn server_handshake<T: Read + Write>(
        &self,
        tls: &mut ServerConnection,
        socket: &mut T,
    ) -> AppResult<()> {
        tls.complete_io(socket)?;
        // complete_io may return Ok after partial progress followed by WouldBlock.
        // A parsed certificate/ALPN alone is not proof of a completed handshake.
        if tls.is_handshaking() || tls.wants_write() {
            return Err("TLS handshake incomplete".into());
        }
        self.check_pin(tls.peer_certificates())?;
        if tls.alpn_protocol() != Some(ALPN) {
            return Err(ALPN_ERROR.into());
        }
        Ok(())
    }

    fn check_pin(&self, chain: Option<&[CertificateDer<'_>]>) -> AppResult<()> {
        if chain.and_then(|certs| certs.first()) == Some(&self.peer) {
            Ok(())
        } else {
            Err("peer certificate does not match the configured pin".into())
        }
    }
}

pub(crate) struct DeadlineStream<C>(StreamOwned<C, DeadlineSocket>);

impl<C> DeadlineStream<C> {
    fn new(stream: StreamOwned<C, DeadlineSocket>) -> io::Result<Self> {
        stream.sock.remaining()?;
        Ok(Self(stream))
    }

    #[cfg(test)]
    pub(crate) fn set_test_deadline(&mut self, deadline: Instant) {
        self.0.sock.deadline = deadline;
    }
}

// rustls can serve buffered plaintext or accept more plaintext without
// reaching the socket. Apply the same cutoff before each application call.
impl<C> Read for DeadlineStream<C>
where
    StreamOwned<C, DeadlineSocket>: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.sock.remaining()?;
        self.0.read(buf)
    }
}

impl<C> Write for DeadlineStream<C>
where
    StreamOwned<C, DeadlineSocket>: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.sock.remaining()?;
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.sock.remaining()?;
        self.0.flush()
    }
}

struct DeadlineSocket {
    stream: TcpStream,
    deadline: Instant,
    cancellation: Option<Arc<AtomicBool>>,
}

const CANCEL_POLL: Duration = Duration::from_millis(100);

impl DeadlineSocket {
    #[cfg(test)]
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            deadline: Instant::now() + TIMEOUT,
            cancellation: None,
        }
    }

    fn remaining(&self) -> io::Result<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            Err(io::ErrorKind::TimedOut.into())
        } else {
            Ok(remaining)
        }
    }

    fn io(
        &mut self,
        mut operation: impl FnMut(&mut TcpStream, Duration) -> io::Result<usize>,
    ) -> io::Result<usize> {
        loop {
            let mut remaining = self.remaining()?;
            if let Some(canceled) = &self.cancellation {
                if canceled.load(Relaxed) {
                    return Err(io::ErrorKind::ConnectionAborted.into());
                }
                remaining = remaining.min(CANCEL_POLL);
            }
            match operation(&mut self.stream, remaining) {
                Err(error)
                    if self.cancellation.is_some()
                        && matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) => {}
                result => return result,
            }
        }
    }
}

impl Read for DeadlineSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.io(|stream, remaining| {
            socket_io_with_timeout(stream, remaining, TcpStream::set_read_timeout, |stream| {
                stream.read(buf)
            })
        })
    }
}

impl Write for DeadlineSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.io(|stream, remaining| {
            socket_io_with_timeout(stream, remaining, TcpStream::set_write_timeout, |stream| {
                stream.write(buf)
            })
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn socket_io_with_timeout(
    stream: &mut TcpStream,
    remaining: Duration,
    set_timeout: impl FnOnce(&TcpStream, Option<Duration>) -> io::Result<()>,
    operation: impl FnOnce(&mut TcpStream) -> io::Result<usize>,
) -> io::Result<usize> {
    set_timeout(stream, Some(remaining))?;
    operation(stream)
}

pub(crate) fn bounded_file(path: &Path, limit: usize) -> AppResult<Vec<u8>> {
    bounded(path, limit, false)
}
fn bounded(path: &Path, limit: usize, private: bool) -> AppResult<Vec<u8>> {
    bounded_with(
        path,
        limit,
        private,
        |path| fs::File::open(path),
        fs::File::metadata,
    )
}

fn bounded_with<R: Read>(
    path: &Path,
    limit: usize,
    private: bool,
    open: impl FnOnce(&Path) -> io::Result<R>,
    opened_metadata: impl FnOnce(&R) -> io::Result<fs::Metadata>,
) -> AppResult<Vec<u8>> {
    // Configuration lives in a trusted directory. Reject special files before
    // opening (notably blocking FIFOs), then recheck the opened handle.
    if !fs::metadata(path)?.is_file() {
        return Err("credential must be a regular file".into());
    }
    let file = open(path)?;
    let metadata = opened_metadata(&file)?;
    if !metadata.is_file() {
        return Err("credential must be a regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if private && metadata.permissions().mode() & 0o077 != 0 {
            return Err("private key must have owner-only permissions".into());
        }
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > limit {
        return Err("credential size outside bounds".into());
    }
    Ok(bytes)
}

#[cfg(test)]
impl Credentials {
    pub(crate) fn certificate(&self) -> CertificateDer<'static> {
        self.cert.clone()
    }
    pub(crate) fn write_fixture(&self, dir: &Path) {
        fs::create_dir(dir).unwrap();
        fs::write(dir.join("cert.der"), self.cert.as_ref()).unwrap();
        let mut key = fs::OpenOptions::new();
        key.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            key.mode(0o600);
        }
        key.open(dir.join("key.der"))
            .unwrap()
            .write_all(self.key.secret_der())
            .unwrap();
    }
}
#[cfg(test)]
impl Tls {
    pub(crate) fn test_alpn(&mut self, protocols: Vec<Vec<u8>>) {
        Arc::make_mut(&mut self.client).alpn_protocols = protocols;
    }
}

#[cfg(test)]
mod deadline_tests {
    use super::*;

    fn connected_sockets() -> (TcpStream, TcpStream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        (stream, peer)
    }

    #[test]
    fn session_construction_preserves_the_rustls_configuration_error() {
        let local = Credentials::generate().unwrap();
        let peer = Credentials::generate().unwrap();
        let mut tls = Tls::new(&local, peer.certificate()).unwrap();
        Arc::make_mut(&mut tls.client).max_fragment_size = Some(1);
        Arc::make_mut(&mut tls.server).max_fragment_size = Some(1);

        for client in [true, false] {
            let (stream, _peer) = connected_sockets();
            let mut socket = DeadlineSocket::new(stream);
            socket.deadline = Instant::now();
            let result = if client {
                tls.client_session(socket).map(drop)
            } else {
                tls.server_session(socket).map(drop)
            };
            let error = result.unwrap_err();
            assert_eq!(
                error.downcast_ref::<rustls::Error>(),
                Some(&rustls::Error::BadMaxFragmentSize),
                "client={client}"
            );
        }
    }

    #[test]
    fn session_handshake_preserves_an_expired_deadline_without_writing() {
        let local = Credentials::generate().unwrap();
        let peer = Credentials::generate().unwrap();
        let tls = Tls::new(&local, peer.certificate()).unwrap();

        for client in [true, false] {
            let (stream, mut peer) = connected_sockets();
            let mut socket = DeadlineSocket::new(stream);
            socket.deadline = Instant::now();
            let result = if client {
                tls.client_session(socket).map(drop)
            } else {
                tls.server_session(socket).map(drop)
            };
            assert_eq!(
                result
                    .unwrap_err()
                    .downcast_ref::<io::Error>()
                    .map(io::Error::kind),
                Some(io::ErrorKind::TimedOut),
                "client={client}"
            );
            peer.set_read_timeout(Some(TIMEOUT)).unwrap();
            assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0, "client={client}");
        }
    }

    #[test]
    fn both_finished_sessions_preserve_the_post_handshake_deadline() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();

        let (left, right) = connected_sockets();
        std::thread::scope(|scope| {
            let peer = scope.spawn(|| server.server_session(DeadlineSocket::new(right)).unwrap());
            let ending = client.client_session_with(DeadlineSocket::new(left), |mut stream| {
                let _peer = peer.join().unwrap();
                assert!(!stream.conn.is_handshaking());
                assert_eq!(stream.conn.alpn_protocol(), Some(ALPN));
                stream.sock.deadline = Instant::now();
                DeadlineStream::new(stream)
            });
            let error = ending.err().expect("expired client session was admitted");
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(io::ErrorKind::TimedOut)
            );
        });

        let (left, right) = connected_sockets();
        std::thread::scope(|scope| {
            let peer = scope.spawn(|| client.client_session(DeadlineSocket::new(left)).unwrap());
            let mut socket = DeadlineSocket::new(right);
            socket.cancellation = Some(Arc::new(AtomicBool::new(false)));
            let ending = server.server_session_with(socket, |mut stream| {
                let _peer = peer.join().unwrap();
                assert!(!stream.conn.is_handshaking());
                assert_eq!(stream.conn.alpn_protocol(), Some(ALPN));
                assert!(stream.sock.cancellation.is_none());
                stream.sock.deadline = Instant::now();
                DeadlineStream::new(stream)
            });
            let error = ending.err().expect("expired server session was admitted");
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(io::ErrorKind::TimedOut)
            );
        });
    }

    #[test]
    fn transport_setup_preserves_native_errors_before_starting_io() {
        let local = Credentials::generate().unwrap();
        let peer = Credentials::generate().unwrap();
        let tls = Tls::new(&local, peer.certificate()).unwrap();
        let (stream, mut peer) = connected_sockets();
        let local_address = stream.local_addr().unwrap();
        let error = tls
            .accept_at_with(stream, Instant::now(), None, |socket, nonblocking| {
                assert_eq!(socket.local_addr().unwrap(), local_address);
                assert!(!nonblocking);
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "fixture blocking mode",
                ))
            })
            .err()
            .expect("accept ignored a blocking mode error");
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(io::ErrorKind::PermissionDenied)
        );
        assert_eq!(error.to_string(), "fixture blocking mode");
        peer.set_read_timeout(Some(TIMEOUT)).unwrap();
        assert_eq!(peer.read(&mut [0; 1]).unwrap(), 0);

        let (mut stream, _peer) = connected_sockets();
        let address = stream.local_addr().unwrap();
        let remaining = Duration::from_millis(25);
        let error = socket_io_with_timeout(
            &mut stream,
            remaining,
            |socket, limit| {
                assert_eq!(socket.local_addr().unwrap(), address);
                assert_eq!(limit, Some(remaining));
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "fixture timeout",
                ))
            },
            |_| panic!("I/O started after timeout setup failed"),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "fixture timeout");
        assert_eq!(
            socket_io_with_timeout(
                &mut stream,
                remaining,
                |_, limit| {
                    assert_eq!(limit, Some(remaining));
                    Ok(())
                },
                |socket| {
                    assert_eq!(socket.local_addr().unwrap(), address);
                    Ok(7)
                },
            )
            .unwrap(),
            7
        );
    }

    #[test]
    fn exact_leaf_pin_controls_are_distinct_from_chain_trust() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let tls = Tls::new(&a, b.certificate()).unwrap();
        assert!(tls.check_pin(None).is_err());
        assert!(tls.check_pin(Some(&[])).is_err());
        assert!(tls.check_pin(Some(&[a.certificate()])).is_err());
        assert!(tls.check_pin(Some(&[b.certificate()])).is_ok());
        assert!(
            tls.check_pin(Some(&[a.certificate(), b.certificate()]))
                .is_err()
        );
        assert!(
            tls.check_pin(Some(&[b.certificate(), a.certificate()]))
                .is_ok()
        );
    }
    #[test]
    fn absolute_deadline_is_not_renewed_by_progress() {
        let (stream, mut peer) = connected_sockets();
        let mut socket = DeadlineSocket::new(stream);
        let deadline = socket.deadline;
        assert!(socket.remaining().is_ok());
        peer.write_all(&[42]).unwrap();
        let mut byte = [0];
        socket.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [42]);
        assert_eq!(socket.deadline, deadline);
        // Explicit clock control proves admission before socket I/O, not an OS
        // timeout or a bound on filesystem verification/flush duration.
        socket.deadline = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            socket.remaining().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            socket.read(&mut byte).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            socket.write(&byte).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn stream_construction_preserves_an_expired_socket_deadline() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let server = Tls::new(&a, b.certificate()).unwrap();
        for expired in [false, true] {
            let (stream, _peer) = connected_sockets();
            let mut socket = DeadlineSocket::new(stream);
            if expired {
                socket.deadline = Instant::now();
            }
            let deadline = socket.deadline;
            let connection = ServerConnection::new(server.server.clone()).unwrap();
            let result = DeadlineStream::new(StreamOwned::new(connection, socket));
            if expired {
                assert_eq!(
                    result.err().expect("expired socket was admitted").kind(),
                    io::ErrorKind::TimedOut
                );
            } else {
                assert_eq!(result.unwrap().0.sock.deadline, deadline);
            }
        }
    }

    #[test]
    fn socket_retries_only_slice_expiry_and_checks_cancel_before_reentry() {
        let (stream, _peer) = connected_sockets();
        let mut socket = DeadlineSocket::new(stream);
        socket.deadline = Instant::now() + Duration::from_secs(30);
        let original_deadline = socket.deadline;
        let cancel = Arc::new(AtomicBool::new(false));

        for kind in [io::ErrorKind::WouldBlock, io::ErrorKind::TimedOut] {
            let mut called = false;
            let error = socket
                .io(|_, wait| {
                    assert!(!called, "uncancelable socket retried a timeout");
                    called = true;
                    assert!(
                        wait > CANCEL_POLL,
                        "uncancelable socket used a polling slice"
                    );
                    Err(io::Error::new(kind, "original socket timeout"))
                })
                .unwrap_err();
            assert!(called);
            assert_eq!(error.kind(), kind);
            assert_eq!(error.to_string(), "original socket timeout");
        }

        socket.cancellation = Some(cancel.clone());
        let mut steps = [io::ErrorKind::WouldBlock, io::ErrorKind::TimedOut].into_iter();
        let count = socket.io(|_, wait| {
            assert_eq!(
                wait, CANCEL_POLL,
                "cancelable socket did not use a polling slice"
            );
            match steps.next() {
                Some(kind) => Err(kind.into()),
                None => Ok(7),
            }
        });
        assert_eq!(
            count.ok(),
            Some(7),
            "slice expiry did not resume the operation"
        );
        assert_eq!(socket.deadline, original_deadline);

        let mut called = false;
        let error = socket
            .io(|_, _| {
                assert!(!called, "cancelable socket retried a terminal error");
                called = true;
                Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "original reset",
                ))
            })
            .unwrap_err();
        assert!(called);
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        assert_eq!(error.to_string(), "original reset");

        let mut called = false;
        let error = socket
            .io(|_, _| {
                assert!(!called, "canceled socket began another operation");
                called = true;
                cancel.store(true, Relaxed);
                Err(io::ErrorKind::TimedOut.into())
            })
            .unwrap_err();
        assert!(called);
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);

        socket.deadline = Instant::now();
        let error = socket
            .io(|_, _| panic!("expired socket began an operation"))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn expired_tls_stream_rejects_buffered_reads_and_new_output() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::scope(|scope| {
            let receiver = scope.spawn(|| {
                let mut stream = server.accept(listener.accept().unwrap().0).unwrap();
                let mut first = [0];
                stream.read_exact(&mut first).unwrap();
                assert_eq!(first, [42]);
                assert_eq!(stream.0.conn.reader().into_first_chunk().unwrap(), &[43]);
                assert!(!stream.0.conn.wants_write());
                stream.set_test_deadline(Instant::now());
                let mut remaining = [0];
                for result in [
                    stream.read(&mut remaining).map(|_| ()),
                    stream.flush(),
                    stream.write(&[44]).map(|_| ()),
                ] {
                    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
                }
                assert_eq!(remaining, [0]);
                assert_eq!(stream.0.conn.reader().into_first_chunk().unwrap(), &[43]);
                assert!(!stream.0.conn.wants_write());
            });
            let mut stream = client.connect(address).unwrap();
            stream.write_all(&[42, 43]).unwrap();
            stream.flush().unwrap();
            receiver.join().unwrap();
        });
    }

    #[test]
    fn pending_handshake_waits_for_cancellation_across_io_time_slices() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let server = Tls::new(&a, b.certificate()).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let cancellation = Arc::new(AtomicBool::new(false));
        let (accepted, started) = std::sync::mpsc::channel();
        let (finished, result) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let cancel = cancellation.clone();
            scope.spawn(move || {
                let socket = listener.accept().unwrap().0;
                let now = Instant::now();
                accepted.send(()).unwrap();
                let error = server
                    .accept_at(socket, now, Some(cancel))
                    .err()
                    .expect("idle handshake unexpectedly succeeded");
                finished.send(error).unwrap();
            });
            let _peer = TcpStream::connect(address).unwrap();
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(matches!(
                result.recv_timeout(CANCEL_POLL * 3),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ));
            cancellation.store(true, Relaxed);
            let error = result.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(io::ErrorKind::ConnectionAborted)
            );
        });
    }
}

#[cfg(test)]
mod partial_handshake_tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn exact_pin_and_alpn_without_finished_never_admit_a_session() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let mut client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        Arc::make_mut(&mut client.client).max_fragment_size = Some(64);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::scope(|scope| {
            let receiver = scope.spawn(|| {
                let (stream, _) = listener.accept().unwrap();
                let result = server.server_session(DeadlineSocket {
                    stream,
                    deadline: Instant::now() + Duration::from_secs(1),
                    cancellation: None,
                });
                assert!(result.is_err());
                // On Unix, WouldBlock after partial progress makes complete_io
                // return Ok with the exact pin/ALPN already parsed. This must
                // be rejected by the explicit unfinished-handshake guard.
                #[cfg(unix)]
                assert_eq!(
                    result.err().unwrap().to_string(),
                    "TLS handshake incomplete"
                );
            });
            let mut socket = TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut tls = ClientConnection::new(
                client.client.clone(),
                ServerName::try_from(SERVER_NAME).unwrap(),
            )
            .unwrap();
            while tls.wants_write() {
                tls.write_tls(&mut socket).unwrap();
            }
            while tls.is_handshaking() {
                let mut header = [0; 5];
                socket.read_exact(&mut header).unwrap();
                let len = u16::from_be_bytes(header[3..5].try_into().unwrap()) as usize;
                let mut record = vec![0; len + 5];
                record[..5].copy_from_slice(&header);
                socket.read_exact(&mut record[5..]).unwrap();
                let mut record = Cursor::new(record);
                while record.position() < record.get_ref().len() as u64 {
                    tls.read_tls(&mut record).unwrap();
                    tls.process_new_packets().unwrap();
                }
            }
            let mut flight = Vec::new();
            while tls.wants_write() {
                tls.write_tls(&mut flight).unwrap();
            }
            let mut offsets = Vec::new();
            let mut offset = 0;
            while offset < flight.len() {
                offsets.push(offset);
                offset += 5
                    + u16::from_be_bytes(flight[offset + 3..offset + 5].try_into().unwrap())
                        as usize;
            }
            assert!(offsets.len() > 1);
            socket
                .write_all(&flight[..*offsets.last().unwrap()])
                .unwrap();
            // Send Certificate/CertificateVerify, withhold the final Finished.
            receiver.join().unwrap();
        });
    }
}

#[cfg(test)]
impl Credentials {
    pub(crate) fn anchor_and_signed_alternate() -> (Self, Self) {
        let params = rcgen::CertificateParams::new(vec![SERVER_NAME.to_owned()]).unwrap();
        let anchor_key = rcgen::KeyPair::generate().unwrap();
        let anchor = params.self_signed(&anchor_key).unwrap();
        let alternate_key = rcgen::KeyPair::generate().unwrap();
        let alternate = rcgen::CertificateParams::new(vec![SERVER_NAME.to_owned()])
            .unwrap()
            .signed_by(
                &alternate_key,
                &rcgen::Issuer::from_params(&params, &anchor_key),
            )
            .unwrap();
        (
            Self {
                cert: anchor.der().clone(),
                key: PrivatePkcs8KeyDer::from(anchor_key.serialize_der()).into(),
            },
            Self {
                cert: alternate.der().clone(),
                key: PrivatePkcs8KeyDer::from(alternate_key.serialize_der()).into(),
            },
        )
    }
}

#[cfg(test)]
mod audit_admission_tests {
    use super::*;
    #[test]
    fn self_pin_empty_file_and_public_private_mode_controls() {
        let a = Credentials::generate().unwrap();
        assert!(Tls::new(&a, a.certificate()).is_err());
        let path =
            std::env::temp_dir().join(format!("snapshot-credential-audit-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let _cleanup = Cleanup(&path);
        let file = path.join("value");
        let missing = bounded_file(&file, 64).unwrap_err();
        assert_eq!(
            missing.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            bounded_file(&path, 64).unwrap_err().to_string(),
            "credential must be a regular file"
        );
        for bytes in [&[][..], &[42; 65]] {
            fs::write(&file, bytes).unwrap();
            assert_eq!(
                bounded_file(&file, 64).unwrap_err().to_string(),
                "credential size outside bounds"
            );
        }
        fs::write(&file, [42; 64]).unwrap();
        assert_eq!(bounded_file(&file, 64).unwrap(), [42; 64]);
        fs::write(&file, b"public").unwrap();
        assert_eq!(bounded_file(&file, 64).unwrap(), b"public");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(bounded(&file, 64, false).unwrap(), b"public");
            assert_eq!(
                bounded(&file, 64, true).unwrap_err().to_string(),
                "private key must have owner-only permissions"
            );
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(bounded(&file, 64, true).unwrap(), b"public");
        }
    }

    struct Cleanup<'a>(&'a Path);

    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.0);
        }
    }
}

#[cfg(test)]
mod study_legacy_tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn sessions_require_the_configured_cert_on_both_ends() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let unknown = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.cert.clone()).unwrap();
        let server = Tls::new(&b, a.cert.clone()).unwrap();
        let wrong_server_pin = Tls::new(&a, unknown.cert.clone()).unwrap();
        let unknown_client = Tls::new(&unknown, b.cert.clone()).unwrap();

        for (client, success) in [
            (&client, true),
            (&wrong_server_pin, false),
            (&unknown_client, false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (sent, received) = std::thread::scope(|scope| {
                let served = scope.spawn(|| -> AppResult<()> {
                    let mut stream = server.accept(listener.accept()?.0)?;
                    let mut offer = [0];
                    stream.read_exact(&mut offer)?;
                    assert_eq!(offer, [42]);
                    stream.write_all(&[43])?;
                    stream.flush()?;
                    Ok(())
                });
                let reply = (|| -> AppResult<()> {
                    let mut stream = client.connect(address)?;
                    stream.write_all(&[42])?;
                    stream.flush()?;
                    let mut reply = [0];
                    stream.read_exact(&mut reply)?;
                    assert_eq!(reply, [43]);
                    Ok(())
                })();
                (reply, served.join().unwrap())
            });
            assert_eq!(sent.is_ok(), success, "{sent:?}");
            assert_eq!(received.is_ok(), success, "{received:?}");
        }
    }
}

#[cfg(test)]
mod study_legacy_decisions {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn pin_admission_and_expired_absolute_deadline() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        assert!(Tls::new(&a, a.cert.clone()).is_err());
        assert!(Tls::new(&a, vec![0; 4].into()).is_err());
        let tls = Tls::new(&a, b.cert.clone()).unwrap();
        assert!(tls.check_pin(None).is_err());
        assert!(tls.check_pin(Some(&[])).is_err());
        assert!(tls.check_pin(Some(std::slice::from_ref(&a.cert))).is_err());
        assert!(tls.check_pin(Some(std::slice::from_ref(&b.cert))).is_ok());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut other, _) = listener.accept().unwrap();
        let mut socket = DeadlineSocket::new(socket);
        assert!(socket.remaining().unwrap() > Duration::ZERO);
        other.write_all(&[42]).unwrap();
        let mut b = [0];
        assert_eq!(socket.read(&mut b).unwrap(), 1);
        assert_eq!(b, [42]);
        // Direct private deadline control; no claim that a native timeout elapsed.
        socket.deadline = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            socket.read(&mut b).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            socket.write(&[1]).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        // A stalled native peer cannot hold the handshake beyond the configured
        // whole-session deadline. This uses the actual blocking TCP/TLS path.
        let stalled = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        let start = Instant::now();
        assert!(
            tls.server_session(DeadlineSocket {
                stream: peer,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: None,
            })
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(7));
        drop(stalled);
    }
}

#[cfg(test)]
mod shared_admission_tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn chain_trusted_alternate_leaves_fail_exact_pins_on_both_endpoints() {
        for alternate_client in [true, false] {
            let (a, alternate_a) = Credentials::anchor_and_signed_alternate();
            let (b, alternate_b) = Credentials::anchor_and_signed_alternate();
            let client = Tls::new(
                if alternate_client { &alternate_a } else { &a },
                b.certificate(),
            )
            .unwrap();
            let server = Tls::new(
                if alternate_client { &b } else { &alternate_b },
                a.certificate(),
            )
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            std::thread::scope(|scope| {
                let receiving = scope.spawn(|| server.accept(listener.accept().unwrap().0));
                let sent = client.connect(address);
                let received = receiving.join().unwrap();
                let rejected = if alternate_client {
                    received.err()
                } else {
                    sent.err()
                };
                assert_eq!(
                    rejected.unwrap().to_string(),
                    "peer certificate does not match the configured pin"
                );
            });
        }
    }
    #[test]
    fn absent_or_other_protocol_alpn_cannot_admit_matching_certificates() {
        for protocols in [
            Vec::new(),
            vec![if ALPN == b"weekend-snapshot/1" {
                b"study-progress/1".to_vec()
            } else {
                b"weekend-snapshot/1".to_vec()
            }],
        ] {
            let a = Credentials::generate().unwrap();
            let b = Credentials::generate().unwrap();
            let mut client = Tls::new(&a, b.certificate()).unwrap();
            client.test_alpn(protocols);
            let server = Tls::new(&b, a.certificate()).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            std::thread::scope(|scope| {
                let receiving = scope.spawn(|| server.accept(listener.accept().unwrap().0));
                assert!(client.connect(address).is_err());
                assert!(receiving.join().unwrap().is_err());
            });
        }
    }
}

#[cfg(test)]
mod credential_boundary_tests {
    use super::*;
    use std::{path::PathBuf, sync::atomic::AtomicU64};

    struct FixtureDirectory(PathBuf);

    impl FixtureDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let sequence = NEXT.fetch_add(1, Relaxed);
            let path = std::env::temp_dir().join(format!(
                "weekend-credential-test-{}-{sequence}",
                std::process::id(),
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for FixtureDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn assert_io_failure(
        error: &(dyn std::error::Error + 'static),
        kind: io::ErrorKind,
        message: &str,
    ) {
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(kind)
        );
        assert_eq!(error.to_string(), message);
    }

    #[test]
    fn opened_credential_failures_preserve_error_and_skip_later_reads() {
        use std::cell::Cell;

        struct FailedRead<'a>(&'a Cell<usize>);
        impl Read for FailedRead<'_> {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                self.0.set(self.0.get() + 1);
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "fixture credential read",
                ))
            }
        }

        let root = FixtureDirectory::new();
        let path = root.0.join("credential");
        fs::write(&path, b"credential data").unwrap();
        let reads = Cell::new(0);
        let error = bounded_with(
            &path,
            64,
            false,
            |_| {
                Err::<FailedRead<'_>, _>(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "fixture credential open",
                ))
            },
            |_| panic!("handle metadata requested after a failed open"),
        )
        .unwrap_err();
        assert_io_failure(
            error.as_ref(),
            io::ErrorKind::PermissionDenied,
            "fixture credential open",
        );

        let error = bounded_with(
            &path,
            64,
            false,
            |_| Ok(FailedRead(&reads)),
            |_| {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "fixture credential metadata",
                ))
            },
        )
        .unwrap_err();
        assert_io_failure(
            error.as_ref(),
            io::ErrorKind::NotFound,
            "fixture credential metadata",
        );
        let error = bounded_with(
            &path,
            64,
            false,
            |_| Ok(FailedRead(&reads)),
            |_| fs::metadata(&root.0),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "credential must be a regular file");
        assert_eq!(reads.get(), 0);

        let error = bounded_with(
            &path,
            64,
            false,
            |_| Ok((&b"ab"[..]).chain(FailedRead(&reads))),
            |_| fs::metadata(&path),
        )
        .unwrap_err();
        assert_io_failure(
            error.as_ref(),
            io::ErrorKind::UnexpectedEof,
            "fixture credential read",
        );
        assert_eq!(reads.get(), 1);
        assert_eq!(fs::read(&path).unwrap(), b"credential data");
    }

    #[test]
    fn generation_preserves_the_original_certificate_error() {
        let error = Credentials::from_generated(Err(rcgen::Error::RingUnspecified))
            .err()
            .expect("failed certificate generation returned credentials");
        assert!(matches!(
            error.downcast_ref::<rcgen::Error>(),
            Some(rcgen::Error::RingUnspecified)
        ));
        assert_eq!(error.to_string(), rcgen::Error::RingUnspecified.to_string());
    }

    #[cfg(unix)]
    #[test]
    fn failed_keygen_preserves_the_error_and_the_completed_file_prefix() {
        use std::os::unix::fs::PermissionsExt;

        let root = FixtureDirectory::new();
        let absent = root.0.join("generation");
        let error = Credentials::create_with::<fs::File>(
            &absent,
            || Err(io::Error::new(io::ErrorKind::PermissionDenied, "fixture generation").into()),
            |_, _| panic!("failed generation opened a key"),
            |_, _| panic!("failed generation opened a certificate"),
        )
        .unwrap_err();
        assert_io_failure(
            error.as_ref(),
            io::ErrorKind::PermissionDenied,
            "fixture generation",
        );
        assert!(!absent.exists());

        let unopened = root.0.join("key-open");
        let error = Credentials::create_with(
            &unopened,
            Credentials::generate,
            |_, _| {
                Err::<fs::File, _>(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "fixture key open",
                ))
            },
            |_, _| panic!("failed key open started the certificate"),
        )
        .unwrap_err();
        assert_io_failure(
            error.as_ref(),
            io::ErrorKind::PermissionDenied,
            "fixture key open",
        );
        assert_eq!(fs::read_dir(&unopened).unwrap().count(), 0);

        struct PartialKey(fs::File, bool);
        impl Write for PartialKey {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.1 {
                    return Err(io::Error::new(
                        io::ErrorKind::StorageFull,
                        "fixture key write",
                    ));
                }
                self.1 = true;
                self.0.write(&bytes[..1])
            }
            fn flush(&mut self) -> io::Result<()> {
                self.0.flush()
            }
        }

        let partial = root.0.join("key-write");
        let error = Credentials::create_with(
            &partial,
            Credentials::generate,
            |options, path| Ok(PartialKey(options.open(path)?, false)),
            |_, _| panic!("failed key write started the certificate"),
        )
        .unwrap_err();
        assert_io_failure(
            error.as_ref(),
            io::ErrorKind::StorageFull,
            "fixture key write",
        );
        let key_metadata = fs::metadata(partial.join("key.der")).unwrap();
        assert_eq!(key_metadata.len(), 1);
        assert_eq!(key_metadata.permissions().mode() & 0o777, 0o600);
        assert!(!partial.join("cert.der").exists());

        let written = root.0.join("certificate-write");
        let credentials = Credentials::generate().unwrap();
        let expected_key = credentials.key.secret_der().to_vec();
        let error = Credentials::create_with(
            &written,
            || Ok(credentials),
            |options, path| options.open(path),
            |_, _| {
                Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "fixture certificate write",
                ))
            },
        )
        .unwrap_err();
        assert_io_failure(
            error.as_ref(),
            io::ErrorKind::StorageFull,
            "fixture certificate write",
        );
        assert!(
            fs::read(written.join("key.der")).unwrap() == expected_key,
            "completed key differs"
        );
        assert!(!written.join("cert.der").exists());
        for dir in [unopened, partial, written] {
            let error = Credentials::load(&dir)
                .err()
                .expect("incomplete credential pair loaded");
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(io::ErrorKind::NotFound)
            );
        }
    }

    #[test]
    fn load_bounds_and_regular_file_checks_precede_tls() {
        let root = FixtureDirectory::new();
        let path = root.0.join("credentials");
        let local = Credentials::generate().unwrap();
        local.write_fixture(&path);
        let loaded = Credentials::load(&path).unwrap();
        assert_eq!(loaded.certificate(), local.certificate());
        assert!(
            loaded.key.secret_der() == local.key.secret_der(),
            "loaded key differs"
        );
        for (name, cap) in [("cert.der", 65536), ("key.der", 16384)] {
            let file = path.join(name);
            let held = path.join("held.der");
            fs::rename(&file, &held).unwrap();
            let error = Credentials::load(&path)
                .err()
                .expect("missing credential loaded");
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(io::ErrorKind::NotFound),
                "{name}"
            );
            fs::rename(&held, &file).unwrap();

            let original = fs::read(&file).unwrap();
            fs::write(&file, vec![0; cap + 1]).unwrap();
            assert!(Credentials::load(&path).is_err());
            fs::write(&file, &original).unwrap();
        }
        #[cfg(unix)]
        {
            let file = path.join("key.der");
            fs::remove_file(&file).unwrap();
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(&file)
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(Credentials::load(&path).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn keygen_directory_failure_preserves_native_error_and_existing_files() {
        use std::os::unix::fs::PermissionsExt;

        let root = FixtureDirectory::new();
        let missing_parent = root.0.join("missing").join("credentials");
        let error = Credentials::create(&missing_parent).unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(io::ErrorKind::NotFound)
        );
        assert!(!missing_parent.exists());

        let path = root.0.join("credentials");
        Credentials::create(&path).unwrap();
        let key = path.join("key.der");
        let cert = path.join("cert.der");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&key).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let before_key = fs::read(&key).unwrap();
        let before_cert = fs::read(&cert).unwrap();
        let error = Credentials::create(&path).unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(io::ErrorKind::AlreadyExists)
        );
        assert!(
            fs::read(&key).unwrap() == before_key,
            "existing key changed"
        );
        assert!(
            fs::read(&cert).unwrap() == before_cert,
            "existing certificate changed"
        );
        assert_eq!(
            Credentials::load(&path).unwrap().certificate().as_ref(),
            before_cert
        );
    }

    #[test]
    fn tls_configuration_preserves_certificate_and_key_errors() {
        let local = Credentials::generate().unwrap();
        let peer = Credentials::generate().unwrap();
        let same = Tls::new(&local, local.certificate())
            .err()
            .expect("local certificate accepted as peer");
        assert_eq!(same.to_string(), "configure the other peer's certificate");

        let malformed = CertificateDer::from(vec![0x30, 0]);
        let expected = RootCertStore::empty().add(malformed.clone()).unwrap_err();
        let error = Tls::new(&local, malformed)
            .err()
            .expect("malformed peer certificate accepted");
        assert_eq!(error.downcast_ref::<rustls::Error>(), Some(&expected));

        let mut roots = RootCertStore::empty();
        roots.add(peer.certificate()).unwrap();
        for key in [
            PrivatePkcs8KeyDer::from(vec![0x30, 0]).into(),
            peer.key.clone_key(),
        ] {
            let invalid = Credentials {
                cert: local.certificate(),
                key,
            };
            let expected = ClientConfig::builder()
                .with_root_certificates(roots.clone())
                .with_client_auth_cert(vec![invalid.certificate()], invalid.key.clone_key())
                .unwrap_err();
            let error = Tls::new(&invalid, peer.certificate())
                .err()
                .expect("invalid local key accepted");
            assert_eq!(error.downcast_ref::<rustls::Error>(), Some(&expected));
        }
        assert!(Tls::new(&local, peer.certificate()).is_ok());
    }

    #[test]
    fn server_configuration_preserves_the_original_setup_error() {
        let local = Credentials::generate().unwrap();
        let peer = Credentials::generate().unwrap();
        let expected = rustls::Error::FailedToGetRandomBytes;
        let error = Tls::configure(&local, peer.certificate(), |_, received| {
            assert_eq!(received.certificate(), local.certificate());
            Err(expected.clone())
        })
        .err()
        .expect("failed server configuration produced a TLS setup");
        assert_eq!(error.downcast_ref::<rustls::Error>(), Some(&expected));
        assert_eq!(error.to_string(), expected.to_string());
    }

    #[test]
    fn connection_refusal_preserves_the_original_socket_error() {
        let local = Credentials::generate().unwrap();
        let peer = Credentials::generate().unwrap();
        let tls = Tls::new(&local, peer.certificate()).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let error = tls
            .connect(address)
            .err()
            .expect("closed endpoint accepted a connection");
        assert_eq!(
            error.downcast_ref::<io::Error>().map(io::Error::kind),
            Some(io::ErrorKind::ConnectionRefused)
        );
    }
}

#[cfg(test)]
mod handshake_decision_tests {
    use super::*;
    use std::{collections::VecDeque, io::Cursor};

    #[test]
    fn both_endpoints_preserve_the_first_handshake_io_error() {
        struct Refused;
        impl Read for Refused {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "handshake read",
                ))
            }
        }
        impl Write for Refused {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "handshake write"))
            }
            fn flush(&mut self) -> io::Result<()> {
                panic!("failed handshake reached flush");
            }
        }

        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let client = Tls::new(&a, b.certificate()).unwrap();
        let server = Tls::new(&b, a.certificate()).unwrap();
        let mut local_client = ClientConnection::new(
            client.client.clone(),
            ServerName::try_from(SERVER_NAME).unwrap(),
        )
        .unwrap();
        let mut local_server = ServerConnection::new(server.server.clone()).unwrap();
        for (error, kind, message) in [
            (
                client
                    .client_handshake(&mut local_client, &mut Refused)
                    .unwrap_err(),
                io::ErrorKind::BrokenPipe,
                "handshake write",
            ),
            (
                server
                    .server_handshake(&mut local_server, &mut Refused)
                    .unwrap_err(),
                io::ErrorKind::ConnectionReset,
                "handshake read",
            ),
        ] {
            assert_eq!(
                error.downcast_ref::<io::Error>().map(io::Error::kind),
                Some(kind)
            );
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn both_finished_endpoints_require_the_exact_pin_and_alpn() {
        const PIN_ERROR: &str = "peer certificate does not match the configured pin";
        for local_is_client in [true, false] {
            for (exact_pin, peer_alpn, expected_error) in [
                (true, true, None),
                (false, true, Some(PIN_ERROR)),
                (true, false, Some(ALPN_ERROR)),
            ] {
                let a = Credentials::generate().unwrap();
                let b = Credentials::generate().unwrap();
                let mut client = Tls::new(&a, b.certificate()).unwrap();
                let mut server = Tls::new(&b, a.certificate()).unwrap();
                if !exact_pin {
                    client.peer = a.certificate();
                    server.peer = b.certificate();
                }
                if !peer_alpn {
                    if local_is_client {
                        Arc::make_mut(&mut server.server).alpn_protocols.clear();
                    } else {
                        Arc::make_mut(&mut client.client).alpn_protocols.clear();
                    }
                }
                let client_conn = ClientConnection::new(
                    client.client.clone(),
                    ServerName::try_from(SERVER_NAME).unwrap(),
                )
                .unwrap();
                let server_conn = ServerConnection::new(server.server.clone()).unwrap();
                let (mut local, peer) = if local_is_client {
                    (
                        rustls::Connection::Client(client_conn),
                        rustls::Connection::Server(server_conn),
                    )
                } else {
                    (
                        rustls::Connection::Server(server_conn),
                        rustls::Connection::Client(client_conn),
                    )
                };
                let mut io = PeerTransport {
                    peer,
                    incoming: VecDeque::new(),
                    local_is_client,
                    fault: Fault::None,
                    sent_peer_flight: false,
                    fault_hit: false,
                };
                let result = match &mut local {
                    rustls::Connection::Client(conn) => client.client_handshake(conn, &mut io),
                    rustls::Connection::Server(conn) => server.server_handshake(conn, &mut io),
                };
                assert!(!local.is_handshaking());
                assert!(!local.wants_write());
                assert_eq!(local.alpn_protocol(), peer_alpn.then_some(ALPN));
                let expected_peer = if local_is_client {
                    b.certificate()
                } else {
                    a.certificate()
                };
                assert_eq!(
                    local.peer_certificates().and_then(|chain| chain.first()),
                    Some(&expected_peer)
                );
                assert_eq!(
                    result.err().map(|error| error.to_string()).as_deref(),
                    expected_error,
                    "client={local_is_client}, exact pin={exact_pin}, peer ALPN={peer_alpn}"
                );
            }
        }
    }

    #[derive(Clone, Copy)]
    enum Fault {
        None,
        WithholdFinished,
        BlockFinalWrite,
    }

    // A deterministic duplex transport, not a replacement TLS implementation.
    // Its remote endpoint processes the actual bytes with rustls. Faults only
    // withhold a final TLS record or return WouldBlock at a write boundary.
    struct PeerTransport {
        peer: rustls::Connection,
        incoming: VecDeque<u8>,
        local_is_client: bool,
        fault: Fault,
        sent_peer_flight: bool,
        fault_hit: bool,
    }

    impl Read for PeerTransport {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if out.is_empty() {
                return Ok(0);
            }
            if self.incoming.is_empty() {
                let mut flight = Vec::new();
                while self.peer.wants_write() {
                    self.peer.write_tls(&mut flight)?;
                }
                if !flight.is_empty() {
                    let final_flight = self.local_is_client || !self.peer.is_handshaking();
                    if matches!(self.fault, Fault::WithholdFinished) && final_flight {
                        let mut offsets = Vec::new();
                        let mut at = 0;
                        while at < flight.len() {
                            offsets.push(at);
                            at += 5 + usize::from(u16::from_be_bytes([
                                flight[at + 3],
                                flight[at + 4],
                            ]));
                        }
                        assert_eq!(at, flight.len());
                        assert!(offsets.len() > 1);
                        flight.truncate(*offsets.last().unwrap());
                        self.fault_hit = true;
                    }
                    self.sent_peer_flight = true;
                    self.incoming.extend(flight);
                }
            }
            if self.incoming.is_empty() {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let n = out.len().min(self.incoming.len());
            for byte in &mut out[..n] {
                *byte = self.incoming.pop_front().unwrap();
            }
            Ok(n)
        }
    }

    impl Write for PeerTransport {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let final_write = if self.local_is_client {
                self.sent_peer_flight
            } else {
                !self.peer.is_handshaking()
            };
            if matches!(self.fault, Fault::BlockFinalWrite) && final_write {
                self.fault_hit = true;
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let mut input = Cursor::new(bytes);
            while input.position() < bytes.len() as u64 {
                self.peer.read_tls(&mut input)?;
                self.peer.process_new_packets().map_err(io::Error::other)?;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn both_endpoints_reject_independent_unfinished_and_pending_write_states() {
        for local_is_client in [true, false] {
            for fault in [Fault::None, Fault::WithholdFinished, Fault::BlockFinalWrite] {
                let a = Credentials::generate().unwrap();
                let b = Credentials::generate().unwrap();
                let mut client = Tls::new(&a, b.certificate()).unwrap();
                let mut server = Tls::new(&b, a.certificate()).unwrap();
                Arc::make_mut(&mut client.client).max_fragment_size = Some(64);
                Arc::make_mut(&mut server.server).max_fragment_size = Some(64);
                let mut client_conn = ClientConnection::new(
                    client.client.clone(),
                    ServerName::try_from(SERVER_NAME).unwrap(),
                )
                .unwrap();
                let mut server_conn = ServerConnection::new(server.server.clone()).unwrap();
                let peer = if local_is_client {
                    rustls::Connection::Server(
                        ServerConnection::new(server.server.clone()).unwrap(),
                    )
                } else {
                    rustls::Connection::Client(
                        ClientConnection::new(
                            client.client.clone(),
                            ServerName::try_from(SERVER_NAME).unwrap(),
                        )
                        .unwrap(),
                    )
                };
                let mut io = PeerTransport {
                    peer,
                    incoming: VecDeque::new(),
                    local_is_client,
                    fault,
                    sent_peer_flight: false,
                    fault_hit: false,
                };
                let (result, handshaking, pending, pin, alpn) = if local_is_client {
                    let result = client.client_handshake(&mut client_conn, &mut io);
                    (
                        result,
                        client_conn.is_handshaking(),
                        client_conn.wants_write(),
                        client.check_pin(client_conn.peer_certificates()),
                        client_conn.alpn_protocol(),
                    )
                } else {
                    let result = server.server_handshake(&mut server_conn, &mut io);
                    (
                        result,
                        server_conn.is_handshaking(),
                        server_conn.wants_write(),
                        server.check_pin(server_conn.peer_certificates()),
                        server_conn.alpn_protocol(),
                    )
                };
                assert!(pin.is_ok());
                assert_eq!(alpn, Some(ALPN));
                let expected = match fault {
                    Fault::None => (false, false),
                    Fault::WithholdFinished => (true, false),
                    Fault::BlockFinalWrite => (false, true),
                };
                assert_eq!((handshaking, pending), expected, "client={local_is_client}");
                if matches!(fault, Fault::None) {
                    assert!(result.is_ok());
                    assert!(!io.fault_hit);
                } else {
                    assert!(io.fault_hit);
                    // This exact error is produced only after complete_io Ok,
                    // by the same admission guard called by native sessions.
                    assert_eq!(
                        result.err().unwrap().to_string(),
                        "TLS handshake incomplete"
                    );
                }
            }
        }
    }
}
