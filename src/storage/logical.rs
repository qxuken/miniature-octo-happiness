//! Versioned logical snapshots, independent of physical offsets and value sharing.
use super::{ByteStore, MAX_IMAGE_BYTES, Storage, codec};
use crate::{
    Error, Kind, Limits, MAX_RETAINED_SNAPSHOTS, Result, Value,
    engine::{Collection, Head, Record, Retained, Retention},
    memory::{Memory, Shared},
    tree::{List, Map},
};

const MAGIC: &[u8; 8] = b"WDBLOG01";
const VERSION: u64 = 1;
const MAX_NODES: usize = MAX_IMAGE_BYTES / super::REGION;
struct Writer<'a> {
    bytes: Vec<u8>,
    mem: &'a Memory,
    nodes: usize,
}
impl Writer<'_> {
    fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
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
        self.bytes(&n.to_le_bytes())
    }
    fn blob(&mut self, bytes: &[u8]) -> Result<()> {
        self.int(bytes.len() as u64)?;
        self.bytes(bytes)
    }
    fn value(&mut self, value: &Value) -> Result<()> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(Error::Limit);
        }
        self.bytes(&[value.kind() as u8])?;
        match value.kind() {
            Kind::None => self.int(0),
            Kind::Boolean => self.int(u64::from(value.as_bool()?)),
            Kind::Integer => self.int(value.as_i64()? as u64),
            Kind::Unsigned => self.int(value.as_u64()?),
            Kind::Float => self.int(value.as_f64()?.to_bits()),
            Kind::Bytes => self.blob(value.as_bytes()?),
            Kind::Map => {
                let entries = value.map_entries()?;
                self.int(entries.len() as u64)?;
                for (key, child) in entries {
                    self.blob(&key)?;
                    self.value(&child)?;
                }
                Ok(())
            }
            Kind::List => {
                self.int(value.len()? as u64)?;
                for index in 0..value.len()? {
                    self.value(&value.list_get(index)?)?;
                }
                Ok(())
            }
        }
    }
    fn data(&mut self, head: &Head, next: u64) -> Result<()> {
        self.int(head.data_revision)?;
        self.int(next)?;
        let mut collections = Vec::new();
        self.mem.reserve(&mut collections, head.collections.len())?;
        head.collections.visit(&mut |key, collection| {
            collections.push((
                u64::from_le_bytes(key.try_into().map_err(|_| Error::Corrupt)?),
                collection,
            ));
            Ok(())
        })?;
        collections.sort_unstable_by_key(|(id, _)| *id);
        self.int(collections.len() as u64)?;
        for (id, collection) in collections {
            self.int(id)?;
            self.int(collection.revision)?;
            let mut records = Vec::new();
            self.mem.reserve(&mut records, collection.records.len())?;
            collection.records.visit(&mut |key, record| {
                records.push((key, record));
                Ok(())
            })?;
            records.sort_unstable_by(|a, b| a.0.cmp(b.0));
            self.int(records.len() as u64)?;
            for (key, record) in records {
                self.blob(key)?;
                self.int(record.revision)?;
                self.value(&record.value)?;
            }
        }
        Ok(())
    }
}

/// Verify and export a quiescent source under an exclusive lease, without any
/// source write or sync. The result is bounded to 64 MiB of expanded values.
pub fn export_logical_on<B: ByteStore + 'static>(source: B, limits: Limits) -> Result<Vec<u8>> {
    let mem = Memory::new(limits)?;
    let mut source = Storage::locked(source, &mem)?;
    let recovered = codec::recover(&mut *source.backend, &mem)?;
    let mut writer = Writer {
        bytes: Vec::new(),
        mem: &mem,
        nodes: 0,
    };
    writer.bytes(MAGIC)?;
    writer.int(VERSION)?;
    writer.bytes(&recovered.identity)?;
    writer.int(recovered.head.revision)?;
    writer.int(recovered.head.data_revision)?;
    writer.int(recovered.retention.next_id)?;
    writer.int(recovered.retention.entries.len() as u64)?;
    let mut data = Vec::new();
    mem.reserve(&mut data, recovered.retention.entries.len() + 1)?;
    data.push((&*recovered.head, recovered.next_collection));
    for root in &recovered.retention.entries {
        writer.int(root.id)?;
        writer.int(root.head.revision)?;
        writer.int(root.head.data_revision)?;
        data.push((&*root.head, root.next_collection));
    }
    data.sort_unstable_by_key(|(head, _)| head.data_revision);
    data.dedup_by_key(|(head, _)| head.data_revision);
    writer.int(data.len() as u64)?;
    for (head, next) in data {
        writer.data(head, next)?;
    }
    writer.int(codec::crc(&writer.bytes))?;
    // Expansion of shared values can outgrow the physical format. Promise a
    // restorable export by checking the same resource envelope before return.
    decode(&writer.bytes, &mem)?;
    Ok(writer.bytes)
}

