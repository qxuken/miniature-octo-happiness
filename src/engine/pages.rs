use super::*;
use crate::storage::codec::packed::Tree;
use crate::storage::codec::paging::{Directory, Graph};

#[derive(Clone)]
pub(crate) enum Backing {
    Legacy(Shared<Directory>),
    Packed(Shared<Tree>),
}
impl Backing {
    fn revision(&self, mem: &Memory, key: &[u8]) -> Result<Option<u64>> {
        match self {
            Self::Legacy(d) => Ok(d.find(mem, key)?.map(|r| r.0)),
            Self::Packed(t) => t.revision(mem, key),
        }
    }
    fn find(&self, mem: &Memory, key: &[u8]) -> Result<Option<Row<'_>>> {
        match self {
            Self::Legacy(d) => Ok(d.find(mem, key)?.map(|(revision, offset)| Row::Disk {
                graph: &d.graph,
                revision,
                offset,
            })),
            Self::Packed(t) => Ok(t.find(mem, key)?.map(Row::Packed)),
        }
    }
    fn visit(&self, mem: &Memory, mut f: impl FnMut(&[u8], Row<'_>) -> Result<()>) -> Result<()> {
        match self {
            Self::Legacy(d) => d.visit(mem, |k, revision, offset| {
                f(
                    k,
                    Row::Disk {
                        graph: &d.graph,
                        revision,
                        offset,
                    },
                )
            }),
            Self::Packed(t) => t.visit(mem, |k, r| f(k, Row::Packed(r))),
        }
    }
}
#[derive(Clone)]
pub(crate) struct Base {
    pub(crate) directory: Backing,
    pub(crate) deleted: Map<()>,
    len: usize,
}
impl Base {
    pub(crate) fn packed(tree: Shared<Tree>, len: usize) -> Self {
        Self {
            directory: Backing::Packed(tree),
            deleted: Map::new(),
            len,
        }
    }
    pub(crate) fn new(directory: Shared<Directory>, len: usize) -> Self {
        Self {
            directory: Backing::Legacy(directory),
            deleted: Map::new(),
            len,
        }
    }
}
pub(crate) enum Row<'a> {
    Memory(&'a Record),
    Packed(Record),
    Disk {
        graph: &'a Graph,
        offset: u64,
        revision: u64,
    },
}
impl Row<'_> {
    pub(crate) fn revision(&self) -> u64 {
        match self {
            Self::Memory(r) => r.revision,
            Self::Packed(r) => r.revision,
            Self::Disk { revision, .. } => *revision,
        }
    }
    pub(crate) fn load(&self, mem: &Memory) -> Result<Value> {
        match self {
            Self::Memory(r) => Ok(r.value.clone()),
            Self::Packed(r) => Ok(r.value.clone()),
            Self::Disk { graph, offset, .. } => graph.load(mem, *offset),
        }
    }
}
impl Collection {
    pub(crate) fn len(&self) -> usize {
        self.base.as_ref().map_or(self.records.len(), |b| b.len)
    }
    fn row(&self, mem: &Memory, key: &[u8]) -> Result<Option<Row<'_>>> {
        if let Some(r) = self.records.get(key) {
            return Ok(Some(Row::Memory(r)));
        }
        let Some(b) = &self.base else { return Ok(None) };
        if b.deleted.get(key).is_some() {
            return Ok(None);
        }
        b.directory.find(mem, key)
    }
    pub(super) fn lookup(&self, mem: &Memory, key: &[u8]) -> Result<Option<Value>> {
        self.row(mem, key)?.map(|r| r.load(mem)).transpose()
    }
    pub(super) fn record_revision(&self, mem: &Memory, key: &[u8]) -> Result<Option<u64>> {
        if let Some(r) = self.records.get(key) {
            return Ok(Some(r.revision));
        }
        match &self.base {
            Some(b) if b.deleted.get(key).is_none() => b.directory.revision(mem, key),
            _ => Ok(None),
        }
    }
    // Only unpublished collection clones are mutated. Allocation or I/O failure drops the clone.
    pub(super) fn edit(
        &mut self,
        mem: &Memory,
        key: &[u8],
        value: Option<&Value>,
        revision: u64,
    ) -> Result<()> {
        if let Some(b) = &mut self.base {
            let overlay = self.records.get(key).is_some();
            let visible = b.deleted.get(key).is_none();
            let from_disk = visible
                && (value.is_none() || !overlay)
                && b.directory.revision(mem, key)?.is_some();
            let exists = overlay || from_disk;
            if value.is_some() && !exists {
                if b.len >= mem.limits().max_entries {
                    return Err(Error::Limit);
                }
                b.len += 1;
            }
            if value.is_none() && exists {
                b.len -= 1;
                if from_disk {
                    b.deleted = b.deleted.set(mem, key, ())?;
                }
            }
            if value.is_some() {
                b.deleted = b.deleted.remove(mem, key)?;
            }
        }
        self.records = match value {
            Some(value) => self.records.set(
                mem,
                key,
                Record {
                    revision,
                    value: value.clone(),
                },
            )?,
            None => self.records.remove(mem, key)?,
        };
        Ok(())
    }
    // Merge the already sorted disk pages with the usually small set of edited records.
    pub(crate) fn visit_paged(
        &self,
        mem: &Memory,
        mut visit: impl FnMut(&[u8], Row<'_>) -> Result<()>,
    ) -> Result<()> {
        let b = self.base.as_ref().ok_or(Error::Invalid)?;
        let mut edits = ChargedVec::new();
        mem.reserve(&mut edits, self.records.len())?;
        self.records.visit(&mut |k, v| {
            edits.push((k, v));
            Ok(())
        })?;
        edits.sort_unstable_by(|a, b| a.0.cmp(b.0));
        let mut edits = edits.as_slice().iter().copied().peekable();
        b.directory.visit(mem, |key, row| {
            while edits.peek().is_some_and(|(k, _)| *k < key) {
                let (k, v) = edits.next().unwrap();
                visit(k, Row::Memory(v))?;
            }
            if edits.peek().is_some_and(|(k, _)| *k == key) {
                let (_, v) = edits.next().unwrap();
                visit(key, Row::Memory(v))?;
            } else if b.deleted.get(key).is_none() {
                visit(key, row)?;
            }
            Ok(())
        })?;
        for (k, v) in edits {
            visit(k, Row::Memory(v))?;
        }
        Ok(())
    }
    pub(super) fn visit_values(
        &self,
        mem: &Memory,
        mut visit: impl FnMut(&[u8], &Value) -> Result<()>,
    ) -> Result<()> {
        if self.base.is_none() {
            for (k, r) in self
                .records
                .borrowed_entries(mem, mem.limits().max_transaction_bytes)?
            {
                visit(k, &r.value)?;
            }
            return Ok(());
        }
        // Consecutive aliases are common in directories. Keep one value alive
        // during traversal and release it before loading a different object.
        let mut last: Option<(u64, Value)> = None;
        self.visit_paged(mem, |key, row| match row {
            Row::Memory(r) => visit(key, &r.value),
            Row::Packed(r) => visit(key, &r.value),
            Row::Disk { graph, offset, .. } => {
                if let Some((old, v)) = &last
                    && *old == offset
                {
                    return visit(key, v);
                }
                drop(last.take());
                let v = graph.load(mem, offset)?;
                visit(key, &v)?;
                last = Some((offset, v));
                Ok(())
            }
        })
    }
    fn paged_rows<K>(
        &self,
        mem: &Memory,
        copy: impl Fn(&[u8]) -> Result<K>,
    ) -> Result<ChargedVec<(K, Value)>> {
        let limit = mem.limits().max_transaction_bytes;
        let mut used = self
            .len()
            .checked_mul(std::mem::size_of::<(Vec<u8>, Value)>())
            .ok_or(Error::Limit)?;
        if used > limit {
            return Err(Error::Limit);
        }
        self.visit_paged(mem, |k, _| {
            used = used
                .checked_add(k.len())
                .filter(|n| *n <= limit)
                .ok_or(Error::Limit)?;
            Ok(())
        })?;
        let mut out = ChargedVec::new();
        mem.reserve(&mut out, self.len())?;
        self.visit_values(mem, |k, value| {
            out.push((copy(k)?, value.clone()));
            Ok(())
        })?;
        Ok(out)
    }
    pub(super) fn rows_owned(&self, mem: &Memory) -> Result<OwnedRows> {
        if self.base.is_some() {
            self.paged_rows(mem, |k| mem.copy(k))
        } else {
            self.records.materialize_owned(mem, |r| r.value.clone())
        }
    }
    pub(super) fn rows(&self, mem: &Memory) -> Result<Rows> {
        if self.base.is_some() {
            self.paged_rows(mem, |k| mem.copy_external(k))?
                .into_external()
        } else {
            self.records.materialize(mem, |r| r.value.clone())
        }
    }
}
