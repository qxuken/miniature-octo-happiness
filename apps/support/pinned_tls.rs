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
    sync::Arc,
    time::{Duration, Instant},
};

pub(crate) struct Credentials {
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

impl Credentials {
    #[cfg(any(unix, test))]
    pub(crate) fn generate() -> AppResult<Self> {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed([SERVER_NAME.to_owned()])?;
        Ok(Self {
            cert: cert.der().clone(),
            key: PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        })
    }

    #[cfg(unix)]
    pub(crate) fn create(dir: &Path) -> AppResult<()> {
        let creds = Self::generate()?;
        let mut key = fs::OpenOptions::new();
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        fs::DirBuilder::new().mode(0o700).create(dir)?;
        key.mode(0o600);
        key.write(true)
            .create_new(true)
            .open(dir.join("key.der"))?
            .write_all(creds.key.secret_der())?;
        fs::write(dir.join("cert.der"), creds.cert.as_ref())?;
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

pub(crate) type Incoming = StreamOwned<ServerConnection, DeadlineSocket>;

impl Tls {
    pub(crate) fn new(local: &Credentials, peer: CertificateDer<'static>) -> AppResult<Self> {
        if peer == local.cert {
            return Err("configure the other peer's certificate".into());
        }
        let mut roots = RootCertStore::empty();
        roots.add(peer.clone())?;
        let mut client = ClientConfig::builder()
            .with_root_certificates(roots.clone())
            .with_client_auth_cert(vec![local.cert.clone()], local.key.clone_key())?;
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
        let mut server = ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(vec![local.cert.clone()], local.key.clone_key())?;
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
    ) -> AppResult<StreamOwned<ClientConnection, DeadlineSocket>> {
        let deadline = Instant::now() + TIMEOUT;
        let stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
        self.client_session(DeadlineSocket { stream, deadline })
    }

    fn client_session(
        &self,
        mut socket: DeadlineSocket,
    ) -> AppResult<StreamOwned<ClientConnection, DeadlineSocket>> {
        let name = ServerName::try_from(SERVER_NAME)?;
        let mut tls = ClientConnection::new(self.client.clone(), name)?;
        tls.complete_io(&mut socket)?;
        // complete_io may return Ok after partial progress followed by WouldBlock.
        // A parsed certificate/ALPN alone is not proof of a completed handshake.
        if tls.is_handshaking() || tls.wants_write() {
            return Err("TLS handshake incomplete".into());
        }
        self.check_pin(tls.peer_certificates())?;
        if tls.alpn_protocol() != Some(ALPN) {
            return Err(ALPN_ERROR.into());
        }
        Ok(StreamOwned::new(tls, socket))
    }

    pub(crate) fn accept(&self, socket: TcpStream) -> AppResult<Incoming> {
        socket.set_nonblocking(false)?;
        self.server_session(DeadlineSocket::new(socket))
    }

    fn server_session(&self, mut socket: DeadlineSocket) -> AppResult<Incoming> {
        let mut tls = ServerConnection::new(self.server.clone())?;
        tls.complete_io(&mut socket)?;
        // complete_io may return Ok after partial progress followed by WouldBlock.
        // A parsed certificate/ALPN alone is not proof of a completed handshake.
        if tls.is_handshaking() || tls.wants_write() {
            return Err("TLS handshake incomplete".into());
        }
        self.check_pin(tls.peer_certificates())?;
        if tls.alpn_protocol() != Some(ALPN) {
            return Err(ALPN_ERROR.into());
        }
        Ok(StreamOwned::new(tls, socket))
    }

    fn check_pin(&self, chain: Option<&[CertificateDer<'_>]>) -> AppResult<()> {
        if chain.and_then(|certs| certs.first()) == Some(&self.peer) {
            Ok(())
        } else {
            Err("peer certificate does not match the configured pin".into())
        }
    }
}

pub(crate) struct DeadlineSocket {
    stream: TcpStream,
    deadline: Instant,
}

impl DeadlineSocket {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            deadline: Instant::now() + TIMEOUT,
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
}

impl Read for DeadlineSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream.read(buf)
    }
}

impl Write for DeadlineSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

pub(crate) fn bounded_file(path: &Path, limit: usize) -> AppResult<Vec<u8>> {
    bounded(path, limit, false)
}
fn bounded(path: &Path, limit: usize, private: bool) -> AppResult<Vec<u8>> {
    // Configuration lives in a trusted directory. Reject special files before
    // opening (notably blocking FIFOs), then recheck the opened handle.
    if !fs::metadata(path)?.is_file() {
        return Err("credential must be a regular file".into());
    }
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
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
    #[test]
    fn exact_leaf_pin_controls_are_distinct_from_chain_trust() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        let tls = Tls::new(&a, b.certificate()).unwrap();
        assert!(tls.check_pin(None).is_err());
        assert!(tls.check_pin(Some(&[])).is_err());
        assert!(tls.check_pin(Some(&[a.certificate()])).is_err());
        assert!(tls.check_pin(Some(&[b.certificate()])).is_ok());
    }
    #[test]
    fn absolute_deadline_is_not_renewed_by_progress() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        let mut socket = DeadlineSocket::new(stream);
        let deadline = socket.deadline;
        peer.write_all(&[42]).unwrap();
        let mut byte = [0];
        socket.read_exact(&mut byte).unwrap();
        assert_eq!(byte, [42]);
        assert_eq!(socket.deadline, deadline);
        // Explicit clock control proves admission before socket I/O, not an OS
        // timeout or a bound on filesystem verification/flush duration.
        socket.deadline = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            socket.read(&mut byte).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            socket.write(&byte).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
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
        assert!(bounded_file(&path, 64).is_err());
        let file = path.join("value");
        fs::write(&file, []).unwrap();
        assert_eq!(
            bounded_file(&file, 64).err().unwrap().to_string(),
            "credential size outside bounds"
        );
        fs::write(&file, b"public").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(bounded(&file, 64, false).unwrap(), b"public");
            assert!(bounded(&file, 64, true).is_err());
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(bounded(&file, 64, true).unwrap(), b"public");
        }
        fs::remove_dir_all(path).unwrap();
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
                deadline: Instant::now() + Duration::from_secs(1)
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
    #[test]
    fn load_bounds_and_regular_file_checks_precede_tls() {
        let path = std::env::temp_dir().join(format!("shared-tls-bounds-{}", std::process::id()));
        let local = Credentials::generate().unwrap();
        local.write_fixture(&path);
        let loaded = Credentials::load(&path).unwrap();
        assert_eq!(loaded.certificate(), local.certificate());
        for (name, cap) in [("cert.der", 65536), ("key.der", 16384)] {
            let file = path.join(name);
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
        fs::remove_dir_all(&path).unwrap();
    }
}
