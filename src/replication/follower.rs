use super::{DurableAck, Enrollment, Offer, image_digest, wire};
use crate::{
    CollectionId, Database, Error, Limits, Result, Snapshot,
    storage::{ByteStore, REGION, write_all},
};
use std::io::{Read, Write};

/// Read access to an installed image. Borrowed snapshots pin this view and
/// prevent another installation until those borrows end.
pub struct Replica(Database);
impl Replica {
    pub fn snapshot(&self) -> Snapshot<'_> {
        self.0.snapshot()
    }
    pub fn retained_ids(&self) -> Result<Vec<u64>> {
        self.0.retained_ids()
    }
    pub fn retained_snapshot(&self, id: u64) -> Result<Snapshot<'_>> {
        self.0.retained_snapshot(id)
    }
    pub fn collection_id(&self, id: u64) -> CollectionId {
        self.0.collection_id(id)
    }
}

// The manifest lease serializes all access to the two slots. Only the
// inactive slot can be replaced, and only after manifest recovery.
trait Images {
    type Store: ByteStore;
    fn create(&mut self, slot: u8) -> Result<Self::Store>;
    fn open(&mut self, slot: u8) -> Result<Self::Store>;
    fn check_manifest_headroom(&self) -> Result<()> {
        Ok(())
    }
}
struct Lease<B: ByteStore>(B);
impl<B: ByteStore> Lease<B> {
    fn new(mut store: B) -> Result<Self> {
        store.lock()?;
        Ok(Self(store))
    }
}
impl<B: ByteStore> Drop for Lease<B> {
    fn drop(&mut self) {
        self.0.unlock();
    }
}

const MANIFEST_KEY: &[u8] = b"replica-v1";

#[derive(Clone, Copy)]
struct Installed {
    slot: u8,
    offer: Offer,
}
impl Installed {
    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 49 || bytes[0] > 1 {
            return Err(Error::Corrupt);
        }
        let offer = Offer {
            revision: u64::from_le_bytes(bytes[1..9].try_into().unwrap()),
            length: u64::from_le_bytes(bytes[9..17].try_into().unwrap()),
            digest: bytes[17..].try_into().unwrap(),
        };
        offer.validate().map_err(|_| Error::Corrupt)?;
        Ok(Self {
            slot: bytes[0],
            offer,
        })
    }
}

