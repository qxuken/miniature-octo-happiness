//! Format 3. A DATA catalog names branches, each branch names ordered immutable
//! leaves, and each leaf contains whole key/revision/value records.
use super::*;
use crate::engine::{Backing, Base};
pub(super) mod blob;
use blob::Blob;
mod write;
pub(super) use write::Relocation;

const LEAF: u32 = 4;
const BRANCH: u32 = 5;
const FANOUT: usize = 128;
const LEAF_BYTES: usize = REGION - OBJECT_HEADER;

impl Cursor<'_> {
    fn short(&mut self) -> Result<usize> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }
}
fn sized<'a>(c: &mut Cursor<'a>, limit: usize) -> Result<&'a [u8]> {
    let len = c.short()?;
    if len > limit {
        return Err(Error::Limit);
    }
    c.take(len)
}
fn encode_value(w: &mut Writer<'_>, v: &Value) -> Result<()> {
    w.add(&[v.kind() as u8])?;
    match v.kind() {
        Kind::None => Ok(()),
        Kind::Boolean => w.add(&[u8::from(v.as_bool()?)]),
        Kind::Integer => w.int(v.as_i64()? as u64),
        Kind::Unsigned => w.int(v.as_u64()?),
        Kind::Float => w.int(v.as_f64()?.to_bits()),
        Kind::Bytes => w.small_bytes(v.as_bytes()?),
        Kind::Map => {
            let entries = v.map_entries_for_encoding(w.mem)?;
            w.short(entries.len())?;
            for (k, child) in entries {
                w.small_bytes(k)?;
                encode_value(w, child)?;
            }
            Ok(())
        }
        Kind::List => {
            w.short(v.len()?)?;
            for i in 0..v.len()? {
                encode_value(w, &v.list_get(i)?)?;
            }
            Ok(())
        }
    }
}
impl Writer<'_> {
    fn short(&mut self, n: usize) -> Result<()> {
        self.add(&u32::try_from(n).map_err(|_| Error::Limit)?.to_le_bytes())
    }
    fn small_bytes(&mut self, b: &[u8]) -> Result<()> {
        self.short(b.len())?;
        self.add(b)
    }
}

fn decode(mem: &Memory, bytes: &[u8]) -> Result<Value> {
    fn value(mem: &Memory, c: &mut Cursor<'_>, depth: usize) -> Result<Value> {
        if depth > mem.limits().max_depth {
            return Err(Error::Limit);
        }
        let tag = c.take(1)?[0];
        match tag {
            0 => Value::scalar(mem, Kind::None, 0),
            1 => {
                let v = c.take(1)?[0];
                if v > 1 {
                    return Err(Error::Corrupt);
                }
                Value::scalar(mem, Kind::Boolean, u64::from(v))
            }
            2..=4 => Value::scalar(
                mem,
                match tag {
                    2 => Kind::Integer,
                    3 => Kind::Unsigned,
                    _ => Kind::Float,
                },
                c.int()?,
            ),
            5 => Value::bytes_new(mem, sized(c, mem.limits().max_bytes)?),
            6 | 7 => {
                let n = c.short()?;
                if n > mem.limits().max_entries {
                    return Err(Error::Limit);
                }
                if tag == 6 {
                    let mut map = Map::new();
                    let mut prev = None;
                    for _ in 0..n {
                        let k = sized(c, mem.limits().max_bytes)?;
                        if prev.is_some_and(|p: &[u8]| p >= k) {
                            return Err(Error::Corrupt);
                        }
                        map = map.set(mem, k, value(mem, c, depth + 1)?)?;
                        prev = Some(k);
                    }
                    Value::from_map(mem, map)
                } else {
                    let mut list = List::new();
                    for _ in 0..n {
                        list = list.push(mem, value(mem, c, depth + 1)?)?;
                    }
                    Value::from_list(mem, list)
                }
            }
            _ => Err(Error::Corrupt),
        }
    }
    let mut c = Cursor::new(bytes);
    let v = value(mem, &mut c, 1)?;
    c.done()?;
    Ok(v)
}
struct Entry<'a> {
    key: &'a [u8],
    revision: u64,
    value: &'a [u8],
}
fn entry<'a>(mem: &Memory, c: &mut Cursor<'a>) -> Result<Entry<'a>> {
    let key = sized(c, mem.limits().max_bytes)?;
    let revision = c.int()?;
    let length = c.short()?;
    Ok(Entry {
        key,
        revision,
        value: c.take(length)?,
    })
}

