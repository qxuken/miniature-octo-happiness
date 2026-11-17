use std::{
    fs,
    io::{self, Read, Write},
    net::SocketAddr,
    path::Path,
    time::Instant,
};
use weekend_db::{
    CollectionId, Database, Error, Limits, Snapshot,
    replication::{DurableAck, Enrollment, Follower, Primary},
};

// Same pins, ALPN and deadlines as the CLI. This driver needs only the client.
#[allow(dead_code)]
#[path = "../../src/tls.rs"]
mod tls;

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const DAYS: u64 = 30;
const ROWS: u8 = 32;
const BYTES: usize = 32 * 1024;
const STREAM: [u8; 16] = [0x11; 16];
const HEARTBEAT: &[u8] = b"heartbeat";

pub(super) fn run() -> AppResult<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [cmd, source] if cmd == "seed" => seed(Path::new(source)),
        [cmd, source, staging, peer, credentials, pin] if cmd == "send" => send(
            Path::new(source),
            Path::new(staging),
            peer.to_str().ok_or("expected a socket address")?.parse()?,
            &tls::Tls::new(
                &tls::Credentials::load(Path::new(credentials))?,
                tls::bounded_file(Path::new(pin), 65536)?.into(),
            )?,
        ),
        [cmd, follower, identity] if cmd == "verify-follower" => {
            let identity = identity.to_str().ok_or("expected file identity in hex")?;
            if identity.len() != 32 || !identity.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("expected 32 hexadecimal digits".into());
            }
            verify_follower(
                Path::new(follower),
                u128::from_str_radix(identity, 16)?.to_be_bytes(),
            )
        }
        _ => Err("usage: live_month seed SOURCE | send SOURCE NEW_STAGING ADDRESS CREDENTIALS_DIR PEER_CERT | verify-follower DIR FILE_IDENTITY_HEX".into()),
    }
}

fn put_day(db: &Database, day: u64) -> weekend_db::Result<u64> {
    let collection = db.collection_id(1);
    let mut tx = db.begin();
    let mut bytes = [0; BYTES];
    for row in 0..ROWS {
        bytes.fill((day as u8) * 5 + row);
        tx.put(collection, &[row], &db.bytes(&bytes)?)?;
    }
    if day == 0 {
        tx.put(collection, HEARTBEAT, &db.unsigned(0)?)?;
    }
    tx.commit()
}

fn heartbeat(db: &Database, day: u64) -> weekend_db::Result<u64> {
    let mut tx = db.begin();
    tx.put(db.collection_id(1), HEARTBEAT, &db.unsigned(day)?)?;
    tx.commit()
}

fn seed(path: &Path) -> AppResult<()> {
    let db = Database::create_file(path, Limits::default())?;
    assert_eq!(db.create_collection()?.as_u64(), 1);
    assert_eq!(put_day(&db, 0)?, 2);
    assert_eq!(db.retain_snapshot(&db.snapshot())?, 1);
    let primary = Primary::new(&db, STREAM)?;
    println!("{}", hex(&primary.enrollment().file_identity));
    Ok(())
}

// The first flush sends the offer. A new install flushes the image before
// reading the final reply. On selected days consume the first authenticated
// byte of that reply and give ship() an I/O error instead.
struct DuringShip<S, F> {
    channel: S,
    before_first_reply: Option<F>,
    flushes: u8,
    lose_reply: bool,
    discarded: bool,
    upload_flushed: Option<Instant>,
    reply_after_upload_us: Option<u128>,
}
impl<S: Read, F: FnOnce() -> io::Result<()>> Read for DuringShip<S, F> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if let Some(write) = self.before_first_reply.take() {
            write()?;
        }
        if self.lose_reply && self.flushes == 2 {
            self.channel.read_exact(&mut [0])?;
            self.record_reply();
            self.discarded = true;
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let bytes = self.channel.read(out)?;
        if bytes != 0 {
            self.record_reply();
        }
        Ok(bytes)
    }
}
impl<S, F> DuringShip<S, F> {
    fn record_reply(&mut self) {
        if let Some(start) = self.upload_flushed.take() {
            self.reply_after_upload_us = Some(start.elapsed().as_micros());
        }
    }
}
impl<S: Write, F> Write for DuringShip<S, F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.channel.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.channel.flush()?;
        self.flushes += 1;
        if self.flushes == 2 {
            self.upload_flushed = Some(Instant::now());
        }
        Ok(())
    }
}

