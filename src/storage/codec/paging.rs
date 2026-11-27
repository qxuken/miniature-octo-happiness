//! Process-local directory pages over an immutable, strictly verified v2 generation.
use super::*;
use crate::memory::AllocatorHandle;
use std::sync::{Mutex, OnceLock};

// All positional operations use the same lease, including reads by old snapshots.
// Releasing the last owner unlocks the original backend even on a failed open.
struct Lease {
    backend: Box<dyn ByteStore, AllocatorHandle>,
    locked: bool,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.locked {
            self.backend.unlock();
        }
    }
}
#[derive(Clone)]
pub(crate) struct Source(Shared<Mutex<Lease>>, usize, usize, pub(super) FileFormat);
impl Source {
    pub(crate) fn acquire<B: ByteStore + 'static>(backend: B, mem: &Memory) -> Result<Self> {
        let window = backend.read_window();
        let mut lease = Lease {
            backend: mem.boxed(backend)?,
            locked: false,
        };
        lease.backend.lock()?;
        lease.locked = true;
        // Owned Values can outlive this lease. Never recycle a source identity.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        let id = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        Ok(Self(
            mem.shared(Mutex::new(lease))?,
            window,
            id,
            FileFormat::V2,
        ))
    }
    pub(in crate::storage) fn with_format(mut self, format: FileFormat) -> Self {
        self.3 = format;
        self
    }
    pub(crate) fn id(&self) -> usize {
        self.2
    }
    pub(super) fn access<T>(&self, f: impl FnOnce(&mut dyn ByteStore) -> Result<T>) -> Result<T> {
        let mut lease = self.0.lock().map_err(|_| Error::RecoveryRequired)?;
        f(&mut *lease.backend)
    }
}
impl ByteStore for Source {
    fn read_window(&self) -> usize {
        self.1
    }

