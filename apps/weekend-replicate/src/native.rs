use crate::diagnostics::{self, Phase};
mod admission;
mod capture_record;
#[cfg(test)]
mod tests;
#[path = "tls.rs"]
mod tls;
#[cfg(test)]
use std::net::TcpStream;
use std::{
    ffi::OsStr,
    io::{self, Write},
    net::{SocketAddr, TcpListener},
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
weekend-replicate cleanup-inactive <follower-dir> <stream-hex> <file-identity-hex>
weekend-replicate compact-manifest <follower-dir> <stream-hex> <file-identity-hex>
weekend-replicate status <follower-dir> <stream-hex> <file-identity-hex>
weekend-replicate serve <follower-dir> <stream-hex> <file-identity-hex> <listen-ip:port> <credentials-dir> <peer-cert.der> [--max-exchanges N]
weekend-replicate ship <source-db> <stream-hex> <new-export-file> <peer-ip:port> <credentials-dir> <peer-cert.der> [--attempts N]
weekend-replicate capture <source-db> <stream-hex> <new-export-file> <new-descriptor-db>
weekend-replicate ship-captured <source-db> <stream-hex> <export-file> <descriptor-db> <peer-ip:port> <credentials-dir> <peer-cert.der> [--attempts N]
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
    let credentials =
        diagnostics::at(Phase::CredentialsLoad, || Credentials::load(Path::new(dir)))?;
    let pin = diagnostics::at(Phase::CredentialsLoad, || {
        bounded_file(Path::new(pin), 65536)
    })?;
    diagnostics::at(Phase::TlsConfig, || Tls::new(&credentials, pin.into()))
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
pub(crate) fn main(args: Vec<std::ffi::OsString>) -> AppResult<()> {
    match args.as_slice() {
        [c] if c == "--help" || c == "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        [c, dir] if c == "keygen" => diagnostics::at(Phase::CredentialsLoad, || {
            Credentials::create(Path::new(dir))
        }),
        [c, source, stream] if c == "describe" => {
            let stream = identity(stream)?;
            let info = diagnostics::storage(Phase::StorageVerify, || {
                weekend_db::storage::inspect_file(source, Limits::default())
            })?;
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
            let f = diagnostics::storage(Phase::StorageCreate, || {
                Follower::create(dir, e, Limits::default())
            })?;
            println!("{}", ack_line(f.progress()?));
            Ok(())
        }
        [c, dir, stream, file] if c == "status" => {
            let e = enrollment(stream, file)?;
            let f = diagnostics::storage(Phase::StorageOpen, || {
                Follower::open(dir, e, Limits::default())
            })?;
            let s = f.status()?;
            println!("{}", ack_line(s.durable_ack));
            println!(
                "status requires_recovery={} manifest_bytes={} maintenance_required={} selected_slot={:?}",
                s.requires_recovery, s.manifest_bytes, s.maintenance_required, s.selected_slot
            );
            Ok(())
        }
        [c, dir, stream, file] if c == "cleanup-inactive" => {
            let e = enrollment(stream, file)?;
            let mut f = diagnostics::storage(Phase::StorageOpen, || {
                Follower::open(dir, e, Limits::default())
            })?;
            let removed = diagnostics::storage(Phase::Maintenance, || f.cleanup_inactive())?;
            println!("maintenance removed_inactive={removed}");
            println!("{}", ack_line(f.progress()?));
            Ok(())
        }
        [c, dir, stream, file] if c == "compact-manifest" => {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                let e = enrollment(stream, file)?;
                let f = diagnostics::storage(Phase::StorageOpen, || {
                    Follower::open(dir, e, Limits::default())
                })?;
                let before = f.status()?.manifest_bytes;
                let f = diagnostics::storage(Phase::Maintenance, || f.compact_manifest())?;
                let after = f.status()?;
                println!(
                    "maintenance manifest_before={before} manifest_after={} maintenance_required={}",
                    after.manifest_bytes, after.maintenance_required
                );
                println!("{}", ack_line(after.durable_ack));
                Ok(())
            }
            #[cfg(windows)]
            {
                let _ = (dir, stream, file);
                Err(
                    "compact-manifest unsupported on Windows; reseed a fresh follower directory"
                        .into(),
                )
            }
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
            let follower = diagnostics::storage(Phase::StorageOpen, || {
                Follower::open(dir, e, Limits::default())
            })?;
            serve(follower, address, &tls, max)
        }
        [c, source, stream, export, descriptor] if c == "capture" => {
            let stream = identity(stream)?;
            let prepared = prepare(source.as_ref(), stream, export.as_ref())?;
            let capture = prepared.export.descriptor();
            capture_record::store(Path::new(descriptor), capture)?;
            let mut output = io::stdout().lock();
            diagnostics::at(Phase::LocalCommand, || {
                announce(&prepared, &mut output)?;
                writeln!(
                    output,
                    "local export_length={} export_digest={}",
                    capture.length,
                    hex(&capture.digest),
                )?;
                output.flush()?;
                Ok(())
            })
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
            let prepared = prepare(source.as_ref(), stream, export.as_ref())?;
            deliver(
                prepared,
                &tls,
                address,
                attempts,
                &mut io::stdout(),
                "Export file retained. Use capture to preserve a descriptor for a future restart.",
            )
        }
        [
            c,
            source,
            stream,
            export,
            descriptor,
            address,
            credentials,
            pin,
            options @ ..,
        ] if c == "ship-captured" => {
            let stream = identity(stream)?;
            let address = text(address)?.parse()?;
            let attempts = option(options, "--attempts", 3, 3)?;
            let tls = transport(credentials, pin)?;
            let prepared = prepare_saved(
                source.as_ref(),
                stream,
                export.as_ref(),
                descriptor.as_ref(),
            )?;
            deliver(
                prepared,
                &tls,
                address,
                attempts,
                &mut io::stdout(),
                "Export and descriptor retained. Retry with ship-captured.",
            )
        }
        [c, source, stream, address, credentials, pin] if c == "poll" => {
            let stream = identity(stream)?;
            let address = text(address)?.parse()?;
            let tls = transport(credentials, pin)?;
            let db = diagnostics::storage(Phase::StorageOpen, || {
                Database::open_file(source, Limits::default())
            })?;
            let revision = db.snapshot().revision();
            let mut primary =
                diagnostics::storage(Phase::Replication, || Primary::new(&db, stream))?;
            drop(db);
            println!("local verified_revision={revision}");
            let channel = diagnostics::at(Phase::Connect, || tls.connect(address))?;
            let ack = diagnostics::storage(Phase::Replication, || primary.poll(channel))?;
            println!("{}", ack_line(ack));
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}
struct Prepared {
    primary: Primary,
    export: Export,
}
fn announce(prepared: &Prepared, output: &mut impl Write) -> AppResult<()> {
    let e = prepared.primary.enrollment();
    writeln!(
        output,
        "local captured_revision={} stream={} file_identity={}",
        prepared.export.revision(),
        hex(&e.stream_id),
        hex(&e.file_identity),
    )?;
    output.flush()?;
    Ok(())
}
fn prepare(source: &Path, stream: [u8; 16], destination: &Path) -> AppResult<Prepared> {
    let mut db = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(source, Limits::default())
    })?;
    let mut primary = diagnostics::storage(Phase::Replication, || Primary::new(&db, stream))?;
    let export = diagnostics::storage(Phase::Export, || primary.export_file(&mut db, destination))?;
    drop(db);
    Ok(Prepared { primary, export })
}
fn prepare_saved(
    source: &Path,
    stream: [u8; 16],
    image: &Path,
    record: &Path,
) -> AppResult<Prepared> {
    let descriptor = diagnostics::at(Phase::StorageVerify, || Ok(capture_record::load(record)?))?;
    let db = diagnostics::storage(Phase::StorageOpen, || {
        Database::open_file(source, Limits::default())
    })?;
    let primary = diagnostics::storage(Phase::Replication, || Primary::new(&db, stream))?;
    drop(db);
    let export = diagnostics::storage(Phase::StorageVerify, || {
        primary.reopen_export_file(image, descriptor, Limits::default())
    })?;
    Ok(Prepared { primary, export })
}
fn deliver(
    mut prepared: Prepared,
    tls: &Tls,
    address: SocketAddr,
    attempts: usize,
    output: &mut impl Write,
    context: &str,
) -> AppResult<()> {
    diagnostics::at(Phase::LocalCommand, || announce(&prepared, output))?;
    let ack = ship(&mut prepared, tls, address, attempts).map_err(|error| {
        if diagnostics::settings().enabled { error } else {
            format!("transfer not confirmed; follower outcome may be uncertain: {error}. Poll on a fresh authenticated connection. {context}").into()
        }
    })?;
    diagnostics::at(Phase::LocalCommand, || {
        writeln!(output, "{}", ack_line(Some(ack)))?;
        output.flush()?;
        Ok(())
    })
}
fn ship(
    prepared: &mut Prepared,
    tls: &Tls,
    address: SocketAddr,
    attempts: usize,
) -> AppResult<DurableAck> {
    for attempt in 0..attempts {
        let result = diagnostics::at(Phase::Connect, || tls.connect(address)).and_then(|channel| {
            Ok(diagnostics::storage(Phase::Replication, || {
                prepared.primary.ship(&mut prepared.export, channel)
            })?)
        });
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
#[cfg(test)]
#[derive(Debug)]
enum SessionError {
    Authentication(Box<dyn std::error::Error + Send + Sync>),
    Replication(Error),
}
#[cfg(test)]
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authentication(e) => write!(f, "authentication/transport: {e}"),
            Self::Replication(e) => write!(f, "replication: {e}"),
        }
    }
}
#[cfg(test)]
impl std::error::Error for SessionError {}
#[cfg(test)]
fn serve_one(follower: &mut Follower, tls: &Tls, socket: TcpStream) -> Result<(), SessionError> {
    let channel = tls.accept(socket).map_err(SessionError::Authentication)?;
    follower
        .serve_authenticated(channel)
        .map_err(SessionError::Replication)
}
fn serve(follower: Follower, address: SocketAddr, tls: &Tls, maximum: usize) -> AppResult<()> {
    let listener = diagnostics::at(Phase::Listen, || Ok(TcpListener::bind(address)?))?;
    listener.set_nonblocking(true)?;
    println!("ready {}", listener.local_addr()?);
    io::stdout().flush()?;
    diagnostics::at(Phase::Replication, || {
        admission::serve(listener, follower, tls, maximum)
    })
}

#[cfg(test)]
mod pin_tests;
