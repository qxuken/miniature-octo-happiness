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

const SERVER_NAME: &str = "study-peer";
const TIMEOUT: Duration = Duration::from_secs(3);

pub(super) struct Credentials {
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

impl Credentials {
    fn generate() -> AppResult<Self> {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed([SERVER_NAME.to_owned()])?;
        Ok(Self {
            cert: cert.der().clone(),
            key: PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        })
    }

    pub(super) fn create(dir: &Path) -> AppResult<()> {
        let creds = Self::generate()?;
        let mut key = fs::OpenOptions::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
            fs::DirBuilder::new().mode(0o700).create(dir)?;
            key.mode(0o600);
        }
        #[cfg(not(unix))]
        fs::create_dir(dir)?;
        key.write(true)
            .create_new(true)
            .open(dir.join("key.der"))?
            .write_all(creds.key.secret_der())?;
        fs::write(dir.join("cert.der"), creds.cert.as_ref())?;
        Ok(())
    }

    pub(super) fn load(dir: &Path) -> AppResult<Self> {
        Ok(Self {
            cert: fs::read(dir.join("cert.der"))?.into(),
            key: PrivatePkcs8KeyDer::from(fs::read(dir.join("key.der"))?).into(),
        })
    }
}

pub(super) struct Tls {
    client: Arc<ClientConfig>,
    server: Arc<ServerConfig>,
    peer: CertificateDer<'static>,
}

pub(super) type Incoming = StreamOwned<ServerConnection, DeadlineSocket>;

impl Tls {
    pub(super) fn new(local: &Credentials, peer: CertificateDer<'static>) -> AppResult<Self> {
        if peer == local.cert {
            return Err("configure the other actor's certificate".into());
        }
        let mut roots = RootCertStore::empty();
        roots.add(peer.clone())?;
        let client = ClientConfig::builder()
            .with_root_certificates(roots.clone())
            .with_client_auth_cert(vec![local.cert.clone()], local.key.clone_key())?;
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
        let server = ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(vec![local.cert.clone()], local.key.clone_key())?;
        Ok(Self {
            client: Arc::new(client),
            server: Arc::new(server),
            peer,
        })
    }

    pub(super) fn connect(
        &self,
        address: SocketAddr,
    ) -> AppResult<StreamOwned<ClientConnection, DeadlineSocket>> {
        let mut socket = DeadlineSocket::new(TcpStream::connect_timeout(&address, TIMEOUT)?);
        let name = ServerName::try_from(SERVER_NAME)?;
        let mut tls = ClientConnection::new(self.client.clone(), name)?;
        tls.complete_io(&mut socket)?;
        self.check_pin(tls.peer_certificates())?;
        Ok(StreamOwned::new(tls, socket))
    }

    pub(super) fn accept(&self, socket: TcpStream) -> AppResult<Incoming> {
        socket.set_nonblocking(false)?;
        let mut socket = DeadlineSocket::new(socket);
        let mut tls = ServerConnection::new(self.server.clone())?;
        tls.complete_io(&mut socket)?;
        self.check_pin(tls.peer_certificates())?;
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

pub(super) struct DeadlineSocket {
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

#[cfg(test)]
mod tests {
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
mod decisions {
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
        assert!(tls.accept(peer).is_err());
        assert!(start.elapsed() < Duration::from_secs(10));
        drop(stalled);
    }
}
