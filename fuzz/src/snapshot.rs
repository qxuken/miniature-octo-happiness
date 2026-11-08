use std::{
    fs,
    io::{self, Cursor, Read, Write},
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use weekend_db::{
    Database, Error, Limits, Result,
    replication::{DurableAck, Enrollment, Follower, Primary},
    storage::MemoryStore,
};

const STREAM: [u8; 16] = [42; 16];
const ID: [u8; 16] = [43; 16];

struct Channel {
    incoming: Cursor<Vec<u8>>,
    outgoing: Vec<u8>,
    fail_write: Option<usize>,
    writes: usize,
}
impl Channel {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            incoming: Cursor::new(bytes),
            outgoing: Vec::new(),
            fail_write: None,
            writes: 0,
        }
    }
}
impl Read for Channel {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.incoming.read(out)
    }
}
impl Write for Channel {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let step = self.writes;
        self.writes += 1;
        if self.fail_write == Some(step) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.outgoing.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn reply(ack: DurableAck) -> Vec<u8> {
    let mut bytes = vec![2];
    bytes.extend_from_slice(&ack.stream_id);
    bytes.extend_from_slice(&ack.revision.to_le_bytes());
    bytes.extend_from_slice(&ack.digest);
    bytes
}

fn expected_poll(input: &[u8], prior: DurableAck) -> Result<Option<DurableAck>> {
    let Some(&tag) = input.first() else {
        return Err(Error::Io);
    };
    match tag {
        0 | 1 => Err(Error::Conflict),
        2 if input.len() < 57 => Err(Error::Io),
        2 => {
            let ack = DurableAck {
                stream_id: input[1..17].try_into().unwrap(),
                revision: u64::from_le_bytes(input[17..25].try_into().unwrap()),
                digest: input[25..57].try_into().unwrap(),
            };
            if ack.stream_id != STREAM || ack.revision > 2 {
                Err(Error::ForeignStore)
            } else if ack.revision < prior.revision
                || (ack.revision == prior.revision && ack != prior)
            {
                Err(Error::Conflict)
            } else {
                Ok(Some(ack))
            }
        }
        3 => Err(Error::UnsupportedFormat),
        4 => Err(Error::ForeignStore),
        5 => Err(Error::Conflict),
        6 => Err(Error::Limit),
        7 => Err(Error::Corrupt),
        8 => Err(Error::RecoveryRequired),
        9 => Err(Error::Invalid),
        10 => Err(Error::Io),
        _ => Err(Error::UnsupportedFormat),
    }
}

pub fn snapshot_frames(input: &[u8]) {
    if input.len() > 4096 {
        return;
    }
    let db = Database::create_on(MemoryStore::new().unwrap(), ID, Limits::default()).unwrap();
    db.begin().commit().unwrap();
    db.begin().commit().unwrap();
    let mut primary = Primary::new(&db, STREAM).unwrap();
    let prior = DurableAck {
        stream_id: STREAM,
        revision: 1,
        digest: [3; 32],
    };
    assert_eq!(primary.poll(Channel::new(reply(prior))), Ok(Some(prior)));
    let expected = expected_poll(input, prior);
    assert_eq!(primary.poll(Channel::new(input.to_vec())), expected);
    let retained = expected.ok().flatten().unwrap_or(prior);
    assert_eq!(
        primary.poll(Channel::new(reply(retained))),
        Ok(Some(retained))
    );
}

struct Fixture {
    offers: [Vec<u8>; 2],
    acknowledgments: [DurableAck; 2],
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let mut db =
            Database::create_on(MemoryStore::new().unwrap(), ID, Limits::default()).unwrap();
        let c = db.create_collection().unwrap();
        let mut primary = Primary::new(&db, STREAM).unwrap();
        let offers = std::array::from_fn(|index| {
            let mut tx = db.begin();
            tx.put(
                c,
                b"value",
                &db.unsigned(if index == 0 { 10 } else { 20 }).unwrap(),
            )
            .unwrap();
            tx.commit().unwrap();
            drop(tx);
            if index == 0 {
                db.retain_snapshot(&db.snapshot()).unwrap();
            }
            let mut export = primary
                .export_to(&mut db, MemoryStore::new().unwrap())
                .unwrap();
            let mut channel = Channel::new(vec![1, 0]);
            assert_eq!(
                primary.ship(&mut export, &mut channel),
                Err(Error::Conflict)
            );
            channel.outgoing
        });
        let acknowledgments = std::array::from_fn(|index| DurableAck {
            stream_id: STREAM,
            revision: if index == 0 { 3 } else { 4 },
            digest: offers[index][57..89].try_into().unwrap(),
        });
        Fixture {
            offers,
            acknowledgments,
        }
    })
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "weekend-snapshot-fuzz-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => panic!("{e}"),
            }
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

