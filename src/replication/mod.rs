//! One primary and one passive snapshot follower. Local commits never use the
//! network. The application schedules exports and supplies a duplex channel
//! authenticated to the configured peer (for example, mutual TLS with peer
//! pinning). Stream IDs and SHA-256 digests are not peer credentials.
use crate::{
    Database, Error, Limits, Result,
    memory::Memory,
    storage::{APPEND_START, ByteStore, MAX_IMAGE_BYTES, REGION, Storage, read_all},
};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

#[cfg(any(test, target_os = "macos", target_os = "linux", windows))]
mod follower;
#[cfg(test)]
mod tests;
mod wire;

#[cfg(any(test, target_os = "macos", target_os = "linux", windows))]
pub use follower::Replica;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub use follower::{Follower, FollowerStatus, MANIFEST_LIMIT_BYTES};

/// Configure both peers with the same enrollment. Use a fresh, nonzero stream
/// ID whenever restoring, forking or reenrolling a primary history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enrollment {
    pub stream_id: [u8; 16],
    pub file_identity: [u8; 16],
}
impl Enrollment {
    fn validate(self) -> Result<()> {
        if self.stream_id == [0; 16] || self.file_identity == [0; 16] {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

/// The follower has durably selected this exact image. Later revisions can
/// supersede it, including any historical roots it no longer retains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DurableAck {
    pub stream_id: [u8; 16],
    pub revision: u64,
    pub digest: [u8; 32],
}

/// A verified local revision sampled before one authenticated poll, and the
/// follower's durable receipt from that exchange. An empty follower has no receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplicaObservation {
    pub local_revision: u64,
    pub follower: Option<DurableAck>,
}
impl ReplicaObservation {
    /// Revision difference, including metadata commits.
    pub fn revision_gap(&self) -> Option<u64> {
        self.local_revision.checked_sub(self.follower?.revision)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Offer {
    revision: u64,
    length: u64,
    digest: [u8; 32],
}

/// Identifies an exact local capture. Persist this descriptor before starting
/// a shipment to resume that image after a sender restart. The descriptor is
/// local state. A follower reports its own durable acknowledgment separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportDescriptor {
    pub enrollment: Enrollment,
    pub revision: u64,
    pub length: u64,
    pub digest: [u8; 32],
}
impl ExportDescriptor {
    fn offer(self) -> Offer {
        Offer {
            revision: self.revision,
            length: self.length,
            digest: self.digest,
        }
    }
}
/// Maximum snapshot frame a receiver can elect to accept.
pub const MAX_TRANSFER_BYTES: u64 = 1 << 30;
impl Offer {
    fn validate(self) -> Result<()> {
        if !(APPEND_START as u64..=MAX_TRANSFER_BYTES).contains(&self.length) {
            return Err(Error::Limit);
        }
        Ok(())
    }
    fn ack(self, enrollment: Enrollment) -> DurableAck {
        DurableAck {
            stream_id: enrollment.stream_id,
            revision: self.revision,
            digest: self.digest,
        }
    }
}

/// An immutable, verified compact image. Keep it for retries and drop it to
/// release the destination lease. The caller owns the destination's cleanup.
pub struct Export {
    enrollment: Enrollment,
    offer: Offer,
    storage: Storage,
}
impl Export {
    pub fn revision(&self) -> u64 {
        self.offer.revision
    }
    pub fn descriptor(&self) -> ExportDescriptor {
        ExportDescriptor {
            enrollment: self.enrollment,
            revision: self.offer.revision,
            length: self.offer.length,
            digest: self.offer.digest,
        }
    }
}

/// Sender for one configured follower. Reconstruct this from the recovered
/// primary before polling after a restart.
pub struct Primary {
    enrollment: Enrollment,
    verified_revision: u64,
    progress: Option<DurableAck>,
}
impl Primary {
    pub fn new(source: &Database, stream_id: [u8; 16]) -> Result<Self> {
        let (file_identity, verified_revision) = source.replication_head()?;
        let enrollment = Enrollment {
            stream_id,
            file_identity,
        };
        enrollment.validate()?;
        Ok(Self {
            enrollment,
            verified_revision,
            progress: None,
        })
    }
    pub fn enrollment(&self) -> Enrollment {
        self.enrollment
    }
    /// Quiescent local export to an empty store. The returned image no longer
    /// borrows the source and can be sent while new commits proceed.
    pub fn export_to(
        &mut self,
        source: &mut Database,
        destination: impl ByteStore + 'static,
    ) -> Result<Export> {
        let revision = self.check_source(source)?;
        self.capture(source, destination, revision)
    }
    /// Create a fresh local export file. The caller owns this path and may
    /// remove it once the export is dropped, including after a failed export.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn export_file(
        &mut self,
        source: &mut Database,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Export> {
        let revision = self.check_source(source)?;
        self.capture(source, crate::storage::FileStore::create(path)?, revision)
    }
    /// Pin the current acknowledged head and retained roots of an open
    /// primary. Source writes can proceed during destination encoding and I/O.
    /// The copy has its own limits. Pinned values remain charged to the
    /// source until encoding releases them. The destination must be empty.
    /// On failure the caller may need to discard a partial destination.
    pub fn export_live_to(
        &mut self,
        source: &Database,
        destination: impl ByteStore + 'static,
        limits: Limits,
    ) -> Result<Export> {
        self.capture_live(source, limits, || Ok(destination))
    }
    /// Live export to a fresh file. Validate the source and pin the revision
    /// before creating the destination. The caller owns destination cleanup.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn export_live_file(
        &mut self,
        source: &Database,
        path: impl AsRef<std::path::Path>,
        limits: Limits,
    ) -> Result<Export> {
        self.capture_live(source, limits, || crate::storage::FileStore::create(path))
    }
    /// Reopen a previously captured image using its trusted local descriptor.
    /// Verify the exact length and digest, witnessed revision, identity and
    /// complete graph under one exclusive lease. No bytes are written.
    pub fn reopen_export(
        &self,
        image: impl ByteStore + 'static,
        descriptor: ExportDescriptor,
        limits: Limits,
    ) -> Result<Export> {
        if descriptor.enrollment != self.enrollment || descriptor.revision > self.verified_revision
        {
            return Err(Error::ForeignStore);
        }
        let offer = descriptor.offer();
        offer.validate()?;
        let mem = Memory::new(limits)?;
        let mut storage = Storage::locked(image, &mem)?;
        let backend = storage.image_bytes();
        if image_digest(backend, offer.length)? != offer.digest {
            return Err(Error::Corrupt);
        }
        let (storage, _) = storage.verified_replica(
            &mem,
            self.enrollment.file_identity,
            offer.revision,
            offer.length,
        )?;
        Ok(Export {
            enrollment: self.enrollment,
            offer,
            storage,
        })
    }
    /// Reopen an exact saved capture on this host with a read-only file handle.
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    pub fn reopen_export_file(
        &self,
        path: impl AsRef<std::path::Path>,
        descriptor: ExportDescriptor,
        limits: Limits,
    ) -> Result<Export> {
        self.reopen_export(
            crate::storage::FileStore::open_read_only(path)?,
            descriptor,
            limits,
        )
    }
    fn check_source(&self, source: &Database) -> Result<u64> {
        let (identity, revision) = source.replication_head()?;
        self.check_revision(identity, revision)
    }
    fn check_revision(&self, identity: [u8; 16], revision: u64) -> Result<u64> {
        if identity != self.enrollment.file_identity || revision < self.verified_revision {
            return Err(Error::ForeignStore);
        }
        Ok(revision)
    }
    fn capture(
        &mut self,
        source: &mut Database,
        destination: impl ByteStore + 'static,
        revision: u64,
    ) -> Result<Export> {
        let (storage, stats) = source.compact_copy(destination)?;
        self.finish_capture(storage, revision, stats.after_bytes)
    }
    fn capture_live<B: ByteStore + 'static>(
        &mut self,
        source: &Database,
        limits: Limits,
        destination: impl FnOnce() -> Result<B>,
    ) -> Result<Export> {
        let mem = Memory::new(limits)?;
        let state = source.replication_snapshot(&mem)?;
        let revision = self.check_revision(state.identity, state.head.revision)?;
        let (mut storage, bytes) = state.encode(destination()?, &mem)?;
        storage.verify(&mem)?;
        self.finish_capture(storage, revision, bytes)
    }
    fn finish_capture(
        &mut self,
        mut storage: Storage,
        revision: u64,
        length: u64,
    ) -> Result<Export> {
        let digest = image_digest(storage.image_bytes(), length)?;
        let offer = Offer {
            revision,
            length,
            digest,
        };
        offer.validate()?;
        self.verified_revision = revision;
        Ok(Export {
            enrollment: self.enrollment,
            offer,
            storage,
        })
    }
    /// Ask the authenticated follower for its recovered durable progress. Each
    /// channel handles one exchange. The caller configures deadlines and closes
    /// the channel at the end of the exchange, including on error.
    pub fn poll(&mut self, mut channel: impl Read + Write) -> Result<Option<DurableAck>> {
        wire::request(&mut channel, self.enrollment, None)?;
        match wire::read_reply(&mut channel)? {
            wire::Reply::Installed(ack) => self.observe(ack).map(Some),
            wire::Reply::Empty if self.progress.is_none() => Ok(None),
            _ => Err(Error::Conflict),
        }
    }
    /// Sample the acknowledged primary revision and poll over an authenticated
    /// channel while the source stays open. The sender preserves its follower
    /// progress checks across calls. The source locks are released before I/O.
    pub fn poll_live(
        &mut self,
        source: &Database,
        channel: impl Read + Write,
    ) -> Result<ReplicaObservation> {
        let local_revision = self.check_source(source)?;
        self.verified_revision = local_revision;
        let follower = self.poll(channel)?;
        Ok(ReplicaObservation {
            local_revision,
            follower,
        })
    }
    /// Transfer or retry one export over an authenticated channel. If the reply
    /// is lost, poll on a fresh channel. A stale offer can return a newer ack.
    pub fn ship(
        &mut self,
        export: &mut Export,
        mut channel: impl Read + Write,
    ) -> Result<DurableAck> {
        let offer = export.offer;
        if export.enrollment != self.enrollment || offer.revision > self.verified_revision {
            return Err(Error::ForeignStore);
        }
        let expected = offer.ack(self.enrollment);
        wire::request(&mut channel, self.enrollment, Some(offer))?;
        match wire::read_reply(&mut channel)? {
            wire::Reply::Installed(ack) if ack.revision > expected.revision || ack == expected => {
                self.observe(ack)
            }
            wire::Reply::Ready => {
                visit_image(export.storage.image_bytes(), offer.length, |chunk| {
                    channel.write_all(chunk).map_err(|_| Error::Io)
                })?;
                channel.flush().map_err(|_| Error::Io)?;
                match wire::read_reply(&mut channel)? {
                    wire::Reply::Installed(ack) if ack == expected => self.observe(ack),
                    _ => Err(Error::Conflict),
                }
            }
            _ => Err(Error::Conflict),
        }
    }
    fn observe(&mut self, ack: DurableAck) -> Result<DurableAck> {
        if ack.stream_id != self.enrollment.stream_id || ack.revision > self.verified_revision {
            return Err(Error::ForeignStore);
        }
        if self.progress.is_some_and(|prior| {
            prior.revision > ack.revision || (prior.revision == ack.revision && prior != ack)
        }) {
            return Err(Error::Conflict);
        }
        self.progress = Some(ack);
        Ok(ack)
    }
}

fn visit_image(
    backend: &mut dyn ByteStore,
    length: u64,
    mut visit: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    if backend.length()? != length {
        return Err(Error::Corrupt);
    }
    let mut buffer = [0; REGION];
    let mut offset = 0;
    while offset < length {
        let chunk = &mut buffer[..(length - offset).min(REGION as u64) as usize];
        read_all(backend, offset, chunk)?;
        visit(chunk)?;
        offset += chunk.len() as u64;
    }
    Ok(())
}

fn image_digest(backend: &mut dyn ByteStore, length: u64) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    visit_image(backend, length, |chunk| {
        hash.update(chunk);
        Ok(())
    })?;
    Ok(hash.finalize().into())
}
