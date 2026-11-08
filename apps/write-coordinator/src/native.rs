use std::{
    ffi::OsStr,
    io::{self, BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::Path,
    time::Duration,
};
use weekend_db::{Database, Error, Limits, Result as DbResult};

const USAGE: &str = "write-coordinator init <file>
write-coordinator serve <file> <port>
write-coordinator client <port>
client commands: get, set <expected-revision> <unsigned-value>, quit";
const COLLECTION: u64 = 1;
const KEY: &[u8] = b"counter";
const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_LINE: u64 = 64;

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

pub(super) fn run() -> AppResult<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [command] if command == "--help" || command == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [command, path] if command == "init" => Ok(initialize(Path::new(path))?),
        [command, path, port] if command == "serve" => serve(Path::new(path), address(port)?),
        [command, port] if command == "client" => client(address(port)?),
        _ => Err(USAGE.into()),
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
    let mut words = line.split_whitespace();
    match (words.next(), words.next(), words.next(), words.next()) {
        (Some("get"), None, None, None) => {
            let (revision, value) = state(db)?;
            Ok(format!("value {revision} {value}"))
        }
        (Some("set"), Some(expected), Some(value), None) => {
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
        _ => Err(Error::Invalid),
    }
}

fn configure(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))
}

fn read_line(stream: &TcpStream) -> io::Result<String> {
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE)).read_line(&mut line)?;
    if line.pop() != Some('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing or oversized line",
        ));
    }
    Ok(line)
}

fn serve(path: &Path, address: SocketAddr) -> AppResult<()> {
    let mut db = Database::open_file(path, Limits::default())?;
    state(&db)?;
    let listener = TcpListener::bind(address)?;
    writeln!(io::stdout().lock(), "ready {}", listener.local_addr()?)?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        let result = (|| -> io::Result<()> {
            configure(&stream)?;
            let request = read_line(&stream)?;
            let reply = handle(&mut db, &request).unwrap_or_else(|error| format!("error {error}"));
            writeln!(stream, "{reply}")
        })();
        if let Err(error) = result {
            eprintln!("client connection: {error}");
        }
    }
    Ok(())
}

fn client(address: SocketAddr) -> AppResult<()> {
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
        writeln!(stream, "{command}")?;
        let reply = read_line(&stream)?;
        writeln!(io::stdout().lock(), "{reply}")?;
    }
    Ok(())
}
