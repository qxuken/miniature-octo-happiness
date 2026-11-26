mod journal;
mod protocol;
mod service;
mod stop;
use protocol::{COLLECTION, KEY, sequenced};
use service::serve;
use std::{
    ffi::OsStr,
    io::{self, BufRead, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};
use weekend_db::{Database, Limits, Result as DbResult};

const USAGE: &str = "write-coordinator init <file>
write-coordinator serve <file> <port>
write-coordinator client <port>
write-coordinator serve <file> <port> --secret-file <file>
write-coordinator client <port> --secret-file <file>
write-coordinator init-sequenced <file>
write-coordinator serve-sequenced <file> <port> --secret-file <file>
write-coordinator journal init <file> <port> <slot> --secret-file <file>
write-coordinator journal set <file> <port> <expected-revision> <unsigned-value> --secret-file <file>
write-coordinator journal resume <file> <port> --secret-file <file>
write-coordinator journal check <file> <port> --secret-file <file>
write-coordinator journal status <file>
client commands: get, set <expected-revision> <unsigned-value>,
set-id <request-id> <expected-revision> <unsigned-value>,
sequenced commands: receipt <slot>, set-seq <slot> <sequence> <expected-revision> <unsigned-value>, quit";
const TIMEOUT: Duration = Duration::from_secs(3);

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

enum Profile {
    Legacy,
    Sequenced,
}

pub(super) fn run() -> AppResult<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [command] if command == "--help" || command == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [command, path] if command == "init" => Ok(initialize(Path::new(path))?),
        [command, path] if command == "init-sequenced" => Ok(sequenced::initialize(
            &Database::create_file(Path::new(path), Limits::default())?,
        )?),
        [command, path, port] if command == "serve" => {
            eprintln!("unauthenticated loopback demo mode");
            serve(Path::new(path), address(port)?, None, Profile::Legacy)
        }
        [command, port] if command == "client" => client(address(port)?, None),
        [command, path, port, flag, secret] if command == "serve" && flag == "--secret-file" => {
            serve(
                Path::new(path),
                address(port)?,
                Some(Secret::load(Path::new(secret))?),
                Profile::Legacy,
            )
        }
        [command, path, port, flag, secret]
            if command == "serve-sequenced" && flag == "--secret-file" =>
        {
            serve(
                Path::new(path),
                address(port)?,
                Some(Secret::load(Path::new(secret))?),
                Profile::Sequenced,
            )
        }
        [command, port, flag, secret] if command == "client" && flag == "--secret-file" => {
            client(address(port)?, Some(Secret::load(Path::new(secret))?))
        }
        [command, journal_args @ ..] if command == "journal" => journal::run(journal_args),
        _ => Err(USAGE.into()),
    }
}

struct Secret([u8; 64]);
impl Secret {
    fn load(path: &Path) -> AppResult<Self> {
        // Configuration lives in a trusted namespace. Reject special files
        // before opening (notably blocking FIFOs), then recheck the handle.
        if !std::fs::metadata(path)?.is_file() {
            return Err("secret must be a regular file".into());
        }
        let file = std::fs::File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("secret must be a regular file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("secret file must have owner-only permissions".into());
            }
        }
        let mut bytes = Vec::new();
        file.take(66).read_to_end(&mut bytes)?;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.len() != 64 || !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err("secret must contain 64 hexadecimal characters".into());
        }
        Ok(Self(bytes.try_into().unwrap()))
    }
    fn authorize<'a>(&self, request: &'a str) -> Option<&'a str> {
        protocol::authorize(&self.0, request)
    }
}

fn address(port: &OsStr) -> AppResult<SocketAddr> {
    let port = port
        .to_str()
        .ok_or("expected a port number")?
        .parse::<u16>()?;
    Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}

fn initialize(path: &Path) -> DbResult<()> {
    let db = Database::create_file(path, Limits::default())?;
    let collection = db.create_collection()?;
    assert_eq!(collection.as_u64(), COLLECTION);
    let mut tx = db.begin();
    tx.put(collection, KEY, &db.unsigned(0)?)?;
    tx.commit()?;
    Ok(())
}

fn time_remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "coordinator deadline exceeded"))
}

fn read_line(stream: &TcpStream, deadline: Instant) -> io::Result<String> {
    protocol::read_line_from(&mut &*stream, || {
        stream.set_read_timeout(Some(time_remaining(deadline)?))
    })
}

