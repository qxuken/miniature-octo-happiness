mod extents;
pub(crate) mod packed;
pub(crate) mod paging;
use super::diagnostics::{self, VerificationStage as Stage};
use super::{APPEND_START, ByteStore, FileFormat, MAX_IMAGE_BYTES, REGION, read_all, write_all};
use crate::memory::ChargedVec;
use crate::{
    Error, Kind, Result, Value,
    engine::{Collection, Head, MAX_RETAINED_SNAPSHOTS, Record, Retained, Retention},
    memory::{Memory, Shared},
    tree::{List, Map},
};
use paging::{Directory, Graph, Shape, Source};

const VERSION: u32 = crate::version::FORMAT_VERSION;
pub(crate) const PACKED_VERSION: u32 = 3;
const EXTENDED_VERSION: u32 = 4;
const MAX_OBJECTS: usize = MAX_IMAGE_BYTES / REGION;
const OBJECT_HEADER: usize = 64;
const VALUE: u32 = 1;
const DATA: u32 = 2;
const COMMIT: u32 = 3;

static CRC_TABLES: [[u64; 256]; 8] = crc_tables();

const fn crc_tables() -> [[u64; 256]; 8] {
    const POLYNOMIAL: u64 = 0x42f0e1eba9ea3693;
    let mut tables = [[0; 256]; 8];
    let mut byte = 0;
    while byte < 256 {
        let mut crc = (byte as u64) << 56;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & (1 << 63) != 0 {
                (crc << 1) ^ POLYNOMIAL
            } else {
                crc << 1
            };
            bit += 1;
        }
        tables[0][byte] = crc;
        byte += 1;
    }
    // Row n advances the single-byte remainder through n more zero bytes.
    let mut row = 1;
    while row < tables.len() {
        let mut byte = 0;
        while byte < 256 {
            let crc = tables[row - 1][byte];
            tables[row][byte] = (crc << 8) ^ tables[0][(crc >> 56) as usize];
            byte += 1;
        }
        row += 1;
    }
    tables
}

/// CRC-64/ECMA-182: polynomial 0x42f0e1eba9ea3693, init/xorout zero,
/// non-reflected. Checksums are damage detection, not authentication.
pub(crate) fn crc(bytes: &[u8]) -> u64 {
    update_crc(0, bytes)
}
fn update_crc(mut crc: u64, bytes: &[u8]) -> u64 {
    let (words, tail) = bytes.as_chunks::<8>();
    for word in words {
        // Earlier bytes have more zero-byte advances before the word ends.
        let [a, b, c, d, e, f, g, h] = (crc ^ u64::from_be_bytes(*word)).to_be_bytes();
        crc = CRC_TABLES[7][usize::from(a)]
            ^ CRC_TABLES[6][usize::from(b)]
            ^ CRC_TABLES[5][usize::from(c)]
            ^ CRC_TABLES[4][usize::from(d)]
            ^ CRC_TABLES[3][usize::from(e)]
            ^ CRC_TABLES[2][usize::from(f)]
            ^ CRC_TABLES[1][usize::from(g)]
            ^ CRC_TABLES[0][usize::from(h)];
    }
    for &byte in tail {
        crc = (crc << 8) ^ CRC_TABLES[0][usize::from(((crc >> 56) as u8) ^ byte)];
    }
    crc
}
fn checksum(bytes: &[u8], at: usize) -> u64 {
    update_crc(update_crc(crc(&bytes[..at]), &[0; 8]), &bytes[at + 8..])
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
pub(crate) fn align(n: u64) -> Result<u64> {
    align_to(n, MAX_IMAGE_BYTES as u64)
}
pub(super) fn align_to(n: u64, capacity: u64) -> Result<u64> {
    let value = n
        .checked_add(REGION as u64 - 1)
        .map(|n| n / REGION as u64 * REGION as u64)
        .ok_or(Error::Limit)?;
    if value > capacity {
        Err(Error::Limit)
    } else {
        Ok(value)
    }
}
#[cfg(test)]
pub(crate) fn header(identity: [u8; 16]) -> [u8; REGION] {
    file_header(identity, FileFormat::V2)
}
pub(super) fn file_header(identity: [u8; 16], packed: FileFormat) -> [u8; REGION] {
    let mut out = [0; REGION];
    out[..8].copy_from_slice(b"WEEKDB01");
    put32(&mut out, 8, packed.version());
    put64(&mut out, 16, REGION as u64);
    put64(&mut out, 24, APPEND_START as u64);
    out[32..48].copy_from_slice(&identity);
    put64(&mut out, 48, packed.capacity());
    put64(
        &mut out,
        56,
        if packed.extended() {
            0
        } else {
            MAX_OBJECTS as u64
        },
    );
    let crc = checksum(&out, 64);
    put64(&mut out, 64, crc);
    out
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub index: usize,
    pub revision: u64,
    pub descriptor: u64,
    pub end: u64,
}
pub(crate) fn metadata(identity: [u8; 16], slot: Slot, witness: bool) -> [u8; REGION] {
    let mut out = [0; REGION];
    out[..8].copy_from_slice(if witness { b"WDBACK01" } else { b"WDBSLOT1" });
    put32(&mut out, 8, VERSION);
    put32(&mut out, 12, slot.index as u32);
    out[16..32].copy_from_slice(&identity);
    put64(&mut out, 32, slot.revision);
    put64(&mut out, 40, slot.descriptor);
    put64(&mut out, 48, slot.end);
    put64(&mut out, 56, slot.revision.saturating_sub(1));
    let crc = checksum(&out, 64);
    put64(&mut out, 64, crc);
    out
}
fn parse_metadata(
    bytes: &[u8; REGION],
    identity: [u8; 16],
    index: usize,
    witness: bool,
    length: u64,
) -> Result<Option<Slot>> {
    if bytes.iter().all(|b| *b == 0) {
        return Ok(None);
    }
    if checksum(bytes, 64) != u64_at(bytes, 64) {
        return Ok(None);
    }
    let magic = if witness { b"WDBACK01" } else { b"WDBSLOT1" };
    if &bytes[..8] != magic
        || u32_at(bytes, 8) != VERSION
        || u32_at(bytes, 12) != index as u32
        || bytes[16..32] != identity
        || bytes[72..].iter().any(|b| *b != 0)
    {
        return Err(Error::Corrupt);
    }
    let slot = Slot {
        index,
        revision: u64_at(bytes, 32),
        descriptor: u64_at(bytes, 40),
        end: u64_at(bytes, 48),
    };
    if slot.index != (slot.revision & 1) as usize
        || u64_at(bytes, 56) != slot.revision.saturating_sub(1)
        || slot.end > length
        || !slot.end.is_multiple_of(REGION as u64)
        || slot.descriptor < APPEND_START as u64
        || !slot.descriptor.is_multiple_of(REGION as u64)
        || slot.descriptor.checked_add(REGION as u64) != Some(slot.end)
    {
        return Err(Error::Corrupt);
    }
    Ok(Some(slot))
}

#[derive(Clone)]
pub(crate) struct CacheEntry {
    pub value: Value,
    pub offset: u64,
    pub end: u64,
}
/// Encoder-only, half-full open-addressed index into the emission-order cache.
/// Entries own no pointers: the cache's Value clones keep every identity alive.
/// Built from recovered cache order, then copied only when that cache forks.
/// Hash/slot order never controls serialization. Zero is empty, n+1 indexes n.
struct ValueIndex {
    slots: ChargedVec<usize>,
}
impl ValueIndex {
    fn new() -> Self {
        Self {
            slots: ChargedVec::new(),
        }
    }
    fn bucket(identity: usize, slots: usize) -> usize {
        // Mix allocator alignment and high address bits before masking. Only
        // integer arithmetic: no pointer reconstruction or unsafe dereference.
        let mut hash = identity as u64;
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((hash ^ (hash >> 31)) as usize) & (slots - 1)
    }
    fn position(&self, value: &Value, cache: &[CacheEntry]) -> usize {
        let mut slot = Self::bucket(value.node_identity(), self.slots.len());
        while self.slots[slot] != 0 {
            let old = &cache[self.slots[slot] - 1];
            if old.value.same_node(value) {
                break;
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
        slot
    }
    fn get(&self, value: &Value, cache: &[CacheEntry]) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        self.slots[self.position(value, cache)].checked_sub(1)
    }
    fn reserve(&mut self, mem: &Memory, cache: &[CacheEntry], entries: usize) -> Result<()> {
        if entries > MAX_OBJECTS {
            return Err(Error::Limit);
        }
        if entries <= self.slots.len() / 2 {
            return Ok(());
        }
        let size = (entries * 2).next_power_of_two().max(8);
        let mut next = Self::new();
        mem.reserve(&mut next.slots, size)?;
        next.slots.resize(size, 0);
        for (i, old) in cache.iter().enumerate() {
            let slot = next.position(&old.value, cache);
            // Preserve the original first-match rule even for redundant input
            // cache entries; normal encoder/recovery caches are unique.
            if next.slots[slot] == 0 {
                next.slots[slot] = i + 1;
            }
        }
        *self = next;
        Ok(())
    }
    fn insert(&mut self, value: &Value, cache: &[CacheEntry], index: usize) {
        let slot = self.position(value, cache);
        self.slots[slot] = index + 1;
    }
}
#[derive(Clone)]
pub(crate) struct DataEntry {
    pub revision: u64,
    pub next_collection: u64,
    pub offset: u64,
}
/// Encoder-only revision order, separate from DATA emission/cache order.
/// Built once from arbitrary recovered cache order; forks copy the sorted order.
struct DataIndex {
    order: ChargedVec<usize>,
}
impl DataIndex {
    fn new() -> Self {
        Self {
            order: ChargedVec::new(),
        }
    }
    fn compare(a: u64, b: u64) -> std::cmp::Ordering {
        a.cmp(&b)
    }
    fn from_cache(mem: &Memory, cache: &[DataEntry]) -> Result<Self> {
        let mut index = Self::new();
        index.reserve(mem, cache.len())?;
        index.order.extend(0..cache.len());
        index.order.sort_unstable_by(|a, b| {
            Self::compare(cache[*a].revision, cache[*b].revision).then(a.cmp(b))
        });
        Ok(index)
    }
    fn position(&self, revision: u64, cache: &[DataEntry]) -> usize {
        self.order
            .partition_point(|i| Self::compare(cache[*i].revision, revision).is_lt())
    }
    fn get(&self, revision: u64, cache: &[DataEntry]) -> Option<usize> {
        self.order
            .get(self.position(revision, cache))
            .copied()
            .filter(|i| Self::compare(cache[*i].revision, revision).is_eq())
    }
    fn reserve(&mut self, mem: &Memory, entries: usize) -> Result<()> {
        if entries > MAX_OBJECTS {
            return Err(Error::Limit);
        }
        if entries > self.order.len() {
            // Geometric charged capacity, with no allocation when already large
            // enough. The object bound makes rounding/subtraction safe.
            let extra = entries.next_power_of_two() - self.order.len();
            mem.reserve(&mut self.order, extra)?;
        }
        Ok(())
    }
    fn insert(&mut self, revision: u64, cache: &[DataEntry], index: usize) {
        if self
            .order
            .last()
            .is_none_or(|i| Self::compare(cache[*i].revision, revision).is_lt())
        {
            self.order.push(index);
            return;
        }
        let position = self.position(revision, cache);
        self.order.push(index);
        self.order[position..].rotate_right(1);
    }
}
// Cache entries and their indexes share one immutable lifetime. Forks keep
// entry order and index positions, so copying an index needs no rehash/sort.
pub(crate) struct ValueCache {
    pub(super) source_id: usize,
    entries: ChargedVec<CacheEntry>,
    index: ValueIndex,
}
impl ValueCache {
    fn new() -> Self {
        Self {
            source_id: 0,
            entries: ChargedVec::new(),
            index: ValueIndex::new(),
        }
    }
    pub(super) fn from_entries(mem: &Memory, entries: ChargedVec<CacheEntry>) -> Result<Self> {
        let mut index = ValueIndex::new();
        index.reserve(mem, &entries, entries.len())?;
        Ok(Self {
            source_id: 0,
            entries,
            index,
        })
    }
    fn fork(&self, mem: &Memory) -> Result<Self> {
        let entries = fork_vec(mem, &self.entries, MAX_OBJECTS)?;
        let slots = fork_vec(mem, &self.index.slots, 2 * MAX_OBJECTS)?;
        Ok(Self {
            source_id: self.source_id,
            entries,
            index: ValueIndex { slots },
        })
    }
}
pub(crate) struct DataCache {
    entries: ChargedVec<DataEntry>,
    index: DataIndex,
}
impl DataCache {
    fn new() -> Self {
        Self {
            entries: ChargedVec::new(),
            index: DataIndex::new(),
        }
    }
    pub(super) fn from_entries(mem: &Memory, entries: ChargedVec<DataEntry>) -> Result<Self> {
        let index = DataIndex::from_cache(mem, &entries)?;
        Ok(Self { entries, index })
    }
    fn fork(&self, mem: &Memory) -> Result<Self> {
        let entries = fork_vec(mem, &self.entries, MAX_OBJECTS)?;
        let order = fork_vec(mem, &self.index.order, MAX_OBJECTS)?;
        Ok(Self {
            entries,
            index: DataIndex { order },
        })
    }
}
fn fork_vec<T: Clone>(mem: &Memory, old: &[T], maximum: usize) -> Result<ChargedVec<T>> {
    let mut next = ChargedVec::new();
    mem.reserve_geometric(&mut next, old.len(), maximum)?;
    next.extend(old.iter().cloned());
    Ok(next)
}
// Borrow the acknowledged cache until an actual append requires a private fork.
// All allocations (including the shared header) precede publication I/O.
enum CacheState<'a, T> {
    Borrowed(&'a Shared<T>),
    Owned(T),
}
impl<T> std::ops::Deref for CacheState<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Self::Borrowed(old) => old,
            Self::Owned(cache) => cache,
        }
    }
}
impl<T> CacheState<'_, T> {
    fn make_mut(&mut self, fork: impl FnOnce(&T) -> Result<T>) -> Result<&mut T> {
        if let Self::Borrowed(old) = self {
            *self = Self::Owned(fork(old)?);
        }
        match self {
            Self::Owned(cache) => Ok(cache),
            Self::Borrowed(_) => unreachable!(),
        }
    }
    fn finish(self, mem: &Memory) -> Result<Shared<T>> {
        match self {
            Self::Borrowed(old) => Ok(old.clone()),
            Self::Owned(cache) => mem.shared(cache),
        }
    }
}

pub(crate) struct Candidate {
    pub(super) rebased: Option<Shared<Head>>,
    start: u64,
    bytes: AppendBytes,
    pub descriptor: u64,
    pub end: u64,
    pub cache: Shared<ValueCache>,
    pub data_cache: Shared<DataCache>,
}
impl Candidate {
    pub(super) fn write_append(&self, backend: &mut dyn ByteStore) -> Result<()> {
        let segments = match &self.bytes {
            AppendBytes::FreshFile { .. } => return Ok(()),
            AppendBytes::Contiguous(bytes) => std::slice::from_ref(bytes),
            AppendBytes::Segmented(segments) => segments.as_slice(),
        };
        let mut offset = self.start;
        for segment in segments {
            write_all(backend, offset, segment)?;
            offset += segment.len() as u64;
        }
        Ok(())
    }
}

// Keep fresh-image staging out of large contiguous heap allocations. Full
// segments are region-aligned, so this changes allocation without changing
// the ordered writes or their boundaries at the adapter.
const APPEND_SEGMENT_BYTES: usize = 16 * REGION;

pub(super) enum AppendBytes {
    /// Used only for a new, unpublished copy. Failure leaves the source intact.
    FreshFile {
        destination: Source,
        offset: u64,
    },
    Contiguous(ChargedVec<u8>),
    Segmented(ChargedVec<ChargedVec<u8>>),
}
impl AppendBytes {
    fn object(
        &mut self,
        mem: &Memory,
        parts: [&[u8]; 3],
        size: usize,
        maximum: usize,
    ) -> Result<()> {
        match self {
            Self::FreshFile {
                destination,
                offset,
            } => {
                for part in parts {
                    if !part.is_empty() {
                        write_all(destination, *offset, part)?;
                        *offset = offset.checked_add(part.len() as u64).ok_or(Error::Limit)?;
                    }
                }
            }
            Self::Contiguous(bytes) => {
                mem.reserve_geometric(bytes, size, maximum)?;
                for part in parts {
                    bytes.extend_from_slice(part);
                }
            }
            Self::Segmented(segments) => {
                let mut remaining = size;
                for mut part in parts {
                    while !part.is_empty() {
                        if segments
                            .last()
                            .is_none_or(|segment| segment.len() == APPEND_SEGMENT_BYTES)
                        {
                            mem.reserve_geometric(
                                segments,
                                1,
                                MAX_IMAGE_BYTES.div_ceil(APPEND_SEGMENT_BYTES),
                            )?;
                            segments.push(ChargedVec::new());
                        }
                        let segment = segments.last_mut().expect("append segment");
                        let available = (APPEND_SEGMENT_BYTES - segment.len()).min(remaining);
                        mem.reserve_geometric(segment, available, APPEND_SEGMENT_BYTES)?;
                        let len = available.min(part.len());
                        segment.extend_from_slice(&part[..len]);
                        part = &part[len..];
                        remaining -= len;
                    }
                }
            }
        }
        Ok(())
    }
}