    fn lock(&mut self) -> Result<()> {
        Ok(())
    }
    fn unlock(&mut self) {}
    fn read(&mut self, o: u64, b: &mut [u8]) -> Result<usize> {
        self.access(|io| io.read(o, b))
    }
    fn write(&mut self, o: u64, b: &[u8]) -> Result<usize> {
        self.access(|io| io.write(o, b))
    }
    fn length(&mut self) -> Result<u64> {
        self.access(|io| io.length())
    }
    fn sync(&mut self) -> Result<()> {
        self.access(|io| io.sync())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub end: u64,
    pub weight: usize,
    pub depth: usize,
    pub digest: u64,
    pub composite: bool,
}
struct Index {
    regions: ChargedVec<usize>,
    shapes: ChargedVec<Shape>,
}
pub(crate) struct Graph {
    source: Source,
    index: OnceLock<Index>,
}
impl Graph {
    pub(super) fn new(source: Source) -> Self {
        Self {
            source,
            index: OnceLock::new(),
        }
    }
    pub(super) fn finish(&self, regions: ChargedVec<usize>, shapes: ChargedVec<Shape>) {
        assert!(self.index.set(Index { regions, shapes }).is_ok());
    }
    pub(super) fn source(&self) -> &Source {
        &self.source
    }
    pub(crate) fn source_id(&self) -> usize {
        self.source.id()
    }
    fn read(&self, mem: &Memory, offset: u64, n: usize) -> Result<ChargedVec<u8>> {
        let mut bytes = ChargedVec::new();
        mem.reserve(&mut bytes, n)?;
        bytes.resize(n, 0);
        self.source.access(|io| read_all(io, offset, &mut bytes))?;
        Ok(bytes)
    }
    pub(crate) fn load(&self, mem: &Memory, off: u64) -> Result<Value> {
        self.load_cached(mem, off, &mut ChargedVec::new())
    }
    pub(super) fn prepare_cache(
        &self,
        mem: &Memory,
        cache: &mut ChargedVec<Option<Value>>,
    ) -> Result<()> {
        if cache.is_empty() {
            let n = self.index.get().ok_or(Error::Corrupt)?.shapes.len();
            mem.reserve(cache, n)?;
            cache.resize(n, None);
        }
        Ok(())
    }
    pub(crate) fn load_cached(
        &self,
        mem: &Memory,
        off: u64,
        cache: &mut ChargedVec<Option<Value>>,
    ) -> Result<Value> {
        let index = self.index.get().ok_or(Error::Corrupt)?;
        if !off.is_multiple_of(REGION as u64) {
            return Err(Error::Corrupt);
        }
        let i = index
            .regions
            .get((off / REGION as u64) as usize)
            .and_then(|v| v.checked_sub(2))
            .ok_or(Error::Corrupt)?;
        let shape = index.shapes.get(i).ok_or(Error::Corrupt)?;
        if let Some(Some(v)) = cache.get(i) {
            return Ok(v.clone());
        }
        diagnostics::verify(Stage::Value, Some(off), || {
            let length = shape
                .end
                .checked_sub(off)
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(Error::Corrupt)?;
            let b = self.read(mem, off, length)?;
            if u64_at(&b, 48) != shape.digest || checksum(&b, 48) != shape.digest {
                return Err(Error::Corrupt);
            }
            if shape.composite && cache.is_empty() {
                mem.reserve(cache, index.shapes.len())?;
                cache.resize(index.shapes.len(), None);
            }
            let length = u64_at(&b, 16) as usize;
            let payload = b
                .get(OBJECT_HEADER..)
                .and_then(|p| p.get(..length))
                .ok_or(Error::Corrupt)?;
            if b[OBJECT_HEADER + length..].iter().any(|&v| v != 0) {
                return Err(Error::Corrupt);
            }
            let v = decode_value(mem, payload, |child| self.load_cached(mem, child, cache))?
                .with_disk_origin(self.source_id(), off);
            if !cache.is_empty() {
                cache[i] = Some(v.clone());
            }
            Ok(v)
        })
    }
}

struct Fence {
    first: ChargedVec<u8>,
    offset: u64,
    length: usize,
    digest: u64,
}
pub(crate) struct Directory {
    pub(crate) graph: Shared<Graph>,
    fences: ChargedVec<Fence>,
}
impl Directory {
    pub(super) fn new(graph: Shared<Graph>) -> Self {
        Self {
            graph,
            fences: ChargedVec::new(),
        }
    }
    pub(super) fn add(&mut self, mem: &Memory, off: u64, rows: &[u8]) -> Result<()> {
        let first = Cursor::new(rows).key(mem.limits().max_bytes)?;
        mem.reserve_geometric(&mut self.fences, 1, mem.limits().max_entries)?;
        self.fences.push(Fence {
            first: mem.copy(first)?,
            offset: off,
            length: rows.len(),
            digest: crc(rows),
        });
        Ok(())
    }
    fn page(&self, mem: &Memory, f: &Fence) -> Result<ChargedVec<u8>> {
        let bytes = self.graph.read(mem, f.offset, f.length)?;
        if crc(&bytes) != f.digest {
            return Err(Error::Corrupt);
        }
        Ok(bytes)
    }
    pub(crate) fn find(&self, mem: &Memory, key: &[u8]) -> Result<Option<(u64, u64)>> {
        let n = self.fences.partition_point(|p| p.first.as_slice() <= key);
        if n == 0 {
            return Ok(None);
        }
        let b = self.page(mem, &self.fences[n - 1])?;
        let mut c = Cursor::new(&b);
        while c.pos < b.len() {
            let k = c.key(mem.limits().max_bytes)?;
            let rev = c.int()?;
            let off = c.int()?;
            match k.cmp(key) {
                std::cmp::Ordering::Equal => return Ok(Some((rev, off))),
                std::cmp::Ordering::Greater => break,
                _ => (),
            }
        }
        Ok(None)
    }
    pub(crate) fn visit(
        &self,
        mem: &Memory,
        mut f: impl FnMut(&[u8], u64, u64) -> Result<()>,
    ) -> Result<()> {
        for page in &self.fences {
            let b = self.page(mem, page)?;
            let mut c = Cursor::new(&b);
            while c.pos < b.len() {
                f(c.key(mem.limits().max_bytes)?, c.int()?, c.int()?)?;
            }
        }
        Ok(())
    }
}
