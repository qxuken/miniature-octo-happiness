//! One primary and one passive snapshot follower. Local commits never use the
//! network. The application schedules exports and supplies a duplex channel
//! authenticated to the configured peer (for example, mutual TLS with peer
//! pinning). Stream IDs and SHA-256 digests are not peer credentials.
use crate::{
    Database, Error, Result,
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct Offer {
    revision: u64,
    length: u64,
    digest: [u8; 32],
}
impl Offer {
    fn validate(self) -> Result<()> {
        if !(APPEND_START as u64..=MAX_IMAGE_BYTES as u64).contains(&self.length) {
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
    fn check_source(&self, source: &Database) -> Result<u64> {
        let (identity, revision) = source.replication_head()?;
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
        let (mut storage, stats) = source.compact_copy(destination)?;
        let length = stats.after_bytes;
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