struct Writer<'a> {
    mem: &'a Memory,
    bytes: ChargedVec<u8>,
}
impl<'a> Writer<'a> {
    fn new(mem: &'a Memory) -> Self {
        Self {
            mem,
            bytes: ChargedVec::new(),
        }
    }
    fn add(&mut self, bytes: &[u8]) -> Result<()> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_IMAGE_BYTES)
        {
            return Err(Error::Limit);
        }
        if bytes.len() > self.bytes.len() {
            // A dominating bulk append already more than doubles logical
            // length. Reserve its known size exactly, avoiding unused large
            // payload capacity; incremental field assembly grows geometrically.
            self.mem.reserve(&mut self.bytes, bytes.len())?;
        } else {
            self.mem
                .reserve_geometric(&mut self.bytes, bytes.len(), MAX_IMAGE_BYTES)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn int(&mut self, n: u64) -> Result<()> {
        self.add(&n.to_le_bytes())
    }
    fn key(&mut self, bytes: &[u8]) -> Result<()> {
        self.int(bytes.len() as u64)?;
        self.add(bytes)
    }
}
struct Encoder<'a> {
    relocation: packed::Relocation,
    packed: Option<Source>,
    rebased: Option<Shared<Head>>,
    copied: ChargedVec<Option<Value>>,
    mem: &'a Memory,
    identity: [u8; 16],
    revision: u64,
    start: u64,
    end: u64,
    bytes: AppendBytes,
    cache: CacheState<'a, ValueCache>,
    data_cache: CacheState<'a, DataCache>,
}
impl Encoder<'_> {
    fn object(&mut self, kind: u32, payload: &[u8]) -> Result<(u64, u64)> {
        self.object_born(kind, payload, self.revision)
    }
    fn object_born(&mut self, kind: u32, payload: &[u8], born: u64) -> Result<(u64, u64)> {
        self.object_with_crc(kind, payload, born)
            .map(|(off, end, _)| (off, end))
    }
    fn object_with_crc(&mut self, kind: u32, payload: &[u8], born: u64) -> Result<(u64, u64, u64)> {
        let length = OBJECT_HEADER
            .checked_add(payload.len())
            .ok_or(Error::Limit)?;
        let size = align(length as u64)? as usize;
        let offset = self.end;
        let capacity = self
            .packed
            .as_ref()
            .map_or(MAX_IMAGE_BYTES as u64, |s| s.3.capacity());
        let end = offset
            .checked_add(size as u64)
            .filter(|n| *n <= capacity)
            .ok_or(Error::Limit)?;
        let mut header = [0; OBJECT_HEADER];
        header[..8].copy_from_slice(b"WDBOBJ01");
        put32(&mut header, 8, kind);
        put32(&mut header, 12, VERSION);
        put64(&mut header, 16, payload.len() as u64);
        put64(&mut header, 24, born);
        header[32..48].copy_from_slice(&self.identity);
        let padding = &[0; REGION][..size - length];
        let crc = update_crc(update_crc(crc(&header), payload), padding);
        put64(&mut header, 48, crc);
        self.bytes.object(
            self.mem,
            [&header, payload, padding],
            size,
            usize::try_from(capacity - self.start).unwrap_or(usize::MAX),
        )?;
        self.end = end;
        Ok((offset, end, crc))
    }
    fn value(&mut self, value: &Value) -> Result<u64> {
        if let Some(offset) = value.offset_in(self.cache.source_id) {
            return Ok(offset);
        }
        let cached = self.cache.index.get(value, &self.cache.entries);
        #[cfg(test)]
        assert_eq!(
            cached,
            self.cache
                .entries
                .iter()
                .position(|entry| entry.value.same_node(value))
        );
        if let Some(index) = cached {
            return Ok(self.cache.entries[index].offset);
        }
        if self.cache.entries.len() >= MAX_OBJECTS {
            return Err(Error::Limit);
        }
        let mut payload = Writer::new(self.mem);
        payload.int(value.kind() as u64)?;
        match value.kind() {
            Kind::None => payload.int(0)?,
            Kind::Boolean => payload.int(u64::from(value.as_bool()?))?,
            Kind::Integer => payload.int(value.as_i64()? as u64)?,
            Kind::Unsigned => payload.int(value.as_u64()?)?,
            Kind::Float => payload.int(value.as_f64()?.to_bits())?,
            Kind::Bytes => payload.key(value.as_bytes()?)?,
            Kind::Map => {
                let entries = value.map_entries_for_encoding(self.mem)?;
                payload.int(entries.len() as u64)?;
                for (map_key, child) in entries {
                    let offset = self.value(child)?;
                    payload.key(map_key)?;
                    payload.int(offset)?;
                }
            }
            Kind::List => {
                payload.int(value.len()? as u64)?;
                for i in 0..value.len()? {
                    let offset = self.value(&value.list_get(i)?)?;
                    payload.int(offset)?;
                }
            }
        }
        let (offset, end) = self.object(VALUE, &payload.bytes)?;
        let cache = self.cache.make_mut(|old| old.fork(self.mem))?;
        cache
            .index
            .reserve(self.mem, &cache.entries, cache.entries.len() + 1)?;
        self.mem
            .reserve_geometric(&mut cache.entries, 1, MAX_OBJECTS)?;
        cache
            .index
            .insert(value, &cache.entries, cache.entries.len());
        cache.entries.push(CacheEntry {
            value: value.clone(),
            offset,
            end,
        });
        Ok(offset)
    }
    fn data(&mut self, head: &Head, next_collection: u64) -> Result<u64> {
        let cached = self
            .data_cache
            .index
            .get(head.data_revision, &self.data_cache.entries);
        #[cfg(test)]
        assert_eq!(
            cached,
            self.data_cache
                .entries
                .iter()
                .position(|entry| entry.revision == head.data_revision)
        );
        if let Some(index) = cached {
            let old = &self.data_cache.entries[index];
            if old.next_collection != next_collection {
                return Err(Error::Invalid);
            }
            return Ok(old.offset);
        }
        if let Some(source) = self.packed.clone() {
            return self.packed_data(head, next_collection, source);
        }
        let mut payload = Writer::new(self.mem);
        payload.int(head.data_revision)?;
        payload.int(next_collection)?;
        payload.int(head.collections.len() as u64)?;
        let mut collections = ChargedVec::new();
        self.mem.reserve(&mut collections, head.collections.len())?;
        head.collections.visit(&mut |key, c| {
            let id = u64::from_le_bytes(key.try_into().map_err(|_| Error::Invalid)?);
            collections.push((id, c));
            Ok(())
        })?;
        collections.sort_unstable_by_key(|(id, _)| *id);
        for (id, c) in collections {
            payload.int(id)?;
            payload.int(c.revision)?;
            payload.int(c.len() as u64)?;
            if c.base.is_some() {
                c.visit_paged(self.mem, |key, row| {
                    let revision = row.revision();
                    let off = match row {
                        crate::engine::Row::Memory(r) => self.value(&r.value)?,
                        crate::engine::Row::Packed(r) => self.value(&r.value)?,
                        crate::engine::Row::Disk { graph, offset, .. }
                            if graph.source_id() == self.cache.source_id =>
                        {
                            offset
                        }
                        crate::engine::Row::Disk { graph, offset, .. } => {
                            graph.prepare_cache(self.mem, &mut self.copied)?;
                            let v = graph.load_cached(self.mem, offset, &mut self.copied)?;
                            self.value(&v)?
                        }
                    };
                    payload.key(key)?;
                    payload.int(revision)?;
                    payload.int(off)
                })?;
                continue;
            }
            let mut records = ChargedVec::new();
            self.mem.reserve(&mut records, c.records.len())?;
            c.records.visit(&mut |key, record| {
                records.push((key, record));
                Ok(())
            })?;
            records.sort_unstable_by(|a, b| a.0.cmp(b.0));
            for (key, r) in records {
                let offset = self.value(&r.value)?;
                payload.key(key)?;
                payload.int(r.revision)?;
                payload.int(offset)?;
            }
        }
        let offset = self
            .object_born(DATA, &payload.bytes, head.data_revision)?
            .0;
        self.cache_data(head.data_revision, next_collection, offset)
    }
    fn cache_data(&mut self, revision: u64, next_collection: u64, offset: u64) -> Result<u64> {
        let cache = self.data_cache.make_mut(|old| old.fork(self.mem))?;
        cache.index.reserve(self.mem, cache.entries.len() + 1)?;
        self.mem
            .reserve_geometric(&mut cache.entries, 1, MAX_OBJECTS)?;
        cache
            .index
            .insert(revision, &cache.entries, cache.entries.len());
        cache.entries.push(DataEntry {
            revision,
            next_collection,
            offset,
        });
        Ok(offset)
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    mem: &Memory,
    head: &Head,
    next_collection: u64,
    identity: [u8; 16],
    start: u64,
    old_cache: Option<&Shared<ValueCache>>,
    old_data: Option<&Shared<DataCache>>,
    retention: &Retention,
) -> Result<Candidate> {
    encode_with_append(
        mem,
        head,
        next_collection,
        identity,
        start,
        old_cache,
        old_data,
        retention,
        AppendBytes::Contiguous(ChargedVec::new()),
        None,
    )
}