struct Receiver<I> {
    images: I,
    manifest: Database,
    enrollment: Enrollment,
    replica_limits: Limits,
    installed: Option<(Installed, Replica)>,
}
impl<I: Images> Receiver<I> {
    fn create(
        manifest: Database,
        images: I,
        enrollment: Enrollment,
        replica_limits: Limits,
    ) -> Result<Self> {
        manifest.create_collection()?;
        let follower = Self {
            images,
            manifest,
            enrollment,
            replica_limits,
            installed: None,
        };
        follower.select(None)?;
        Ok(follower)
    }
    fn open(
        manifest: Database,
        mut images: I,
        enrollment: Enrollment,
        replica_limits: Limits,
    ) -> Result<Self> {
        let installed = {
            let record = manifest
                .snapshot()
                .get(manifest.collection_id(1), MANIFEST_KEY)
                .map_err(|e| {
                    if e == Error::NotFound {
                        Error::Corrupt
                    } else {
                        e
                    }
                })?
                .ok_or(Error::Corrupt)?;
            let bytes = record.as_bytes().map_err(|_| Error::Corrupt)?;
            let (pair, active) = bytes.split_at_checked(32).ok_or(Error::Corrupt)?;
            if pair[..16] != enrollment.stream_id || pair[16..] != enrollment.file_identity {
                return Err(Error::ForeignStore);
            }
            if active.is_empty() {
                None
            } else {
                let selected = Installed::decode(active)?;
                let mut image = Lease::new(images.open(selected.slot).map_err(|e| {
                    if e == Error::NotFound {
                        Error::Corrupt
                    } else {
                        e
                    }
                })?)?;
                let replica =
                    Self::verify(&mut image.0, selected.offer, enrollment, replica_limits)?;
                Some((selected, replica))
            }
        };
        Ok(Self {
            images,
            manifest,
            enrollment,
            replica_limits,
            installed,
        })
    }
    fn select(&self, selected: Option<Installed>) -> Result<()> {
        let mut bytes = [0; 81];
        bytes[..16].copy_from_slice(&self.enrollment.stream_id);
        bytes[16..32].copy_from_slice(&self.enrollment.file_identity);
        let len = if let Some(Installed { slot, offer }) = selected {
            bytes[32] = slot;
            bytes[33..41].copy_from_slice(&offer.revision.to_le_bytes());
            bytes[41..49].copy_from_slice(&offer.length.to_le_bytes());
            bytes[49..].copy_from_slice(&offer.digest);
            bytes.len()
        } else {
            32
        };
        let value = self.manifest.bytes(&bytes[..len])?;
        let mut tx = self.manifest.begin();
        tx.put(self.manifest.collection_id(1), MANIFEST_KEY, &value)?;
        tx.commit()?;
        Ok(())
    }
    fn verify(
        image: &mut dyn ByteStore,
        offer: Offer,
        enrollment: Enrollment,
        limits: Limits,
    ) -> Result<Replica> {
        if image_digest(image, offer.length)? != offer.digest {
            return Err(Error::Corrupt);
        }
        Database::decode_replica(
            image,
            enrollment.file_identity,
            offer.revision,
            offer.length,
            limits,
        )
        .map(Replica)
    }
    fn progress(&self) -> Result<Option<DurableAck>> {
        if self.manifest.requires_recovery() {
            return Err(Error::RecoveryRequired);
        }
        Ok(self
            .installed
            .as_ref()
            .map(|(s, _)| s.offer.ack(self.enrollment)))
    }
    fn handle(&mut self, channel: &mut (impl Read + Write)) -> Result<wire::Reply> {
        let progress = self.progress()?;
        let Some(offer) = wire::read_request(channel, self.enrollment)? else {
            return Ok(progress.map_or(wire::Reply::Empty, wire::Reply::Installed));
        };
        if let Some((prior, _)) = &self.installed {
            if offer.revision < prior.offer.revision || offer == prior.offer {
                return Ok(wire::Reply::Installed(prior.offer.ack(self.enrollment)));
            }
            if offer.revision == prior.offer.revision {
                return Err(Error::Conflict);
            }
        }
        let slot = self
            .installed
            .as_ref()
            .map_or(0, |(prior, _)| 1 - prior.slot);
        self.images.check_manifest_headroom()?;
        let selected = Installed { slot, offer };
        let mut image = Lease::new(self.images.create(slot)?)?;
        if image.0.length()? != 0 {
            return Err(Error::Corrupt);
        }
        wire::reply(channel, Ok(wire::Reply::Ready))?;
        let mut buffer = [0; REGION];
        let mut offset = 0;
        while offset < offer.length {
            let chunk = &mut buffer[..(offer.length - offset).min(REGION as u64) as usize];
            channel.read_exact(chunk).map_err(|_| Error::Io)?;
            write_all(&mut image.0, offset, chunk)?;
            offset += chunk.len() as u64;
        }
        image.0.sync()?;
        let replica = Self::verify(&mut image.0, offer, self.enrollment, self.replica_limits)?;
        drop(image);
        self.select(Some(selected))?;
        self.installed = Some((selected, replica));
        Ok(wire::Reply::Installed(offer.ack(self.enrollment)))
    }
    fn serve(&mut self, mut channel: impl Read + Write) -> Result<()> {
        let result = self.handle(&mut channel);
        let sent = wire::reply(&mut channel, result);
        result?;
        sent
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub use native::{Follower, FollowerStatus, MANIFEST_LIMIT_BYTES};

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod native {
    use super::*;
    use crate::storage::FileStore;
    use std::{
        fs, io,
        path::{Path, PathBuf},
    };

    struct Directory(PathBuf);
    impl Directory {
        fn new(path: &Path) -> Result<Self> {
            let path = path.canonicalize().map_err(io_error)?;
            if !path.is_dir() {
                return Err(Error::Invalid);
            }
            Ok(Self(path))
        }
        fn manifest(&self) -> PathBuf {
            self.0.join("replica-manifest.wdb")
        }
        fn slot(&self, slot: u8) -> PathBuf {
            self.0.join(if slot == 0 {
                "replica-0.wdb"
            } else {
                "replica-1.wdb"
            })
        }
    }
    fn io_error(error: io::Error) -> Error {
        match error.kind() {
            io::ErrorKind::NotFound => Error::NotFound,
            _ => Error::Io,
        }
    }
    impl Images for Directory {
        type Store = FileStore;
        fn create(&mut self, slot: u8) -> Result<Self::Store> {
            let path = self.slot(slot);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_error(e)),
            }
            FileStore::create(path)
        }
        fn open(&mut self, slot: u8) -> Result<Self::Store> {
            FileStore::open(self.slot(slot))
        }
        fn check_manifest_headroom(&self) -> Result<()> {
            let length = fs::metadata(self.manifest()).map_err(io_error)?.len();
            if length > MANIFEST_LIMIT_BYTES - 4 * REGION as u64 {
                return Err(Error::Limit);
            }
            Ok(())
        }
    }

    /// Admission ceiling, including reserved room for one selection commit.
    pub const MANIFEST_LIMIT_BYTES: u64 = 1024 * 1024;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FollowerStatus {
        pub enrollment: Enrollment,
        pub durable_ack: Option<DurableAck>,
        pub requires_recovery: bool,
        pub manifest_bytes: u64,
        pub manifest_limit_bytes: u64,
        pub maintenance_required: bool,
        pub selected_slot: Option<u8>,
    }

