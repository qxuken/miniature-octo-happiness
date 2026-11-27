use super::*;

type Edit<'a> = (&'a [u8], Option<&'a Record>);

// A builder is scoped to a single collection. Only the input leaf containing an
// edit is decoded; undisturbed siblings and branches keep their physical offsets.
struct Builder<'a, 'm> {
    encoder: &'a mut Encoder<'m>,
    leaf: Writer<'m>,
    row: Writer<'m>,
    first: std::ops::Range<usize>,
    last: std::ops::Range<usize>,
    born: u64,
    count: usize,
    max_revision: u64,
    leaves: ChargedVec<Shared<Leaf>>,
    branches: ChargedVec<Shared<Branch>>,
}
impl<'a, 'm> Builder<'a, 'm> {
    fn new(e: &'a mut Encoder<'m>, born: u64) -> Self {
        let mem = e.mem;
        Self {
            born,
            encoder: e,
            leaf: Writer::new(mem),
            row: Writer::new(mem),
            first: 0..0,
            last: 0..0,
            count: 0,
            max_revision: 0,
            leaves: ChargedVec::new(),
            branches: ChargedVec::new(),
        }
    }
    fn finish_branch(&mut self) -> Result<()> {
        if self.leaves.is_empty() {
            return Ok(());
        }
        let mut payload = Writer::new(self.encoder.mem);
        for l in &self.leaves {
            payload.int(l.offset)?;
        }
        let (offset, end) = self
            .encoder
            .object_born(BRANCH, &payload.bytes, self.born)?;
        let leaves = std::mem::replace(&mut self.leaves, ChargedVec::new());
        let b = self.encoder.mem.shared(Branch {
            leaves,
            offset,
            end,
            born: self.born,
        })?;
        self.push_branch(b)
    }
    fn push_branch(&mut self, b: Shared<Branch>) -> Result<()> {
        self.encoder.mem.reserve_geometric(
            &mut self.branches,
            1,
            if self.encoder.packed.as_ref().is_some_and(|s| s.3.extended()) {
                self.encoder.mem.limits().max_entries
            } else {
                MAX_OBJECTS
            },
        )?;
        self.branches.push(b);
        Ok(())
    }
    fn push_leaf(&mut self, l: Shared<Leaf>) -> Result<()> {
        if self.leaves.len() == FANOUT {
            self.finish_branch()?;
        }
        self.encoder
            .mem
            .reserve_geometric(&mut self.leaves, 1, FANOUT)?;
        self.leaves.push(l);
        Ok(())
    }
    fn finish_leaf(&mut self) -> Result<()> {
        if self.count == 0 {
            return Ok(());
        }
        let raw = &self.leaf.bytes;
        let (offset, end) = self.encoder.object_born(LEAF, raw, self.born)?;
        let mem = self.encoder.mem;
        let leaf = mem.shared(Leaf {
            first: mem.copy(&raw[self.first.clone()])?,
            last: mem.copy(&raw[self.last.clone()])?,
            offset,
            end,
            len: raw.len(),
            digest: crc(raw),
            count: self.count,
            max_revision: self.max_revision,
            born: self.born,
        })?;
        self.count = 0;
        self.max_revision = 0;
        self.leaf.bytes.clear();
        self.push_leaf(leaf)
    }
    fn emit(&mut self, key: &[u8], revision: u64, raw: &[u8]) -> Result<()> {
        let row_len = 16usize
            .checked_add(key.len())
            .and_then(|n| n.checked_add(raw.len()))
            .ok_or(Error::Limit)?;
        if self.count > 0 && self.leaf.bytes.len() + row_len > LEAF_BYTES {
            self.finish_leaf()?;
        }
        self.leaf.short(key.len())?;
        let start = self.leaf.bytes.len();
        self.leaf.add(key)?;
        self.last = start..self.leaf.bytes.len();
        if self.count == 0 {
            self.first = self.last.clone();
        }
        self.leaf.int(revision)?;
        self.leaf.small_bytes(raw)?;
        self.count += 1;
        self.max_revision = self.max_revision.max(revision);
        Ok(())
    }
    fn edit(&mut self, (key, record): Edit<'_>) -> Result<()> {
        if let Some(r) = record {
            if self.encoder.packed.as_ref().is_some_and(|s| s.3.extended())
                && r.value.kind() == Kind::Bytes
                && r.value.as_bytes()?.len() > LEAF_BYTES
            {
                let blob = Blob::write(self.encoder, r.value.as_bytes()?, r.revision)?;
                return self.emit(key, r.revision, &blob.bytes());
            }
            let mut row = std::mem::replace(&mut self.row, Writer::new(self.encoder.mem));
            row.bytes.clear();
            encode_value(&mut row, &r.value)?;
            self.emit(key, r.revision, &row.bytes)?;
            self.row = row;
        }
        Ok(())
    }
    fn rewrite(&mut self, src: &Source, l: &Leaf, mut edits: &[Edit<'_>]) -> Result<()> {
        let mem = self.encoder.mem;
        let raw = src.access(|io| l.read(io, mem))?;
        let mut rows = Cursor::new(&raw);
        while rows.pos < raw.len() {
            let r = entry(mem, &mut rows)?;
            while let Some((&e, rest)) = edits.split_first()
                && e.0 < r.key
            {
                self.edit(e)?;
                edits = rest;
            }
            if let Some((&e, rest)) = edits.split_first()
                && e.0 == r.key
            {
                self.edit(e)?;
                edits = rest;
            } else {
                self.emit(r.key, r.revision, r.value)?;
            }
        }
        for &e in edits {
            self.edit(e)?;
        }
        self.finish_leaf()
    }
    fn patch(&mut self, t: &Tree, mut edits: &[Edit<'_>]) -> Result<()> {
        for (i, b) in t.branches.iter().enumerate() {
            let n = t.branches.get(i + 1).map_or(edits.len(), |next| {
                edits.partition_point(|e| e.0 < next.first())
            });
            let (mut changes, rest) = edits.split_at(n);
            edits = rest;
            if changes.is_empty() {
                self.push_branch(b.clone())?;
                continue;
            }
            for (i, l) in b.leaves.iter().enumerate() {
                let n = b.leaves.get(i + 1).map_or(changes.len(), |next| {
                    changes.partition_point(|e| e.0 < next.first.as_slice())
                });
                let (in_leaf, rest) = changes.split_at(n);
                changes = rest;
                if in_leaf.is_empty() {
                    self.push_leaf(l.clone())?;
                } else {
                    self.rewrite(&t.source, l, in_leaf)?;
                }
            }
            self.finish_branch()?;
        }
        // The empty tree has no key boundaries yet.
        for &e in edits {
            self.edit(e)?;
        }
        Ok(())
    }
}
impl Encoder<'_> {
    fn packed_collection(&mut self, c: &Collection, source: &Source) -> Result<Shared<Tree>> {
        let mem = self.mem;
        let old = c.base.as_ref().and_then(|base| match &base.directory {
            Backing::Packed(t) if t.source.id() == source.id() => Some((t, &base.deleted)),
            _ => None,
        });
        if let Some((t, deleted)) = old
            && c.records.len() == 0
            && deleted.len() == 0
        {
            return Ok(t.clone());
        }
        if let Some(base) = &c.base
            && let Backing::Packed(t) = &base.directory
            && old.is_none()
            && t.source.3.extended() == source.3.extended()
            && c.records.len() == 0
            && base.deleted.len() == 0
        {
            return self.copy_tree(t, source);
        }
        let mut build = Builder::new(self, c.revision);
        if c.base.is_some() && old.is_none() {
            c.visit_paged(mem, |k, r| {
                let revision = r.revision();
                let value = r.load(mem)?;
                build.edit((k, Some(&Record { revision, value })))
            })?;
        } else {
            let mut edits = ChargedVec::new();
            mem.reserve(
                &mut edits,
                c.records.len() + old.map_or(0, |(_, d)| d.len()),
            )?;
            c.records.visit(&mut |k, r| {
                edits.push((k, Some(r)));
                Ok(())
            })?;
            if let Some((_, deleted)) = old {
                deleted.visit(&mut |k, _| {
                    edits.push((k, None));
                    Ok(())
                })?;
            }
            edits.sort_unstable_by(|a, b| a.0.cmp(b.0));
            if let Some((t, _)) = old {
                build.patch(t, &edits)?;
            } else {
                for e in edits {
                    build.edit(e)?;
                }
            }
        }
        build.finish_leaf()?;
        build.finish_branch()?;
        mem.shared(Tree {
            source: source.clone(),
            branches: build.branches,
        })
    }
    pub(in crate::storage::codec) fn packed_data(
        &mut self,
        head: &Head,
        next_collection: u64,
        source: Source,
    ) -> Result<u64> {
        let mem = self.mem;
        let mut rows = ChargedVec::new();
        mem.reserve(&mut rows, head.collections.len())?;
        head.collections.visit(&mut |k, c| {
            rows.push((
                u64::from_le_bytes(k.try_into().map_err(|_| Error::Invalid)?),
                c,
            ));
            Ok(())
        })?;
        rows.sort_unstable_by_key(|r| r.0);
        let mut w = Writer::new(mem);
        w.int(head.data_revision)?;
        w.int(next_collection)?;
        w.int(rows.len() as u64)?;
        let mut collections = Map::new();
        for (id, c) in rows {
            let tree = self.packed_collection(c, &source)?;
            w.int(id)?;
            w.int(c.revision)?;
            w.int(c.len() as u64)?;
            w.int(tree.branches.len() as u64)?;
            for b in &tree.branches {
                w.int(b.offset)?;
            }
            if head.revision == self.revision {
                collections = collections.set(
                    mem,
                    &id.to_le_bytes(),
                    Collection {
                        records: Map::new(),
                        revision: c.revision,
                        base: Some(Base::packed(tree, c.len())),
                    },
                )?;
            }
        }
        let (offset, _) = self.object_born(DATA, &w.bytes, head.data_revision)?;
        if head.revision == self.revision {
            self.rebased = Some(mem.shared(Head {
                revision: head.revision,
                data_revision: head.data_revision,
                collections,
            })?);
        }
        self.cache_data(head.data_revision, next_collection, offset)
    }
}

// Offsets are local to an image. During a fresh copy, retain the relocation
// table across the current and historical catalogs so their common pages are
// emitted once. It is dropped before destination verification.
pub(in crate::storage::codec) struct Relocation {
    source: usize,
    pages: extents::Offsets<VerifiedPage>,
    blobs: extents::Offsets<Blob>,
}
impl Relocation {
    pub(in crate::storage::codec) fn new() -> Self {
        Self {
            source: 0,
            pages: extents::Offsets::new(),
            blobs: extents::Offsets::new(),
        }
    }
    fn select(&mut self, source: usize) {
        if self.source != source {
            self.pages = extents::Offsets::new();
            self.blobs = extents::Offsets::new();
            self.source = source;
        }
    }
}

impl Encoder<'_> {
    fn relocate_blobs(&mut self, src: &Source, raw: &mut [u8]) -> Result<()> {
        let mut pos = 0;
        while pos < raw.len() {
            let mut cursor = Cursor::new(&raw[pos..]);
            let entry = entry(self.mem, &mut cursor)?;
            let blob = Blob::parse(entry.value)?;
            let born = entry.revision;
            pos += cursor.pos;
            if let Some(blob) = blob {
                let new = if let Some(copy) = self.relocation.blobs.get(blob.offset) {
                    *copy
                } else {
                    let data = src.access(|io| blob.read(io, self.mem))?;
                    let copy = Blob::write(self, &data, born)?;
                    self.relocation.blobs.insert(self.mem, blob.offset, copy)?;
                    copy
                };
                let descriptor = new.bytes();
                raw[pos - descriptor.len()..pos].copy_from_slice(&descriptor);
            }
        }
        Ok(())
    }
    fn copy_tree(&mut self, tree: &Tree, destination: &Source) -> Result<Shared<Tree>> {
        let mem = self.mem;
        let mut branches = ChargedVec::new();
        mem.reserve(&mut branches, tree.branches.len())?;
        self.relocation.select(tree.source.id());
        for b in &tree.branches {
            if let Some(VerifiedPage::Branch(old)) = self.relocation.pages.get(b.offset) {
                branches.push(old.clone());
                continue;
            }
            let mut leaves = ChargedVec::new();
            mem.reserve(&mut leaves, b.leaves.len())?;
            let mut pointers = Writer::new(mem);
            for l in &b.leaves {
                let copy =
                    if let Some(VerifiedPage::Leaf(old)) = self.relocation.pages.get(l.offset) {
                        old.clone()
                    } else {
                        let mut raw = tree.source.access(|io| l.read(io, mem))?;
                        if tree.source.3.extended() {
                            self.relocate_blobs(&tree.source, &mut raw)?;
                        }

                        let (offset, end) = self.object_born(LEAF, &raw, l.born)?;
                        let copy = mem.shared(Leaf {
                            first: mem.copy(&l.first)?,
                            last: mem.copy(&l.last)?,
                            offset,
                            end,
                            len: l.len,
                            digest: crc(&raw),
                            count: l.count,
                            max_revision: l.max_revision,
                            born: l.born,
                        })?;
                        self.relocation.pages.insert(
                            mem,
                            l.offset,
                            VerifiedPage::Leaf(copy.clone()),
                        )?;
                        copy
                    };
                pointers.int(copy.offset)?;
                leaves.push(copy);
            }
            let (offset, end) = self.object_born(BRANCH, &pointers.bytes, b.born)?;
            let copy = mem.shared(Branch {
                offset,
                end,
                born: b.born,
                leaves,
            })?;
            self.relocation
                .pages
                .insert(mem, b.offset, VerifiedPage::Branch(copy.clone()))?;
            branches.push(copy);
        }
        mem.shared(Tree {
            source: destination.clone(),
            branches,
        })
    }
}
