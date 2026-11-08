use super::{ByteStore, MAX_IMAGE_BYTES};
use crate::{
    Error, Limits, Result,
    memory::{Memory, Shared},
};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoKind {
    Length,
    Read,
    Write,
    Sync,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoEvent {
    pub kind: IoKind,
    pub offset: u64,
    pub len: usize,
}
#[derive(Clone, Copy, Debug)]
pub enum Crash {
    DropAll,
    PersistAll,
    /// Persist a selected subset of pending write calls in reverse call order.
    Mask(u64),
    /// Persist only a prefix of one pending write call.
    Torn {
        write: usize,
        prefix: usize,
    },
}
#[derive(Clone, Copy, Debug)]
pub enum Fault {
    Error,
    Short(usize),
    /// Modify a prefix of a write, optionally persist that prefix, then fail.
    Tear {
        prefix: usize,
        durable: bool,
    },
    /// A failed sync may still have persisted any selected pending writes.
    SyncError(Crash),
}
struct Pending {
    offset: usize,
    bytes: Vec<u8>,
}
struct Disk {
    volatile: Vec<u8>,
    durable: Vec<u8>,
    pending: Vec<Pending>,
    events: Vec<IoEvent>,
    fault: Option<(usize, Fault)>,
    steps: usize,
    epoch: u64,
    locked: bool,
    record: bool,
}
/// Test control for a shared simulated medium. Cloning never clones a writer
/// lease. A crash invalidates outstanding adapter leases and resets volatile
/// bytes to the surviving durable image; discard the old Database afterward.
pub struct SimulatedDisk {
    mem: Memory,
    disk: Shared<Mutex<Disk>>,
    lease: Option<u64>,
}
impl Clone for SimulatedDisk {
    fn clone(&self) -> Self {
        Self {
            mem: self.mem.clone(),
            disk: self.disk.clone(),
            lease: None,
        }
    }
}
impl SimulatedDisk {
    pub fn new() -> Result<Self> {
        Self::from_image(&[])
    }
    pub fn from_image(image: &[u8]) -> Result<Self> {
        if image.len() > MAX_IMAGE_BYTES {
            return Err(Error::Limit);
        }
        let mem = Memory::new(Limits::default())?;
        let disk = mem.shared(Mutex::new(Disk {
            volatile: mem.copy(image)?,
            durable: mem.copy(image)?,
            pending: Vec::new(),
            events: Vec::new(),
            fault: None,
            steps: 0,
            epoch: 0,
            locked: false,
            record: true,
        }))?;
        Ok(Self {
            mem,
            disk,
            lease: None,
        })
    }
    /// Zero-based next-I/O fault position; recording begins again at zero.
    pub fn arm(&self, step: usize, fault: Fault) {
        let mut disk = self.disk.lock().unwrap();
        disk.fault = Some((step, fault));
        disk.steps = 0;
        disk.events.clear();
    }
    pub fn clear_fault(&self) {
        self.disk.lock().unwrap().fault = None;
    }
    pub fn trace(&self) -> Result<Vec<IoEvent>> {
        let disk = self.disk.lock().unwrap();
        let mut out = Vec::new();
        self.mem.reserve(&mut out, disk.events.len())?;
        out.extend_from_slice(&disk.events);
        Ok(out)
    }
    pub fn durable_image(&self) -> Result<Vec<u8>> {
        self.mem.copy(&self.disk.lock().unwrap().durable)
    }
    pub fn crash(&self, policy: Crash) -> Result<()> {
        let mut disk = self.disk.lock().unwrap();
        let epoch = disk.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        let durable = survive(&self.mem, &disk, policy)?;
        let volatile = self.mem.copy(&durable)?;
        disk.durable = durable;
        disk.volatile = volatile;
        disk.pending.clear();
        disk.epoch = epoch;
        disk.locked = false;
        disk.fault = None;
        disk.steps = 0;
        disk.events.clear();
        Ok(())
    }
    #[cfg(any(test, feature = "fault-injection"))]
    pub fn fail_allocations_after(&self, n: Option<usize>) {
        self.mem.fail_after(n);
    }
    fn check(&self, disk: &Disk) -> Result<()> {
        if disk.locked && self.lease == Some(disk.epoch) {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
    fn event(&self, disk: &mut Disk, event: IoEvent) -> Result<Option<Fault>> {
        self.check(disk)?;
        if disk.record {
            if disk.events.len() >= 262_144 {
                return Err(Error::Limit);
            }
            self.mem.reserve(&mut disk.events, 1)?;
            disk.events.push(event);
        }
        let step = disk.steps;
        disk.steps = disk.steps.checked_add(1).ok_or(Error::Exhausted)?;
        if disk.fault.is_some_and(|(at, _)| at == step) {
            Ok(disk.fault.take().map(|(_, f)| f))
        } else {
            Ok(None)
        }
    }
}
fn range(offset: u64, len: usize) -> Result<(usize, usize)> {
    let start = usize::try_from(offset).map_err(|_| Error::Limit)?;
    let end = start
        .checked_add(len)
        .filter(|end| *end <= MAX_IMAGE_BYTES)
        .ok_or(Error::Limit)?;
    Ok((start, end))
}
fn apply(mem: &Memory, image: &mut Vec<u8>, offset: usize, bytes: &[u8]) -> Result<()> {
    let (_, end) = range(offset as u64, bytes.len())?;
    if end > image.len() {
        mem.reserve(image, end - image.len())?;
        image.resize(end, 0);
    }
    image[offset..end].copy_from_slice(bytes);
    Ok(())
}
fn survive(mem: &Memory, disk: &Disk, policy: Crash) -> Result<Vec<u8>> {
    let mut image = mem.copy(&disk.durable)?;
    match policy {
        Crash::DropAll => {}
        Crash::PersistAll => {
            for pending in &disk.pending {
                apply(mem, &mut image, pending.offset, &pending.bytes)?;
            }
        }
        Crash::Mask(mask) => {
            for (index, pending) in disk.pending.iter().enumerate().rev() {
                if index < 64 && mask & (1 << index) != 0 {
                    apply(mem, &mut image, pending.offset, &pending.bytes)?;
                }
            }
        }
        Crash::Torn { write, prefix } => {
            if let Some(pending) = disk.pending.get(write) {
                apply(
                    mem,
                    &mut image,
                    pending.offset,
                    &pending.bytes[..prefix.min(pending.bytes.len())],
                )?;
            }
        }
    }
    Ok(image)
}
impl ByteStore for SimulatedDisk {
    fn lock(&mut self) -> Result<()> {
        let mut disk = self.disk.lock().unwrap();
        if disk.locked {
            return Err(Error::Busy);
        }
        disk.locked = true;
        self.lease = Some(disk.epoch);
        Ok(())
    }
    fn unlock(&mut self) {
        let mut disk = self.disk.lock().unwrap();
        if self.lease == Some(disk.epoch) {
            disk.locked = false;
        }
        self.lease = None;
    }
    fn length(&mut self) -> Result<u64> {
        let mut disk = self.disk.lock().unwrap();
        if self
            .event(
                &mut disk,
                IoEvent {
                    kind: IoKind::Length,
                    offset: 0,
                    len: 0,
                },
            )?
            .is_some()
        {
            return Err(Error::Io);
        }
        Ok(disk.volatile.len() as u64)
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        let mut disk = self.disk.lock().unwrap();
        let fault = self.event(
            &mut disk,
            IoEvent {
                kind: IoKind::Read,
                offset,
                len: out.len(),
            },
        )?;
        let requested = match fault {
            None => out.len(),
            Some(Fault::Short(n)) => n.min(out.len()),
            _ => return Err(Error::Io),
        };
        let (start, _) = range(offset, requested)?;
        let n = requested.min(disk.volatile.len().saturating_sub(start));
        if n != 0 {
            out[..n].copy_from_slice(&disk.volatile[start..start + n]);
        }
        Ok(n)
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        let mut disk = self.disk.lock().unwrap();
        let fault = self.event(
            &mut disk,
            IoEvent {
                kind: IoKind::Write,
                offset,
                len: bytes.len(),
            },
        )?;
        let (n, fail, durable) = match fault {
            None => (bytes.len(), false, false),
            Some(Fault::Short(n)) => (n.min(bytes.len()), false, false),
            Some(Fault::Tear { prefix, durable }) => (prefix.min(bytes.len()), true, durable),
            _ => return Err(Error::Io),
        };
        let (start, end) = range(offset, n)?;
        if n != 0 {
            if disk.pending.len() >= 2 * MAX_IMAGE_BYTES / super::REGION {
                return Err(Error::Limit);
            }
            let copy = self.mem.copy(&bytes[..n])?;
            self.mem.reserve(&mut disk.pending, 1)?;
            let extra = end.saturating_sub(disk.volatile.len());
            self.mem.reserve(&mut disk.volatile, extra)?;
            // Reserve the possible immediate durable tear before changing either image.
            if durable {
                let extra = end.saturating_sub(disk.durable.len());
                self.mem.reserve(&mut disk.durable, extra)?;
            }
            apply(&self.mem, &mut disk.volatile, start, &copy)?;
            if durable {
                apply(&self.mem, &mut disk.durable, start, &copy)?;
            }
            disk.pending.push(Pending {
                offset: start,
                bytes: copy,
            });
        }
        if fail { Err(Error::Io) } else { Ok(n) }
    }
    fn sync(&mut self) -> Result<()> {
        let mut disk = self.disk.lock().unwrap();
        let fault = self.event(
            &mut disk,
            IoEvent {
                kind: IoKind::Sync,
                offset: 0,
                len: 0,
            },
        )?;
        match fault {
            None => {
                let durable = self.mem.copy(&disk.volatile)?;
                disk.durable = durable;
                disk.pending.clear();
                Ok(())
            }
            Some(Fault::SyncError(policy)) => {
                disk.durable = survive(&self.mem, &disk, policy)?;
                Err(Error::Io)
            }
            _ => Err(Error::Io),
        }
    }
}

/// A process-memory byte adapter using the identical codec/protocol with no
/// exposed fault controls. Clones share bytes, and exclude a second writer.
#[derive(Clone)]
pub struct MemoryStore(SimulatedDisk);
impl MemoryStore {
    pub fn new() -> Result<Self> {
        let disk = SimulatedDisk::new()?;
        disk.disk.lock().unwrap().record = false;
        Ok(Self(disk))
    }
}
impl ByteStore for MemoryStore {
    fn lock(&mut self) -> Result<()> {
        self.0.lock()
    }
    fn unlock(&mut self) {
        self.0.unlock();
    }
    fn length(&mut self) -> Result<u64> {
        self.0.length()
    }
    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        self.0.read(offset, out)
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.0.write(offset, bytes)
    }
    fn sync(&mut self) -> Result<()> {
        self.0.sync()
    }
}
