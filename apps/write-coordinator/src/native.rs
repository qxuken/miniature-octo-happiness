use std::{
    ffi::OsStr,
    io::{self, BufRead, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::Path,
    time::{Duration, Instant},
};
use weekend_db::{Database, Error, Limits, Result as DbResult};

const USAGE: &str = "write-coordinator init <file>
write-coordinator serve <file> <port>
write-coordinator client <port>
write-coordinator serve <file> <port> --secret-file <file>
write-coordinator client <port> --secret-file <file>
client commands: get, set <expected-revision> <unsigned-value>,
set-id <request-id> <expected-revision> <unsigned-value>, quit";
const COLLECTION: u64 = 1;
const KEY: &[u8] = b"counter";
const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_LINE: usize = 256;
const MAX_RECEIPTS: usize = 1024;

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
    fn matches(&self, candidate: &str) -> bool {
        if candidate.len() != self.0.len() {
            return false;
        }
        self.0
            .iter()
            .zip(candidate.bytes())
            .fold(0, |difference, (a, b)| {
                difference | std::hint::black_box(a ^ b)
            })
            == 0
    }
    fn authorize<'a>(&self, request: &'a str) -> Option<&'a str> {
        let (candidate, command) = request.strip_prefix("auth ")?.split_once(' ')?;
        self.matches(candidate).then_some(command)
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

fn state(db: &Database) -> DbResult<(u64, u64)> {
    let snapshot = db.snapshot();
    let value = snapshot
        .get(db.collection_id(COLLECTION), KEY)?
        .ok_or(Error::NotFound)?
        .as_u64()?;
    Ok((snapshot.revision(), value))
}

fn handle(db: &mut Database, line: &str) -> DbResult<String> {
    if db.requires_recovery() {
        return Err(Error::RecoveryRequired);
    }
    let words: Vec<_> = line.split_whitespace().collect();
    match words.as_slice() {
        ["get"] => {
            let (revision, value) = state(db)?;
            Ok(format!("value {revision} {value}"))
        }
        ["set", expected, value] => {
            let expected = expected.parse::<u64>().map_err(|_| Error::Invalid)?;
            let value = value.parse::<u64>().map_err(|_| Error::Invalid)?;
            // The same owner handles the comparison and commit before accepting
            // another request. This demo uses the revision of its one-record DB.
            let (revision, current) = state(db)?;
            if revision != expected {
                return Ok(format!("conflict {revision} {current}"));
            }
            let mut tx = db.begin();
            tx.put(db.collection_id(COLLECTION), KEY, &db.unsigned(value)?)?;
            Ok(format!("committed {}", tx.commit()?))
        }
        ["set-id", id, expected, value] => {
            let expected = expected.parse::<u64>().map_err(|_| Error::Invalid)?;
            let value = value.parse::<u64>().map_err(|_| Error::Invalid)?;
            submit_identified(db, id, expected, value)
        }
        _ => Err(Error::Invalid),
    }
}

fn submit_identified(db: &Database, id: &str, expected: u64, value: u64) -> DbResult<String> {
    if id.is_empty()
        || id.len() > 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Invalid);
    }
    let collection = db.collection_id(COLLECTION);
    let key = format!("receipt/{id}");
    let mut tx = db.begin_with_validation(weekend_db::Validation::WholeDatabase);
    if let Some(receipt) = tx.get(collection, key.as_bytes())? {
        let bytes = receipt.as_bytes().map_err(|_| Error::Corrupt)?;
        if bytes.len() != 25 || bytes[0] != 1 {
            return Err(Error::Corrupt);
        }
        let original_expected = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
        let original_value = u64::from_le_bytes(bytes[9..17].try_into().unwrap());
        let acknowledged = u64::from_le_bytes(bytes[17..25].try_into().unwrap());
        if original_expected.checked_add(1) != Some(acknowledged)
            || acknowledged > db.snapshot().revision()
        {
            return Err(Error::Corrupt);
        }
        if original_expected != expected || original_value != value {
            return Err(Error::Invalid);
        }
        return Ok(format!("committed {acknowledged}"));
    }
    let (revision, current) = state(db)?;
    if revision != expected {
        return Ok(format!("conflict {revision} {current}"));
    }
    // Receipts never expire or get evicted. Exhaustion rejects new IDs while
    // retaining every existing retry result.
    if tx.count(collection)? > MAX_RECEIPTS {
        return Err(Error::Limit);
    }
    let acknowledged = revision.checked_add(1).ok_or(Error::Exhausted)?;
    let mut receipt = [0u8; 25];
    receipt[0] = 1;
    receipt[1..9].copy_from_slice(&expected.to_le_bytes());
    receipt[9..17].copy_from_slice(&value.to_le_bytes());
    receipt[17..25].copy_from_slice(&acknowledged.to_le_bytes());
    tx.put(collection, KEY, &db.unsigned(value)?)?;
    tx.put(collection, key.as_bytes(), &db.bytes(&receipt)?)?;
    assert_eq!(tx.commit()?, acknowledged);
    Ok(format!("committed {acknowledged}"))
}

fn configure(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))
}

fn read_line(stream: &TcpStream) -> io::Result<String> {
    let deadline = Instant::now() + TIMEOUT;
    let mut bytes = Vec::new();
    let mut stream = stream;
    while bytes.len() < MAX_LINE {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "request deadline exceeded"))?;
        stream.set_read_timeout(Some(remaining))?;
        let mut byte = [0];
        if stream.read(&mut byte)? == 0 {
            break;
        }
        if byte[0] == b'\n' {
            return String::from_utf8(bytes)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid text"));
        }
        bytes.push(byte[0]);
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "missing or oversized line",
    ))
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