pub fn snapshot_install_schedule(input: &[u8]) {
    if input.len() < 2 {
        return;
    }
    let fixture = fixture();
    let directory = Directory::new();
    let enrollment = Enrollment {
        stream_id: STREAM,
        file_identity: ID,
    };
    let mut follower = Follower::create(&directory.0, enrollment, Limits::default()).unwrap();
    let mut installed: Option<usize> = None;
    for &[control, parameter] in input.as_chunks::<2>().0.iter().take(6) {
        let index = usize::from(control & 1);
        let mode = (control >> 1) % 8;
        if mode <= 4 || mode == 7 {
            let mut bytes = fixture.offers[index].clone();
            if mode == 1 {
                bytes.truncate(89 + usize::from(parameter) % (bytes.len() - 89));
            }
            if mode == 2 {
                *bytes.last_mut().unwrap() ^= parameter | 1;
            }
            let malformed = match parameter % 4 {
                0 => Error::UnsupportedFormat,
                1 => Error::ForeignStore,
                2 => Error::Invalid,
                _ => Error::Io,
            };
            if mode == 7 {
                match parameter % 4 {
                    0 => bytes[0] ^= 1,
                    1 => bytes[9] ^= 1,
                    2 => bytes[8] = 2,
                    _ => bytes.truncate(usize::from(parameter) % 41),
                }
            }
            let mut channel = Channel::new(bytes);
            if mode == 3 {
                channel.fail_write = Some(0);
            }
            if mode == 4 {
                channel.fail_write = Some(1);
            }
            let duplicate = installed.is_some_and(|prior| prior >= index);
            let expected = if mode == 7 {
                Err(malformed)
            } else if mode == 3 {
                Err(Error::Io)
            } else if duplicate {
                Ok(())
            } else {
                match mode {
                    1 => Err(Error::Io),
                    2 => Err(Error::Corrupt),
                    4 => Err(Error::Io),
                    _ => Ok(()),
                }
            };
            assert_eq!(follower.serve_authenticated(&mut channel), expected);
            if !duplicate && (mode == 0 || mode == 4) {
                installed = Some(index);
            }
        } else if mode == 5 {
            let mut query = fixture.offers[0][..41].to_vec();
            query[8] = 0;
            let mut channel = Channel::new(query);
            assert_eq!(follower.serve_authenticated(&mut channel), Ok(()));
            assert_eq!(
                channel.outgoing,
                installed.map_or_else(|| vec![0], |i| reply(fixture.acknowledgments[i]))
            );
        } else {
            follower.cleanup_inactive().unwrap();
        }
        // Every outcome is checked through a fresh lease and manifest recovery.
        drop(follower);
        follower = Follower::open(&directory.0, enrollment, Limits::default()).unwrap();
        assert_eq!(
            follower.progress(),
            Ok(installed.map(|i| fixture.acknowledgments[i]))
        );
        if let Some(index) = installed {
            let replica = follower.replica().unwrap();
            let c = replica.collection_id(1);
            assert_eq!(
                replica
                    .snapshot()
                    .get(c, b"value")
                    .unwrap()
                    .unwrap()
                    .as_u64(),
                Ok(if index == 0 { 10 } else { 20 })
            );
            assert_eq!(replica.retained_ids().unwrap(), [1]);
            assert_eq!(
                replica
                    .retained_snapshot(1)
                    .unwrap()
                    .get(c, b"value")
                    .unwrap()
                    .unwrap()
                    .as_u64(),
                Ok(10)
            );
        }
    }
}

#[test]
fn snapshot_cases_replay() {
    for tag in 0..=11 {
        snapshot_frames(&[tag]);
    }
    for revision in 0..=3 {
        for stream_id in [STREAM, [0; 16]] {
            for digest in [[3; 32], [4; 32]] {
                snapshot_frames(&reply(DurableAck {
                    stream_id,
                    revision,
                    digest,
                }));
            }
        }
    }
    for mode in 0..8 {
        snapshot_install_schedule(&[0, 0, mode * 2 + 1, 7, 1, 0, 10, 0]);
    }
    for malformed in 0..4 {
        snapshot_install_schedule(&[15, malformed, 1, 0]);
    }
}
