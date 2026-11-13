use super::diagnostics::{self, VerificationStage as Stage};
use super::{APPEND_START, ByteStore, MAX_IMAGE_BYTES, REGION, read_all};
use crate::memory::ChargedVec;
use crate::{
    Error, Kind, Result, Value,
    engine::{Collection, Head, MAX_RETAINED_SNAPSHOTS, Record, Retained, Retention},
    memory::{Memory, Shared},
    tree::{List, Map},
};

const VERSION: u32 = crate::version::FORMAT_VERSION;
const MAX_OBJECTS: usize = MAX_IMAGE_BYTES / REGION;
const OBJECT_HEADER: usize = 64;
const VALUE: u32 = 1;
const DATA: u32 = 2;
const COMMIT: u32 = 3;

/// CRC-64/ECMA-182: polynomial 0x42f0e1eba9ea3693, init/xorout zero,
/// non-reflected. Checksums are damage detection, not authentication.
pub(crate) fn crc(bytes: &[u8]) -> u64 {
    update_crc(0, bytes)
}
fn update_crc(mut crc: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        crc ^= u64::from(*byte) << 56;
        for _ in 0..8 {
            crc = if crc & (1 << 63) != 0 {
                (crc << 1) ^ 0x42f0e1eba9ea3693
            } else {
                crc << 1
            };
        }
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
    let value = n
        .checked_add(REGION as u64 - 1)
        .map(|n| n / REGION as u64 * REGION as u64)
        .ok_or(Error::Limit)?;
    if value > MAX_IMAGE_BYTES as u64 {
        Err(Error::Limit)
    } else {
        Ok(value)
    }
}
pub(crate) fn header(identity: [u8; 16]) -> [u8; REGION] {
    let mut out = [0; REGION];
    out[..8].copy_from_slice(b"WEEKDB01");
    put32(&mut out, 8, VERSION);
    put64(&mut out, 16, REGION as u64);
    put64(&mut out, 24, APPEND_START as u64);
    out[32..48].copy_from_slice(&identity);
    put64(&mut out, 48, MAX_IMAGE_BYTES as u64);
    put64(&mut out, 56, MAX_OBJECTS as u64);
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
        || slot.end > MAX_IMAGE_BYTES as u64
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
/// Rebuilt on each encode, including from recovery's differently ordered cache.
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
pub(crate) struct Candidate {
    pub start: u64,
    pub bytes: ChargedVec<u8>,
    pub descriptor: u64,
    pub end: u64,
    pub cache: ChargedVec<CacheEntry>,
    pub data_cache: ChargedVec<DataEntry>,
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
        self.mem.reserve(&mut self.bytes, bytes.len())?;
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
    mem: &'a Memory,
    identity: [u8; 16],
    revision: u64,
    start: u64,
    bytes: ChargedVec<u8>,
    cache: ChargedVec<CacheEntry>,
    value_index: ValueIndex,
    data_cache: ChargedVec<DataEntry>,
}
impl Encoder<'_> {
    fn object(&mut self, kind: u32, payload: &[u8]) -> Result<(u64, u64)> {
        self.object_born(kind, payload, self.revision)
    }
    fn object_born(&mut self, kind: u32, payload: &[u8], born: u64) -> Result<(u64, u64)> {
        let size = align(
            (OBJECT_HEADER as u64)
                .checked_add(payload.len() as u64)
                .ok_or(Error::Limit)?,
        )? as usize;
        let offset = self
            .start
            .checked_add(self.bytes.len() as u64)
            .ok_or(Error::Limit)?;
        let end = offset
            .checked_add(size as u64)
            .filter(|n| *n <= MAX_IMAGE_BYTES as u64)
            .ok_or(Error::Limit)?;
        self.mem.reserve(&mut self.bytes, size)?;
        let old = self.bytes.len();
        self.bytes.resize(old + size, 0);
        let bytes = &mut self.bytes[old..];
        bytes[..8].copy_from_slice(b"WDBOBJ01");
        put32(bytes, 8, kind);
        put32(bytes, 12, VERSION);
        put64(bytes, 16, payload.len() as u64);
        put64(bytes, 24, born);
        bytes[32..48].copy_from_slice(&self.identity);
        bytes[OBJECT_HEADER..OBJECT_HEADER + payload.len()].copy_from_slice(payload);
        let crc = checksum(bytes, 48);
        put64(bytes, 48, crc);
        Ok((offset, end))
    }
    fn value(&mut self, value: &Value) -> Result<u64> {
        let cached = self.value_index.get(value, &self.cache);
        #[cfg(test)]
        assert_eq!(
            cached,
            self.cache
                .iter()
                .position(|entry| entry.value.same_node(value))
        );
        if let Some(index) = cached {
            return Ok(self.cache[index].offset);
        }
        if self.cache.len() >= MAX_OBJECTS {
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
                let entries = value.map_entries()?;
                payload.int(entries.len() as u64)?;
                for (key, child) in entries {
                    let offset = self.value(&child)?;
                    payload.key(&key)?;
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
        self.value_index
            .reserve(self.mem, &self.cache, self.cache.len() + 1)?;
        self.mem.reserve(&mut self.cache, 1)?;
        self.value_index
            .insert(value, &self.cache, self.cache.len());
        self.cache.push(CacheEntry {
            value: value.clone(),
            offset,
            end,
        });
        Ok(offset)
    }
    fn data(&mut self, head: &Head, next_collection: u64) -> Result<u64> {
        if let Some(old) = self
            .data_cache
            .iter()
            .find(|r| r.revision == head.data_revision)
        {
            if old.next_collection != next_collection {
                return Err(Error::Invalid);
            }
            return Ok(old.offset);
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
            payload.int(c.records.len() as u64)?;
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
        self.mem.reserve(&mut self.data_cache, 1)?;
        self.data_cache.push(DataEntry {
            revision: head.data_revision,
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
    old_cache: &[CacheEntry],
    old_data: &[DataEntry],
    retention: &Retention,
) -> Result<Candidate> {
    if start < APPEND_START as u64 || !start.is_multiple_of(REGION as u64) {
        return Err(Error::Invalid);
    }
    let mut cache = ChargedVec::new();
    mem.reserve(&mut cache, old_cache.len())?;
    cache.extend(old_cache.iter().cloned());
    let mut value_index = ValueIndex::new();
    value_index.reserve(mem, &cache, cache.len())?;
    let mut data_cache = ChargedVec::new();
    mem.reserve(&mut data_cache, old_data.len())?;
    data_cache.extend(old_data.iter().cloned());
    let mut encoder = Encoder {
        mem,
        identity,
        revision: head.revision,
        start,
        bytes: ChargedVec::new(),
        cache,
        value_index,
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
        start,
        bytes: encoder.bytes,
        descriptor,
        end,
        cache: encoder.cache,
        data_cache: encoder.data_cache,
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
    payload: ChargedVec<u8>,
    born: u64,
    end: u64,
}
struct Reader<'a> {
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
        if self.objects >= MAX_OBJECTS
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
        self.work = self
            .work
            .checked_add(size as usize + OBJECT_HEADER)
            .filter(|n| *n <= 2 * MAX_IMAGE_BYTES)
            .ok_or(Error::Limit)?;
        let mut bytes = ChargedVec::new();
        self.mem.reserve(&mut bytes, size as usize)?;
        bytes.resize(size as usize, 0);
        read_all(self.backend, offset, &mut bytes)?;
        if bytes[..OBJECT_HEADER] != header
            || checksum(&bytes, 48) != u64_at(&bytes, 48)
            || bytes[OBJECT_HEADER + payload_len..].iter().any(|b| *b != 0)
        {
            return Err(Error::Corrupt);
        }
        // Allocate lazily at the former range-registration boundary, after the
        // full object validates. Malformed first headers still fail before any
        // index allocation. Every accepted region is marked exactly once.
        if self.regions.is_empty() {
            self.mem.reserve(&mut self.regions, self.region_count)?;
            self.regions.resize(self.region_count, 0);
        }
        self.regions
            .get_mut(first_region..end_region)
            .ok_or(Error::Corrupt)?
            .fill(1);
        self.objects += 1;
        // Move the payload down in the already allocated buffer; no second allocation.
        bytes.copy_within(OBJECT_HEADER..OBJECT_HEADER + payload_len, 0);
        bytes.truncate(payload_len);
        Ok(Object {
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
        let mut cursor = Cursor::new(&object.payload);
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
                Value::scalar(self.mem, kind, bits).map_err(|e| {
                    if e == Error::Invalid {
                        Error::Corrupt
                    } else {
                        e
                    }
                })?
            }
            5 => Value::bytes_new(self.mem, cursor.key(self.mem.limits().max_bytes)?)?,
            6 => {
                let count = cursor.count(self.mem.limits().max_entries)?;
                let mut map = Map::new();
                let mut previous: Option<&[u8]> = None;
                for _ in 0..count {
                    let key = cursor.key(self.mem.limits().max_bytes)?;
                    if previous.is_some_and(|old| old >= key) {
                        return Err(Error::Corrupt);
                    }
                    previous = Some(key);
                    let child = self.value(cursor.int()?, offset, depth + 1)?;
                    map = map.set(self.mem, key, child)?;
                }
                Value::from_map(self.mem, map)?
            }
            7 => {
                let count = cursor.count(self.mem.limits().max_entries)?;
                let mut list = List::new();
                for _ in 0..count {
                    let child = self.value(cursor.int()?, offset, depth + 1)?;
                    list = list.push(self.mem, child)?;
                }
                Value::from_list(self.mem, list)?
            }
            _ => return Err(Error::Corrupt),
        };
        cursor.done()?;
        self.mem.reserve(&mut self.cache, 1)?;
        self.regions[(offset / REGION as u64) as usize] = self.cache.len() + 2;
        self.cache.push(CacheEntry {
            value: value.clone(),
            offset,
            end: object.end,
        });
        Ok(value)
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
            let mut records = Map::new();
            let mut previous: Option<&[u8]> = None;
            for _ in 0..records_count {
                let key = cursor.key(self.mem.limits().max_bytes)?;
                if previous.is_some_and(|old| old >= key) {
                    return Err(Error::Corrupt);
                }
                previous = Some(key);
                let write_revision = cursor.int()?;
                if write_revision == 0 || write_revision > content_revision {
                    return Err(Error::Corrupt);
                }
                let value = self.value(cursor.int()?, offset, 1)?;
                records = records.set(
                    self.mem,
                    key,
                    Record {
                        revision: write_revision,
                        value,
                    },
                )?;
            }
            collections = collections.set(
                self.mem,
                &id.to_le_bytes(),
                Collection {
                    revision: content_revision,
                    records,
                },
            )?;
        }
        cursor.done()?;
        self.mem.reserve(&mut self.data_cache, 1)?;
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
    diagnostics::clear_verification();
    let length = diagnostics::verify(Stage::ImageLength, None, || {
        let length = backend.length()?;
        if length < APPEND_START as u64 || length > MAX_IMAGE_BYTES as u64 {
            return Err(Error::Corrupt);
        }
        Ok(length)
    })?;
    let identity = diagnostics::verify(Stage::Header, Some(0), || {
        let mut header = [0; REGION];
        read_all(backend, 0, &mut header)?;
        if &header[..8] != b"WEEKDB01" || u32_at(&header, 8) != VERSION {
            return Err(Error::UnsupportedFormat);
        }
        if checksum(&header, 64) != u64_at(&header, 64)
            || u32_at(&header, 12) != 0
            || u64_at(&header, 16) != REGION as u64
            || u64_at(&header, 24) != APPEND_START as u64
            || u64_at(&header, 48) != MAX_IMAGE_BYTES as u64
            || u64_at(&header, 56) != MAX_OBJECTS as u64
            || header[72..].iter().any(|b| *b != 0)
        {
            return Err(Error::Corrupt);
        }
        let identity: [u8; 16] = header[32..48].try_into().unwrap();
        if identity == [0; 16] {
            return Err(Error::Corrupt);
        }
        Ok(identity)
    })?;
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
        let mut reader = Reader {
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
        Ok(Recovered {
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
    use crate::{Database, Limits, storage::SimulatedDisk};
    const ID: [u8; 16] = *b"codec-test-00001";
    #[test]
    fn value_index_collisions_wrap_growth_and_first_identity_match() {
        let mem = Memory::new(Limits::default()).unwrap();
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
        mem.fail_after(Some(0));
        assert_eq!(index.reserve(&mem, &cache, 5), Err(Error::OutOfMemory));
        assert_eq!(&*index.slots, slots);
        assert_eq!(mem.public_usage().live_bytes, before);
        mem.fail_after(None);
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
            let mem = Memory::new(Limits {
                max_live_bytes: cap,
                ..Limits::default()
            })
            .unwrap();
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
            mem.fail_after(Some(0));
            assert_eq!(index.reserve(&mem, &[], MAX_OBJECTS + 1), Err(Error::Limit));
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
        for length in [APPEND_START as u64 - 1, MAX_IMAGE_BYTES as u64 + 1] {
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
            if offset == 32 {
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
        put32(&mut image, 8, VERSION + 1);
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
            let mem = Memory::new(Limits::default()).unwrap();
            mem.fail_after(Some(budget));
            let result = recover(&mut disk, &mem);
            mem.fail_after(None);
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
        let mem = Memory::new(Limits::default()).unwrap();
        mem.fail_after(Some(0));
        let mut disk = SimulatedDisk::from_image(&image).unwrap();
        disk.lock().unwrap();
        let failure =
            crate::storage::with_storage_diagnostics(|| recover(&mut disk, &mem).map(|_| ()))
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
    #[cfg(not(miri))]
    fn writer_cap_rejects_before_allocator_and_encoder_last_region_is_atomic() {
        let mem = Memory::new(Limits::default()).unwrap();
        let bytes = vec![0; MAX_IMAGE_BYTES + 1];
        mem.fail_after(Some(0));
        let mut writer = Writer::new(&mem);
        assert_eq!(
            writer.add(&bytes[..MAX_IMAGE_BYTES]),
            Err(Error::OutOfMemory)
        );
        assert_eq!(writer.add(&bytes), Err(Error::Limit));
        assert!(writer.bytes.is_empty());
        mem.fail_after(None);
        let mut encoder = Encoder {
            mem: &mem,
            identity: ID,
            revision: 0,
            start: (MAX_IMAGE_BYTES - REGION) as u64,
            bytes: ChargedVec::new(),
            cache: ChargedVec::new(),
            value_index: ValueIndex::new(),
            data_cache: ChargedVec::new(),
        };
        assert_eq!(
            encoder.object(VALUE, &[0; 16]),
            Ok(((MAX_IMAGE_BYTES - REGION) as u64, MAX_IMAGE_BYTES as u64))
        );
        let before = encoder.bytes.to_vec();
        assert_eq!(encoder.object(VALUE, &[0; 16]), Err(Error::Limit));
        assert_eq!(&*encoder.bytes, before);
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
                encode(&mem, &head, 1, ID, start, &[], &[], &retention).err(),
                if start == APPEND_START as u64 {
                    None
                } else {
                    Some(Error::Invalid)
                }
            );
        }
        let mut encoder = Encoder {
            mem: &mem,
            identity: ID,
            revision: 0,
            start: APPEND_START as u64,
            bytes: ChargedVec::new(),
            cache: ChargedVec::new(),
            value_index: ValueIndex::new(),
            data_cache: ChargedVec::new(),
        };
        let offset = encoder.data(&head, 1).unwrap();
        let before = encoder.bytes.to_vec();
        assert_eq!(encoder.data(&head, 1), Ok(offset));
        assert_eq!(encoder.data(&head, 2), Err(Error::Invalid));
        assert_eq!(&*encoder.bytes, before);
        assert_eq!(encoder.data_cache.len(), 1);
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