struct Leaf {
    first: ChargedVec<u8>,
    last: ChargedVec<u8>,
    offset: u64,
    end: u64,
    len: usize,
    digest: u64,
    count: usize,
    max_revision: u64,
    born: u64,
}
impl Leaf {
    fn read(&self, io: &mut dyn ByteStore, mem: &Memory) -> Result<ChargedVec<u8>> {
        let mut bytes = ChargedVec::new();
        mem.reserve(&mut bytes, self.len)?;
        bytes.resize(self.len, 0);
        read_all(io, self.offset + OBJECT_HEADER as u64, &mut bytes)?;
        if crc(&bytes) != self.digest {
            return Err(Error::Corrupt);
        }
        Ok(bytes)
    }
}
struct Branch {
    offset: u64,
    end: u64,
    born: u64,
    leaves: ChargedVec<Shared<Leaf>>,
}
impl Branch {
    fn first(&self) -> &[u8] {
        &self.leaves[0].first
    }
    fn last(&self) -> &[u8] {
        &self.leaves.last().unwrap().last
    }
}
pub(crate) struct Tree {
    source: Source,
    branches: ChargedVec<Shared<Branch>>,
}
impl Tree {
    fn load(&self, mem: &Memory, raw: &[u8]) -> Result<Value> {
        if let Some(blob) = Blob::parse(raw)? {
            if !self.source.3.extended() {
                return Err(Error::Corrupt);
            }
            let bytes = self.source.access(|io| blob.read(io, mem))?;
            Value::bytes_new(mem, &bytes)
        } else {
            decode(mem, raw)
        }
    }

