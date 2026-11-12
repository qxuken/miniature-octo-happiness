#[cfg(test)]
mod tests;
#[path = "tls.rs"]
mod tls;
use std::{
    ffi::OsStr,
    io::{self, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};
use tls::{Credentials, Tls, bounded_file};
use weekend_db::{
    Database, Error, Limits,
    replication::{DurableAck, Enrollment, Export, Follower, Primary},
};
pub(crate) type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const USAGE:&str="weekend-replicate keygen <new-credentials-dir>
weekend-replicate describe <source-db> <stream-hex>
weekend-replicate follower-init <empty-dir> <stream-hex> <file-identity-hex>
weekend-replicate status <follower-dir> <stream-hex> <file-identity-hex>
weekend-replicate serve <follower-dir> <stream-hex> <file-identity-hex> <listen-ip:port> <credentials-dir> <peer-cert.der> [--max-exchanges N]
weekend-replicate ship <source-db> <stream-hex> <new-export-file> <peer-ip:port> <credentials-dir> <peer-cert.der> [--attempts N]
weekend-replicate poll <source-db> <stream-hex> <peer-ip:port> <credentials-dir> <peer-cert.der>
IDs are 32 hexadecimal digits. Ship attempts: 1..3 (default 3). Each socket exchange has a 30 s absolute I/O budget including connect; local disk work is not timed out.";
fn text(s: &OsStr) -> AppResult<&str> {
    s.to_str().ok_or_else(|| "expected UTF-8 argument".into())
}
fn identity(s: &OsStr) -> AppResult<[u8; 16]> {
    let s = text(s)?;
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("identity must be 32 hexadecimal digits".into());
    }
    let id = u128::from_str_radix(s, 16)?.to_be_bytes();
    if id == [0; 16] {
        return Err("identity must be nonzero".into());
    }
    Ok(id)
}
fn enrollment(stream: &OsStr, file: &OsStr) -> AppResult<Enrollment> {
    Ok(Enrollment {
        stream_id: identity(stream)?,
        file_identity: identity(file)?,
    })
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn ack_line(ack: Option<DurableAck>) -> String {
    match ack {
        Some(a) => format!(
            "follower durable_revision={} stream={} digest={}",
            a.revision,
            hex(&a.stream_id),
            hex(&a.digest)
        ),
        None => "follower empty".into(),
    }
}
fn transport(dir: &OsStr, pin: &OsStr) -> AppResult<Tls> {
    Tls::new(
        &Credentials::load(Path::new(dir))?,
        bounded_file(Path::new(pin), 65536)?.into(),
    )
}
fn option(
    args: &[std::ffi::OsString],
    name: &str,
    default: usize,
    maximum: usize,
) -> AppResult<usize> {
    match args {
        [] => Ok(default),
        [flag, value] if flag == name => {
            let n = text(value)?.parse::<usize>()?;
            if n == 0 || n > maximum {
                Err("option outside bounds".into())
            } else {
                Ok(n)
            }
        }
        _ => Err(USAGE.into()),
    }
}
pub(crate) fn main() -> AppResult<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [c] if c == "--help" || c == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [c, dir] if c == "keygen" => Credentials::create(Path::new(dir)),
        [c, source, stream] if c == "describe" => {
            let stream = identity(stream)?;
            let info = weekend_db::storage::inspect_file(source, Limits::default())?;
            println!(
                "local verified_revision={} stream={} file_identity={}",
                info.revision,
                hex(&stream),
                hex(&info.identity)
            );
            println!(
                "weekend-replicate follower-init EMPTY_DIR {} {}",
                hex(&stream),
                hex(&info.identity)
            );
            Ok(())
        }
        [c, dir, stream, file] if c == "follower-init" => {
            let e = enrollment(stream, file)?;
            let f = Follower::create(dir, e, Limits::default())?;
            println!("{}", ack_line(f.progress()?));
            Ok(())
        }
        [c, dir, stream, file] if c == "status" => {
            let f = Follower::open(dir, enrollment(stream, file)?, Limits::default())?;
            let s = f.status()?;
            println!("{}", ack_line(s.durable_ack));
            println!(
                "status requires_recovery={} manifest_bytes={} maintenance_required={} selected_slot={:?}",
                s.requires_recovery, s.manifest_bytes, s.maintenance_required, s.selected_slot
            );
            Ok(())
        }
        [
            c,
            dir,
            stream,
            file,
            address,
            credentials,
            pin,
            options @ ..,
        ] if c == "serve" => {
            let e = enrollment(stream, file)?;
            let address = text(address)?.parse()?;
            let max = option(options, "--max-exchanges", usize::MAX, usize::MAX)?;
            let tls = transport(credentials, pin)?;
            let follower = Follower::open(dir, e, Limits::default())?;
            serve(follower, address, &tls, max)
        }
        [
            c,
            source,
            stream,
            export,
            address,
            credentials,
            pin,
            options @ ..,
        ] if c == "ship" => {
            let stream = identity(stream)?;
            let address = text(address)?.parse()?;
            let attempts = option(options, "--attempts", 3, 3)?;
            let tls = transport(credentials, pin)?;
            let mut prepared = prepare(source.as_ref(), stream, export.as_ref())?;
            println!(
                "local captured_revision={} stream={} file_identity={}",
                prepared.revision,
                hex(&stream),
                hex(&prepared.primary.enrollment().file_identity)
            );
            io::stdout().flush()?;
            match ship(&mut prepared,&tls,address,attempts) {
                Ok(ack)=>{println!("{}",ack_line(Some(ack)));Ok(())},
                Err(e)=>Err(format!("transfer not confirmed; follower outcome may be uncertain: {e}. Reopen and poll on a fresh authenticated connection; do not infer rollback. Export file retained; a restarted send needs a new export path.").into())
            }
        }
        [c, source, stream, address, credentials, pin] if c == "poll" => {
            let stream = identity(stream)?;
            let address = text(address)?.parse()?;
            let tls = transport(credentials, pin)?;
            let db = Database::open_file(source, Limits::default())?;
            let revision = db.snapshot().revision();
            let mut primary = Primary::new(&db, stream)?;
            drop(db);
            println!("local verified_revision={revision}");
            println!("{}", ack_line(primary.poll(tls.connect(address)?)?));
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}
struct Prepared {
    primary: Primary,
    export: Export,
    revision: u64,
}
fn prepare(source: &Path, stream: [u8; 16], destination: &Path) -> AppResult<Prepared> {
    let mut db = Database::open_file(source, Limits::default())?;
    let mut primary = Primary::new(&db, stream)?;
    let export = primary.export_file(&mut db, destination)?;
    let revision = export.revision();
    drop(db);
    Ok(Prepared {
        primary,
        export,
        revision,
    })
}
fn ship(
    prepared: &mut Prepared,
    tls: &Tls,
    address: SocketAddr,
    attempts: usize,
) -> AppResult<DurableAck> {
    for attempt in 0..attempts {
        let result = tls
            .connect(address)
            .and_then(|channel| Ok(prepared.primary.ship(&mut prepared.export, channel)?));
        match result {
            Ok(ack) => return Ok(ack),
            Err(error) => {
                let permanent = error.downcast_ref::<Error>().is_some_and(|e| {
                    matches!(
                        e,
                        Error::ForeignStore
                            | Error::UnsupportedFormat
                            | Error::Conflict
                            | Error::Invalid
                            | Error::Corrupt
                            | Error::Limit
                            | Error::RecoveryRequired
                    )
                });
                if permanent || attempt + 1 == attempts {
                    return Err(error);
                }
                eprintln!(
                    "exchange unconfirmed; reconnecting with the same export (attempt {})",
                    attempt + 2
                );
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
    Err("no transfer attempt configured".into())
}
#[derive(Debug)]
enum SessionError {
    Authentication(Box<dyn std::error::Error + Send + Sync>),
    Replication(Error),
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authentication(e) => write!(f, "authentication/transport: {e}"),
            Self::Replication(e) => write!(f, "replication: {e}"),
        }
    }
}
impl std::error::Error for SessionError {}
fn serve_one(follower: &mut Follower, tls: &Tls, socket: TcpStream) -> Result<(), SessionError> {
    let channel = tls.accept(socket).map_err(SessionError::Authentication)?;
    follower
        .serve_authenticated(channel)
        .map_err(SessionError::Replication)
}
fn serve(mut follower: Follower, address: SocketAddr, tls: &Tls, maximum: usize) -> AppResult<()> {
    let listener = TcpListener::bind(address)?;
    println!("ready {}", listener.local_addr()?);
    io::stdout().flush()?;
    for incoming in listener.incoming().take(maximum) {
        let socket = incoming?;
        if let Err(error) = serve_one(&mut follower, tls, socket) {
            eprintln!("{error}");
        }
        // Never keep serving a poisoned manifest or report an old receipt as current.
        follower.progress()?;
    }
    Ok(())
}

#[cfg(test)]
mod pin_tests;
