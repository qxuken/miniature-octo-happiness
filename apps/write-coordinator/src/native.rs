mod protocol;
#[cfg(test)]
use protocol::MAX_RECEIPTS;
use protocol::{COLLECTION, KEY, handle, state};
use std::{
    ffi::OsStr,
    io::{self, BufRead, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::Path,
    time::{Duration, Instant},
};
#[cfg(test)]
use weekend_db::Error;
use weekend_db::{Database, Limits, Result as DbResult};

const USAGE: &str = "write-coordinator init <file>
write-coordinator serve <file> <port>
write-coordinator client <port>
write-coordinator serve <file> <port> --secret-file <file>
write-coordinator client <port> --secret-file <file>
client commands: get, set <expected-revision> <unsigned-value>,
set-id <request-id> <expected-revision> <unsigned-value>, quit";
const TIMEOUT: Duration = Duration::from_secs(3);

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

pub(super) fn run() -> AppResult<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [command] if command == "--help" || command == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [command, path] if command == "init" => Ok(initialize(Path::new(path))?),
        [command, path, port] if command == "serve" => {
            eprintln!("unauthenticated loopback demo mode");
            serve(Path::new(path), address(port)?, None)
        }
        [command, port] if command == "client" => client(address(port)?, None),
        [command, path, port, flag, secret] if command == "serve" && flag == "--secret-file" => {
            serve(
                Path::new(path),
                address(port)?,
                Some(Secret::load(Path::new(secret))?),
            )
        }
        [command, port, flag, secret] if command == "client" && flag == "--secret-file" => {
            client(address(port)?, Some(Secret::load(Path::new(secret))?))
        }
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

fn configure(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))
}

fn read_line(stream: &TcpStream) -> io::Result<String> {
    let deadline = Instant::now() + TIMEOUT;
    protocol::read_line_from(&mut &*stream, || {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "request deadline exceeded"))?;
        stream.set_read_timeout(Some(remaining))
    })
}

fn serve(path: &Path, address: SocketAddr, secret: Option<Secret>) -> AppResult<()> {
    let mut db = Database::open_file(path, Limits::default())?;
    state(&db)?;
    let listener = TcpListener::bind(address)?;
    writeln!(io::stdout().lock(), "ready {}", listener.local_addr()?)?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        let result = (|| -> io::Result<()> {
            configure(&stream)?;
            let request = read_line(&stream)?;
            let command = match &secret {
                Some(secret) => secret.authorize(&request),
                None => Some(request.as_str()),
            };
            let reply = match command {
                Some(command) => {
                    handle(&mut db, command).unwrap_or_else(|error| format!("error {error}"))
                }
                None => "error Unauthorized".to_owned(),
            };
            writeln!(stream, "{reply}")
        })();
        if let Err(error) = result {
            eprintln!("client connection: {error}");
        }
    }
    Ok(())
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
        let mut stream = TcpStream::connect_timeout(&address, TIMEOUT)?;
        configure(&stream)?;
        if let Some(secret) = &secret {
            writeln!(stream, "auth {} {command}", std::str::from_utf8(&secret.0)?)?;
        } else {
            writeln!(stream, "{command}")?;
        }
        let reply = read_line(&stream)?;
        writeln!(io::stdout().lock(), "{reply}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_receipts_cannot_acknowledge_or_change_state() {
        for (size, version, expected, acknowledged) in [
            (24, 1, 2, 3u64),
            (25, 2, 2, 3),
            (25, 1, 2, 4),
            (25, 1, 9, 10),
            (25, 1, u64::MAX, 0),
        ] {
            let mut db = Database::new().unwrap();
            let c = db.create_collection().unwrap();
            let mut bytes = vec![0; 25];
            bytes[0] = version;
            bytes[1..9].copy_from_slice(&expected.to_le_bytes());
            bytes[17..25].copy_from_slice(&acknowledged.to_le_bytes());
            bytes.truncate(size);
            let mut setup = db.begin();
            setup.put(c, KEY, &db.unsigned(0).unwrap()).unwrap();
            setup
                .put(c, b"receipt/bad", &db.bytes(&bytes).unwrap())
                .unwrap();
            setup.commit().unwrap();
            drop(setup);
            assert_eq!(
                handle(&mut db, &format!("set-id bad {expected} 0")),
                Err(Error::Corrupt)
            );
            assert_eq!(state(&db), Ok((2, 0)));
        }
    }

    #[test]
    fn receipts_replay_original_ack_and_never_expire_at_capacity() {
        let mut db = Database::new().unwrap();
        let c = db.create_collection().unwrap();
        let mut setup = db.begin();
        setup.put(c, KEY, &db.unsigned(0).unwrap()).unwrap();
        setup.commit().unwrap();
        drop(setup);
        assert_eq!(handle(&mut db, "set-id first 2 10").unwrap(), "committed 3");
        assert_eq!(handle(&mut db, "set 3 20").unwrap(), "committed 4");
        assert_eq!(handle(&mut db, "set-id first 2 10").unwrap(), "committed 3");
        assert_eq!(handle(&mut db, "set-id first 2 11"), Err(Error::Invalid));
        assert_eq!(handle(&mut db, "set-id first 4 10"), Err(Error::Invalid));
        assert_eq!(state(&db), Ok((4, 20)));
        // Fill the remaining receipt slots using the same real request path.
        for n in 1..MAX_RECEIPTS {
            let revision = db.snapshot().revision();
            assert_eq!(
                handle(&mut db, &format!("set-id r{n} {revision} {n}")).unwrap(),
                format!("committed {}", revision + 1)
            );
        }
        let revision = db.snapshot().revision();
        assert_eq!(
            handle(&mut db, &format!("set-id full {revision} 8")),
            Err(Error::Limit)
        );
        assert_eq!(handle(&mut db, "set-id first 2 10").unwrap(), "committed 3");
        assert_eq!(db.snapshot().revision(), revision);
        for bad in [
            "set-id x/y 1 2",
            "set-id a 1 2 extra",
            "set-id a -1 2",
            "set-id a 1 nope",
        ] {
            assert_eq!(handle(&mut db, bad), Err(Error::Invalid));
        }
    }
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
        assert!(Secret::load(&root).is_err());
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
            assert_eq!(Secret::load(&path).is_ok(), good);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&path, vec![b'a'; 64]).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(Secret::load(&path).is_err());
        }
        assert_eq!(address(OsStr::new("0")).unwrap().ip(), Ipv4Addr::LOCALHOST);
        assert_eq!(address(OsStr::new("65535")).unwrap().port(), 65535);
        for port in ["65536", "-1", "nope"] {
            assert!(address(OsStr::new(port)).is_err());
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            assert!(address(OsStr::from_bytes(&[255])).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
