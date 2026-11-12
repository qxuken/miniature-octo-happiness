use super::AppResult;
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

const SERVER_NAME: &str = "weekend-replicate";
const ALPN: &[u8] = b"weekend-snapshot/1";
pub(crate) const TIMEOUT: Duration = Duration::from_secs(30);

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
            return Err("snapshot ALPN negotiation required".into());
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
            return Err("snapshot ALPN negotiation required".into());
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