    pub(crate) fn find(&self, mem: &Memory, key: &[u8]) -> Result<Option<Record>> {
        self.point(mem, key, |r| {
            Ok(Record {
                revision: r.revision,
                value: self.load(mem, r.value)?,
            })
        })
    }
    pub(crate) fn revision(&self, mem: &Memory, key: &[u8]) -> Result<Option<u64>> {
        self.point(mem, key, |r| Ok(r.revision))
    }
    fn point<T>(
        &self,
        mem: &Memory,
        key: &[u8],
        project: impl FnOnce(Entry<'_>) -> Result<T>,
    ) -> Result<Option<T>> {
        let i = self.branches.partition_point(|b| b.first() <= key);
        if i == 0 {
            return Ok(None);
        }
        let branch = &self.branches[i - 1];
        let i = branch.leaves.partition_point(|l| l.first.as_slice() <= key);
        let leaf = &branch.leaves[i - 1];
        if key > leaf.last.as_slice() {
            return Ok(None);
        }
        let bytes = self.source.access(|io| leaf.read(io, mem))?;
        let mut c = Cursor::new(&bytes);
        while c.pos < bytes.len() {
            let row = entry(mem, &mut c)?;
            match row.key.cmp(key) {
                std::cmp::Ordering::Equal => {
                    return project(row).map(Some);
                }
                std::cmp::Ordering::Greater => break,
                _ => (),
            }
        }
        Ok(None)
    }
    pub(crate) fn visit(
        &self,
        mem: &Memory,
        mut f: impl FnMut(&[u8], Record) -> Result<()>,
    ) -> Result<()> {
        for branch in &self.branches {
            for leaf in &branch.leaves {
                let bytes = self.source.access(|io| leaf.read(io, mem))?;
                let mut c = Cursor::new(&bytes);
                while c.pos < bytes.len() {
                    let r = entry(mem, &mut c)?;
                    f(
                        r.key,
                        Record {
                            revision: r.revision,
                            value: self.load(mem, r.value)?,
                        },
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
enum VerifiedPage {
    Leaf(Shared<Leaf>),
    Branch(Shared<Branch>),
}
pub(super) struct PageCache(
    ChargedVec<Option<VerifiedPage>>,
    extents::Offsets<VerifiedPage>,
);
impl PageCache {
    pub(super) fn new() -> Self {
        Self(ChargedVec::new(), extents::Offsets::new())
    }
    fn get(&self, i: usize) -> Option<&VerifiedPage> {
        if self.0.is_empty() {
            self.1.get(i as u64)
        } else {
            self.0[i].as_ref()
        }
    }
    fn save(&mut self, mem: &Memory, i: usize, p: VerifiedPage) -> Result<()> {
        if self.0.is_empty() {
            self.1.insert(mem, i as u64, p)
        } else {
            self.0[i] = Some(p);
            Ok(())
        }
    }
    fn index(
        &mut self,
        mem: &Memory,
        regions: usize,
        off: u64,
        parent: u64,
        sparse: bool,
    ) -> Result<usize> {
        if off < APPEND_START as u64 || off >= parent || !off.is_multiple_of(REGION as u64) {
            return Err(Error::Corrupt);
        }
        let i = (off / REGION as u64) as usize;
        if i >= regions {
            return Err(Error::Corrupt);
        }
        if !sparse && self.0.is_empty() {
            mem.reserve(&mut self.0, regions)?;
            self.0.resize(regions, None);
        }
        Ok(i)
    }
}
impl Reader<'_> {
    fn packed_leaf(&mut self, off: u64, parent: u64) -> Result<Shared<Leaf>> {
        let i = self.page_cache.index(
            self.mem,
            self.region_count,
            off,
            parent,
            self.packed.extended(),
        )?;
        if let Some(page) = self.page_cache.get(i) {
            return match page {
                VerifiedPage::Leaf(l) if l.end <= parent => Ok(l.clone()),
                _ => Err(Error::Corrupt),
            };
        }
        let obj = self.object(off, LEAF, parent)?;
        let mut c = Cursor::new(&obj.payload);
        let mut first = None;
        let mut prev = None;
        let mut count = 0;
        let mut max_revision = 0;
        while c.pos < obj.payload.len() {
            let r = entry(self.mem, &mut c)?;
            if prev.is_some_and(|old: &[u8]| old >= r.key)
                || r.revision == 0
                || r.revision > obj.born
            {
                return Err(Error::Corrupt);
            }
            if let Some(blob) = Blob::parse(r.value)? {
                self.verify_blob(blob, off, r.revision)?;
            } else {
                decode(self.mem, r.value)?;
            }
            first.get_or_insert(r.key);
            prev = Some(r.key);
            count += 1;
            max_revision = max_revision.max(r.revision);
        }
        if count != 1 && obj.payload.len() > LEAF_BYTES {
            return Err(Error::Corrupt);
        }
        let leaf = self.mem.shared(Leaf {
            first: self.mem.copy(first.ok_or(Error::Corrupt)?)?,
            last: self.mem.copy(prev.unwrap())?,
            offset: off,
            end: obj.end,
            len: obj.payload.len(),
            digest: crc(&obj.payload),
            count,
            max_revision,
            born: obj.born,
        })?;
        self.page_cache
            .save(self.mem, i, VerifiedPage::Leaf(leaf.clone()))?;
        self.scratch = obj.payload;
        Ok(leaf)
    }
    fn packed_branch(&mut self, off: u64, parent: u64) -> Result<Shared<Branch>> {
        let i = self.page_cache.index(
            self.mem,
            self.region_count,
            off,
            parent,
            self.packed.extended(),
        )?;
        if let Some(page) = self.page_cache.get(i) {
            return match page {
                VerifiedPage::Branch(b) if b.end <= parent => Ok(b.clone()),
                _ => Err(Error::Corrupt),
            };
        }
        let obj = self.object(off, BRANCH, parent)?;
        if obj.payload.is_empty() || obj.payload.len() > 8 * FANOUT {
            return Err(Error::Corrupt);
        }
        let mut c = Cursor::new(&obj.payload);
        let mut leaves: ChargedVec<Shared<Leaf>> = ChargedVec::new();
        self.mem.reserve(&mut leaves, obj.payload.len() / 8)?;
        while c.pos < obj.payload.len() {
            let leaf = self.packed_leaf(c.int()?, off)?;
            if leaf.born > obj.born
                || leaves
                    .last()
                    .is_some_and(|l| l.last.as_slice() >= leaf.first.as_slice())
            {
                return Err(Error::Corrupt);
            }
            leaves.push(leaf);
        }
        let branch = self.mem.shared(Branch {
            offset: off,
            end: obj.end,
            born: obj.born,
            leaves,
        })?;
        self.page_cache
            .save(self.mem, i, VerifiedPage::Branch(branch.clone()))?;
        Ok(branch)
    }
    pub(super) fn packed_collection(
        &mut self,
        c: &mut Cursor<'_>,
        offset: u64,
        revision: u64,
        count: usize,
    ) -> Result<Collection> {
        let n = c.count(MAX_OBJECTS)?;
        if n > count {
            return Err(Error::Corrupt);
        }
        let mut branches: ChargedVec<Shared<Branch>> = ChargedVec::new();
        self.mem.reserve(&mut branches, n)?;
        let mut total = 0usize;
        let mut records = Map::new();
        for _ in 0..n {
            let b = self.packed_branch(c.int()?, offset)?;
            if b.born > revision || branches.last().is_some_and(|old| old.last() >= b.first()) {
                return Err(Error::Corrupt);
            }
            for l in &b.leaves {
                if l.max_revision > revision {
                    return Err(Error::Corrupt);
                }
                total = total
                    .checked_add(l.count)
                    .filter(|v| *v <= count)
                    .ok_or(Error::Corrupt)?;
                // Quiescent consumers that borrow a backend need owned values after recovery.
                if self.graph.is_none() {
                    let raw = l.read(self.backend, self.mem)?;
                    let mut rows = Cursor::new(&raw);
                    while rows.pos < raw.len() {
                        let r = entry(self.mem, &mut rows)?;
                        records = records.set(
                            self.mem,
                            r.key,
                            Record {
                                revision: r.revision,
                                value: if let Some(blob) = Blob::parse(r.value)? {
                                    Value::bytes_new(self.mem, &blob.read(self.backend, self.mem)?)?
                                } else {
                                    decode(self.mem, r.value)?
                                },
                            },
                        )?;
                    }
                }
            }
            branches.push(b);
        }
        if total != count {
            return Err(Error::Corrupt);
        }
        let base = if let Some(g) = &self.graph {
            Some(Base::packed(
                self.mem.shared(Tree {
                    source: g.source().clone(),
                    branches,
                })?,
                count,
            ))
        } else {
            None
        };
        Ok(Collection {
            revision,
            records,
            base,
        })
    }
}