fn send(source: &Path, staging: &Path, address: SocketAddr, tls: &tls::Tls) -> AppResult<()> {
    let limits = Limits::default();
    let db = Database::open_file(source, limits)?;
    assert_eq!(db.snapshot().revision(), 3);
    fs::create_dir(staging)?;
    let mut primary = Primary::new(&db, STREAM)?;
    assert_eq!(
        primary.poll_live(&db, tls.connect(address)?)?.follower,
        None
    );
    println!(
        "day,local_revision,captured_revision,ship_revision,poll_revision,gap,image_bytes,commit_us,retain_us,capture_us,heartbeat_us,ship_us,post_upload_us,poll_us,digest"
    );

    for day in 1..=DAYS + 1 {
        let final_export = day > DAYS;
        let (commit_us, retain_us) = if final_export {
            (0, 0)
        } else {
            let start = Instant::now();
            assert_eq!(put_day(&db, day)?, day * 3 + 1);
            let commit_us = start.elapsed().as_micros();
            let start = Instant::now();
            assert_eq!(db.retain_snapshot(&db.snapshot())?, day + 1);
            (commit_us, start.elapsed().as_micros())
        };
        let path = staging.join(format!("snapshot-{day:02}.wdb"));
        let start = Instant::now();
        let mut export = primary.export_live_file(&db, &path, limits)?;
        let capture_us = start.elapsed().as_micros();
        let descriptor = export.descriptor();
        let expected_capture = if final_export {
            DAYS * 3 + 3
        } else {
            day * 3 + 2
        };
        assert_eq!(descriptor.revision, expected_capture);
        let expected = DurableAck {
            stream_id: STREAM,
            revision: descriptor.revision,
            digest: descriptor.digest,
        };

        let lose_reply = !final_export && day % 7 == 0;
        let mut heartbeat_us = 0;
        let start = Instant::now();
        let mut channel = DuringShip {
            channel: tls.connect(address)?,
            before_first_reply: Some(|| {
                if !final_export {
                    let start = Instant::now();
                    let committed = std::thread::scope(|scope| {
                        scope.spawn(|| heartbeat(&db, day)).join().expect("writer")
                    })
                    .map_err(io::Error::other)?;
                    assert_eq!(committed, day * 3 + 3);
                    heartbeat_us = start.elapsed().as_micros();
                }
                Ok(())
            }),
            flushes: 0,
            lose_reply,
            discarded: false,
            upload_flushed: None,
            reply_after_upload_us: None,
        };
        let shipped = primary.ship(&mut export, &mut channel);
        let ship_us = start.elapsed().as_micros();
        assert_eq!(channel.discarded, lose_reply);
        let post_upload_us = channel
            .reply_after_upload_us
            .ok_or("missing upload reply")?;
        drop(channel);
        let ship_revision = if lose_reply {
            assert_eq!(shipped, Err(Error::Io));
            String::new()
        } else {
            assert_eq!(shipped?, expected);
            expected.revision.to_string()
        };

        let start = Instant::now();
        let observed = primary.poll_live(&db, tls.connect(address)?)?;
        let poll_us = start.elapsed().as_micros();
        let gap = u64::from(!final_export);
        assert_eq!(observed.local_revision, descriptor.revision + gap);
        assert_eq!(observed.follower, Some(expected));
        assert_eq!(observed.revision_gap(), Some(gap));
        println!(
            "{day},{},{},{ship_revision},{},{gap},{},{commit_us},{retain_us},{capture_us},{heartbeat_us},{ship_us},{post_upload_us},{poll_us},{}",
            observed.local_revision,
            descriptor.revision,
            expected.revision,
            descriptor.length,
            hex(&descriptor.digest)
        );
        io::stdout().flush()?;
        drop(export);
        if !final_export {
            fs::remove_file(path)?;
        }
    }
    drop(db);
    let reopened = Database::open_file(source, limits)?;
    verify_history(
        reopened.collection_id(1),
        reopened.snapshot(),
        reopened.retained_ids()?,
        |id| reopened.retained_snapshot(id),
    )
}

fn verify_day(
    collection: CollectionId,
    snapshot: Snapshot<'_>,
    day: u64,
    heartbeat: u64,
) -> AppResult<()> {
    for row in 0..ROWS {
        let value = snapshot.get(collection, &[row])?.ok_or("missing row")?;
        let bytes = value.as_bytes()?;
        assert_eq!(bytes.len(), BYTES);
        assert!(bytes.iter().all(|byte| *byte == (day as u8) * 5 + row));
    }
    assert_eq!(
        snapshot
            .get(collection, HEARTBEAT)?
            .ok_or("missing heartbeat")?
            .as_u64()?,
        heartbeat
    );
    Ok(())
}

fn verify_history<'db>(
    collection: CollectionId,
    current: Snapshot<'db>,
    retained_ids: Vec<u64>,
    retained_snapshot: impl Fn(u64) -> weekend_db::Result<Snapshot<'db>>,
) -> AppResult<()> {
    assert_eq!(current.revision(), DAYS * 3 + 3);
    verify_day(collection, current, DAYS, DAYS)?;
    assert_eq!(retained_ids, (1..=DAYS + 1).collect::<Vec<_>>());
    for day in 0..=DAYS {
        let retained = retained_snapshot(day + 1)?;
        assert_eq!(retained.revision(), if day == 0 { 2 } else { day * 3 + 1 });
        verify_day(collection, retained, day, day.saturating_sub(1))?;
    }
    Ok(())
}

fn verify_follower(path: &Path, file_identity: [u8; 16]) -> AppResult<()> {
    let follower = Follower::open(
        path,
        Enrollment {
            stream_id: STREAM,
            file_identity,
        },
        Limits::default(),
    )?;
    let ack = follower.progress()?.ok_or("follower is empty")?;
    assert_eq!(ack.revision, DAYS * 3 + 3);
    let replica = follower.replica().ok_or("missing selected database")?;
    verify_history(
        replica.collection_id(1),
        replica.snapshot(),
        replica.retained_ids()?,
        |id| replica.retained_snapshot(id),
    )?;
    println!(
        "reopened revision={} retained={} digest={}",
        ack.revision,
        DAYS + 1,
        hex(&ack.digest)
    );
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