fn client(address: SocketAddr, secret: Option<Secret>) -> AppResult<()> {
    for line in io::stdin().lock().lines() {
        let line = line?;
        let command = line.trim();
        if command == "quit" {
            break;
        }
        if command.is_empty() {
            continue;
        }
        let reply = request(address, secret.as_ref(), command)?;
        writeln!(io::stdout().lock(), "{reply}")?;
    }
    Ok(())
}

fn request(address: SocketAddr, secret: Option<&Secret>, command: &str) -> AppResult<String> {
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    if let Some(secret) = secret {
        writeln!(stream, "auth {} {command}", std::str::from_utf8(&secret.0)?)?;
    } else {
        writeln!(stream, "{command}")?;
    }
    Ok(read_line(&stream, Instant::now() + TIMEOUT)?)
}

#[cfg(test)]
mod decisions {
    use super::*;
    #[test]
    fn secret_file_and_loopback_address_admission() {
        let root = std::env::temp_dir().join(format!(
            "coordinator-secret-decisions-{}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("secret");
        let missing = Secret::load(&path)
            .err()
            .expect("missing secret was accepted");
        assert_eq!(
            missing.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            Secret::load(&root).err().unwrap().to_string(),
            "secret must be a regular file"
        );
        for (bytes, good) in [
            (vec![b'a'; 64], true),
            ([vec![b'A'; 64], vec![b'\n']].concat(), true),
            (vec![b'a'; 63], false),
            (vec![b'x'; 64], false),
            ([vec![b'a'; 64], vec![b'\n', b'\n']].concat(), false),
        ] {
            std::fs::write(&path, bytes).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            let diagnostic = Secret::load(&path).err().map(|error| error.to_string());
            assert_eq!(
                diagnostic.as_deref(),
                (!good).then_some("secret must contain 64 hexadecimal characters")
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&path, vec![b'a'; 64]).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(
                Secret::load(&path).err().unwrap().to_string(),
                "secret file must have owner-only permissions"
            );
        }
        assert_eq!(address(OsStr::new("0")).unwrap().ip(), Ipv4Addr::LOCALHOST);
        assert_eq!(address(OsStr::new("65535")).unwrap().port(), 65535);
        for port in ["65536", "-1", "nope"] {
            let error = address(OsStr::new(port)).unwrap_err();
            assert_eq!(
                error.to_string(),
                port.parse::<u16>().unwrap_err().to_string()
            );
            assert!(error.downcast_ref::<std::num::ParseIntError>().is_some());
        }
        #[cfg(unix)]
        let non_unicode = {
            use std::os::unix::ffi::OsStrExt;
            OsStr::from_bytes(&[255]).to_owned()
        };
        #[cfg(windows)]
        let non_unicode = {
            use std::os::windows::ffi::OsStringExt;
            std::ffi::OsString::from_wide(&[0xd800])
        };
        assert_eq!(
            address(&non_unicode).unwrap_err().to_string(),
            "expected a port number"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reply_deadline_preserves_the_unconsumed_socket_line() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        peer.write_all(b"value 2 0\n").unwrap();
        let error = read_line(&socket, Instant::now()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(error.to_string(), "coordinator deadline exceeded");
        assert_eq!(
            read_line(&socket, Instant::now() + TIMEOUT).unwrap(),
            "value 2 0"
        );
    }

    #[test]
    fn connection_preserves_request_bytes_and_incomplete_reply_error() {
        use std::io::BufReader;

        let secret = Secret([b'a'; 64]);
        for (secret, reply) in [
            (None, b"value 2 0\n".as_slice()),
            (Some(&secret), b"value 2 0".as_slice()),
        ] {
            let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
            let listener = std::net::TcpListener::bind(address).unwrap();
            let address = listener.local_addr().unwrap();
            std::thread::scope(|scope| {
                let server = scope.spawn(|| {
                    let (socket, _) = listener.accept().unwrap();
                    socket.set_read_timeout(Some(TIMEOUT)).unwrap();
                    let mut input = BufReader::new(socket);
                    let mut line = String::new();
                    input.read_line(&mut line).unwrap();
                    if secret.is_some() {
                        assert_eq!(line, format!("auth {} get\n", "a".repeat(64)));
                    } else {
                        assert_eq!(line, "get\n");
                    }
                    input.get_mut().write_all(reply).unwrap();
                });
                let actual = request(address, secret, "get");
                server.join().unwrap();
                if reply.ends_with(b"\n") {
                    assert_eq!(actual.unwrap(), "value 2 0");
                } else {
                    let error = actual.unwrap_err();
                    assert_eq!(
                        error.downcast_ref::<io::Error>().unwrap().kind(),
                        io::ErrorKind::InvalidData
                    );
                    assert_eq!(error.to_string(), "missing or oversized line");
                }
            });
        }
    }
}