pub(super) fn encode_fresh(
    mem: &Memory,
    head: &Head,
    next_collection: u64,
    identity: [u8; 16],
    retention: &Retention,
) -> Result<Candidate> {
    encode_with_append(
        mem,
        head,
        next_collection,
        identity,
        APPEND_START as u64,
        None,
        None,
        retention,
        AppendBytes::Segmented(ChargedVec::new()),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn encode_with_append(
    mem: &Memory,
    head: &Head,
    next_collection: u64,
    identity: [u8; 16],
    start: u64,
    old_cache: Option<&Shared<ValueCache>>,
    old_data: Option<&Shared<DataCache>>,
    retention: &Retention,
    bytes: AppendBytes,
    packed: Option<Source>,
) -> Result<Candidate> {
    if start < APPEND_START as u64 || !start.is_multiple_of(REGION as u64) {
        return Err(Error::Invalid);
    }
    let cache = match old_cache {
        Some(old) => CacheState::Borrowed(old),
        None => CacheState::Owned(ValueCache::new()),
    };
    let data_cache = match old_data {
        Some(old) => CacheState::Borrowed(old),
        None => CacheState::Owned(DataCache::new()),
    };
    let mut encoder = Encoder {
        packed,
        rebased: None,
        relocation: packed::Relocation::new(),
        copied: ChargedVec::new(),
        mem,
        identity,
        revision: head.revision,
        start,
        end: start,
        bytes,
        cache,
        data_cache,
    };
    let data_ref = encoder.data(head, next_collection)?;
    let mut commit = Writer::new(mem);
    commit.int(head.revision)?;
    commit.int(data_ref)?;
    commit.int(head.data_revision)?;
    commit.int(retention.next_id)?;
    commit.int(retention.entries.len() as u64)?;
    for retained in &retention.entries {
        let offset = encoder.data(&retained.head, retained.next_collection)?;
        commit.int(retained.id)?;
        commit.int(retained.head.revision)?;
        commit.int(offset)?;
        commit.int(retained.head.data_revision)?;
    }
    let (descriptor, end) = encoder.object(COMMIT, &commit.bytes)?;
    Ok(Candidate {
        rebased: encoder.rebased,
        start,
        bytes: encoder.bytes,
        descriptor,
        end,
        cache: encoder.cache.finish(mem)?,
        data_cache: encoder.data_cache.finish(mem)?,
    })
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|n| *n <= self.bytes.len())
            .ok_or(Error::Corrupt)?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn int(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn count(&mut self, maximum: usize) -> Result<usize> {
        let n = usize::try_from(self.int()?).map_err(|_| Error::Corrupt)?;
        if n > maximum {
            Err(Error::Limit)
        } else {
            Ok(n)
        }
    }
    fn key(&mut self, maximum: usize) -> Result<&'a [u8]> {
        let n = self.count(maximum)?;
        self.take(n)
    }
    fn done(&self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(Error::Corrupt)
        }
    }
}
struct Object {
    payload_len: usize,
    deferred_blob: bool,
    digest: u64,
    payload: ChargedVec<u8>,
    born: u64,
    end: u64,
}
struct Reader<'a> {
    packed: FileFormat,
    page_cache: packed::PageCache,
    extents: extents::Offsets<u64>,
    blobs: extents::Offsets<(packed::blob::Blob, u64, u64)>,
    defer_blobs: bool,
    graph: Option<Shared<Graph>>,
    shapes: ChargedVec<Shape>,
    scratch: ChargedVec<u8>,
    backend: &'a mut dyn ByteStore,
    mem: &'a Memory,
    identity: [u8; 16],
    revision: u64,
    cache: ChargedVec<CacheEntry>,
    // One slot per aligned region in the selected image. Zero is unoccupied,
    // one is occupied, and n >= 2 names value cache entry n - 2 at its start.
    regions: ChargedVec<usize>,
    region_count: usize,
    objects: usize,
    work: usize,
    data_cache: ChargedVec<DecodedData>,
}
struct DecodedData {
    entry: DataEntry,
    collections: Map<Collection>,
    end: u64,
}
fn decode_value(
    mem: &Memory,
    payload: &[u8],
    mut child: impl FnMut(u64) -> Result<Value>,
) -> Result<Value> {
    let mut cursor = Cursor::new(payload);
    let tag = cursor.int()?;
    let value = match tag {
        0..=4 => {
            let bits = cursor.int()?;
            let kind = match tag {
                0 => Kind::None,
                1 => Kind::Boolean,
                2 => Kind::Integer,
                3 => Kind::Unsigned,
                _ => Kind::Float,
            };
            Value::scalar(mem, kind, bits).map_err(|e| {
                if e == Error::Invalid {
                    Error::Corrupt
                } else {
                    e
                }
            })?
        }
        5 => Value::bytes_new(mem, cursor.key(mem.limits().max_bytes)?)?,
        6 => {
            let count = cursor.count(mem.limits().max_entries)?;
            let mut map = Map::new();
            let mut previous: Option<&[u8]> = None;
            for _ in 0..count {
                let key = cursor.key(mem.limits().max_bytes)?;
                if previous.is_some_and(|old| old >= key) {
                    return Err(Error::Corrupt);
                }
                previous = Some(key);
                let child = child(cursor.int()?)?;
                map = map.set(mem, key, child)?;
            }
            Value::from_map(mem, map)?
        }
        7 => {
            let count = cursor.count(mem.limits().max_entries)?;
            let mut list = List::new();
            for _ in 0..count {
                let child = child(cursor.int()?)?;
                list = list.push(mem, child)?;
            }
            Value::from_list(mem, list)?
        }
        _ => return Err(Error::Corrupt),
    };
    cursor.done()?;
    Ok(value)
}
impl Reader<'_> {
    fn object(&mut self, offset: u64, kind: u32, bound: u64) -> Result<Object> {
        if offset < APPEND_START as u64
            || !offset.is_multiple_of(REGION as u64)
            || offset
                .checked_add(OBJECT_HEADER as u64)
                .is_none_or(|end| end > bound)
        {
            return Err(Error::Corrupt);
        }
        let mut header = [0; OBJECT_HEADER];
        read_all(self.backend, offset, &mut header)?;
        if &header[..8] != b"WDBOBJ01"
            || u32_at(&header, 8) != kind
            || u32_at(&header, 12) != VERSION
            || header[32..48] != self.identity
            || header[56..].iter().any(|b| *b != 0)
        {
            return Err(Error::Corrupt);
        }
        let payload_len = usize::try_from(u64_at(&header, 16)).map_err(|_| Error::Corrupt)?;
        let length = (OBJECT_HEADER as u64)
            .checked_add(payload_len as u64)
            .ok_or(Error::Corrupt)?;
        let size = align(length).map_err(|_| Error::Corrupt)?;
        let end = offset
            .checked_add(size)
            .filter(|end| *end <= bound)
            .ok_or(Error::Corrupt)?;
        let born = u64_at(&header, 24);
        if born > self.revision {
            return Err(Error::Corrupt);
        }
        let first_region = (offset / REGION as u64) as usize;
        let end_region = (end / REGION as u64) as usize;
        if self.objects >= MAX_OBJECTS && !self.packed.extended()
            || (!self.regions.is_empty()
                && self
                    .regions
                    .get(first_region..end_region)
                    .ok_or(Error::Corrupt)?
                    .iter()
                    .any(|&entry| entry != 0))
        {
            return Err(Error::Corrupt);
        }
        let work_limit =
            usize::try_from(self.packed.capacity().saturating_mul(2)).unwrap_or(usize::MAX);
        self.work = self
            .work
            .checked_add(size as usize + OBJECT_HEADER)
            .filter(|n| *n <= work_limit)
            .ok_or(Error::Limit)?;
        if self.packed.extended() {
            self.extents.check_free(offset, end)?;
        }
        let mut bytes = std::mem::replace(&mut self.scratch, ChargedVec::new());
        bytes.clear();
        // Only a witnessed, read-only view may defer a multi-region byte value.
        // Validate its declared extent and type/length prefix now. All structural
        // objects and single-region values take the complete verification path.
        let mut deferred_blob = self.defer_blobs && kind == packed::blob::BLOB;
        if self.defer_blobs && kind == VALUE && size > REGION as u64 && payload_len >= 16 {
            let mut prefix = [0; 16];
            read_all(self.backend, offset + OBJECT_HEADER as u64, &mut prefix)?;
            if u64_at(&prefix, 0) == 5 {
                if u64_at(&prefix, 8) != (payload_len - 16) as u64 {
                    return Err(Error::Corrupt);
                }
                self.mem.reserve(&mut bytes, 16)?;
                bytes.extend_from_slice(&prefix);
                deferred_blob = true;
            }
        }
        if !deferred_blob {
            self.mem.reserve(&mut bytes, size as usize)?;
            bytes.resize(size as usize, 0);
            read_all(self.backend, offset, &mut bytes)?;
            if bytes[..OBJECT_HEADER] != header
                || checksum(&bytes, 48) != u64_at(&bytes, 48)
                || bytes[OBJECT_HEADER + payload_len..].iter().any(|b| *b != 0)
            {
                return Err(Error::Corrupt);
            }
            bytes.copy_within(OBJECT_HEADER..OBJECT_HEADER + payload_len, 0);
            bytes.truncate(payload_len);
        }
        // Allocate lazily at the former range-registration boundary, after the
        // full object validates. Malformed first headers still fail before any
        // index allocation. Every accepted region is marked exactly once.
        if self.packed.extended() {
            self.extents.insert(self.mem, offset, end)?;
        } else {
            if self.regions.is_empty() {
                self.mem.reserve(&mut self.regions, self.region_count)?;
                self.regions.resize(self.region_count, 0);
            }
            self.regions
                .get_mut(first_region..end_region)
                .ok_or(Error::Corrupt)?
                .fill(1);
        }
        self.objects += 1;
        Ok(Object {
            payload_len,
            deferred_blob,
            digest: u64_at(&header, 48),
            payload: bytes,
            born,
            end,
        })
    }
    fn value(&mut self, offset: u64, parent: u64, depth: usize) -> Result<Value> {
        diagnostics::verify(Stage::Value, Some(offset), || {
            self.value_inner(offset, parent, depth)
        })
    }
    fn value_inner(&mut self, offset: u64, parent: u64, depth: usize) -> Result<Value> {
        if depth > self.mem.limits().max_depth || offset >= parent {
            return Err(Error::Corrupt);
        }
        // An unaligned reference must never alias the cache entry in the same
        // region. Let object() reject it at the existing validation boundary.
        let cached = offset
            .is_multiple_of(REGION as u64)
            .then(|| self.regions.get((offset / REGION as u64) as usize))
            .flatten()
            .and_then(|entry| entry.checked_sub(2));
        if let Some(old) = cached.map(|index| &self.cache[index]) {
            if old.end > parent {
                return Err(Error::Corrupt);
            }
            return Ok(old.value.clone());
        }
        let object = self.object(offset, VALUE, parent)?;
        let value = decode_value(self.mem, &object.payload, |child| {
            self.value(child, offset, depth + 1)
        })?;
        self.mem
            .reserve_geometric(&mut self.cache, 1, MAX_OBJECTS)?;
        self.regions[(offset / REGION as u64) as usize] = self.cache.len() + 2;
        self.cache.push(CacheEntry {
            value: value.clone(),
            offset,
            end: object.end,
        });
        Ok(value)
    }
    // Validate a value graph without constructing the public immutable values.
    fn verify_shape(&mut self, offset: u64, parent: u64, depth: usize) -> Result<Shape> {
        diagnostics::verify(Stage::Value, Some(offset), || {
            if depth > self.mem.limits().max_depth
                || offset >= parent
                || !offset.is_multiple_of(REGION as u64)
            {
                return Err(Error::Corrupt);
            }
            let region = (offset / REGION as u64) as usize;
            if let Some(i) = self.regions.get(region).and_then(|s| s.checked_sub(2)) {
                let shape = self.shapes[i];
                return if shape.end <= parent {
                    Ok(shape)
                } else {
                    Err(Error::Corrupt)
                };
            }
            let obj = self.object(offset, VALUE, parent)?;
            let limits = self.mem.limits();
            let mut c = Cursor::new(&obj.payload);
            let tag = c.int()?;
            let mut weight = 16usize;
            let mut height = 1;
            match tag {
                0..=4 => {
                    let bits = c.int()?;
                    if (tag == 0 && bits != 0) || (tag == 1 && bits > 1) {
                        return Err(Error::Corrupt);
                    }
                }
                5 => {
                    let len = if obj.deferred_blob {
                        c.count(limits.max_bytes)?
                    } else {
                        c.key(limits.max_bytes)?.len()
                    };
                    weight = weight.checked_add(len).ok_or(Error::Limit)?
                }
                6 | 7 => {
                    let count = c.count(limits.max_entries)?;
                    let mut previous = None;
                    for _ in 0..count {
                        if tag == 6 {
                            let k = c.key(limits.max_bytes)?;
                            if previous.is_some_and(|old: &[u8]| old >= k) {
                                return Err(Error::Corrupt);
                            }
                            previous = Some(k);
                            weight = weight.checked_add(k.len()).ok_or(Error::Limit)?;
                        }
                        let child = self.verify_shape(c.int()?, offset, depth + 1)?;
                        weight = weight.checked_add(child.weight).ok_or(Error::Limit)?;
                        height = height.max(child.depth + 1);
                    }
                }
                _ => return Err(Error::Corrupt),
            }
            c.done()?;
            if weight > limits.max_value_bytes || height > limits.max_depth {
                return Err(Error::Limit);
            }
            let shape = Shape {
                end: obj.end,
                depth: height,
                weight,
                digest: obj.digest,
                composite: tag == 6 || tag == 7,
            };
            self.mem
                .reserve_geometric(&mut self.shapes, 1, MAX_OBJECTS)?;
            self.regions[region] = self.shapes.len() + 2;
            self.shapes.push(shape);
            self.scratch = obj.payload;
            Ok(shape)
        })
    }
    fn data(
        &mut self,
        offset: u64,
        parent: u64,
        expected_revision: u64,
    ) -> Result<(Map<Collection>, u64)> {
        diagnostics::verify(Stage::Directory, Some(offset), || {
            self.data_inner(offset, parent, expected_revision)
        })
    }
    fn data_inner(
        &mut self,
        offset: u64,
        parent: u64,
        expected_revision: u64,
    ) -> Result<(Map<Collection>, u64)> {
        if let Some(old) = self.data_cache.iter().find(|r| r.entry.offset == offset) {
            if old.end > parent || old.entry.revision != expected_revision {
                return Err(Error::Corrupt);
            }
            return Ok((old.collections.clone(), old.entry.next_collection));
        }
        if self
            .data_cache
            .iter()
            .any(|r| r.entry.revision == expected_revision)
        {
            return Err(Error::Corrupt);
        }
        let object = self.object(offset, DATA, parent)?;
        let mut cursor = Cursor::new(&object.payload);
        let revision = cursor.int()?;
        let next_collection = cursor.int()?;
        if revision != expected_revision || object.born != revision || next_collection == 0 {
            return Err(Error::Corrupt);
        }
        let count = cursor.count(self.mem.limits().max_entries)?;
        let mut collections = Map::new();
        let mut previous_id = 0;
        for _ in 0..count {
            let id = cursor.int()?;
            let content_revision = cursor.int()?;
            if id <= previous_id
                || id >= next_collection
                || content_revision == 0
                || content_revision > revision
            {
                return Err(Error::Corrupt);
            }
            previous_id = id;
            let records_count = cursor.count(self.mem.limits().max_entries)?;
            if self.packed != FileFormat::V2 {
                let c =
                    self.packed_collection(&mut cursor, offset, content_revision, records_count)?;
                collections = collections.set(self.mem, &id.to_le_bytes(), c)?;
                continue;
            }
            let mut records = Map::new();
            let mut previous: Option<&[u8]> = None;
            let mut directory = self.graph.clone().map(Directory::new);
            let mut start = cursor.pos;
            for _ in 0..records_count {
                let row_start = cursor.pos;
                let key = cursor.key(self.mem.limits().max_bytes)?;
                if previous.is_some_and(|old| old >= key) {
                    return Err(Error::Corrupt);
                }
                previous = Some(key);
                let write_revision = cursor.int()?;
                if write_revision == 0 || write_revision > content_revision {
                    return Err(Error::Corrupt);
                }
                let value_offset = cursor.int()?;
                if let Some(dir) = directory.as_mut() {
                    self.verify_shape(value_offset, offset, 1)?;
                    if row_start > start && cursor.pos - start > REGION {
                        dir.add(
                            self.mem,
                            offset + OBJECT_HEADER as u64 + start as u64,
                            &object.payload[start..row_start],
                        )?;
                        start = row_start;
                    }
                    continue;
                }
                let value = self.value(value_offset, offset, 1)?;
                records = records.set(
                    self.mem,
                    key,
                    Record {
                        revision: write_revision,
                        value,
                    },
                )?;
            }
            let base = if let Some(mut dir) = directory {
                if cursor.pos > start {
                    dir.add(
                        self.mem,
                        offset + OBJECT_HEADER as u64 + start as u64,
                        &object.payload[start..cursor.pos],
                    )?;
                }
                Some(crate::engine::Base::new(
                    self.mem.shared(dir)?,
                    records_count,
                ))
            } else {
                None
            };
            collections = collections.set(
                self.mem,
                &id.to_le_bytes(),
                Collection {
                    base,
                    revision: content_revision,
                    records,
                },
            )?;
        }
        cursor.done()?;
        self.mem
            .reserve_geometric(&mut self.data_cache, 1, MAX_OBJECTS)?;
        self.data_cache.push(DecodedData {
            entry: DataEntry {
                revision,
                next_collection,
                offset,
            },
            collections: collections.clone(),
            end: object.end,
        });
        Ok((collections, next_collection))
    }
}
pub(crate) struct Recovered {
    pub packed: FileFormat,
    pub identity: [u8; 16],
    pub slot: Slot,
    pub witnessed: bool,
    pub head: Shared<Head>,
    pub next_collection: u64,
    pub cache: ChargedVec<CacheEntry>,
    pub data_cache: ChargedVec<DataEntry>,
    pub retention: Retention,
}
pub(crate) fn recover(backend: &mut dyn ByteStore, mem: &Memory) -> Result<Recovered> {
    recover_with_pages(backend, mem, None)
}
pub(super) fn recover_with_pages(
    backend: &mut dyn ByteStore,
    mem: &Memory,
    source: Option<Source>,
) -> Result<Recovered> {
    recover_mode(backend, mem, source, false)
}
pub(crate) fn recover_read_only(
    source: Source,
    mem: &Memory,
    defer_blobs: bool,
) -> Result<Recovered> {
    recover_mode(&mut source.clone(), mem, Some(source), defer_blobs)
}
fn recover_mode(
    backend: &mut dyn ByteStore,
    mem: &Memory,
    source: Option<Source>,
    defer_blobs: bool,
) -> Result<Recovered> {
    let header = read_header(backend)?;
    recover_after_header(backend, mem, source, defer_blobs, header)
}
pub(super) struct ImageHeader {
    length: u64,
    identity: [u8; 16],
    pub(super) packed: FileFormat,
}
pub(super) fn read_header(backend: &mut dyn ByteStore) -> Result<ImageHeader> {
    diagnostics::clear_verification();
    let length = diagnostics::verify(Stage::ImageLength, None, || {
        let length = backend.length()?;
        if length < APPEND_START as u64
            || length > FileFormat::Extended(super::FileCapacity::Unbounded).capacity()
        {
            return Err(Error::Corrupt);
        }
        Ok(length)
    })?;
    let (identity, packed) = diagnostics::verify(Stage::Header, Some(0), || {
        let mut header = [0; REGION];
        read_all(backend, 0, &mut header)?;
        if &header[..8] != b"WEEKDB01"
            || !matches!(
                u32_at(&header, 8),
                VERSION | PACKED_VERSION | EXTENDED_VERSION
            )
        {
            return Err(Error::UnsupportedFormat);
        }
        let format = FileFormat::from_header(u32_at(&header, 8), u64_at(&header, 48))?;
        if length > format.capacity()
            || checksum(&header, 64) != u64_at(&header, 64)
            || u32_at(&header, 12) != 0
            || u64_at(&header, 16) != REGION as u64
            || u64_at(&header, 24) != APPEND_START as u64
            || u64_at(&header, 56)
                != if format.extended() {
                    0
                } else {
                    MAX_OBJECTS as u64
                }
            || header[72..].iter().any(|b| *b != 0)
        {
            return Err(Error::Corrupt);
        }
        let identity: [u8; 16] = header[32..48].try_into().unwrap();
        if identity == [0; 16] {
            return Err(Error::Corrupt);
        }
        Ok((identity, format))
    })?;
    Ok(ImageHeader {
        length,
        identity,
        packed,
    })
}
pub(super) fn recover_after_header(
    backend: &mut dyn ByteStore,
    mem: &Memory,
    source: Option<Source>,
    defer_blobs: bool,
    ImageHeader {
        length,
        identity,
        packed,
    }: ImageHeader,
) -> Result<Recovered> {
    let mut slots = [None; 2];
    let mut witnesses = [None; 2];
    for i in 0..2 {
        slots[i] = diagnostics::verify(Stage::RootSlot, Some(((1 + i) * REGION) as u64), || {
            let mut bytes = [0; REGION];
            read_all(backend, ((1 + i) * REGION) as u64, &mut bytes)?;
            parse_metadata(&bytes, identity, i, false, length)
        })?;
        witnesses[i] =
            diagnostics::verify(Stage::Witness, Some(((3 + i) * REGION) as u64), || {
                let mut bytes = [0; REGION];
                read_all(backend, ((3 + i) * REGION) as u64, &mut bytes)?;
                parse_metadata(&bytes, identity, i, true, length)
            })?;
    }
    let slot = diagnostics::verify(Stage::RootSelection, None, || {
        let slot = slots
            .into_iter()
            .flatten()
            .max_by_key(|s| s.revision)
            .ok_or(Error::Corrupt)?;
        for witness in witnesses.into_iter().flatten() {
            if witness.revision > slot.revision
                || (witness.revision == slot.revision && witness != slot)
            {
                return Err(Error::Corrupt);
            }
        }
        if slots[0].is_some_and(|a| slots[1].is_some_and(|b| a.revision.abs_diff(b.revision) != 1))
        {
            return Err(Error::Corrupt);
        }
        Ok(slot)
    })?;
    diagnostics::verify(Stage::CommitDescriptor, Some(slot.descriptor), || {
        let graph = source
            .map(|s| mem.shared(Graph::new(s.with_format(packed))))
            .transpose()?;
        let mut reader = Reader {
            packed,
            page_cache: packed::PageCache::new(),
            extents: extents::Offsets::new(),
            blobs: extents::Offsets::new(),
            defer_blobs: defer_blobs && witnesses[slot.index] == Some(slot),
            graph: graph.clone(),
            shapes: ChargedVec::new(),
            scratch: ChargedVec::new(),
            backend,
            mem,
            identity,
            revision: slot.revision,
            cache: ChargedVec::new(),
            regions: ChargedVec::new(),
            region_count: (slot.end / REGION as u64) as usize,
            objects: 0,
            work: 0,
            data_cache: ChargedVec::new(),
        };
        let root = reader.object(slot.descriptor, COMMIT, slot.end)?;
        if root.end != slot.end || root.born != slot.revision {
            return Err(Error::Corrupt);
        }
        let mut cursor = Cursor::new(&root.payload);
        let revision = cursor.int()?;
        let data_ref = cursor.int()?;
        let data_revision = cursor.int()?;
        if revision != slot.revision || data_revision > revision {
            return Err(Error::Corrupt);
        }
        let next_id = cursor.int()?;
        if next_id == 0 {
            return Err(Error::Corrupt);
        }
        let count = cursor.count(MAX_RETAINED_SNAPSHOTS)?;
        let (collections, next_collection) =
            reader.data(data_ref, slot.descriptor, data_revision)?;
        let head = mem.shared(Head {
            revision,
            data_revision,
            collections,
        })?;
        let mut entries = ChargedVec::new();
        let head_next_collection = next_collection;
        mem.reserve(&mut entries, count)?;
        let mut previous_id = 0;
        for _ in 0..count {
            let id = cursor.int()?;
            let snapshot_revision = cursor.int()?;
            let offset = cursor.int()?;
            let snapshot_data_revision = cursor.int()?;
            if id <= previous_id
                || id >= next_id
                || snapshot_revision >= revision
                || snapshot_data_revision > snapshot_revision
                || snapshot_data_revision > data_revision
            {
                return Err(Error::Corrupt);
            }
            previous_id = id;
            let (collections, next_collection) =
                reader.data(offset, slot.descriptor, snapshot_data_revision)?;
            if next_collection > head_next_collection {
                return Err(Error::Corrupt);
            }
            let head = mem.shared(Head {
                revision: snapshot_revision,
                data_revision: snapshot_data_revision,
                collections,
            })?;
            entries.push(Retained {
                id,
                head,
                next_collection,
            });
        }
        cursor.done()?;
        let mut data_cache = ChargedVec::new();
        mem.reserve(&mut data_cache, reader.data_cache.len())?;
        data_cache.extend(reader.data_cache.into_iter().map(|r| r.entry));
        if let Some(graph) = graph {
            graph.finish(reader.regions, reader.shapes);
        }
        Ok(Recovered {
            packed,
            identity,
            slot,
            witnessed: witnesses[slot.index] == Some(slot),
            head,
            next_collection,
            cache: reader.cache,
            data_cache,
            retention: Retention { next_id, entries },
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Database, Limits,
        memory::test_allocator::TestAllocator,
        storage::{IoKind, SimulatedDisk, Storage},
    };
    use std::sync::Arc;

    const ID: [u8; 16] = *b"codec-test-00001";
    #[test]
    fn cache_forks_preserve_indexes_first_match_and_rollback_every_allocation() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let mut entries = ChargedVec::new();
        mem.reserve(&mut entries, 20).unwrap();
        for i in 0..20 {
            entries.push(CacheEntry {
                value: Value::bytes_new(&mem, &[i]).unwrap(),
                offset: i as u64,
                end: i as u64 + 1,
            });
        }
        let values = mem
            .shared(ValueCache::from_entries(&mem, entries).unwrap())
            .unwrap();
        let data_entries = [100, 0, u64::MAX, 40, 100]
            .into_iter()
            .enumerate()
            .map(|(i, revision)| DataEntry {
                revision,
                next_collection: i as u64 + 1,
                offset: i as u64,
            })
            .collect::<Vec<_>>();
        let data = mem
            .shared(
                DataCache::from_entries(&mem, fork_vec(&mem, &data_entries, MAX_OBJECTS).unwrap())
                    .unwrap(),
            )
            .unwrap();
        let baseline = mem.usage().0;
        let ((), requests) = allocator.count_with_refusal(0, || {
            assert!(
                CacheState::Borrowed(&values)
                    .finish(&mem)
                    .unwrap()
                    .ptr_eq(&values)
            );
            assert!(
                CacheState::Borrowed(&data)
                    .finish(&mem)
                    .unwrap()
                    .ptr_eq(&data)
            );
        });
        assert_eq!(requests, 0);
        assert_eq!(mem.usage().0, baseline);
        // Each fork needs two vector allocations, then one shared header.
        for allowance in 0..=3 {
            let result = allocator.with_refusal(allowance, || {
                let mut state = CacheState::Borrowed(&values);
                state.make_mut(|old| old.fork(&mem))?;
                state.finish(&mem)
            });
            if allowance < 3 {
                assert!(matches!(result, Err(Error::OutOfMemory)));
            } else {
                let next = result.as_ref().unwrap();
                assert!(!next.ptr_eq(&values));
                assert_eq!(&*next.index.slots, &*values.index.slots);
                for (i, old) in values.entries.iter().enumerate() {
                    assert!(next.entries[i].value.same_node(&old.value));
                    assert_eq!(next.index.get(&old.value, &next.entries), Some(i));
                }
            }
            drop(result);
            assert_eq!(mem.usage().0, baseline);
            let result = allocator.with_refusal(allowance, || {
                let mut state = CacheState::Borrowed(&data);
                state.make_mut(|old| old.fork(&mem))?;
                state.finish(&mem)
            });
            if allowance < 3 {
                assert!(matches!(result, Err(Error::OutOfMemory)));
            } else {
                let next = result.as_ref().unwrap();
                assert_eq!(&*next.index.order, &*data.index.order);
                assert_eq!(next.index.get(100, &next.entries), Some(0));
                assert_eq!(next.entries[0].next_collection, 1);
            }
            drop(result);
            assert_eq!(mem.usage().0, baseline);
            assert_eq!(data.index.get(100, &data.entries), Some(0));
        }
    }

    #[test]
    fn cache_state_retries_refused_fork_and_reuses_owned_state() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let published = mem.shared(mem.copy(b"prior").unwrap()).unwrap();
        let baseline = (mem.usage().0, allocator.live_bytes());
        let mut state = CacheState::Borrowed(&published);
        assert_eq!(state.as_slice(), b"prior");

        assert_eq!(
            allocator.with_refusal(0, || {
                state.make_mut(|old| mem.copy(old.as_slice())).map(|_| ())
            }),
            Err(Error::OutOfMemory)
        );
        assert!(matches!(&state, CacheState::Borrowed(old) if old.ptr_eq(&published)));
        assert_eq!((mem.usage().0, allocator.live_bytes()), baseline);
        let scratch = state.make_mut(|old| mem.copy(old.as_slice())).unwrap();
        scratch.clear();
        scratch.extend_from_slice(b"draft");
        assert_eq!(state.as_slice(), b"draft");
        assert_eq!(published.as_slice(), b"prior");
        assert_eq!(
            allocator.with_refusal(0, || {
                state
                    .make_mut(|_| panic!("owned cache forked again"))
                    .map(|scratch| assert_eq!(scratch.as_slice(), b"draft"))
            }),
            Ok(())
        );
        drop(state);
        assert_eq!((mem.usage().0, allocator.live_bytes()), baseline);
    }

    #[test]
    fn candidates_share_unchanged_caches_fork_only_appended_data_and_fail_atomically() {
        use crate::engine::{Collection, Record};
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let value = Value::bytes_new(&mem, b"old").unwrap();
        let records = Map::new()
            .set(&mem, b"row", Record { revision: 1, value })
            .unwrap();
        let collections = Map::new()
            .set(
                &mem,
                &1u64.to_le_bytes(),
                Collection {
                    base: None,
                    revision: 1,
                    records,
                },
            )
            .unwrap();
        let mut head = Head {
            revision: 1,
            data_revision: 1,
            collections,
        };
        let retention = Retention {
            next_id: 1,
            entries: ChargedVec::new(),
        };
        let first = encode(
            &mem,
            &head,
            2,
            ID,
            APPEND_START as u64,
            None,
            None,
            &retention,
        )
        .unwrap();
        head.revision = 2;
        let metadata = encode(
            &mem,
            &head,
            2,
            ID,
            first.end,
            Some(&first.cache),
            Some(&first.data_cache),
            &retention,
        )
        .unwrap();
        assert!(metadata.cache.ptr_eq(&first.cache));
        assert!(metadata.data_cache.ptr_eq(&first.data_cache));
        head.revision = 3;
        head.data_revision = 3;
        let data_only = encode(
            &mem,
            &head,
            2,
            ID,
            metadata.end,
            Some(&metadata.cache),
            Some(&metadata.data_cache),
            &retention,
        )
        .unwrap();
        assert!(data_only.cache.ptr_eq(&metadata.cache));
        assert!(!data_only.data_cache.ptr_eq(&metadata.data_cache));
        assert_eq!(first.data_cache.entries.len(), 1);
        assert_eq!(data_only.data_cache.entries.len(), 2);
        let value = Value::bytes_new(&mem, b"new").unwrap();
        let records = Map::new()
            .set(&mem, b"row", Record { revision: 4, value })
            .unwrap();
        head.collections = Map::new()
            .set(
                &mem,
                &1u64.to_le_bytes(),
                Collection {
                    base: None,
                    revision: 4,
                    records,
                },
            )
            .unwrap();
        head.revision = 4;
        head.data_revision = 4;
        let before = mem.usage().0;
        let mut failures = 0;
        for allowance in 0..100 {
            let result = allocator.with_refusal(allowance, || {
                encode(
                    &mem,
                    &head,
                    2,
                    ID,
                    data_only.end,
                    Some(&data_only.cache),
                    Some(&data_only.data_cache),
                    &retention,
                )
            });
            let success = result.is_ok();
            if !success {
                assert!(matches!(result, Err(Error::OutOfMemory)));
                failures += 1;
            } else {
                let next = result.as_ref().unwrap();
                assert!(!next.cache.ptr_eq(&data_only.cache));
                assert!(!next.data_cache.ptr_eq(&data_only.data_cache));
                assert_eq!(next.cache.entries.len(), 2);
                assert_eq!(next.data_cache.entries.len(), 3);
            }
            drop(result);
            assert_eq!(mem.usage().0, before);
            assert_eq!(data_only.cache.entries.len(), 1);
            assert_eq!(data_only.cache.entries[0].value.as_bytes().unwrap(), b"old");
            assert_eq!(data_only.data_cache.entries.len(), 2);
            if success {
                break;
            }
        }
        assert!(failures > 3 && failures < 99);
    }

    #[test]
    fn value_encoding_reuses_cached_nodes_and_propagates_each_allocation_failure() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let cached = Value::bytes_new(&mem, b"shared").unwrap();
        let uncached = Value::bytes_new(&mem, b"new").unwrap();
        let make_encoder = |start, cache| Encoder {
            packed: None,
            rebased: None,
            relocation: packed::Relocation::new(),
            copied: ChargedVec::new(),
            mem: &mem,
            identity: ID,
            revision: 1,
            start,
            end: start,
            bytes: AppendBytes::Contiguous(ChargedVec::new()),
            cache,
            data_cache: CacheState::Owned(DataCache::new()),
        };
        let mut seed = make_encoder(APPEND_START as u64, CacheState::Owned(ValueCache::new()));
        let cached_offset = seed.value(&cached).unwrap();
        // A half-full index must grow when the first new value is inserted.
        for n in 0..3 {
            seed.value(&Value::scalar(&mem, Kind::Unsigned, n).unwrap())
                .unwrap();
        }
        let start = seed.end;
        let cache = seed.cache.finish(&mem).unwrap();
        drop(seed.bytes);
        assert_eq!(cache.entries.len(), 4);
        assert_eq!(cache.index.slots.len(), 8);

        let empty_map = Value::scalar(&mem, Kind::Map, 0).unwrap();
        let map = empty_map
            .map_set(b"a", &cached)
            .unwrap()
            .map_set(b"b", &uncached)
            .unwrap();
        let empty_list = Value::scalar(&mem, Kind::List, 0).unwrap();
        let list = empty_list
            .list_push(&map)
            .unwrap()
            .list_push(&uncached)
            .unwrap();
        let inputs = [
            Value::scalar(&mem, Kind::None, 0).unwrap(),
            Value::scalar(&mem, Kind::Boolean, 1).unwrap(),
            Value::scalar(&mem, Kind::Integer, u64::MAX).unwrap(),
            Value::scalar(&mem, Kind::Unsigned, u64::MAX).unwrap(),
            Value::scalar(&mem, Kind::Float, 0x7ff8_0000_0000_0042).unwrap(),
            uncached,
            empty_map,
            map,
            empty_list,
            list,
        ];

        let before = mem.usage().0;
        let mut encoder = make_encoder(start, CacheState::Borrowed(&cache));
        assert_eq!(
            allocator.with_refusal(0, || encoder.value(&cached)),
            Ok(cached_offset)
        );
        assert_eq!(encoder.end, start);
        assert!(matches!(&encoder.bytes, AppendBytes::Contiguous(bytes) if bytes.is_empty()));
        assert!(matches!(&encoder.cache, CacheState::Borrowed(old) if old.ptr_eq(&cache)));
        drop(encoder);

        for value in &inputs {
            let mut failures = 0;
            let mut completed = false;
            for allowance in 0..64 {
                let mut encoder = make_encoder(start, CacheState::Borrowed(&cache));
                let result = allocator.with_refusal(allowance, || encoder.value(value));
                match result {
                    Ok(offset) => {
                        let index = encoder
                            .cache
                            .index
                            .get(value, &encoder.cache.entries)
                            .unwrap();
                        assert_eq!(encoder.cache.entries[index].offset, offset);
                        assert!(encoder.cache.entries[index].value.same_node(value));
                        assert_eq!(encoder.value(value), Ok(offset));
                        assert!(encoder.cache.index.slots.len() > cache.index.slots.len());
                        completed = true;
                    }
                    Err(error) => {
                        assert_eq!(
                            error,
                            Error::OutOfMemory,
                            "{:?} at {allowance}",
                            value.kind()
                        );
                        failures += 1;
                    }
                }
                drop(encoder);
                assert_eq!(mem.usage().0, before);
                assert_eq!(cache.entries.len(), 4);
                assert_eq!(cache.index.get(&cached, &cache.entries), Some(0));
                assert_eq!(cache.entries[0].offset, cached_offset);
                if completed {
                    break;
                }
            }
            assert!(completed && failures > 0, "{:?}", value.kind());
        }
    }

    #[test]
    fn writer_bulk_appends_reserve_known_size_and_small_fields_reuse_capacity() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: 100,
            ..Limits::default()
        });
        let mut writer = Writer::new(&mem);
        writer.add(&[7; 17]).unwrap();
        assert_eq!(mem.usage(), (17, 17));
        writer.add(&[8; 18]).unwrap();
        assert_eq!(mem.usage(), (35, 52));
        writer.add(&[9]).unwrap();
        assert_eq!(mem.usage(), (64, 99));
        allocator.with_refusal(0, || writer.add(&[10])).unwrap();
        assert_eq!(&writer.bytes[..17], &[7; 17]);
        assert_eq!(&writer.bytes[17..35], &[8; 18]);
        assert_eq!(&writer.bytes[35..], &[9, 10]);
        drop(writer);
        assert_eq!(mem.usage().0, 0);
    }

    #[test]
    fn writer_equal_append_and_key_failures_preserve_the_written_prefix() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let mut writer = Writer::new(&mem);
        writer.add(&[1; 17]).unwrap();
        assert_eq!(
            allocator.with_refusal(0, || writer.add(&[2; 17])),
            Err(Error::OutOfMemory)
        );
        assert_eq!(writer.bytes.as_slice(), &[1; 17]);
        assert_eq!(mem.usage().0, 17);
        writer.add(&[2; 17]).unwrap();
        assert_eq!(mem.usage(), (64, 81));
        assert_eq!(&writer.bytes[..17], &[1; 17]);
        assert_eq!(&writer.bytes[17..], &[2; 17]);
        drop(writer);

        let key = [3; 17];
        let mut writer = Writer::new(&mem);
        assert_eq!(
            allocator.with_refusal(0, || writer.key(&[])),
            Err(Error::OutOfMemory)
        );
        assert!(writer.bytes.is_empty());
        assert_eq!(
            allocator.with_refusal(1, || writer.key(&key)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(writer.bytes.as_slice(), &17u64.to_le_bytes());
        drop(writer);
        assert_eq!(mem.usage().0, 0);

        let mut writer = Writer::new(&mem);
        writer.key(&key).unwrap();
        assert_eq!(&writer.bytes[..8], &17u64.to_le_bytes());
        assert_eq!(&writer.bytes[8..], &key);
    }
    #[test]
    fn data_index_rebuild_insert_and_first_match_preserve_cache_order() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let mut cache: Vec<_> = [100, 0, u64::MAX, 40, 100]
            .into_iter()
            .enumerate()
            .map(|(i, revision)| DataEntry {
                revision,
                next_collection: i as u64 + 1,
                offset: i as u64,
            })
            .collect();
        let mut index = DataIndex::from_cache(&mem, &cache).unwrap();
        assert_eq!(&*index.order, &[1, 3, 0, 4, 2]);
        for revision in [0, 1, 40, 41, 100, 101, u64::MAX] {
            assert_eq!(
                index.get(revision, &cache),
                cache.iter().position(|entry| entry.revision == revision)
            );
        }
        // A duplicate revision selects its first original entry, even when
        // its counter differs from the later entry. Rebuild never sorts cache.
        assert_eq!(cache[index.get(100, &cache).unwrap()].next_collection, 1);
        assert_eq!(
            cache.iter().map(|entry| entry.revision).collect::<Vec<_>>(),
            [100, 0, u64::MAX, 40, 100]
        );
        let live = mem.public_usage().live_bytes;
        let order = index.order.to_vec();
        // Existing eight-slot capacity.
        allocator
            .with_refusal(0, || index.reserve(&mem, 6))
            .unwrap();
        assert_eq!(
            allocator.with_refusal(0, || index.reserve(&mem, 9)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(&*index.order, order);
        assert_eq!(mem.public_usage().live_bytes, live);
        assert!(matches!(
            allocator.with_refusal(0, || DataIndex::from_cache(&mem, &cache)),
            Err(Error::OutOfMemory)
        ));
        assert_eq!(mem.public_usage().live_bytes, live);
        for revision in [1, 55, u64::MAX - 1] {
            index.reserve(&mem, cache.len() + 1).unwrap();
            index.insert(revision, &cache, cache.len());
            cache.push(DataEntry {
                revision,
                next_collection: 1,
                offset: cache.len() as u64,
            });
            assert_eq!(index.get(revision, &cache), Some(cache.len() - 1));
        }
        let mut index = DataIndex::new();
        let mut cache = Vec::new();
        for revision in [50, 0, 100, 25, u64::MAX] {
            assert_eq!(index.get(revision, &cache), None);
            index.reserve(&mem, cache.len() + 1).unwrap();
            index.insert(revision, &cache, cache.len());
            cache.push(DataEntry {
                revision,
                next_collection: 1,
                offset: cache.len() as u64,
            });
        }
        assert_eq!(&*index.order, &[1, 3, 0, 2, 4]);
        for (i, entry) in cache.iter().enumerate() {
            assert_eq!(index.get(entry.revision, &cache), Some(i));
        }
        // Inserting an equal revision takes the lower-bound path even
        // when it equals the final entry. Retain cache emission order.
        let duplicate = cache.len();
        index.reserve(&mem, duplicate + 1).unwrap();
        index.insert(u64::MAX, &cache, duplicate);
        cache.push(DataEntry {
            revision: u64::MAX,
            next_collection: 9,
            offset: duplicate as u64,
        });
        assert_eq!(&*index.order, &[1, 3, 0, 2, duplicate, 4]);
        assert_eq!(index.get(u64::MAX, &cache), Some(duplicate));
    }
    #[test]
    fn data_encoding_orders_records_and_keeps_the_acknowledged_cache() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let make_encoder = |start, data_cache| Encoder {
            packed: None,
            rebased: None,
            relocation: packed::Relocation::new(),
            copied: ChargedVec::new(),
            mem: &mem,
            identity: ID,
            revision: 6,
            start,
            end: start,
            bytes: AppendBytes::Contiguous(ChargedVec::new()),
            cache: CacheState::Owned(ValueCache::new()),
            data_cache,
        };
        let next_collection = 257;
        let mut empty = Head {
            revision: 5,
            data_revision: 1,
            collections: Map::new(),
        };
        let mut seed = make_encoder(APPEND_START as u64, CacheState::Owned(DataCache::new()));
        for revision in 1..=4 {
            empty.data_revision = revision;
            assert_eq!(
                seed.data(&empty, next_collection),
                Ok((APPEND_START + (revision as usize - 1) * REGION) as u64)
            );
        }
        let start = seed.end;
        let cache = seed.data_cache.finish(&mem).unwrap();
        drop(seed.bytes);
        let before = mem.usage().0;
        let mut encoder = make_encoder(start, CacheState::Borrowed(&cache));
        assert_eq!(
            allocator.with_refusal(0, || encoder.data(&empty, next_collection)),
            Ok(start - REGION as u64)
        );
        assert_eq!(
            allocator.with_refusal(0, || encoder.data(&empty, next_collection + 1)),
            Err(Error::Invalid)
        );
        assert_eq!(encoder.end, start);
        assert!(matches!(&encoder.bytes, AppendBytes::Contiguous(bytes) if bytes.is_empty()));
        assert!(matches!(&encoder.data_cache, CacheState::Borrowed(old) if old.ptr_eq(&cache)));
        drop(encoder);
        assert_eq!(mem.usage().0, before);

        let shared = Value::scalar(&mem, Kind::Unsigned, 42).unwrap();
        let distinct = Value::bytes_new(&mem, b"value").unwrap();
        let rows: [(&[u8], u64, &Value); 4] = [
            (b"z", 4, &shared),
            (b"a\0", 2, &distinct),
            (b"", 1, &shared),
            (b"a", 3, &shared),
        ];
        let mut records = Map::new();
        for (key, revision, value) in rows {
            records = records
                .set(
                    &mem,
                    key,
                    Record {
                        revision,
                        value: value.clone(),
                    },
                )
                .unwrap();
        }
        let with_records = |records| Head {
            revision: 5,
            data_revision: 5,
            collections: Map::new()
                .set(
                    &mem,
                    &256u64.to_le_bytes(),
                    Collection {
                        base: None,
                        revision: 2,
                        records: Map::new(),
                    },
                )
                .unwrap()
                .set(
                    &mem,
                    &8u64.to_le_bytes(),
                    Collection {
                        base: None,
                        revision: 4,
                        records,
                    },
                )
                .unwrap(),
        };
        let head = with_records(records);
        let invalid = Head {
            collections: head
                .collections
                .set(
                    &mem,
                    b"short",
                    Collection {
                        base: None,
                        revision: 1,
                        records: Map::new(),
                    },
                )
                .unwrap(),
            ..head
        };
        let before = mem.usage().0;
        let mut encoder = make_encoder(start, CacheState::Borrowed(&cache));
        assert_eq!(encoder.data(&invalid, next_collection), Err(Error::Invalid));
        assert_eq!(encoder.end, start);
        drop(encoder);
        assert_eq!(mem.usage().0, before);

        // The trie visits collection 256 before 8, and a\0 before the empty
        // record key. DATA must nevertheless use numeric IDs and bytewise keys.
        let mut payload = Vec::new();
        for field in [5, next_collection, 2, 8, 4, 4] {
            payload.extend(field.to_le_bytes());
        }
        for (key, revision, offset) in [
            (b"".as_slice(), 1u64, start),
            (b"a", 3, start),
            (b"a\0", 2, start + REGION as u64),
            (b"z", 4, start),
        ] {
            payload.extend((key.len() as u64).to_le_bytes());
            payload.extend(key);
            payload.extend(revision.to_le_bytes());
            payload.extend(offset.to_le_bytes());
        }
        for field in [256u64, 2, 0] {
            payload.extend(field.to_le_bytes());
        }
        let mut encoder = make_encoder(start, CacheState::Borrowed(&cache));
        let offset = encoder.data(&head, next_collection).unwrap();
        assert_eq!(offset, start + 2 * REGION as u64);
        assert_eq!(encoder.end, offset + REGION as u64);
        let AppendBytes::Contiguous(bytes) = &encoder.bytes else {
            unreachable!()
        };
        let object = &bytes[2 * REGION..];
        assert_eq!(u32_at(object, 8), DATA);
        assert_eq!(u64_at(object, 16), payload.len() as u64);
        assert_eq!(u64_at(object, 24), head.data_revision);
        assert_eq!(
            &object[OBJECT_HEADER..OBJECT_HEADER + payload.len()],
            payload
        );
        let entry = &encoder.data_cache.entries[encoder
            .data_cache
            .index
            .get(5, &encoder.data_cache.entries)
            .unwrap()];
        assert_eq!(
            (entry.revision, entry.next_collection, entry.offset),
            (5, next_collection, offset)
        );
        assert_eq!(encoder.data(&head, next_collection), Ok(offset));
        drop(encoder);
        assert_eq!(mem.usage().0, before);

        // These widths put the growth of the DATA payload at the record
        // revision, collection record count and collection ID, respectively.
        let single_record = |width| {
            with_records(
                Map::new()
                    .set(
                        &mem,
                        &vec![b'x'; width],
                        Record {
                            revision: 1,
                            value: shared.clone(),
                        },
                    )
                    .unwrap(),
            )
        };
        for input in [head, single_record(1), single_record(40), single_record(56)] {
            let before = mem.usage().0;
            let mut completed = false;
            for allowance in 0..64 {
                let mut encoder = make_encoder(start, CacheState::Borrowed(&cache));
                let result =
                    allocator.with_refusal(allowance, || encoder.data(&input, next_collection));
                match result {
                    Ok(_) => {
                        assert!(allowance > 0);
                        completed = true;
                    }
                    Err(error) => assert_eq!(error, Error::OutOfMemory),
                }
                drop(encoder);
                assert_eq!(mem.usage().0, before);
                assert_eq!(&*cache.index.order, &[0, 1, 2, 3]);
                assert_eq!(cache.entries.len(), 4);
                if completed {
                    break;
                }
            }
            assert!(completed);
        }
    }

    #[test]
    fn commit_encoding_preserves_retained_references_and_rolls_back() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let initial = Head {
            revision: 1,
            data_revision: 1,
            collections: Map::new(),
        };
        let empty = Retention {
            next_id: 1,
            entries: ChargedVec::new(),
        };
        let Candidate {
            end: start,
            cache,
            data_cache,
            ..
        } = encode(
            &mem,
            &initial,
            1,
            ID,
            APPEND_START as u64,
            None,
            None,
            &empty,
        )
        .unwrap();
        let assert_commit = |candidate: &Candidate, revision, words: &[u64]| {
            let AppendBytes::Contiguous(bytes) = &candidate.bytes else {
                unreachable!()
            };
            let object = &bytes[(candidate.descriptor - start) as usize..];
            let payload = words
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect::<Vec<_>>();
            assert_eq!(u32_at(object, 8), COMMIT);
            assert_eq!(u64_at(object, 16), payload.len() as u64);
            assert_eq!(u64_at(object, 24), revision);
            assert_eq!(
                &object[OBJECT_HEADER..OBJECT_HEADER + payload.len()],
                payload
            );
        };
        let metadata_only = Head {
            revision: 2,
            ..initial
        };
        let candidate = encode(
            &mem,
            &metadata_only,
            1,
            ID,
            start,
            Some(&cache),
            Some(&data_cache),
            &empty,
        )
        .unwrap();
        assert_eq!(candidate.descriptor, start);
        assert_commit(&candidate, 2, &[2, APPEND_START as u64, 1, 1, 0]);
        assert!(candidate.cache.ptr_eq(&cache));
        assert!(candidate.data_cache.ptr_eq(&data_cache));
        drop(candidate);

        let with_row = |revision, data_revision| Head {
            revision,
            data_revision,
            collections: Map::new()
                .set(
                    &mem,
                    &1u64.to_le_bytes(),
                    Collection {
                        base: None,
                        revision: data_revision,
                        records: Map::new()
                            .set(
                                &mem,
                                b"row",
                                Record {
                                    revision: data_revision,
                                    value: Value::scalar(&mem, Kind::Unsigned, data_revision)
                                        .unwrap(),
                                },
                            )
                            .unwrap(),
                    },
                )
                .unwrap(),
        };
        let current = with_row(9, 5);
        let mut entries = ChargedVec::new();
        mem.reserve(&mut entries, 2).unwrap();
        entries.push(Retained {
            id: 2,
            head: mem.shared(with_row(4, 3)).unwrap(),
            next_collection: 2,
        });
        entries.push(Retained {
            id: 7,
            head: mem
                .shared(Head {
                    revision: 7,
                    data_revision: current.data_revision,
                    collections: current.collections.clone(),
                })
                .unwrap(),
            next_collection: 2,
        });
        let retention = Retention {
            next_id: 9,
            entries,
        };
        let encode_candidate = || {
            encode(
                &mem,
                &current,
                2,
                ID,
                start,
                Some(&cache),
                Some(&data_cache),
                &retention,
            )
        };
        let before = mem.usage().0;
        for allowance in 0..64 {
            let result = allocator.with_refusal(allowance, encode_candidate);
            match result {
                Ok(candidate) => {
                    assert!(allowance > 0);
                    let current_data = start + REGION as u64;
                    let retained_data = start + 3 * REGION as u64;
                    assert_eq!(candidate.descriptor, start + 4 * REGION as u64);
                    assert_eq!(candidate.end, start + 5 * REGION as u64);
                    assert_commit(
                        &candidate,
                        9,
                        &[
                            9,
                            current_data,
                            5,
                            9,
                            2,
                            2,
                            4,
                            retained_data,
                            3,
                            7,
                            7,
                            current_data,
                            5,
                        ],
                    );
                    assert_eq!(candidate.cache.entries.len(), 2);
                    assert_eq!(candidate.data_cache.entries.len(), 3);
                    drop(candidate);
                    assert_eq!(mem.usage().0, before);
                    return;
                }
                Err(error) => assert_eq!(error, Error::OutOfMemory, "allocation {allowance}"),
            }
            assert_eq!(mem.usage().0, before);
            assert!(cache.entries.is_empty());
            assert_eq!(&*data_cache.index.order, &[0]);
            let [entry] = &*data_cache.entries else {
                unreachable!()
            };
            assert_eq!(
                (entry.revision, entry.next_collection, entry.offset),
                (1, 1, APPEND_START as u64)
            );
        }
        panic!("COMMIT did not encode after the bounded allocation sweep");
    }

    #[test]
    fn commit_fields_propagate_tight_budget_failures() {
        let limit = 32 * 1024;
        let mem = Memory::new(Limits {
            max_live_bytes: limit,
            ..Limits::default()
        })
        .unwrap();
        let empty = Retention {
            next_id: 1,
            entries: ChargedVec::new(),
        };
        let make_head = |revision| Head {
            revision,
            data_revision: 1,
            collections: Map::new(),
        };
        let Candidate {
            end: start,
            cache,
            data_cache,
            ..
        } = encode(
            &mem,
            &make_head(1),
            1,
            ID,
            APPEND_START as u64,
            None,
            None,
            &empty,
        )
        .unwrap();
        let mut entries = ChargedVec::new();
        mem.reserve(&mut entries, 2).unwrap();
        for revision in 1..=2 {
            entries.push(Retained {
                id: revision,
                head: mem.shared(make_head(revision)).unwrap(),
                next_collection: 1,
            });
        }
        let retention = Retention {
            next_id: 3,
            entries,
        };
        let encode_candidate = || {
            encode(
                &mem,
                &make_head(3),
                1,
                ID,
                start,
                Some(&cache),
                Some(&data_cache),
                &retention,
            )
        };
        let before = mem.usage().0;
        // Both roots hit the cache. These headrooms force refusal at each
        // field, including fields that normally fit in geometric slack.
        for (field, headroom) in [
            ("revision", 0),
            ("DATA reference", 8),
            ("DATA revision", 24),
            ("next snapshot ID", 40),
            ("retained count", 48),
            ("retained ID", 72),
            ("retained revision", 88),
            ("retained DATA revision", 96),
            ("second retained DATA reference", 168),
        ] {
            let mut pressure = ChargedVec::<u8>::new();
            mem.reserve(&mut pressure, limit - before - headroom)
                .unwrap();
            let charged = mem.usage().0;
            assert_eq!(encode_candidate().err(), Some(Error::Limit), "{field}");
            assert_eq!(mem.usage().0, charged, "{field}");
        }
        let candidate = encode_candidate().unwrap();
        assert!(candidate.cache.ptr_eq(&cache));
        assert!(candidate.data_cache.ptr_eq(&data_cache));
        drop(candidate);
        assert_eq!(mem.usage().0, before);
    }

    #[test]
    fn cache_indexes_admit_object_limit_and_reject_next_entry() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let mut values = ValueIndex::new();
        let mut data = DataIndex::new();
        for (entries, expected) in [
            (MAX_OBJECTS, Error::OutOfMemory),
            (MAX_OBJECTS + 1, Error::Limit),
        ] {
            assert_eq!(
                allocator.with_refusal(0, || values.reserve(&mem, &[], entries)),
                Err(expected)
            );
            assert_eq!(
                allocator.with_refusal(0, || data.reserve(&mem, entries)),
                Err(expected)
            );
            assert!(values.slots.is_empty() && data.order.is_empty());
            assert_eq!(mem.usage().0, 0);
        }
    }

    #[test]
    fn data_index_budget_and_object_bound_precede_allocation() {
        let slot = std::mem::size_of::<usize>();
        for cap in [slot - 1, slot] {
            let allocator = Arc::new(TestAllocator::default());
            let mem = allocator.memory(Limits {
                max_live_bytes: cap,
                ..Limits::default()
            });
            let mut index = DataIndex::from_cache(&mem, &[]).unwrap();
            assert_eq!(mem.public_usage().live_bytes, 0);
            assert_eq!(
                index.reserve(&mem, 1),
                if cap == slot {
                    Ok(())
                } else {
                    Err(Error::Limit)
                }
            );
            assert_eq!(
                allocator.with_refusal(0, || index.reserve(&mem, MAX_OBJECTS + 1)),
                Err(Error::Limit)
            );
            drop(index);
            assert_eq!(mem.public_usage().live_bytes, 0);
        }
        for cap in [3 * slot - 1, 3 * slot] {
            let mem = Memory::new(Limits {
                max_live_bytes: cap,
                ..Limits::default()
            })
            .unwrap();
            let mut index = DataIndex::new();
            index.reserve(&mem, 1).unwrap();
            index.order.push(0);
            assert_eq!(
                index.reserve(&mem, 2),
                if cap == 3 * slot {
                    Ok(())
                } else {
                    Err(Error::Limit)
                }
            );
            assert_eq!(&*index.order, &[0]);
            assert_eq!(
                mem.public_usage().live_bytes,
                if cap == 3 * slot { 2 * slot } else { slot }
            );
            drop(index);
            assert_eq!(mem.public_usage().live_bytes, 0);
        }
        let mem = Memory::new(Limits::default()).unwrap();
        let mut index = DataIndex::new();
        index.reserve(&mem, MAX_OBJECTS).unwrap();
        assert_eq!(mem.public_usage().live_bytes, MAX_OBJECTS * slot);
        assert_eq!(index.reserve(&mem, MAX_OBJECTS + 1), Err(Error::Limit));
    }
    #[test]
    fn serialization_data_golden_history() {
        // Capture these bytes with the linear DATA encoder before changing it.
        let output = std::env::var_os("WDB_DATA_IMAGES").map(std::path::PathBuf::from);
        if let Some(path) = &output {
            std::fs::create_dir(path).unwrap();
        }
        let mut captures = Vec::new();
        let mut capture = |disk: &SimulatedDisk| {
            let image = disk.durable_image().unwrap();
            if let Some(path) = &output {
                std::fs::write(path.join(format!("{}.wdb", captures.len())), &image).unwrap();
            }
            captures.push((image.len(), crc(&image)));
        };
        let disk = SimulatedDisk::new().unwrap();
        let mut db =
            Database::create_on(disk.clone(), *b"codec-data-00001", Limits::default()).unwrap();
        let collection = db.create_collection().unwrap().as_u64();
        let mut revisions = Vec::new();
        for epoch in 1..=64u64 {
            let mut tx = db.begin();
            tx.put(
                db.collection_id(collection),
                b"row",
                &db.unsigned(epoch).unwrap(),
            )
            .unwrap();
            let mut revision = tx.commit().unwrap();
            drop(tx);
            if epoch % 3 == 0 {
                // A metadata-only publication shares DATA despite a new head.
                revision = db.begin().commit().unwrap();
            }
            revisions.push(revision);
            assert_eq!(db.retain_snapshot(&db.snapshot()).unwrap(), epoch);
            if epoch % 8 == 0 {
                drop(db);
                db = Database::open_on(disk.clone(), Limits::default()).unwrap();
            }
            if epoch % 16 == 0 {
                capture(&disk);
            }
        }
        for (i, revision) in revisions.iter().enumerate() {
            let view = db.retained_snapshot(i as u64 + 1).unwrap();
            assert_eq!(view.revision(), *revision);
            assert_eq!(
                view.get(db.collection_id(collection), b"row")
                    .unwrap()
                    .unwrap()
                    .as_u64(),
                Ok(i as u64 + 1)
            );
        }
        for id in [1, 32, 64] {
            db.release_retained(id).unwrap();
        }
        capture(&disk);
        let backup = SimulatedDisk::new().unwrap();
        // Backups own a new verification budget, and encode current before
        // retained roots: sorting must not change that original wire order.
        drop(db);
        super::super::backup_on(disk.clone(), backup.clone(), Limits::default()).unwrap();
        capture(&backup);
        let mut db = Database::open_on(disk.clone(), Limits::default()).unwrap();
        let compact = SimulatedDisk::new().unwrap();
        db.compact_to(compact.clone()).unwrap();
        assert_eq!(
            backup.durable_image().unwrap(),
            compact.durable_image().unwrap()
        );
        let reopened = Database::open_on(compact.clone(), Limits::default()).unwrap();
        let mut tx = reopened.begin();
        tx.put(
            reopened.collection_id(collection),
            b"new",
            &reopened.unsigned(65).unwrap(),
        )
        .unwrap();
        tx.commit().unwrap();
        drop(tx);
        assert_eq!(reopened.retain_snapshot(&reopened.snapshot()).unwrap(), 65);
        for id in reopened.retained_ids().unwrap() {
            let view = reopened.retained_snapshot(id).unwrap();
            assert_eq!(
                view.get(reopened.collection_id(collection), b"row")
                    .unwrap()
                    .unwrap()
                    .as_u64(),
                Ok(if id == 65 { 64 } else { id })
            );
        }
        capture(&compact);
        assert_eq!(
            captures,
            [
                (319488, 18344531734244846753),
                (602112, 8454422879921928127),
                (888832, 15111149358676933575),
                (1171456, 12993989903033458364),
                (1183744, 12825919318030204998),
                (532480, 2049205749265809771),
                (548864, 5850840686714030802),
            ]
        );
    }
    #[test]
    fn value_index_collisions_wrap_growth_and_first_identity_match() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let mut values = Vec::new();
        let mut selected = Vec::new();
        // Keep all rejected allocations alive too: allocator address reuse must
        // not turn this directed collision control into repeated identities.
        for _ in 0..4096 {
            let value = Value::bytes_new(&mem, b"equal bytes").unwrap();
            if ValueIndex::bucket(value.node_identity(), 8) == 7 {
                selected.push(value.clone());
            }
            values.push(value);
            if selected.len() == 4 {
                break;
            }
        }
        assert_eq!(selected.len(), 4);
        let mut cache = Vec::new();
        let mut index = ValueIndex::new();
        assert_eq!(index.get(&selected[0], &cache), None);
        index.reserve(&mem, &cache, 4).unwrap();
        for value in &selected {
            assert_eq!(index.get(value, &cache), None);
            index.insert(value, &cache, cache.len());
            cache.push(CacheEntry {
                value: value.clone(),
                offset: cache.len() as u64,
                end: 0,
            });
        }
        // All four collide at the last bucket, wrapping through slots 0..2.
        assert_eq!(&*index.slots, &[2, 3, 4, 0, 0, 0, 0, 1]);
        for (i, value) in selected.iter().enumerate() {
            assert_eq!(index.get(&value.clone(), &cache), Some(i));
        }
        let distinct = Value::bytes_new(&mem, b"equal bytes").unwrap();
        assert_eq!(index.get(&distinct, &cache), None);
        let before = mem.public_usage().live_bytes;
        let slots = index.slots.to_vec();
        assert_eq!(
            allocator.with_refusal(0, || index.reserve(&mem, &cache, 4)),
            Ok(())
        );
        assert_eq!(
            allocator.with_refusal(0, || index.reserve(&mem, &cache, 5)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(&*index.slots, slots);
        assert_eq!(mem.public_usage().live_bytes, before);
        // Rebuild accepts arbitrary cache order and preserves first match.
        cache.reverse();
        cache.push(cache[0].clone());
        index.reserve(&mem, &cache, cache.len()).unwrap();
        for value in selected {
            assert_eq!(
                index.get(&value, &cache),
                cache.iter().position(|entry| entry.value.same_node(&value))
            );
        }
    }

    #[test]
    fn value_index_budget_is_charged_reclaimed_and_bounds_precede_allocation() {
        for cap in [63, 64] {
            let allocator = Arc::new(TestAllocator::default());
            let mem = allocator.memory(Limits {
                max_live_bytes: cap,
                ..Limits::default()
            });
            let mut index = ValueIndex::new();
            index.reserve(&mem, &[], 0).unwrap();
            assert_eq!(mem.public_usage().live_bytes, 0);
            assert_eq!(
                index.reserve(&mem, &[], 4),
                if cap == 64 { Ok(()) } else { Err(Error::Limit) }
            );
            assert_eq!(
                mem.public_usage().live_bytes,
                if cap == 64 { 64 } else { 0 }
            );
            assert_eq!(index.reserve(&mem, &[], 5), Err(Error::Limit));
            assert_eq!(
                allocator.with_refusal(0, || index.reserve(&mem, &[], MAX_OBJECTS + 1)),
                Err(Error::Limit)
            );
            drop(index);
            assert_eq!(mem.public_usage().live_bytes, 0);
        }
    }
    #[test]
    fn serialization_identity_golden_lifecycle() {
        // Fixed identity and operation order: captured with the original linear
        // encoder. An optional fresh output directory supports byte-for-byte
        // baseline/candidate comparison in addition to these fingerprints.
        let output = std::env::var_os("WDB_IDENTITY_IMAGES").map(std::path::PathBuf::from);
        if let Some(path) = &output {
            std::fs::create_dir(path).unwrap();
        }
        let mut captures = Vec::new();
        let mut capture = |disk: &SimulatedDisk| {
            let image = disk.durable_image().unwrap();
            if let Some(path) = &output {
                std::fs::write(path.join(format!("{}.wdb", captures.len())), &image).unwrap();
            }
            captures.push((image.len(), crc(&image)));
        };
        let disk = SimulatedDisk::new().unwrap();
        let mut db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
        let collection = db.create_collection().unwrap().as_u64();
        capture(&disk);
        for epoch in 0..3u8 {
            let c = db.collection_id(collection);
            let leaf = db.bytes(&[epoch, 0, 255]).unwrap();
            let shared = db
                .list()
                .unwrap()
                .list_push(&leaf)
                .unwrap()
                .list_push(&db.float(f64::from_bits(0x7ff8_0000_0000_0042)).unwrap())
                .unwrap();
            let mut tx = db.begin();
            for key in 0..12u8 {
                // Equal bytes with distinct identities must remain distinct;
                // shared descendants must reuse their original offsets.
                let row = db
                    .map()
                    .unwrap()
                    .map_set(b"shared", &shared)
                    .unwrap()
                    .map_set(b"distinct", &db.bytes(&[epoch, 0, 255]).unwrap())
                    .unwrap();
                tx.put(c, &[key], &row).unwrap();
            }
            tx.commit().unwrap();
            drop(tx);
            capture(&disk);
            db.retain_snapshot(&db.snapshot()).unwrap();
            capture(&disk);
            drop(db);
            db = Database::open_on(disk.clone(), Limits::default()).unwrap();
        }
        for id in 1..=3 {
            assert_eq!(
                db.retained_snapshot(id)
                    .unwrap()
                    .scan(db.collection_id(collection))
                    .unwrap()
                    .len(),
                12
            );
        }
        drop(db);
        let backup = SimulatedDisk::new().unwrap();
        super::super::backup_on(disk.clone(), backup.clone(), Limits::default()).unwrap();
        capture(&backup);
        let mut db = Database::open_on(disk.clone(), Limits::default()).unwrap();
        let compact = SimulatedDisk::new().unwrap();
        db.compact_to(compact.clone()).unwrap();
        capture(&compact);
        assert_eq!(
            backup.durable_image().unwrap(),
            compact.durable_image().unwrap()
        );
        let reopened = Database::open_on(compact.clone(), Limits::default()).unwrap();
        let mut tx = reopened.begin();
        let existing = reopened
            .retained_snapshot(1)
            .unwrap()
            .get(reopened.collection_id(collection), &[0])
            .unwrap()
            .unwrap();
        tx.put(reopened.collection_id(collection), b"reused", &existing)
            .unwrap();
        tx.commit().unwrap();
        capture(&compact);
        assert_eq!(
            captures,
            [
                (36864, 17874777462570792639),
                (155648, 12627800862391569603),
                (159744, 3171630150855272638),
                (278528, 2818319004123656995),
                (282624, 17840750070174291155),
                (401408, 390611552297599236),
                (405504, 12736892190795234075),
                (368640, 5816780006189258397),
                (368640, 5816780006189258397),
                (376832, 14514754463903583136),
            ]
        );
    }
    #[test]
    fn compaction_preserves_shared_value_identity_across_historical_roots() {
        let mut db =
            Database::create_on(SimulatedDisk::new().unwrap(), ID, Limits::default()).unwrap();
        let c = db.create_collection().unwrap();
        let child = db
            .list()
            .unwrap()
            .list_push(&db.bytes(b"shared").unwrap())
            .unwrap();
        let value = db.map().unwrap().map_set(b"child", &child).unwrap();
        let mut tx = db.begin();
        for key in [b"a", b"b"] {
            tx.put(c, key, &value).unwrap();
        }
        tx.commit().unwrap();
        drop(tx);
        db.retain_snapshot(&db.snapshot()).unwrap();
        let mut tx = db.begin();
        tx.put(
            c,
            b"a",
            &value.map_set(b"new", &db.none().unwrap()).unwrap(),
        )
        .unwrap();
        tx.commit().unwrap();
        drop(tx);
        let copy = SimulatedDisk::new().unwrap();
        db.compact_to(copy.clone()).unwrap();
        drop(db);
        let db = Database::open_on(copy, Limits::default()).unwrap();
        let c = db.collection_id(c.as_u64());
        let a = db
            .snapshot()
            .get(c, b"a")
            .unwrap()
            .unwrap()
            .map_get(b"child")
            .unwrap()
            .unwrap();
        let b = db
            .snapshot()
            .get(c, b"b")
            .unwrap()
            .unwrap()
            .map_get(b"child")
            .unwrap()
            .unwrap();
        let old = db
            .retained_snapshot(1)
            .unwrap()
            .get(c, b"a")
            .unwrap()
            .unwrap()
            .map_get(b"child")
            .unwrap()
            .unwrap();
        assert!(a.same_node(&b));
        assert!(a.same_node(&old));
        assert_eq!(
            child.list_get(0).unwrap().as_bytes(),
            Ok(b"shared".as_slice())
        );
    }
    fn fixture_store() -> (SimulatedDisk, Database) {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
        let c = db.create_collection().unwrap();
        let bytes = db.bytes(b"payload").unwrap();
        let list = db
            .list()
            .unwrap()
            .list_push(&bytes)
            .unwrap()
            .list_push(&db.unsigned(7).unwrap())
            .unwrap();
        let map = db
            .map()
            .unwrap()
            .map_set(b"a", &bytes)
            .unwrap()
            .map_set(b"b", &list)
            .unwrap();
        let mut tx = db.begin();
        tx.put(c, b"a", &map).unwrap();
        tx.put(c, b"b", &bytes).unwrap();
        tx.put(c, b"flag", &db.boolean(true).unwrap()).unwrap();
        tx.commit().unwrap();
        drop(tx);
        (disk, db)
    }
    fn fixture() -> Vec<u8> {
        let (disk, _db) = fixture_store();
        disk.durable_image().unwrap()
    }
    fn spans(image: &[u8]) -> Vec<(usize, usize, u32)> {
        let mut out = Vec::new();
        let mut offset = APPEND_START;
        while offset < image.len() {
            assert_eq!(&image[offset..offset + 8], b"WDBOBJ01");
            let size = align(OBJECT_HEADER as u64 + u64_at(image, offset + 16)).unwrap() as usize;
            out.push((offset, size, u32_at(image, offset + 8)));
            offset += size;
        }
        out
    }
    fn resign(image: &mut [u8], offset: usize, size: usize, checksum_offset: usize) {
        let crc = checksum(&image[offset..offset + size], checksum_offset);
        put64(image, offset + checksum_offset, crc);
    }
    fn rejected(image: &[u8]) -> Error {
        let disk = SimulatedDisk::from_image(image).unwrap();
        match Database::open_on(disk, Limits::default()) {
            Ok(_) => panic!("malformed image accepted"),
            Err(e) => e,
        }
    }

    fn check_metadata(
        image: &[u8],
        expected: Result<u64>,
        case: &str,
    ) -> Vec<crate::storage::IoEvent> {
        use crate::storage::IoKind;

        let disk = SimulatedDisk::from_image(image).unwrap();
        let actual =
            Database::open_on(disk.clone(), Limits::default()).map(|db| db.snapshot().revision());
        assert_eq!(actual, expected, "{case}");
        let trace = disk.trace().unwrap();
        if expected.is_err() {
            assert_eq!(disk.durable_image().unwrap(), image, "{case}");
            assert!(
                trace
                    .iter()
                    .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync)),
                "{case}"
            );
        }
        trace
    }

    #[test]
    fn recovery_checks_image_length_before_reading_metadata() {
        struct Length(u64);
        impl ByteStore for Length {
            fn lock(&mut self) -> Result<()> {
                Ok(())
            }
            fn unlock(&mut self) {}
            fn length(&mut self) -> Result<u64> {
                Ok(self.0)
            }
            fn read(&mut self, _: u64, _: &mut [u8]) -> Result<usize> {
                panic!("length must be checked before reading")
            }
            fn write(&mut self, _: u64, _: &[u8]) -> Result<usize> {
                unreachable!()
            }
            fn sync(&mut self) -> Result<()> {
                unreachable!()
            }
        }
        for length in [
            APPEND_START as u64 - 1,
            FileFormat::Extended(super::super::FileCapacity::Unbounded).capacity() + 1,
        ] {
            assert!(
                matches!(
                    Database::open_on(Length(length), Limits::default()),
                    Err(Error::Corrupt)
                ),
                "{length}"
            );
        }
        check_metadata(&fixture(), Ok(2), "supported length");
    }

    #[test]
    fn recovery_rejects_each_invalid_header_field_before_reading_slots() {
        let original = fixture();
        check_metadata(&original, Ok(2), "valid header");
        for (case, offset, repair, expected) in [
            ("header magic", 0, true, Error::UnsupportedFormat),
            ("header version", 8, true, Error::UnsupportedFormat),
            ("header checksum", 64, false, Error::Corrupt),
            ("header reserved word", 12, true, Error::Corrupt),
            ("header region size", 16, true, Error::Corrupt),
            ("header append start", 24, true, Error::Corrupt),
            ("header image limit", 48, true, Error::Corrupt),
            ("header object limit", 56, true, Error::Corrupt),
            ("header padding", 72, true, Error::Corrupt),
            ("zero file identity", 32, true, Error::Corrupt),
        ] {
            let mut image = original.clone();
            if offset == 8 {
                put32(&mut image, 8, EXTENDED_VERSION + 1);
            } else if offset == 32 {
                image[32..48].fill(0);
            } else {
                image[offset] ^= 1;
            }
            if repair {
                resign(&mut image, 0, REGION, 64);
            }
            let trace = check_metadata(&image, Err(expected), case);
            assert_eq!(
                trace.last().unwrap().offset,
                0,
                "{case} must fail in the header"
            );
        }
    }

    #[test]
    fn absent_or_torn_metadata_obeys_the_surviving_witness() {
        let original = fixture();
        for (case, torn) in [("zeroed region", false), ("bad checksum", true)] {
            let clear = |image: &mut [u8], region| {
                if torn {
                    image[region * REGION + 64] ^= 1;
                } else {
                    image[region * REGION..(region + 1) * REGION].fill(0);
                }
            };
            let mut image = original.clone();
            clear(&mut image, 3);
            check_metadata(&image, Ok(2), case); // Intact newest root.
            clear(&mut image, 1);
            check_metadata(&image, Ok(1), case); // Neither newest record validates.
            image[3 * REGION..4 * REGION].copy_from_slice(&original[3 * REGION..4 * REGION]);
            check_metadata(&image, Err(Error::Corrupt), case); // Newest witness still proves revision 2.
        }
    }

    #[test]
    fn checksummed_slot_and_witness_metadata_must_have_valid_fields() {
        enum Change {
            Byte(usize),
            Parity,
            EndPastLength,
            UnalignedEnd,
            BeforeAppend,
            UnalignedDescriptor,
            DescriptorEnd,
        }
        use Change::*;

        let original = fixture();
        for region in [1, 3] {
            let offset = region * REGION;
            check_metadata(&original, Ok(2), "valid metadata");
            for (case, change) in [
                ("metadata magic", Byte(0)),
                ("metadata version", Byte(8)),
                ("metadata index", Byte(12)),
                ("metadata identity", Byte(16)),
                ("metadata padding", Byte(72)),
                ("revision parity", Parity),
                ("predecessor", Byte(56)),
                ("end past physical length", EndPastLength),
                ("unaligned end", UnalignedEnd),
                ("descriptor before append start", BeforeAppend),
                ("unaligned descriptor", UnalignedDescriptor),
                ("descriptor/end relation", DescriptorEnd),
            ] {
                let mut image = original.clone();
                if region == 1 {
                    // A structurally invalid newest slot must reject even
                    // when no newest witness would prevent falling back.
                    image[3 * REGION..4 * REGION].fill(0);
                }
                let bytes = &mut image[offset..offset + REGION];
                let descriptor = u64_at(bytes, 40);
                let end = u64_at(bytes, 48);
                match change {
                    Parity => {
                        put64(bytes, 32, 3);
                        put64(bytes, 56, 2);
                    }
                    EndPastLength => {
                        put64(bytes, 40, end);
                        put64(bytes, 48, end + REGION as u64);
                    }
                    UnalignedEnd => put64(bytes, 48, end - 1),
                    BeforeAppend => {
                        put64(bytes, 40, (APPEND_START - REGION) as u64);
                        put64(bytes, 48, APPEND_START as u64);
                    }
                    UnalignedDescriptor => put64(bytes, 40, descriptor + 1),
                    DescriptorEnd => put64(bytes, 48, end - REGION as u64),
                    Byte(field) => bytes[field] ^= 1,
                }
                resign(&mut image, offset, REGION, 64);
                let trace = check_metadata(&image, Err(Error::Corrupt), case);
                assert_eq!(
                    trace.last().unwrap().offset,
                    offset as u64,
                    "{case} must fail in its metadata region"
                );
            }
        }
    }

    fn check_newest_graph(
        image: &[u8],
        newest: u64,
        expected: Result<u64>,
        case: &str,
    ) -> Vec<crate::storage::IoEvent> {
        use crate::storage::IoKind;

        let mut image = image.to_vec();
        let index = (newest & 1) as usize;
        // A newer witness alone would rule out fallback, masking the graph check.
        image[(3 + index) * REGION..(4 + index) * REGION].fill(0);
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let reopened =
            Database::open_on(disk.clone(), Limits::default()).map(|db| db.snapshot().revision());
        assert_eq!(reopened, expected, "{case}");
        let trace = disk.trace().unwrap();
        if expected.is_err() {
            assert_eq!(disk.durable_image().unwrap(), image, "{case}");
            assert!(
                trace
                    .iter()
                    .all(|event| !matches!(event.kind, IoKind::Write | IoKind::Sync)),
                "{case}"
            );
            // The same damaged object bytes still allow the older root to
            // open when the newest slot itself is absent.
            image[(1 + index) * REGION..(2 + index) * REGION].fill(0);
            let older = SimulatedDisk::from_image(&image).unwrap();
            let db = Database::open_on(older, Limits::default()).unwrap();
            assert_eq!(db.snapshot().revision(), newest - 1, "{case}");
        }
        trace
    }

    #[test]
    fn selected_graph_errors_do_not_reopen_the_older_root() {
        use crate::storage::IoKind;

        let (disk, db) = fixture_store();
        let old = db.snapshot();
        assert_eq!(old.revision(), 2);
        let original = disk.durable_image().unwrap();
        let collection = db.collection_id(1);
        let mut edit = db.begin();
        edit.put(collection, b"flag", &db.boolean(false).unwrap())
            .unwrap();
        assert_eq!(edit.commit(), Ok(3));
        let current = disk.durable_image().unwrap();
        let root = current.len() - REGION;
        let data = u64_at(&current, root + OBJECT_HEADER + 8) as usize;
        let (value, _, _) = spans(&current)
            .into_iter()
            .find(|&(offset, _, kind)| offset >= original.len() && kind == VALUE)
            .unwrap();
        check_newest_graph(&current, 3, Ok(3), "valid current graph");

        for (case, object, field, replacement, before_data) in [
            ("root born revision", root, 24, 2, true),
            ("root payload revision", root, OBJECT_HEADER, 4, true),
            ("future data revision", root, OBJECT_HEADER + 16, 4, true),
            ("data revision mismatch", root, OBJECT_HEADER + 16, 2, false),
            ("data born revision", data, 24, 2, false),
            // DATA header is three u64s, followed by the collection header
            // (three u64s), key length, key b"a", then the write revision.
            ("zero write revision", data, OBJECT_HEADER + 57, 0, false),
            ("future write revision", data, OBJECT_HEADER + 57, 4, false),
            (
                "forward value reference",
                data,
                OBJECT_HEADER + 65,
                data as u64,
                false,
            ),
        ] {
            let mut image = current.clone();
            put64(&mut image, object + field, replacement);
            resign(&mut image, object, REGION, 48);
            let trace = check_newest_graph(&image, 3, Err(Error::Corrupt), case);
            if before_data {
                assert!(
                    trace
                        .iter()
                        .all(|event| event.kind != IoKind::Read || event.offset != data as u64),
                    "{case} must fail at the descriptor before reading DATA"
                );
            }
        }

        for (case, field, repair_crc) in [
            ("value magic", 0, true),
            ("value kind", 8, true),
            ("value version", 12, true),
            ("value identity", 32, true),
            ("value reserved bytes", 56, true),
            ("value checksum", OBJECT_HEADER + 8, false),
            ("value padding", REGION - 1, true),
        ] {
            let mut image = current.clone();
            image[value + field] ^= 1;
            if repair_crc {
                resign(&mut image, value, REGION, 48);
            }
            check_newest_graph(&image, 3, Err(Error::Corrupt), case);
        }

        // This value belongs only to the snapshot retained by revision 4.
        // Revision 3 uses a different value and does not retain revision 2.
        assert_eq!(db.retain_snapshot(&old), Ok(1));
        let mut retained = disk.durable_image().unwrap();
        check_newest_graph(&retained, 4, Ok(4), "valid retained graph");
        let (old_flag, size, _) = spans(&original)
            .into_iter()
            .find(|&(offset, _, kind)| {
                kind == VALUE && u64_at(&original, offset + OBJECT_HEADER) == 1
            })
            .unwrap();
        put64(&mut retained, old_flag + OBJECT_HEADER + 8, 2);
        resign(&mut retained, old_flag, size, 48);
        check_newest_graph(&retained, 4, Err(Error::Corrupt), "retained-only value");
    }

    #[test]
    fn graph_traversal_checks_offsets_lengths_order_and_payload_shape() {
        use crate::storage::IoKind;

        let (disk, db) = fixture_store();
        let mut edit = db.begin();
        let list = db
            .list()
            .unwrap()
            .list_push(&db.unsigned(8).unwrap())
            .unwrap();
        let map = db
            .map()
            .unwrap()
            .map_set(b"a", &list)
            .unwrap()
            .map_set(b"b", &db.integer(9).unwrap())
            .unwrap();
        edit.put(db.collection_id(1), b"a", &map).unwrap();
        assert_eq!(edit.commit(), Ok(3));
        let clean = disk.durable_image().unwrap();
        let descriptor = clean.len() - REGION;
        let data = u64_at(&clean, descriptor + OBJECT_HEADER + 8) as usize;
        let first_key = data + OBJECT_HEADER + 56;
        let first_value_ref = first_key + 9;
        let map = u64_at(&clean, first_value_ref) as usize;
        let list = u64_at(&clean, map + OBJECT_HEADER + 25) as usize;
        let leaf = u64_at(&clean, list + OBJECT_HEADER + 16) as usize;
        check_newest_graph(&clean, 3, Ok(3), "valid traversal");

        for (case, target, field, replacement, expected_reads) in [
            (
                "below append",
                data,
                first_value_ref,
                (APPEND_START - REGION) as u64,
                1,
            ),
            ("unaligned", data, first_value_ref, (map + 1) as u64, 0),
            ("forward child", data, first_value_ref, data as u64, 2),
            ("overflow object length", map, map + 16, u64::MAX, 1),
            (
                "overflow object alignment",
                map,
                map + 16,
                u64::MAX - OBJECT_HEADER as u64,
                1,
            ),
            (
                "aligned object exceeds image cap",
                map,
                map + 16,
                MAX_IMAGE_BYTES as u64,
                1,
            ),
            (
                "object crosses parent",
                map,
                map + 16,
                (data - map) as u64,
                1,
            ),
            ("future value born revision", map, map + 24, 4, 1),
            ("unknown tag", map, map + OBJECT_HEADER, 8, 2),
        ] {
            let mut image = clean.clone();
            let read_offset = if target == data {
                replacement
            } else {
                map as u64
            };
            put64(&mut image, field, replacement);
            resign(&mut image, target, REGION, 48);
            let trace = check_newest_graph(&image, 3, Err(Error::Corrupt), case);
            assert_eq!(
                trace
                    .iter()
                    .filter(|e| e.kind == IoKind::Read && e.offset == read_offset)
                    .count(),
                expected_reads,
                "{case}"
            );
        }

        for (case, object, field, replacement) in [
            ("duplicate record key", data, first_key, b'b'),
            ("duplicate map key", map, map + OBJECT_HEADER + 24, b'b'),
        ] {
            let mut image = clean.clone();
            image[field] = replacement;
            resign(&mut image, object, REGION, 48);
            check_newest_graph(&image, 3, Err(Error::Corrupt), case);
        }
        let mut image = clean.clone();
        put64(&mut image, leaf + 16, 15);
        resign(&mut image, leaf, REGION, 48);
        check_newest_graph(&image, 3, Err(Error::Corrupt), "truncated scalar payload");
        put64(&mut image, leaf + 16, 17);
        resign(&mut image, leaf, REGION, 48);
        check_newest_graph(&image, 3, Err(Error::Corrupt), "trailing scalar payload");

        // The same graph fits depth 3. With depth 2 the leaf is never read.
        for depth in [3, 2] {
            let disk = SimulatedDisk::from_image(&clean).unwrap();
            let limits = Limits {
                max_depth: depth,
                ..Limits::default()
            };
            let result = Database::open_on(disk.clone(), limits).map(|db| db.snapshot().revision());
            let trace = disk.trace().unwrap();
            let read_leaf = trace
                .iter()
                .any(|e| e.kind == IoKind::Read && e.offset == leaf as u64);
            if depth == 3 {
                assert_eq!(result, Ok(3));
                assert!(read_leaf);
            } else {
                assert_eq!(result, Err(Error::Corrupt));
                assert!(!read_leaf);
                assert!(
                    trace
                        .iter()
                        .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
                );
                assert_eq!(disk.durable_image().unwrap(), clean);
            }
        }
    }

    #[test]
    fn graph_rejects_overlapping_objects_even_when_both_are_checksummed() {
        use crate::storage::IoKind;

        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
        let collection = db.create_collection().unwrap();
        let mut edit = db.begin();
        edit.put(collection, b"a", &db.bytes(&vec![0; 2 * REGION]).unwrap())
            .unwrap();
        edit.put(collection, b"b", &db.integer(7).unwrap()).unwrap();
        assert_eq!(edit.commit(), Ok(2));
        let mut image = disk.durable_image().unwrap();
        check_newest_graph(&image, 2, Ok(2), "adjacent values");
        let descriptor = image.len() - REGION;
        let data = u64_at(&image, descriptor + OBJECT_HEADER + 8) as usize;
        let a = u64_at(&image, data + OBJECT_HEADER + 65) as usize;
        let b_ref = data + OBJECT_HEADER + 90;
        let b = u64_at(&image, b_ref) as usize;
        let embedded = a + REGION;
        image.copy_within(b..b + REGION, embedded);
        resign(&mut image, a, 3 * REGION, 48);
        check_newest_graph(&image, 2, Ok(2), "unreferenced bytes inside a value");
        put64(&mut image, b_ref, embedded as u64);
        resign(&mut image, data, REGION, 48);
        let trace = check_newest_graph(
            &image,
            2,
            Err(Error::Corrupt),
            "reachable overlapping value",
        );
        assert_eq!(trace.last().unwrap().offset, embedded as u64);
        assert_eq!(trace.last().unwrap().len, OBJECT_HEADER);
        assert_eq!(trace.last().unwrap().kind, IoKind::Read);

        // Decode the embedded scalar first, so the overlapping region now
        // contains a cached start marker rather than an occupied interior.
        put64(&mut image, data + OBJECT_HEADER + 65, embedded as u64);
        put64(&mut image, b_ref, a as u64);
        resign(&mut image, data, REGION, 48);
        let trace = check_newest_graph(&image, 2, Err(Error::Corrupt), "overlap with cached start");
        assert_eq!(trace.last().unwrap().offset, a as u64);
        assert_eq!(trace.last().unwrap().len, OBJECT_HEADER);
    }

    #[test]
    fn unaligned_reference_cannot_reuse_a_cached_value_region() {
        let mut image = fixture();
        check_newest_graph(&image, 2, Ok(2), "shared value at its aligned start");
        let descriptor = image.len() - REGION;
        let data = u64_at(&image, descriptor + OBJECT_HEADER + 8) as usize;
        // Record a's map has already decoded the bytes shared by record b.
        let reference = data + OBJECT_HEADER + 90;
        let shared = u64_at(&image, reference);
        put64(&mut image, reference, shared + 1);
        resign(&mut image, data, REGION, 48);
        let trace =
            check_newest_graph(&image, 2, Err(Error::Corrupt), "unaligned cached reference");
        assert!(trace.iter().all(|event| event.offset != shared + 1));
    }

    #[test]
    fn data_cache_accepts_sharing_and_rejects_inconsistent_aliases() {
        use crate::storage::IoKind;

        let (disk, db) = fixture_store();
        let older = disk.durable_image().unwrap();
        let old_root = older.len() - REGION;
        let spare = u64_at(&older, old_root + OBJECT_HEADER + 8) as usize;
        let mut edit = db.begin();
        edit.put(db.collection_id(1), b"flag", &db.boolean(false).unwrap())
            .unwrap();
        assert_eq!(edit.commit(), Ok(3));
        assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(1));
        let clean = disk.durable_image().unwrap();
        let descriptor = clean.len() - REGION;
        let payload = descriptor + OBJECT_HEADER;
        let data = u64_at(&clean, payload + 8) as usize;
        let reads = |trace: &[crate::storage::IoEvent], offset| {
            trace
                .iter()
                .filter(|e| e.kind == IoKind::Read && e.offset == offset as u64)
                .count()
        };
        let trace = check_newest_graph(&clean, 4, Ok(4), "same offset and revision");
        assert_eq!(reads(&trace, data), 2, "one header and one full read");

        let mut image = clean.clone();
        put64(&mut image, payload + 64, 2);
        resign(&mut image, descriptor, REGION, 48);
        let trace = check_newest_graph(
            &image,
            4,
            Err(Error::Corrupt),
            "same offset, different revision",
        );
        assert_eq!(reads(&trace, data), 2);

        // The current DATA references newer values than the spare can hold.
        // Use a duplicate of the current directory with no collections so
        // the copy is a valid DATA object on its own at the earlier offset.
        image = clean;
        image.copy_within(data..data + REGION, spare);
        image[spare + OBJECT_HEADER + 16..spare + REGION].fill(0);
        put64(&mut image, spare + 16, 24);
        resign(&mut image, spare, REGION, 48);
        put64(&mut image, payload + 56, spare as u64);
        resign(&mut image, descriptor, REGION, 48);
        let trace = check_newest_graph(
            &image,
            4,
            Err(Error::Corrupt),
            "different offset, same revision",
        );
        assert_eq!(reads(&trace, spare), 0);
        put64(&mut image, spare + 24, 2);
        put64(&mut image, spare + OBJECT_HEADER, 2);
        resign(&mut image, spare, REGION, 48);
        put64(&mut image, payload + 64, 2);
        resign(&mut image, descriptor, REGION, 48);
        let trace = check_newest_graph(&image, 4, Ok(4), "different offset and revision");
        assert_eq!(reads(&trace, spare), 2);

        put64(&mut image, spare + OBJECT_HEADER + 8, 3);
        resign(&mut image, spare, REGION, 48);
        let trace = check_newest_graph(
            &image,
            4,
            Err(Error::Corrupt),
            "retained next ID exceeds current",
        );
        assert_eq!(reads(&trace, spare), 2);
    }

    #[test]
    fn graph_rejects_a_changed_header_on_full_object_read() {
        struct Unstable {
            disk: SimulatedDisk,
            offset: u64,
        }
        impl ByteStore for Unstable {
            fn lock(&mut self) -> Result<()> {
                self.disk.lock()
            }
            fn unlock(&mut self) {
                self.disk.unlock();
            }
            fn length(&mut self) -> Result<u64> {
                self.disk.length()
            }
            fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
                let n = self.disk.read(offset, out)?;
                if offset == self.offset && n == REGION {
                    // Change only the born revision to another valid revision.
                    put64(out, 24, 1);
                    resign(out, 0, REGION, 48);
                }
                Ok(n)
            }
            fn write(&mut self, _: u64, _: &[u8]) -> Result<usize> {
                panic!("unstable graph reached recovery write")
            }
            fn sync(&mut self) -> Result<()> {
                panic!("unstable graph reached recovery sync")
            }
        }

        let mut image = fixture();
        let (offset, _, _) = spans(&image)
            .into_iter()
            .find(|&(_, _, kind)| kind == VALUE)
            .unwrap();
        assert_eq!(u64_at(&image, offset + 24), 2);
        image[3 * REGION..4 * REGION].fill(0);
        check_newest_graph(&image, 2, Ok(2), "stable value header");
        let disk = SimulatedDisk::from_image(&image).unwrap();
        let unstable = Unstable {
            disk: disk.clone(),
            offset: offset as u64,
        };
        assert!(matches!(
            Database::open_on(unstable, Limits::default()),
            Err(Error::Corrupt)
        ));
        assert_eq!(disk.durable_image().unwrap(), image);
        image[REGION..2 * REGION].fill(0);
        check_newest_graph(&image, 1, Ok(1), "older root skips unstable value");
    }

    #[test]
    fn empty_data_graph_still_requires_a_next_collection_id() {
        let (disk, db) = fixture_store();
        db.drop_collection(db.collection_id(1)).unwrap();
        let mut image = disk.durable_image().unwrap();
        check_newest_graph(&image, 3, Ok(3), "valid empty data directory");
        let root = image.len() - REGION;
        let data = u64_at(&image, root + OBJECT_HEADER + 8) as usize;
        put64(&mut image, data + OBJECT_HEADER + 8, 0);
        resign(&mut image, data, REGION, 48);
        check_newest_graph(&image, 3, Err(Error::Corrupt), "zero next collection ID");
    }

    #[test]
    fn collection_directory_rejects_each_invalid_id_or_revision() {
        let (disk, db) = fixture_store();
        db.create_collection().unwrap();
        let mut edit = db.begin();
        edit.put(db.collection_id(1), b"flag", &db.boolean(false).unwrap())
            .unwrap();
        assert_eq!(edit.commit(), Ok(4));
        let image = disk.durable_image().unwrap();
        let root = image.len() - REGION;
        let data = u64_at(&image, root + OBJECT_HEADER + 8) as usize;
        check_newest_graph(&image, 4, Ok(4), "valid collection directory");

        let payload = data + OBJECT_HEADER;
        let mut fields = Cursor::new(&image[payload..]);
        // Skip the DATA header and first collection's ID and revision.
        fields.take(5 * 8).unwrap();
        let records = fields.int().unwrap();
        for _ in 0..records {
            fields.key(REGION).unwrap();
            fields.take(2 * 8).unwrap();
        }
        let second = payload + fields.pos;
        assert_eq!(u64_at(&image, second), 2);
        assert_eq!(u64_at(&image, second + 16), 0);
        for (case, field, replacement) in [
            ("duplicate collection ID", 0, 1),
            ("collection ID reaches next ID", 0, 3),
            ("zero collection revision", 8, 0),
            ("future collection revision", 8, 5),
        ] {
            let mut damaged = image.clone();
            put64(&mut damaged, second + field, replacement);
            resign(&mut damaged, data, REGION, 48);
            check_newest_graph(&damaged, 4, Err(Error::Corrupt), case);
        }
    }

    #[test]
    fn recovery_requires_witnesses_to_agree_with_the_selected_root() {
        let (disk, db) = fixture_store();
        let original = disk.durable_image().unwrap();
        let revision = db.snapshot().revision();
        assert_eq!(db.begin().commit(), Ok(revision + 1));
        drop(db);
        let mut image = disk.durable_image().unwrap();

        let matching_offset = (3 + (revision & 1) as usize) * REGION;
        let other_offset = (3 + ((revision + 1) & 1) as usize) * REGION;
        let matching = &original[matching_offset..matching_offset + REGION];
        let older = &original[other_offset..other_offset + REGION];
        let newer = image[other_offset..other_offset + REGION].to_vec();
        // Keep the witness revision and parity. Point it at the next commit's
        // descriptor and end, then repair its checksum so the parser accepts it.
        let mut mismatched = matching.to_vec();
        mismatched[40..56].copy_from_slice(&newer[40..56]);
        resign(&mut mismatched, 0, REGION, 64);

        // Keep both original roots and all appended objects in every case.
        // Each copy presents exactly one valid witness to the same root pair.
        image[REGION..3 * REGION].copy_from_slice(&original[REGION..3 * REGION]);
        image[3 * REGION..5 * REGION].fill(0);
        for (case, offset, witness, expected) in [
            ("older", other_offset, older, Ok(revision)),
            ("newer", other_offset, newer.as_slice(), Err(Error::Corrupt)),
            ("matching", matching_offset, matching, Ok(revision)),
            (
                "same revision, different descriptor and end",
                matching_offset,
                mismatched.as_slice(),
                Err(Error::Corrupt),
            ),
        ] {
            let mut candidate = image.clone();
            candidate[offset..offset + REGION].copy_from_slice(witness);
            let disk = SimulatedDisk::from_image(&candidate).unwrap();
            let reopened = Database::open_on(disk.clone(), Limits::default())
                .map(|db| db.snapshot().revision());
            assert_eq!(reopened, expected, "{case}");
            if expected.is_err() {
                assert_eq!(disk.durable_image().unwrap(), candidate, "{case}");
            }
        }
    }

    #[test]
    fn checksum_known_vector_and_padding() {
        assert_eq!(crc(b"123456789"), 0x6c40df5f0b497347);
        let mut h = header([1; 16]);
        assert_eq!(checksum(&h, 64), u64_at(&h, 64));
        h[REGION - 1] = 1;
        assert_ne!(checksum(&h, 64), u64_at(&h, 64));
    }

    #[test]
    fn crc_words_and_incremental_tails_match_the_bit_recurrence() {
        fn reference(mut crc: u64, bytes: &[u8]) -> u64 {
            for &byte in bytes {
                for shift in (0..8).rev() {
                    let high_bit = (crc >> 63) ^ u64::from((byte >> shift) & 1);
                    crc <<= 1;
                    if high_bit != 0 {
                        crc ^= 0x42f0e1eba9ea3693;
                    }
                }
            }
            crc
        }

        for byte in 0..=u8::MAX {
            assert_eq!(crc(&[byte]), reference(0, &[byte]));
            for position in 0..8 {
                let mut word = [0; 8];
                word[position] = byte;
                assert_eq!(crc(&word), reference(0, &word));
            }
        }

        let data: [u8; 263] = std::array::from_fn(|i| (i as u8).wrapping_mul(197));
        for initial in [0, u64::MAX, 0x0123456789abcdef] {
            for offset in 0..8 {
                let bytes = &data[offset..];
                let expected = reference(initial, bytes);
                assert_eq!(update_crc(initial, bytes), expected);
                for split in 0..=bytes.len() {
                    let (prefix, suffix) = bytes.split_at(split);
                    assert_eq!(update_crc(update_crc(initial, prefix), suffix), expected);
                }
            }
        }
    }

    #[test]
    fn checksummed_but_malformed_payloads_are_checked_before_use() {
        let original = fixture();
        let objects = spans(&original);
        for &(offset, size, kind) in &objects {
            if kind != VALUE {
                continue;
            }
            let tag = u64_at(&original, offset + OBJECT_HEADER);
            let mut image = original.clone();
            put64(&mut image, offset + OBJECT_HEADER, u64::MAX);
            resign(&mut image, offset, size, 48);
            assert_eq!(rejected(&image), Error::Corrupt);
            let mut image = original.clone();
            put64(&mut image, offset + 24, u64::MAX);
            resign(&mut image, offset, size, 48);
            assert_eq!(rejected(&image), Error::Corrupt);
            if tag == 1 {
                let mut image = original.clone();
                put64(&mut image, offset + OBJECT_HEADER + 8, 2);
                resign(&mut image, offset, size, 48);
                assert_eq!(rejected(&image), Error::Corrupt);
            }
            if tag == 6 || tag == 7 {
                let mut image = original.clone();
                put64(&mut image, offset + OBJECT_HEADER + 8, u64::MAX);
                resign(&mut image, offset, size, 48);
                assert_eq!(rejected(&image), Error::Limit);
                let reference = offset + OBJECT_HEADER + if tag == 6 { 25 } else { 16 };
                let mut image = original.clone();
                put64(&mut image, reference, offset as u64);
                resign(&mut image, offset, size, 48);
                assert_eq!(rejected(&image), Error::Corrupt);
            }
            if tag == 6 {
                let mut image = original.clone();
                image[offset + OBJECT_HEADER + 41] = b'a';
                resign(&mut image, offset, size, 48);
                assert_eq!(rejected(&image), Error::Corrupt);
            }
            let mut image = original.clone();
            image[offset + size - 1] = 1;
            resign(&mut image, offset, size, 48);
            assert_eq!(rejected(&image), Error::Corrupt);
        }
        let &(data, size, _) = objects
            .iter()
            .rev()
            .find(|(_, _, kind)| *kind == DATA)
            .unwrap();
        for field in [OBJECT_HEADER + 8, OBJECT_HEADER + 32] {
            let mut image = original.clone();
            put64(
                &mut image,
                data + field,
                if field == OBJECT_HEADER + 8 {
                    0
                } else {
                    u64::MAX
                },
            );
            resign(&mut image, data, size, 48);
            assert_eq!(rejected(&image), Error::Corrupt);
        }
        let &(root, size, _) = objects.last().unwrap();
        let mut image = original.clone();
        let tail = image.len();
        image.extend_from_slice(&original[APPEND_START..APPEND_START + REGION]);
        put64(&mut image, root + OBJECT_HEADER + 8, tail as u64);
        resign(&mut image, root, size, 48);
        assert_eq!(
            rejected(&image),
            Error::Corrupt,
            "reference outside committed boundary"
        );
        let mut image = original.clone();
        put64(&mut image, root + OBJECT_HEADER + 24, 0);
        resign(&mut image, root, size, 48);
        assert_eq!(rejected(&image), Error::Corrupt);
        let mut image = original.clone();
        put32(&mut image, 8, EXTENDED_VERSION + 1);
        resign(&mut image, 0, REGION, 64);
        assert_eq!(rejected(&image), Error::UnsupportedFormat);
        let mut image = original.clone();
        image[root + 32] ^= 1;
        resign(&mut image, root, size, 48);
        assert_eq!(
            rejected(&image),
            Error::Corrupt,
            "object identity is bound into its checksum"
        );
    }

    #[test]
    fn deterministic_byte_fuzz_and_truncation_never_panic() {
        let original = fixture();
        let objects = spans(&original);
        let mut rng = 0x61c8864680b583ebu64;
        for seed in 0..2400 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let offset = rng as usize % original.len();
            let mut image = original.clone();
            let count = 1 + (rng as usize % 8);
            let end = (offset + count).min(image.len());
            for (i, byte) in image[offset..end].iter_mut().enumerate() {
                *byte ^= ((rng >> (i * 8)) as u8) | 1;
            }
            // Half of mutations have a repaired checksum to exercise structural
            // parsing rather than only the outer damage detector.
            if seed % 2 == 0 {
                if offset < APPEND_START {
                    resign(&mut image, offset / REGION * REGION, REGION, 64);
                } else if let Some(&(start, size, _)) = objects
                    .iter()
                    .find(|(start, size, _)| offset >= *start && offset < start + size)
                {
                    resign(&mut image, start, size, 48);
                }
            }
            let disk = SimulatedDisk::from_image(&image).unwrap();
            match Database::open_on(disk, Limits::default()) {
                Ok(db) => {
                    let _ = db.snapshot().scan(db.collection_id(1));
                }
                Err(e) => assert!(
                    matches!(e, Error::Corrupt | Error::UnsupportedFormat | Error::Limit),
                    "seed={seed}, offset={offset}, error={e:?}"
                ),
            }
        }
        for cut in (0..original.len()).step_by(257) {
            assert!(
                matches!(
                    rejected(&original[..cut]),
                    Error::Corrupt | Error::UnsupportedFormat
                ),
                "cut={cut}"
            );
        }
    }

    #[test]
    fn recovery_preserves_shared_value_identity_and_fails_allocation_safely() {
        let image = fixture();
        let mut succeeded = false;
        for budget in 0..512 {
            let mut disk = SimulatedDisk::from_image(&image).unwrap();
            disk.lock().unwrap();
            let allocator = Arc::new(TestAllocator::default());
            let mem = allocator.memory(Limits::default());
            let result = allocator.with_refusal(budget, || recover(&mut disk, &mem));
            disk.unlock();
            assert_eq!(disk.durable_image().unwrap(), image);
            match result {
                Err(e) => assert_eq!(e, Error::OutOfMemory, "budget={budget}"),
                Ok(r) => {
                    let c = r.head.collections.get(&1u64.to_le_bytes()).unwrap();
                    let direct = &c.records.get(b"b").unwrap().value;
                    let through_map = c
                        .records
                        .get(b"a")
                        .unwrap()
                        .value
                        .map_get(b"a")
                        .unwrap()
                        .unwrap();
                    assert!(direct.same_node(&through_map));
                    succeeded = true;
                    break;
                }
            }
        }
        assert!(succeeded);
    }
    #[test]
    fn retained_catalog_bounds_ids_revisions_and_references_are_checked() {
        use crate::storage::IoKind;

        let (disk, db) = fixture_store();
        let old = db.snapshot();
        assert_eq!(db.begin().commit(), Ok(3));
        let mut edit = db.begin();
        edit.put(db.collection_id(1), b"flag", &db.boolean(false).unwrap())
            .unwrap();
        assert_eq!(edit.commit(), Ok(4));
        drop(edit);
        assert_eq!(db.begin().commit(), Ok(5));
        assert_eq!(db.retain_snapshot(&db.snapshot()), Ok(1));
        assert_eq!(db.retain_snapshot(&old), Ok(2));
        assert_eq!(db.snapshot().revision(), 7);
        drop(old);
        drop(db);
        let original = disk.durable_image().unwrap();
        let root = original.len() - REGION;
        check_newest_graph(&original, 7, Ok(7), "valid retained catalog");
        // Payload: revision, current data, data revision, next ID, count,
        // then (ID, snapshot revision, data offset, data revision) entries.
        let payload = root + OBJECT_HEADER;
        let older_data = u64_at(&original, payload + 88);
        for (case, field, value) in [
            ("zero next retained ID", 24, 0),
            ("zero retained ID", 40, 0),
            ("retained ID reaches next ID", 72, 3),
            ("snapshot revision reaches publishing revision", 48, 7),
            ("forward retained data reference", 88, root as u64),
            ("data revision exceeds captured revision", 96, 3),
            ("duplicate retained ID", 72, 1),
        ] {
            let mut image = original.clone();
            put64(&mut image, payload + field, value);
            resign(&mut image, root, REGION, 48);
            let trace = check_newest_graph(&image, 7, Err(Error::Corrupt), case);
            if field == 96 {
                assert!(
                    trace
                        .iter()
                        .all(|e| e.kind != IoKind::Read || e.offset != older_data),
                    "{case} must fail in the catalog before reading the retained DATA"
                );
            }
        }

        // Keep the capture at revision 5 and the current DATA at revision 4.
        // Give the first entry the same uncached DATA reference on both sides
        // of the pair so the read trace distinguishes an early catalog reject.
        let mut image = original.clone();
        put64(&mut image, payload + 56, older_data);
        put64(&mut image, payload + 64, 2);
        resign(&mut image, root, REGION, 48);
        check_newest_graph(&image, 7, Ok(7), "older data under a later capture");
        put64(&mut image, payload + 64, 5);
        resign(&mut image, root, REGION, 48);
        let case = "data revision exceeds current revision";
        let trace = check_newest_graph(&image, 7, Err(Error::Corrupt), case);
        assert!(
            trace
                .iter()
                .all(|e| e.kind != IoKind::Read || e.offset != older_data),
            "{case} must fail in the catalog before reading the retained DATA"
        );

        let mut image = original.clone();
        put64(&mut image, payload + 32, MAX_RETAINED_SNAPSHOTS as u64 + 1);
        resign(&mut image, root, REGION, 48);
        check_newest_graph(&image, 7, Err(Error::Limit), "retained count limit");
        let mut image = original.clone();
        put32(&mut image, 8, 1);
        resign(&mut image, 0, REGION, 64);
        assert_eq!(
            rejected(&image),
            Error::UnsupportedFormat,
            "v1 requires an explicit converter"
        );
    }
    fn verification_failure(image: &[u8], error: Error, stage: Stage, offset: Option<u64>) {
        let disk = SimulatedDisk::from_image(image).unwrap();
        let failure = crate::storage::with_storage_diagnostics(|| {
            crate::storage::inspect_on(disk.clone(), Limits::default())
        })
        .unwrap_err();
        assert_eq!(failure.error, error);
        assert_eq!(failure.native, None);
        assert_eq!(
            failure.verification,
            Some(crate::storage::VerificationDiagnostic { stage, offset })
        );
        assert_eq!(disk.durable_image().unwrap(), image);
    }

    #[test]
    fn verification_diagnostics_identify_metadata_and_selected_graph_failures() {
        let original = fixture();
        verification_failure(&[], Error::Corrupt, Stage::ImageLength, None);
        let mut image = original.clone();
        image[0] ^= 1;
        verification_failure(&image, Error::UnsupportedFormat, Stage::Header, Some(0));
        let mut image = original.clone();
        image[12] = 1;
        resign(&mut image, 0, REGION, 64);
        verification_failure(&image, Error::Corrupt, Stage::Header, Some(0));
        for (region, stage) in [(1, Stage::RootSlot), (3, Stage::Witness)] {
            let mut image = original.clone();
            image[region * REGION + 72] = 1;
            resign(&mut image, region * REGION, REGION, 64);
            verification_failure(
                &image,
                Error::Corrupt,
                stage,
                Some((region * REGION) as u64),
            );
        }
        let mut image = original.clone();
        image[REGION..APPEND_START].fill(0);
        verification_failure(&image, Error::Corrupt, Stage::RootSelection, None);
        // Every damaged object belongs to the selected newest root. The intact
        // older root must not be used as a fallback, and inner value errors must
        // not be relabeled with the parent directory/commit's successful read.
        let objects = spans(&original);
        for (kind, stage) in [
            (COMMIT, Stage::CommitDescriptor),
            (DATA, Stage::Directory),
            (VALUE, Stage::Value),
        ] {
            let offset = if kind == VALUE {
                objects.iter().find(|(_, _, k)| *k == kind).unwrap().0
            } else {
                objects.iter().rev().find(|(_, _, k)| *k == kind).unwrap().0
            };
            let mut image = original.clone();
            image[offset + OBJECT_HEADER] ^= 1;
            verification_failure(&image, Error::Corrupt, stage, Some(offset as u64));
        }
    }

    #[test]
    fn verification_diagnostics_preserve_io_resource_errors_and_success_has_no_marker() {
        let image = fixture();
        let root = spans(&image)
            .into_iter()
            .rev()
            .find(|(_, _, k)| *k == COMMIT)
            .unwrap()
            .0 as u64;
        let disk = SimulatedDisk::from_image(&image).unwrap();
        disk.arm(1, crate::storage::Fault::Error); // length succeeds, header read fails
        let failure = crate::storage::with_storage_diagnostics(|| {
            crate::storage::inspect_on(disk, Limits::default())
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        assert_eq!(failure.native, None); // simulator/custom I/O has no invented OS error
        assert_eq!(
            failure.verification,
            Some(crate::storage::VerificationDiagnostic {
                stage: Stage::Header,
                offset: Some(0)
            })
        );
        let failure = crate::storage::with_storage_diagnostics(|| {
            crate::storage::inspect_on(
                SimulatedDisk::from_image(&image).unwrap(),
                Limits {
                    max_live_bytes: 0,
                    ..Limits::default()
                },
            )
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Limit);
        assert_eq!(
            failure.verification.unwrap(),
            crate::storage::VerificationDiagnostic {
                stage: Stage::CommitDescriptor,
                offset: Some(root)
            }
        );
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let mut disk = SimulatedDisk::from_image(&image).unwrap();
        disk.lock().unwrap();
        let failure = crate::storage::with_storage_diagnostics(|| {
            allocator
                .with_refusal(0, || recover(&mut disk, &mem))
                .map(|_| ())
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::OutOfMemory);
        assert_eq!(
            failure.verification.unwrap(),
            crate::storage::VerificationDiagnostic {
                stage: Stage::CommitDescriptor,
                offset: Some(root)
            }
        );
        disk.unlock();
        let mut torn = image.clone();
        torn[2 * REGION] ^= 1; // stale root's bad checksum is tolerated, not a failure
        let failure = crate::storage::with_storage_diagnostics(|| {
            let bad = SimulatedDisk::from_image(&[]).unwrap();
            assert!(crate::storage::inspect_on(bad, Limits::default()).is_err());
            crate::storage::inspect_on(
                SimulatedDisk::from_image(&torn).unwrap(),
                Limits::default(),
            )?;
            Err::<(), _>(Error::Io) // unrelated error after successful verification
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        assert_eq!(failure.verification, None);
    }

    #[test]
    fn discarded_salvage_probe_does_not_label_destination_failure() {
        let mut image = fixture();
        // Revision 2 in slot0 is good. Make slot1's older graph bad, so its later
        // independent salvage probe is discarded after a successful candidate.
        let old_root = u64_at(&image, 2 * REGION + 40) as usize;
        image[old_root + OBJECT_HEADER] ^= 1;
        let source = SimulatedDisk::from_image(&image).unwrap();
        let destination = SimulatedDisk::new().unwrap();
        destination.arm(0, crate::storage::Fault::Error); // destination length
        let failure = crate::storage::with_storage_diagnostics(|| {
            crate::storage::salvage_on(source.clone(), destination, Limits::default())
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        assert_eq!(failure.verification, None);
        assert_eq!(source.durable_image().unwrap(), image);
    }
    #[test]
    fn alignment_and_cursor_boundaries_preserve_failed_take_position() {
        assert_eq!(crc(&[]), 0);
        for (n, expected) in [
            (0, Ok(0)),
            (1, Ok(REGION as u64)),
            (REGION as u64, Ok(REGION as u64)),
            (MAX_IMAGE_BYTES as u64, Ok(MAX_IMAGE_BYTES as u64)),
            (MAX_IMAGE_BYTES as u64 + 1, Err(Error::Limit)),
            (u64::MAX, Err(Error::Limit)),
        ] {
            assert_eq!(align(n), expected);
        }
        let mut c = Cursor::new(&[7, 8]);
        assert_eq!(c.take(0), Ok(&[][..]));
        assert_eq!(c.done(), Err(Error::Corrupt));
        assert_eq!(c.take(1), Ok(&[7][..]));
        for n in [usize::MAX, 2] {
            assert_eq!(c.take(n), Err(Error::Corrupt));
            assert_eq!(c.pos, 1);
        }
        assert_eq!(c.take(1), Ok(&[8][..]));
        assert_eq!(c.done(), Ok(()));
        for (n, expected) in [(3u64, Ok(3)), (4, Err(Error::Limit))] {
            assert_eq!(Cursor::new(&n.to_le_bytes()).count(3), expected);
        }
        assert_eq!(Cursor::new(&[0; 7]).int(), Err(Error::Corrupt));
        for (length, payload, expected) in [
            (3u64, 3, Ok(&b"xxx"[..])),
            (4, 4, Err(Error::Limit)),
            (3, 2, Err(Error::Corrupt)),
        ] {
            let mut bytes = length.to_le_bytes().to_vec();
            bytes.extend(vec![b'x'; payload]);
            assert_eq!(Cursor::new(&bytes).key(3), expected);
        }
    }

    #[test]
    fn cursor_zero_limits_and_short_tags_preserve_the_expected_position() {
        let mut empty = Cursor::new(&[]);
        assert_eq!(empty.done(), Ok(()));
        assert_eq!(empty.take(0), Ok(&[][..]));
        assert_eq!(empty.take(1), Err(Error::Corrupt));
        assert_eq!(empty.pos, 0);
        for length in 0..8 {
            let bytes = [0_u8; 8];
            let mut count = Cursor::new(&bytes[..length]);
            assert_eq!(count.count(0), Err(Error::Corrupt));
            assert_eq!(count.pos, 0);
            let mut key = Cursor::new(&bytes[..length]);
            assert_eq!(key.key(0), Err(Error::Corrupt));
            assert_eq!(key.pos, 0);
        }
        let zero = 0_u64.to_le_bytes();
        let mut count = Cursor::new(&zero);
        assert_eq!(count.count(0), Ok(0));
        assert_eq!(count.pos, 8);
        assert_eq!(count.done(), Ok(()));
        let mut key = Cursor::new(&zero);
        assert_eq!(key.key(0), Ok(&[][..]));
        assert_eq!(key.pos, 8);
        assert_eq!(key.done(), Ok(()));
        let one = 1_u64.to_le_bytes();
        let mut count = Cursor::new(&one);
        assert_eq!(count.count(0), Err(Error::Limit));
        assert_eq!(count.pos, 8); // The tag was valid and has been consumed.
        let mut key = Cursor::new(&one);
        assert_eq!(key.key(0), Err(Error::Limit));
        assert_eq!(key.pos, 8);
        #[cfg(target_pointer_width = "64")]
        {
            let largest = u64::MAX.to_le_bytes();
            assert_eq!(Cursor::new(&largest).count(usize::MAX), Ok(usize::MAX));
            let mut key = Cursor::new(&largest);
            assert_eq!(key.key(usize::MAX), Err(Error::Corrupt));
            assert_eq!(key.pos, 8);
        }
    }

    #[test]
    #[cfg(not(miri))]
    fn writer_cap_rejects_before_allocator_and_encoder_last_region_is_atomic() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let bytes = vec![0; MAX_IMAGE_BYTES + 1];
        let mut writer = Writer::new(&mem);
        assert_eq!(
            allocator.with_refusal(0, || writer.add(&bytes[..MAX_IMAGE_BYTES])),
            Err(Error::OutOfMemory)
        );
        assert_eq!(
            allocator.with_refusal(0, || writer.add(&bytes)),
            Err(Error::Limit)
        );
        assert!(writer.bytes.is_empty());
        let mut encoder = Encoder {
            packed: None,
            rebased: None,
            relocation: packed::Relocation::new(),
            copied: ChargedVec::new(),
            mem: &mem,
            identity: ID,
            revision: 0,
            start: (MAX_IMAGE_BYTES - REGION) as u64,
            end: (MAX_IMAGE_BYTES - REGION) as u64,
            bytes: AppendBytes::Contiguous(ChargedVec::new()),
            cache: CacheState::Owned(ValueCache::new()),
            data_cache: CacheState::Owned(DataCache::new()),
        };
        assert_eq!(
            allocator.with_refusal(0, || encoder.object(VALUE, &bytes[..MAX_IMAGE_BYTES])),
            Err(Error::Limit)
        );
        assert_eq!(
            allocator.with_refusal(0, || encoder.object(VALUE, &[0; 16])),
            Err(Error::OutOfMemory)
        );
        assert_eq!(encoder.end, (MAX_IMAGE_BYTES - REGION) as u64);
        let AppendBytes::Contiguous(bytes) = &encoder.bytes else {
            unreachable!()
        };
        assert!(bytes.is_empty());
        assert_eq!(
            encoder.object(VALUE, &[0; 16]),
            Ok(((MAX_IMAGE_BYTES - REGION) as u64, MAX_IMAGE_BYTES as u64))
        );
        let AppendBytes::Contiguous(bytes) = &encoder.bytes else {
            unreachable!()
        };
        let before = bytes.to_vec();
        assert_eq!(encoder.object(VALUE, &[0; 16]), Err(Error::Limit));
        let AppendBytes::Contiguous(bytes) = &encoder.bytes else {
            unreachable!()
        };
        assert_eq!(bytes.as_slice(), before);
        assert_eq!(encoder.end, MAX_IMAGE_BYTES as u64);
    }

    #[test]
    fn encoder_start_and_cached_collection_counter_have_independent_controls() {
        let mem = Memory::new(Limits::default()).unwrap();
        let head = Head {
            revision: 0,
            data_revision: 0,
            collections: Map::new(),
        };
        let retention = Retention {
            next_id: 1,
            entries: ChargedVec::new(),
        };
        for start in [
            APPEND_START as u64,
            (APPEND_START - REGION) as u64,
            APPEND_START as u64 + 1,
        ] {
            assert_eq!(
                encode(&mem, &head, 1, ID, start, None, None, &retention).err(),
                if start == APPEND_START as u64 {
                    None
                } else {
                    Some(Error::Invalid)
                }
            );
        }
        let mut encoder = Encoder {
            packed: None,
            rebased: None,
            relocation: packed::Relocation::new(),
            copied: ChargedVec::new(),
            mem: &mem,
            identity: ID,
            revision: 0,
            start: APPEND_START as u64,
            end: APPEND_START as u64,
            bytes: AppendBytes::Contiguous(ChargedVec::new()),
            cache: CacheState::Owned(ValueCache::new()),
            data_cache: CacheState::Owned(DataCache::new()),
        };
        let offset = encoder.data(&head, 1).unwrap();
        let AppendBytes::Contiguous(bytes) = &encoder.bytes else {
            unreachable!()
        };
        let before = bytes.to_vec();
        assert_eq!(encoder.data(&head, 1), Ok(offset));
        assert_eq!(encoder.data(&head, 2), Err(Error::Invalid));
        let AppendBytes::Contiguous(bytes) = &encoder.bytes else {
            unreachable!()
        };
        assert_eq!(bytes.as_slice(), before);
        assert_eq!(encoder.data_cache.entries.len(), 1);
    }

    #[test]
    fn segmented_append_preserves_the_bytes_and_adapter_write_boundaries() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let mut records = Map::new();
        for key in 0..3 {
            let value = Value::bytes_new(&mem, &vec![key; APPEND_SEGMENT_BYTES + 1]).unwrap();
            records = records
                .set(&mem, &[key], Record { revision: 1, value })
                .unwrap();
        }
        let head = Head {
            revision: 1,
            data_revision: 1,
            collections: Map::new()
                .set(
                    &mem,
                    &1u64.to_le_bytes(),
                    Collection {
                        base: None,
                        revision: 1,
                        records,
                    },
                )
                .unwrap(),
        };
        let retention = Retention {
            next_id: 1,
            entries: ChargedVec::new(),
        };
        let contiguous = encode(
            &mem,
            &head,
            2,
            ID,
            APPEND_START as u64,
            None,
            None,
            &retention,
        )
        .unwrap();
        let segmented = encode_fresh(&mem, &head, 2, ID, &retention).unwrap();
        assert_eq!(segmented.descriptor, contiguous.descriptor);
        assert_eq!(segmented.end, contiguous.end);
        let write = |candidate: &Candidate| {
            let mut disk = SimulatedDisk::new().unwrap();
            disk.lock().unwrap();
            candidate.write_append(&mut disk).unwrap();
            disk.sync().unwrap();
            (disk.durable_image().unwrap(), disk.trace().unwrap())
        };
        let (image, trace) = write(&contiguous);
        assert_eq!(write(&segmented), (image, trace.clone()));

        for candidate in [&contiguous, &segmented] {
            for step in [0, APPEND_SEGMENT_BYTES / REGION] {
                let mut disk = SimulatedDisk::new().unwrap();
                disk.lock().unwrap();
                disk.arm(step, crate::storage::Fault::Error);
                assert_eq!(candidate.write_append(&mut disk), Err(Error::Io));
                assert_eq!(disk.trace().unwrap(), trace[..=step]);
                assert!(disk.durable_image().unwrap().is_empty());
            }
        }
        drop((contiguous, segmented));

        let baseline = mem.usage().0;
        for allowance in 0..128 {
            let disk = SimulatedDisk::new().unwrap();
            let result = allocator.with_refusal(allowance, || {
                Storage::create(disk.clone(), &mem, &head, 2, ID, &retention)
            });
            match result {
                Ok(storage) => {
                    drop(storage);
                    assert_eq!(mem.usage().0, baseline);
                    assert!(!disk.durable_image().unwrap().is_empty());
                    assert!(allowance > 0);
                    eprintln!("checked {allowance} fresh-image allocation failures");
                    return;
                }
                Err(error) => {
                    assert_eq!(error, Error::OutOfMemory);
                    assert!(disk.durable_image().unwrap().is_empty());
                    assert!(
                        disk.trace()
                            .unwrap()
                            .iter()
                            .all(|event| event.kind == IoKind::Length)
                    );
                    assert_eq!(mem.usage().0, baseline);
                }
            }
        }
        panic!("fresh-image preparation never succeeded");
    }

    #[test]
    fn all_value_kinds_roundtrip_and_noncanonical_scalars_reject() {
        let disk = SimulatedDisk::new().unwrap();
        let db = Database::create_on(disk.clone(), ID, Limits::default()).unwrap();
        let c = db.create_collection().unwrap();
        let values = [
            db.none().unwrap(),
            db.boolean(false).unwrap(),
            db.boolean(true).unwrap(),
            db.integer(i64::MIN).unwrap(),
            db.unsigned(u64::MAX).unwrap(),
            db.float(f64::from_bits(0x7ff8_0000_0000_0042)).unwrap(),
            db.float(-0.0).unwrap(),
            db.bytes(b"").unwrap(),
            db.map().unwrap(),
            db.list().unwrap(),
        ];
        let mut tx = db.begin();
        for (i, v) in values.iter().enumerate() {
            tx.put(c, &[i as u8], v).unwrap();
        }
        tx.commit().unwrap();
        drop(tx);
        drop(db);
        let image = disk.durable_image().unwrap();
        let db = Database::open_on(disk, Limits::default()).unwrap();
        let c = db.collection_id(c.as_u64());
        let snapshot = db.snapshot();
        for (i, expected) in values.iter().enumerate() {
            let value = snapshot.get(c, &[i as u8]).unwrap().unwrap();
            assert_eq!(value.kind(), expected.kind());
            match value.kind() {
                Kind::None => {}
                Kind::Boolean => assert_eq!(value.as_bool(), expected.as_bool()),
                Kind::Integer => assert_eq!(value.as_i64(), expected.as_i64()),
                Kind::Unsigned => assert_eq!(value.as_u64(), expected.as_u64()),
                Kind::Float => assert_eq!(
                    value.as_f64().unwrap().to_bits(),
                    expected.as_f64().unwrap().to_bits()
                ),
                Kind::Bytes | Kind::Map | Kind::List => assert_eq!(value.len(), Ok(0)),
            }
        }
        for (offset, size, kind) in spans(&image) {
            if kind == VALUE && u64_at(&image, offset + OBJECT_HEADER) <= 1 {
                let mut corrupt = image.clone();
                put64(&mut corrupt, offset + OBJECT_HEADER + 8, 2);
                resign(&mut corrupt, offset, size, 48);
                assert_eq!(rejected(&corrupt), Error::Corrupt);
            }
        }
    }

    #[test]
    fn every_reachable_payload_prefix_rejects_without_writing_or_fallback() {
        let (disk, db) = fixture_store();
        db.retain_snapshot(&db.snapshot()).unwrap();
        drop(db);
        let original = disk.durable_image().unwrap();
        let mem = Memory::new(Limits::default()).unwrap();
        let mut reader = SimulatedDisk::from_image(&original).unwrap();
        reader.lock().unwrap();
        let recovered = recover(&mut reader, &mem).unwrap();
        let mut offsets = recovered.cache.iter().map(|c| c.offset).collect::<Vec<_>>();
        offsets.extend(recovered.data_cache.iter().map(|c| c.offset));
        offsets.push(recovered.slot.descriptor);
        offsets.sort_unstable();
        offsets.dedup();
        assert!(!recovered.retention.entries.is_empty());
        for offset in offsets {
            let offset = offset as usize;
            let len = u64_at(&original, offset + 16) as usize;
            assert_eq!(align((OBJECT_HEADER + len) as u64), Ok(REGION as u64));
            for prefix in 0..len {
                let mut image = original.clone();
                // Do not let the newest witness independently forbid fallback.
                let witness = (3 + (recovered.slot.revision & 1) as usize) * REGION;
                image[witness..witness + REGION].fill(0);
                put64(&mut image, offset + 16, prefix as u64);
                image[offset + OBJECT_HEADER + prefix..offset + REGION].fill(0);
                resign(&mut image, offset, REGION, 48);
                let source = SimulatedDisk::from_image(&image).unwrap();
                assert_eq!(
                    Database::open_on(source.clone(), Limits::default()).err(),
                    Some(Error::Corrupt),
                    "offset {offset}, prefix {prefix}/{len}"
                );
                assert_eq!(source.durable_image().unwrap(), image);
                assert!(source.trace().unwrap().iter().all(|e| !matches!(
                    e.kind,
                    crate::storage::IoKind::Write | crate::storage::IoKind::Sync
                )));
            }
        }
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
mod paging_bench;