struct Reader<'a> {
    bytes: &'a [u8],
    mem: &'a Memory,
    nodes: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let (result, rest) = self.bytes.split_at_checked(n).ok_or(Error::Corrupt)?;
        self.bytes = rest;
        Ok(result)
    }
    fn int(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn count(&mut self, maximum: usize) -> Result<usize> {
        let n = usize::try_from(self.int()?).map_err(|_| Error::Limit)?;
        if n > maximum {
            Err(Error::Limit)
        } else {
            Ok(n)
        }
    }
    fn blob(&mut self) -> Result<Vec<u8>> {
        let n = self.count(self.mem.limits().max_bytes)?;
        let mem = self.mem;
        mem.copy(self.take(n)?)
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        self.nodes += 1;
        if depth > self.mem.limits().max_depth || self.nodes > MAX_NODES {
            return Err(Error::Limit);
        }
        let tag = self.take(1)?[0];
        match tag {
            0..=4 => {
                let kind = [
                    Kind::None,
                    Kind::Boolean,
                    Kind::Integer,
                    Kind::Unsigned,
                    Kind::Float,
                ][tag as usize];
                let bits = self.int()?;
                Value::scalar(self.mem, kind, bits).map_err(|e| {
                    if e == Error::Invalid {
                        Error::Corrupt
                    } else {
                        e
                    }
                })
            }
            5 => {
                let bytes = self.blob()?;
                Value::bytes_new(self.mem, &bytes)
            }
            6 => {
                let n = self.count(self.mem.limits().max_entries)?;
                let mut map = Map::new();
                let mut previous: Option<Vec<u8>> = None;
                for _ in 0..n {
                    let key = self.blob()?;
                    if previous.as_ref().is_some_and(|old| old >= &key) {
                        return Err(Error::Corrupt);
                    }
                    let child = self.value(depth + 1)?;
                    map = map.set(self.mem, &key, child)?;
                    previous = Some(key);
                }
                Value::from_map(self.mem, map)
            }
            7 => {
                let n = self.count(self.mem.limits().max_entries)?;
                let mut list = List::new();
                for _ in 0..n {
                    let child = self.value(depth + 1)?;
                    list = list.push(self.mem, child)?;
                }
                Value::from_list(self.mem, list)
            }
            _ => Err(Error::Corrupt),
        }
    }
    fn data(&mut self) -> Result<(Shared<Head>, u64)> {
        let revision = self.int()?;
        let next = self.int()?;
        if next == 0 {
            return Err(Error::Corrupt);
        }
        let n = self.count(self.mem.limits().max_entries)?;
        let mut collections = Map::new();
        let mut previous_id = 0;
        for _ in 0..n {
            let id = self.int()?;
            let content = self.int()?;
            if id <= previous_id || id >= next || content == 0 || content > revision {
                return Err(Error::Corrupt);
            }
            previous_id = id;
            let count = self.count(self.mem.limits().max_entries)?;
            let mut records = Map::new();
            let mut previous: Option<Vec<u8>> = None;
            for _ in 0..count {
                let key = self.blob()?;
                if previous.as_ref().is_some_and(|old| old >= &key) {
                    return Err(Error::Corrupt);
                }
                let written = self.int()?;
                if written == 0 || written > content {
                    return Err(Error::Corrupt);
                }
                let value = self.value(1)?;
                records = records.set(
                    self.mem,
                    &key,
                    Record {
                        revision: written,
                        value,
                    },
                )?;
                previous = Some(key);
            }
            collections = collections.set(
                self.mem,
                &id.to_le_bytes(),
                Collection {
                    revision: content,
                    records,
                },
            )?;
        }
        Ok((
            self.mem.shared(Head {
                revision,
                data_revision: revision,
                collections,
            })?,
            next,
        ))
    }
}
struct Image {
    identity: [u8; 16],
    head: Shared<Head>,
    next: u64,
    retention: Retention,
}
fn decode(bytes: &[u8], mem: &Memory) -> Result<Image> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(Error::Limit);
    }
    if bytes.len() < 80 {
        return Err(Error::Corrupt);
    }
    let (payload, crc) = bytes.split_at(bytes.len() - 8);
    if &payload[..8] != MAGIC || u64::from_le_bytes(payload[8..16].try_into().unwrap()) != VERSION {
        return Err(Error::UnsupportedFormat);
    }
    if codec::crc(payload) != u64::from_le_bytes(crc.try_into().unwrap()) {
        return Err(Error::Corrupt);
    }
    let mut reader = Reader {
        bytes: &payload[16..],
        mem,
        nodes: 0,
    };
    let identity = reader.take(16)?.try_into().unwrap();
    if identity == [0; 16] {
        return Err(Error::Corrupt);
    }
    let revision = reader.int()?;
    let data_revision = reader.int()?;
    let next_id = reader.int()?;
    if data_revision > revision || next_id == 0 {
        return Err(Error::Corrupt);
    }
    let count = reader.count(MAX_RETAINED_SNAPSHOTS)?;
    let mut roots = Vec::new();
    mem.reserve(&mut roots, count)?;
    let mut previous = 0;
    for _ in 0..count {
        let id = reader.int()?;
        let captured = reader.int()?;
        let data = reader.int()?;
        if id <= previous
            || id >= next_id
            || captured >= revision
            || data > captured
            || data > data_revision
        {
            return Err(Error::Corrupt);
        }
        roots.push((id, captured, data));
        previous = id;
    }
    let count = reader.count(MAX_RETAINED_SNAPSHOTS + 1)?;
    let mut data = Vec::new();
    mem.reserve(&mut data, count)?;
    for index in 0..count {
        let (head, next) = reader.data()?;
        if head.data_revision > data_revision
            || (index > 0
                && data.last().is_some_and(|(old, _): &(Shared<Head>, u64)| {
                    old.data_revision >= head.data_revision
                }))
        {
            return Err(Error::Corrupt);
        }
        if head.data_revision != data_revision
            && !roots.iter().any(|(_, _, r)| *r == head.data_revision)
        {
            return Err(Error::Corrupt);
        }
        data.push((head, next));
    }
    if !reader.bytes.is_empty() {
        return Err(Error::Corrupt);
    }
    let (current, next) = data
        .iter()
        .find(|(head, _)| head.data_revision == data_revision)
        .ok_or(Error::Corrupt)?;
    let head = mem.shared(Head {
        revision,
        data_revision,
        collections: current.collections.clone(),
    })?;
    let mut entries = Vec::new();
    mem.reserve(&mut entries, roots.len())?;
    for (id, captured, revision) in roots {
        let (old, old_next) = data
            .iter()
            .find(|(head, _)| head.data_revision == revision)
            .ok_or(Error::Corrupt)?;
        if old_next > next {
            return Err(Error::Corrupt);
        }
        entries.push(Retained {
            id,
            next_collection: *old_next,
            head: mem.shared(Head {
                revision: captured,
                data_revision: revision,
                collections: old.collections.clone(),
            })?,
        });
    }
    let image = Image {
        identity,
        head,
        next: *next,
        retention: Retention { next_id, entries },
    };
    // Validate physical representability before a file destination is created.
    codec::encode(
        mem,
        &image.head,
        image.next,
        image.identity,
        super::APPEND_START as u64,
        &[],
        &[],
        &image.retention,
    )?;
    Ok(image)
}
fn install<B: ByteStore + 'static>(image: Image, destination: B, mem: &Memory) -> Result<()> {
    let mut storage = Storage::create(
        destination,
        mem,
        &image.head,
        image.next,
        image.identity,
        &image.retention,
    )?;
    storage.verify(mem)
}
/// Validate a complete versioned logical stream before writing an empty target.
pub fn import_logical_on<B: ByteStore + 'static>(
    bytes: &[u8],
    destination: B,
    limits: Limits,
) -> Result<()> {
    let mem = Memory::new(limits)?;
    let image = decode(bytes, &mem)?;
    install(image, destination, &mem)
}
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn export_logical_file(path: impl AsRef<std::path::Path>, limits: Limits) -> Result<Vec<u8>> {
    export_logical_on(super::FileStore::open_read_only(path)?, limits)
}
/// The destination is created only after full logical and physical validation.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn import_logical_file(
    bytes: &[u8],
    path: impl AsRef<std::path::Path>,
    limits: Limits,
) -> Result<()> {
    let mem = Memory::new(limits)?;
    let image = decode(bytes, &mem)?;
    install(image, super::FileStore::create(path)?, &mem)
}
