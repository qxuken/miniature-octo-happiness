use super::{APPEND_START, ByteStore, MAX_IMAGE_BYTES, REGION, read_all};
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
#[derive(Clone)]
pub(crate) struct DataEntry {
    pub revision: u64,
    pub next_collection: u64,
    pub offset: u64,
}
pub(crate) struct Candidate {
    pub start: u64,
    pub bytes: Vec<u8>,
    pub descriptor: u64,
    pub end: u64,
    pub cache: Vec<CacheEntry>,
    pub data_cache: Vec<DataEntry>,
}
struct Writer<'a> {
    mem: &'a Memory,
    bytes: Vec<u8>,
}
impl<'a> Writer<'a> {
    fn new(mem: &'a Memory) -> Self {
        Self {
            mem,
            bytes: Vec::new(),
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
    bytes: Vec<u8>,
    cache: Vec<CacheEntry>,
    data_cache: Vec<DataEntry>,
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
        if let Some(old) = self.cache.iter().find(|old| old.value.same_node(value)) {
            return Ok(old.offset);
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
        self.mem.reserve(&mut self.cache, 1)?;
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
        let mut collections = Vec::new();
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
            let mut records = Vec::new();
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
    let mut cache = Vec::new();
    mem.reserve(&mut cache, old_cache.len())?;
    cache.extend(old_cache.iter().cloned());
    let mut data_cache = Vec::new();
    mem.reserve(&mut data_cache, old_data.len())?;
    data_cache.extend(old_data.iter().cloned());
    let mut encoder = Encoder {
        mem,
        identity,
        revision: head.revision,
        start,
        bytes: Vec::new(),
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
    payload: Vec<u8>,
    born: u64,
    end: u64,
}
struct Reader<'a> {
    backend: &'a mut dyn ByteStore,
    mem: &'a Memory,
    identity: [u8; 16],
    revision: u64,
    cache: Vec<CacheEntry>,
    ranges: Vec<(u64, u64)>,
    work: usize,
    data_cache: Vec<DecodedData>,
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
        if self.ranges.len() >= MAX_OBJECTS
            || self.ranges.iter().any(|(a, b)| offset < *b && end > *a)
        {
            return Err(Error::Corrupt);
        }
        self.work = self
            .work
            .checked_add(size as usize + OBJECT_HEADER)
            .filter(|n| *n <= 2 * MAX_IMAGE_BYTES)
            .ok_or(Error::Limit)?;
        let mut bytes = Vec::new();
        self.mem.reserve(&mut bytes, size as usize)?;
        bytes.resize(size as usize, 0);
        read_all(self.backend, offset, &mut bytes)?;
        if bytes[..OBJECT_HEADER] != header
            || checksum(&bytes, 48) != u64_at(&bytes, 48)
            || bytes[OBJECT_HEADER + payload_len..].iter().any(|b| *b != 0)
        {
            return Err(Error::Corrupt);
        }
        self.mem.reserve(&mut self.ranges, 1)?;
        self.ranges.push((offset, end));
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
        if depth > self.mem.limits().max_depth || offset >= parent {
            return Err(Error::Corrupt);
        }
        if let Some(old) = self.cache.iter().find(|old| old.offset == offset) {
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
    pub cache: Vec<CacheEntry>,
    pub data_cache: Vec<DataEntry>,
    pub retention: Retention,
}
pub(crate) fn recover(backend: &mut dyn ByteStore, mem: &Memory) -> Result<Recovered> {
    let length = backend.length()?;
    if length < APPEND_START as u64 || length > MAX_IMAGE_BYTES as u64 {
        return Err(Error::Corrupt);
    }
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
    let mut slots = [None; 2];
    let mut witnesses = [None; 2];
    for i in 0..2 {
        let mut bytes = [0; REGION];
        read_all(backend, ((1 + i) * REGION) as u64, &mut bytes)?;
        slots[i] = parse_metadata(&bytes, identity, i, false, length)?;
        read_all(backend, ((3 + i) * REGION) as u64, &mut bytes)?;
        witnesses[i] = parse_metadata(&bytes, identity, i, true, length)?;
    }
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
    if slots[0].is_some_and(|a| slots[1].is_some_and(|b| a.revision.abs_diff(b.revision) != 1)) {
        return Err(Error::Corrupt);
    }
    let mut reader = Reader {
        backend,
        mem,
        identity,
        revision: slot.revision,
        cache: Vec::new(),
        ranges: Vec::new(),
        work: 0,
        data_cache: Vec::new(),
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
    let (collections, next_collection) = reader.data(data_ref, slot.descriptor, data_revision)?;
    let head = mem.shared(Head {
        revision,
        data_revision,
        collections,
    })?;
    let mut entries = Vec::new();
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
    let mut data_cache = Vec::new();
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Limits, storage::SimulatedDisk};
    const ID: [u8; 16] = *b"codec-test-00001";
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
}
