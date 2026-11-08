//! Explicit complete-root salvage. Ordinary recovery retains its strict rules.
use super::{ByteStore, REGION, Storage, codec};
use crate::{Error, Limits, Result, memory::Memory};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SalvageReport {
    pub selected_slot: u8,
    pub revision: u64,
    pub data_revision: u64,
    pub retained_roots: usize,
    pub verified_slot_revisions: [Option<u64>; 2],
    pub source_bytes: u64,
    pub destination_bytes: u64,
    /// Salvage deliberately ignores witnesses and can discard acknowledged data.
    pub acknowledged_data_loss_possible: bool,
}

struct CandidateView<'a> {
    source: &'a mut dyn ByteStore,
    slot: usize,
}
impl ByteStore for CandidateView<'_> {
    fn lock(&mut self) -> Result<()> {
        Err(Error::Invalid)
    }
    fn unlock(&mut self) {}
    fn length(&mut self) -> Result<u64> {
        self.source.length()
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        let count = self.source.read(offset, out)?;
        if count > out.len() {
            return Err(Error::Io);
        }
        let end = offset.checked_add(count as u64).ok_or(Error::Corrupt)?;
        for region in [2 - self.slot, 3, 4] {
            let start = (region * REGION) as u64;
            let low = offset.max(start);
            let high = end.min(start + REGION as u64);
            if low < high {
                out[(low - offset) as usize..(high - offset) as usize].fill(0);
            }
        }
        Ok(count)
    }
    fn write(&mut self, _: u64, _: &[u8]) -> Result<usize> {
        Err(Error::Invalid)
    }
    fn sync(&mut self) -> Result<()> {
        Err(Error::Invalid)
    }
}

fn salvage<S: ByteStore + 'static, D: ByteStore + 'static>(
    source: S,
    destination: impl FnOnce() -> Result<D>,
    limits: Limits,
) -> Result<SalvageReport> {
    let mem = Memory::new(limits)?;
    let mut source = Storage::locked(source, &mem)?;
    let source_bytes = source.backend.length()?;
    let mut verified = [None, None];
    let mut selected: Option<(usize, codec::Recovered)> = None;
    for (slot, revision) in verified.iter_mut().enumerate() {
        let mut view = CandidateView {
            source: &mut *source.backend,
            slot,
        };
        match codec::recover(&mut view, &mem) {
            Ok(candidate) => {
                *revision = Some(candidate.head.revision);
                if selected
                    .as_ref()
                    .is_none_or(|(_, prior)| candidate.head.revision > prior.head.revision)
                {
                    selected = Some((slot, candidate));
                }
            }
            Err(Error::Corrupt) => {}
            Err(error) => return Err(error),
        }
    }
    let (slot, recovered) = selected.ok_or(Error::Corrupt)?;
    let mut copy = Storage::create(
        destination()?,
        &mem,
        &recovered.head,
        recovered.next_collection,
        recovered.identity,
        &recovered.retention,
    )?;
    copy.verify(&mem)?;
    Ok(SalvageReport {
        selected_slot: slot as u8,
        revision: recovered.head.revision,
        data_revision: recovered.head.data_revision,
        retained_roots: recovered.retention.entries.len(),
        verified_slot_revisions: verified,
        source_bytes,
        destination_bytes: copy.end,
        acknowledged_data_loss_possible: true,
    })
}

/// Copy the highest independently verified complete root to an empty destination.
/// Ignores witness records explicitly. Source is leased exclusively and never
/// written. Each candidate must include a valid current and retained graph.
/// I/O, resource and unsupported-format errors abort rather than select less data.
/// A failed copy can leave a partial destination, which must be discarded.
pub fn salvage_on<S: ByteStore + 'static, D: ByteStore + 'static>(
    source: S,
    destination: D,
    limits: Limits,
) -> Result<SalvageReport> {
    salvage(source, || Ok(destination), limits)
}

/// Explicit salvage to a new path, created only after a candidate verifies.
/// Preserves identity. Forked replication histories require new enrollment.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn salvage_file(
    source: impl AsRef<std::path::Path>,
    destination: impl AsRef<std::path::Path>,
    limits: Limits,
) -> Result<SalvageReport> {
    salvage(
        super::FileStore::open_read_only(source)?,
        || super::FileStore::create(destination),
        limits,
    )
}