    /// Exclusive lease on one passive follower in a dedicated, private local
    /// directory. Cooperating processes open the manifest through this API.
    /// Do not rename the directory or modify its files while it is in use.
    pub struct Follower(Receiver<Directory>);
    impl Follower {
        /// Enroll in an existing empty directory. If enrollment is interrupted,
        /// use a fresh directory. Installation failures are recovered by open.
        /// The supplied limits apply to replica images. The manifest uses
        /// internal limits.
        pub fn create(
            directory: impl AsRef<Path>,
            enrollment: Enrollment,
            limits: Limits,
        ) -> Result<Self> {
            enrollment.validate()?;
            limits.validate()?;
            let images = Directory::new(directory.as_ref())?;
            if fs::read_dir(&images.0)
                .map_err(io_error)?
                .next()
                .transpose()
                .map_err(io_error)?
                .is_some()
            {
                return Err(Error::AlreadyExists);
            }
            let manifest = Database::create_on(
                FileStore::create(images.manifest())?,
                enrollment.stream_id,
                Limits::default(),
            )?;
            Receiver::create(manifest, images, enrollment, limits).map(Self)
        }
        /// Recover the manifest and verify the selected image before exposing
        /// progress. The unselected slot never participates in recovery.
        /// The supplied limits apply to replica images.
        pub fn open(
            directory: impl AsRef<Path>,
            enrollment: Enrollment,
            limits: Limits,
        ) -> Result<Self> {
            enrollment.validate()?;
            limits.validate()?;
            let images = Directory::new(directory.as_ref())?;
            let manifest = Database::open_file(images.manifest(), Limits::default())?;
            Receiver::open(manifest, images, enrollment, limits).map(Self)
        }
        pub fn progress(&self) -> Result<Option<DurableAck>> {
            self.0.progress()
        }
        pub fn status(&self) -> Result<FollowerStatus> {
            let requires_recovery = self.0.manifest.requires_recovery();
            let manifest_bytes = fs::metadata(self.0.images.manifest())
                .map_err(io_error)?
                .len();
            Ok(FollowerStatus {
                enrollment: self.0.enrollment,
                durable_ack: if requires_recovery {
                    None
                } else {
                    self.progress()?
                },
                requires_recovery,
                manifest_bytes,
                manifest_limit_bytes: MANIFEST_LIMIT_BYTES,
                maintenance_required: manifest_bytes > MANIFEST_LIMIT_BYTES - 4 * REGION as u64,
                selected_slot: self.0.installed.as_ref().map(|(selected, _)| selected.slot),
            })
        }
        /// Verify the authoritative image again, then remove only the reserved
        /// inactive slot. A crash can leave that slot present, so retry is safe.
        /// Other entries in the dedicated directory are never removed.
        pub fn cleanup_inactive(&mut self) -> Result<usize> {
            self.progress()?;
            let selected = self.0.installed.as_ref().map(|(selected, _)| *selected);
            if let Some(selected) = selected {
                let mut image = Lease::new(self.0.images.open(selected.slot)?)?;
                Receiver::<Directory>::verify(
                    &mut image.0,
                    selected.offer,
                    self.0.enrollment,
                    self.0.replica_limits,
                )?;
            }
            let mut removed = 0;
            for slot in 0..2 {
                if selected.is_some_and(|selected| selected.slot == slot) {
                    continue;
                }
                let path = self.0.images.slot(slot);
                match fs::symlink_metadata(&path) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error(error)),
                    Ok(metadata) if !metadata.file_type().is_file() => return Err(Error::Invalid),
                    Ok(_) => {
                        fs::remove_file(path).map_err(io_error)?;
                        removed += 1;
                    }
                }
            }
            Ok(removed)
        }
        /// Close this lease, compact the authoritative manifest using atomic
        /// replacement, then reopen and verify the selected image. A competing
        /// process may acquire the lease during this handoff and cause Busy.
        /// After any error, recover with open before serving more exchanges.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        pub fn compact_manifest(self) -> Result<Self> {
            self.progress()?;
            let directory = self.0.images.0.clone();
            let enrollment = self.0.enrollment;
            let limits = self.0.replica_limits;
            let manifest = self.0.images.manifest();
            drop(self);
            Database::compact_file(manifest, Limits::default())?;
            Self::open(directory, enrollment, limits)
        }
        /// The last verified read view remains available if an installation
        /// requires recovery. Use progress() for the durable acknowledgment.
        pub fn replica(&self) -> Option<&Replica> {
            self.0.installed.as_ref().map(|(_, view)| view)
        }
        /// Serve one query or offer on a duplex channel already authenticated
        /// to the configured primary. Set deadlines and close the channel at
        /// the end of the exchange, including on error.
        pub fn serve_authenticated(&mut self, channel: impl Read + Write) -> Result<()> {
            self.0.serve(channel)
        }
    }
}

#[cfg(test)]
mod tests;
