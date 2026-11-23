use crate::memory::ChargedVec;
use crate::{
    Error, Result,
    memory::{Memory, Shared},
};

type Key = Shared<ChargedVec<u8>>;
type Children<V> = [Option<Shared<MapNode<V>>>; 16];
pub(crate) type BorrowedEntries<'a, V> = ChargedVec<(&'a [u8], &'a V)>;
enum MapNode<V: Clone> {
    Leaf {
        hash: u64,
        entries: ChargedVec<(Key, V)>,
    },
    Branch(Children<V>),
}
pub(crate) struct Map<V: Clone> {
    root: Option<Shared<MapNode<V>>>,
    len: usize,
}
impl<V: Clone> Clone for Map<V> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            len: self.len,
        }
    }
}
pub(crate) fn hash(key: &[u8]) -> u64 {
    key.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn slot(hash: u64, level: usize) -> usize {
    ((hash >> (level * 4)) & 15) as usize
}
impl<V: Clone> Map<V> {
    pub(crate) fn new() -> Self {
        Self { root: None, len: 0 }
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn get(&self, key: &[u8]) -> Option<&V> {
        self.get_hashed(key, hash(key))
    }
    fn get_hashed(&self, key: &[u8], hash: u64) -> Option<&V> {
        let mut node = self.root.as_deref()?;
        let mut level = 0;
        loop {
            match node {
                MapNode::Leaf { hash: h, entries } => {
                    return if *h == hash {
                        entries
                            .iter()
                            .find(|(k, _)| k.as_slice() == key)
                            .map(|(_, v)| v)
                    } else {
                        None
                    };
                }
                MapNode::Branch(children) => {
                    node = children[slot(hash, level)].as_deref()?;
                    level += 1;
                }
            }
        }
    }
    pub(crate) fn set(&self, mem: &Memory, key: &[u8], value: V) -> Result<Self> {
        self.set_hashed(mem, key, value, hash(key))
    }
    fn set_hashed(&self, mem: &Memory, key: &[u8], value: V, hash: u64) -> Result<Self> {
        let exists = self.get_hashed(key, hash).is_some();
        if !exists && self.len >= mem.limits().max_entries {
            return Err(Error::Limit);
        }
        let key = mem.shared(mem.copy(key)?)?;
        let root = insert(mem, self.root.as_ref(), key, value, hash, 0)?;
        Ok(Self {
            root: Some(root),
            len: self.len + usize::from(!exists),
        })
    }
    pub(crate) fn remove(&self, mem: &Memory, key: &[u8]) -> Result<Self> {
        if self.get(key).is_none() {
            return Ok(self.clone());
        }
        Ok(Self {
            root: remove(mem, self.root.as_ref().unwrap(), key, hash(key), 0)?,
            len: self.len - 1,
        })
    }
    /// Materialize owned binary-ordered keys and projected values. Preflight the
    /// complete output buffer/key budget before allocating or cloning handles.
    pub(crate) fn materialize<T>(
        &self,
        mem: &Memory,
        project: impl Fn(&V) -> T,
    ) -> Result<Vec<(Vec<u8>, T)>> {
        self.materialize_with(mem, project, |key| mem.copy_external(key))?
            .into_external()
    }
    pub(crate) fn materialize_owned<T>(
        &self,
        mem: &Memory,
        project: impl Fn(&V) -> T,
    ) -> Result<ChargedVec<(ChargedVec<u8>, T)>> {
        self.materialize_with(mem, project, |key| mem.copy(key))
    }
    pub(crate) fn borrowed_entries(
        &self,
        mem: &Memory,
        row_budget: usize,
    ) -> Result<BorrowedEntries<'_, V>> {
        self.check_row_budget::<V>(row_budget)?;
        self.collect_ordered(mem, |key, value| Ok((key, value)))
    }
    fn materialize_with<T, K: AsRef<[u8]>>(
        &self,
        mem: &Memory,
        project: impl Fn(&V) -> T,
        copy_key: impl Fn(&[u8]) -> Result<K>,
    ) -> Result<ChargedVec<(K, T)>> {
        self.check_row_budget::<T>(mem.limits().max_transaction_bytes)?;
        self.collect_ordered(mem, |key, value| Ok((copy_key(key)?, project(value))))
    }
    fn check_row_budget<T>(&self, budget: usize) -> Result<()> {
        let mut size = self
            .len
            .checked_mul(std::mem::size_of::<(Vec<u8>, T)>())
            .ok_or(Error::Limit)?;
        if size > budget {
            return Err(Error::Limit);
        }
        self.visit(&mut |key, _| {
            size = size.checked_add(key.len()).ok_or(Error::Limit)?;
            if size > budget {
                return Err(Error::Limit);
            }
            Ok(())
        })
    }
    fn collect_ordered<'a, K: AsRef<[u8]>, T>(
        &'a self,
        mem: &Memory,
        row: impl Fn(&'a [u8], &'a V) -> Result<(K, T)>,
    ) -> Result<ChargedVec<(K, T)>> {
        let mut out = ChargedVec::new();
        mem.reserve(&mut out, self.len)?;
        self.visit(&mut |key, value| {
            out.push(row(key, value)?);
            Ok(())
        })?;
        out.sort_unstable_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()));
        Ok(out)
    }
    pub(crate) fn visit<'a>(
        &'a self,
        f: &mut impl FnMut(&'a [u8], &'a V) -> Result<()>,
    ) -> Result<()> {
        fn walk<'a, V: Clone>(
            node: &'a MapNode<V>,
            f: &mut impl FnMut(&'a [u8], &'a V) -> Result<()>,
        ) -> Result<()> {
            match node {
                MapNode::Leaf { entries, .. } => {
                    for (key, value) in entries {
                        f(key, value)?;
                    }
                }
                MapNode::Branch(children) => {
                    for child in children.iter().flatten() {
                        walk(child, f)?;
                    }
                }
            }
            Ok(())
        }
        if let Some(root) = &self.root {
            walk(root, f)?;
        }
        Ok(())
    }
}
fn leaf<V: Clone>(mem: &Memory, key: Key, value: V, hash: u64) -> Result<Shared<MapNode<V>>> {
    let mut entries = ChargedVec::new();
    mem.reserve(&mut entries, 1)?;
    entries.push((key, value));
    mem.shared(MapNode::Leaf { hash, entries })
}
fn insert<V: Clone>(
    mem: &Memory,
    node: Option<&Shared<MapNode<V>>>,
    key: Key,
    value: V,
    hash: u64,
    level: usize,
) -> Result<Shared<MapNode<V>>> {
    let Some(node) = node else {
        return leaf(mem, key, value, hash);
    };
    match &**node {
        MapNode::Leaf {
            hash: old_hash,
            entries,
        } if *old_hash == hash => {
            let index = entries
                .iter()
                .position(|(k, _)| k.as_slice() == key.as_slice());
            let mut copy = ChargedVec::new();
            mem.reserve(&mut copy, entries.len() + usize::from(index.is_none()))?;
            copy.extend(entries.iter().cloned());
            if let Some(i) = index {
                copy[i].1 = value;
            } else {
                copy.push((key, value));
            }
            mem.shared(MapNode::Leaf {
                hash,
                entries: copy,
            })
        }
        MapNode::Leaf { hash: old_hash, .. } => {
            // Distinct 64-bit hashes differ within at most sixteen nibbles.
            let mut children = std::array::from_fn(|_| None);
            let old = slot(*old_hash, level);
            let new = slot(hash, level);
            children[old] = Some(node.clone());
            children[new] = Some(insert(
                mem,
                children[new].as_ref(),
                key,
                value,
                hash,
                level + 1,
            )?);
            mem.shared(MapNode::Branch(children))
        }
        MapNode::Branch(children) => {
            let mut children = children.clone();
            let index = slot(hash, level);
            children[index] = Some(insert(
                mem,
                children[index].as_ref(),
                key,
                value,
                hash,
                level + 1,
            )?);
            mem.shared(MapNode::Branch(children))
        }
    }
}
fn remove<V: Clone>(
    mem: &Memory,
    node: &Shared<MapNode<V>>,
    key: &[u8],
    hash: u64,
    level: usize,
) -> Result<Option<Shared<MapNode<V>>>> {
    match &**node {
        MapNode::Leaf { entries, .. } => {
            if entries.len() == 1 {
                return Ok(None);
            }
            let mut copy = ChargedVec::new();
            mem.reserve(&mut copy, entries.len() - 1)?;
            copy.extend(entries.iter().filter(|(k, _)| k.as_slice() != key).cloned());
            Ok(Some(mem.shared(MapNode::Leaf {
                hash,
                entries: copy,
            })?))
        }
        MapNode::Branch(children) => {
            let mut children = children.clone();
            let index = slot(hash, level);
            children[index] = remove(mem, children[index].as_ref().unwrap(), key, hash, level + 1)?;
            let mut live = children.iter().flatten();
            let Some(first) = live.next() else {
                return Ok(None);
            };
            if live.next().is_none() && matches!(&**first, MapNode::Leaf { .. }) {
                return Ok(Some(first.clone()));
            }
            Ok(Some(mem.shared(MapNode::Branch(children))?))
        }
    }
}

// Indexed radix tree: leaves hold sixteen items, internal nodes sixteen children.
#[derive(Clone)]
enum ListNode<V: Clone> {
    Leaf([Option<V>; 16]),
    Branch([Option<Shared<ListNode<V>>>; 16]),
}
pub(crate) struct List<V: Clone> {
    root: Option<Shared<ListNode<V>>>,
    len: usize,
    height: usize,
}
impl<V: Clone> Clone for List<V> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            len: self.len,
            height: self.height,
        }
    }
}
impl<V: Clone> List<V> {
    pub(crate) fn new() -> Self {
        Self {
            root: None,
            len: 0,
            height: 0,
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn get(&self, index: usize) -> Option<&V> {
        if index >= self.len {
            return None;
        }
        let mut node = self.root.as_deref()?;
        let mut height = self.height;
        loop {
            match node {
                ListNode::Leaf(items) => return items[index & 15].as_ref(),
                ListNode::Branch(children) => {
                    node = children[(index >> (height * 4)) & 15].as_deref()?;
                    height -= 1;
                }
            }
        }
    }
    pub(crate) fn set(&self, mem: &Memory, index: usize, value: V) -> Result<Self> {
        if index >= self.len {
            return Err(Error::Bounds);
        }
        Ok(Self {
            root: Some(list_set(
                mem,
                self.root.as_ref(),
                self.height,
                index,
                value,
            )?),
            len: self.len,
            height: self.height,
        })
    }
    pub(crate) fn push(&self, mem: &Memory, value: V) -> Result<Self> {
        if self.len >= mem.limits().max_entries {
            return Err(Error::Limit);
        }
        let mut root = self.root.clone();
        let mut height = self.height;
        let capacity = 16usize
            .checked_pow((height + 1) as u32)
            .ok_or(Error::Limit)?;
        if self.len == capacity {
            let mut children = std::array::from_fn(|_| None);
            children[0] = root;
            root = Some(mem.shared(ListNode::Branch(children))?);
            height += 1;
        }
        Ok(Self {
            root: Some(list_set(mem, root.as_ref(), height, self.len, value)?),
            len: self.len + 1,
            height,
        })
    }
    pub(crate) fn pop(&self, mem: &Memory) -> Result<Self> {
        if self.len == 0 {
            return Err(Error::Bounds);
        }
        if self.len == 1 {
            return Ok(Self::new());
        }
        let mut root = list_clear(mem, self.root.as_ref().unwrap(), self.height, self.len - 1)?;
        let mut height = self.height;
        while height > 0 && self.len - 1 <= 16usize.pow(height as u32) {
            let ListNode::Branch(children) = &*root else {
                unreachable!()
            };
            root = children[0].as_ref().unwrap().clone();
            height -= 1;
        }
        Ok(Self {
            root: Some(root),
            len: self.len - 1,
            height,
        })
    }
}
fn list_set<V: Clone>(
    mem: &Memory,
    node: Option<&Shared<ListNode<V>>>,
    height: usize,
    index: usize,
    value: V,
) -> Result<Shared<ListNode<V>>> {
    if height == 0 {
        let mut items = match node.map(|n| &**n) {
            Some(ListNode::Leaf(items)) => items.clone(),
            _ => std::array::from_fn(|_| None),
        };
        items[index & 15] = Some(value);
        mem.shared(ListNode::Leaf(items))
    } else {
        let mut children = match node.map(|n| &**n) {
            Some(ListNode::Branch(children)) => children.clone(),
            _ => std::array::from_fn(|_| None),
        };
        let slot = (index >> (height * 4)) & 15;
        children[slot] = Some(list_set(
            mem,
            children[slot].as_ref(),
            height - 1,
            index,
            value,
        )?);
        mem.shared(ListNode::Branch(children))
    }
}
fn list_clear<V: Clone>(
    mem: &Memory,
    node: &Shared<ListNode<V>>,
    height: usize,
    index: usize,
) -> Result<Shared<ListNode<V>>> {
    match &**node {
        ListNode::Leaf(items) => {
            let mut items = items.clone();
            items[index & 15] = None;
            mem.shared(ListNode::Leaf(items))
        }
        ListNode::Branch(children) => {
            let mut children = children.clone();
            let slot = (index >> (height * 4)) & 15;
            if index & ((1 << (height * 4)) - 1) == 0 {
                children[slot] = None;
            } else {
                children[slot] = Some(list_clear(
                    mem,
                    children[slot].as_ref().unwrap(),
                    height - 1,
                    index,
                )?);
            }
            mem.shared(ListNode::Branch(children))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::test_allocator::TestAllocator;
    use std::sync::Arc;

    #[test]
    fn map_capacity_and_collision_lookup_have_independent_controls() {
        let mem = Memory::new(crate::Limits {
            max_entries: 1,
            ..Default::default()
        })
        .unwrap();
        let empty = Map::new();
        let full = empty.set_hashed(&mem, b"a", 1, 0).unwrap();
        assert_eq!(full.len(), 1);
        // At capacity, an existing key is replaceable but another key is not.
        let replacement = full.set_hashed(&mem, b"a", 2, 0).unwrap();
        assert_eq!(replacement.len(), 1);
        assert_eq!(replacement.get_hashed(b"a", 0), Some(&2));
        assert!(matches!(
            full.set_hashed(&mem, b"b", 3, 0),
            Err(Error::Limit)
        ));
        assert_eq!(full.get_hashed(b"a", 0), Some(&1));
        assert_eq!(
            full.get_hashed(b"b", 0),
            None,
            "matching hash, different key"
        );
        assert_eq!(
            full.get_hashed(b"a", 1),
            None,
            "different hash, matching key"
        );
        assert_eq!(empty.get_hashed(b"a", 0), None);
        let removed = remove(&mem, full.root.as_ref().unwrap(), b"a", 0, 0).unwrap();
        assert!(removed.is_none());
        assert_eq!(full.get_hashed(b"a", 0), Some(&1));
    }

    #[test]
    fn full_hash_collisions_and_long_prefixes() {
        let mem = Memory::new(Default::default()).unwrap();
        let a = Map::new().set_hashed(&mem, b"a", 1, 0).unwrap();
        let b = a.set_hashed(&mem, b"b", 2, 0).unwrap();
        let c = b.set_hashed(&mem, b"c", 3, 1 << 60).unwrap();
        assert_eq!(c.get_hashed(b"a", 0), Some(&1));
        assert_eq!(c.get_hashed(b"b", 0), Some(&2));
        assert_eq!(c.get_hashed(b"c", 1 << 60), Some(&3));
        assert_eq!(a.get_hashed(b"b", 0), None);
        // Use the internal removal function with the deliberately injected hash.
        let root = remove(&mem, c.root.as_ref().unwrap(), b"b", 0, 0)
            .unwrap()
            .unwrap();
        let reduced = Map {
            root: Some(root),
            len: 2,
        };
        assert_eq!(reduced.get_hashed(b"a", 0), Some(&1));
        assert_eq!(reduced.get_hashed(b"b", 0), None);
    }

    #[test]
    fn map_edit_refusals_keep_collision_and_removal_roots() {
        use std::collections::BTreeMap;

        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Default::default());
        let mut colliding = Map::new();
        let mut natural = Map::new();
        let mut original = BTreeMap::new();
        for (key, value, hash) in [(b"a", 1, 0), (b"b", 2, 0), (b"c", 3, 16), (b"d", 4, 1)] {
            colliding = colliding.set_hashed(&mem, key, value, hash).unwrap();
            natural = natural.set(&mem, key, value).unwrap();
            original.insert(key.to_vec(), value);
        }
        let rows = |map: &Map<i32>| {
            let mut found = BTreeMap::new();
            map.visit(&mut |key, value| {
                assert!(found.insert(key.to_vec(), *value).is_none());
                Ok(())
            })
            .unwrap();
            assert_eq!(map.len(), found.len());
            found
        };
        let unchanged = allocator
            .with_refusal(0, || natural.remove(&mem, b"missing"))
            .unwrap();
        assert!(
            unchanged
                .root
                .as_ref()
                .unwrap()
                .ptr_eq(natural.root.as_ref().unwrap())
        );

        let add = || colliding.set_hashed(&mem, b"e", 5, 0);
        let replace = || colliding.set_hashed(&mem, b"a", 9, 0);
        let remove_collision = || {
            Ok(Map {
                root: remove(&mem, colliding.root.as_ref().unwrap(), b"b", 0, 0)?,
                len: colliding.len() - 1,
            })
        };
        let remove_public = || natural.remove(&mem, b"b");
        let baseline = (mem.usage().0, allocator.live_bytes());
        for (edit, key, value) in [
            (&add as &dyn Fn() -> Result<Map<i32>>, b"e", Some(5)),
            (&replace, b"a", Some(9)),
            (&remove_collision, b"b", None),
            (&remove_public, b"b", None),
        ] {
            let mut expected = original.clone();
            if let Some(value) = value {
                expected.insert(key.to_vec(), value);
            } else {
                expected.remove(key.as_slice());
            }
            let mut completed = false;
            for allowance in 0..40 {
                match allocator.with_refusal(allowance, edit) {
                    Ok(next) => {
                        assert!(allowance > 0);
                        assert_eq!(rows(&next), expected);
                        completed = true;
                    }
                    Err(error) => assert_eq!(error, Error::OutOfMemory),
                }
                assert_eq!(rows(&natural), original);
                assert_eq!(rows(&colliding), original);
                assert_eq!((mem.usage().0, allocator.live_bytes()), baseline);
                if completed {
                    break;
                }
            }
            assert!(completed);
        }
    }

    #[test]
    fn ordered_row_budget_and_allocation_failures_preserve_tree() {
        assert_eq!(Map::<u64>::new().check_row_budget::<u64>(0), Ok(()));
        let source = Memory::new(Default::default()).unwrap();
        let map = Map::new()
            .set(&source, b"z", 1u64)
            .unwrap()
            .set(&source, b"\xff\0", 2)
            .unwrap()
            .set(&source, b"\0", 3)
            .unwrap();
        let headers = 3 * std::mem::size_of::<(Vec<u8>, u64)>();
        let exact = headers + 4;
        let allocator = Arc::new(TestAllocator::default());
        for cap in [headers - 1, exact - 1] {
            let mem = allocator.memory(crate::Limits {
                max_transaction_bytes: cap,
                ..Default::default()
            });
            assert!(matches!(
                allocator.with_refusal(0, || map.materialize_owned::<u64>(&mem, |_| panic!(
                    "project called before budget approval"
                ))),
                Err(Error::Limit)
            ));
            assert!(matches!(
                allocator.with_refusal(0, || map.borrowed_entries(&mem, cap)),
                Err(Error::Limit)
            ));
            assert_eq!(mem.usage(), (0, 0));
        }
        let mem = allocator.memory(crate::Limits {
            max_transaction_bytes: exact,
            ..Default::default()
        });
        let before = (mem.usage().0, allocator.live_bytes());
        assert!(matches!(
            allocator.with_refusal(0, || map.materialize(&mem, |v| *v)),
            Err(Error::OutOfMemory)
        ));
        assert!(matches!(
            allocator.with_refusal(0, || map.borrowed_entries(&mem, exact)),
            Err(Error::OutOfMemory)
        ));
        let expected = [
            (b"\0".as_slice(), 3),
            (b"z".as_slice(), 1),
            (b"\xff\0".as_slice(), 2),
        ];
        for allowance in 0..=4 {
            let result = allocator.with_refusal(allowance, || map.materialize_owned(&mem, |v| *v));
            if allowance == 4 {
                let rows = result.unwrap();
                assert!(
                    rows.iter()
                        .map(|(key, value)| (key.as_slice(), *value))
                        .eq(expected)
                );
            } else {
                assert!(matches!(result, Err(Error::OutOfMemory)));
            }
            assert_eq!((mem.usage().0, allocator.live_bytes()), before);
        }
        let borrowed = map.borrowed_entries(&mem, exact).unwrap();
        assert!(
            borrowed
                .iter()
                .map(|(key, value)| (*key, **value))
                .eq(expected)
        );
        for (key, value) in expected {
            assert_eq!(map.get(key), Some(&value));
        }
    }

    #[test]
    fn untouched_branches_are_shared_and_path_failures_preserve_roots() {
        let allocator = Arc::new(TestAllocator::default());
        let mem = allocator.memory(Default::default());
        let map = Map::new()
            .set_hashed(&mem, b"a", 1, 0)
            .unwrap()
            .set_hashed(&mem, b"b", 2, 1)
            .unwrap();
        let edited = map.set_hashed(&mem, b"a", 9, 0).unwrap();
        let (MapNode::Branch(old), MapNode::Branch(new)) = (
            &**map.root.as_ref().unwrap(),
            &**edited.root.as_ref().unwrap(),
        ) else {
            panic!()
        };
        assert!(old[1].as_ref().unwrap().ptr_eq(new[1].as_ref().unwrap()));
        assert!(!old[0].as_ref().unwrap().ptr_eq(new[0].as_ref().unwrap()));

        let mut list = List::new();
        for i in 0..258 {
            list = list.push(&mem, i).unwrap();
        }
        let full = list.pop(&mem).unwrap().pop(&mem).unwrap();
        let edited = list.set(&mem, 0, 999).unwrap();
        let (ListNode::Branch(old), ListNode::Branch(new)) = (
            &**list.root.as_ref().unwrap(),
            &**edited.root.as_ref().unwrap(),
        ) else {
            panic!()
        };
        assert!(old[1].as_ref().unwrap().ptr_eq(new[1].as_ref().unwrap()));
        assert!(!old[0].as_ref().unwrap().ptr_eq(new[0].as_ref().unwrap()));
        let baseline = (mem.usage().0, allocator.live_bytes());
        for operation in 0..4 {
            let mut succeeded = false;
            for allowance in 0..80 {
                let result = allocator.with_refusal(allowance, || match operation {
                    0 => map.set_hashed(&mem, b"c", 3, 1 << 60).map(drop),
                    1 => list.set(&mem, 17, 999).map(drop),
                    2 => list.pop(&mem).map(drop),
                    _ => full.push(&mem, 999).map(drop),
                });
                assert_eq!((mem.usage().0, allocator.live_bytes()), baseline);
                assert_eq!(map.get_hashed(b"a", 0), Some(&1));
                assert_eq!(map.get_hashed(b"c", 1 << 60), None);
                assert_eq!(list.get(17), Some(&17));
                assert_eq!(list.get(256), Some(&256));
                match result {
                    Ok(()) => {
                        succeeded = true;
                        break;
                    }
                    Err(e) => assert_eq!(e, Error::OutOfMemory),
                }
            }
            assert!(succeeded);
        }
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;
    #[test]
    fn branch_collapse_distinguishes_one_child_and_leaf_independently() {
        let mem = Memory::new(Default::default()).unwrap();
        let base = Map::new()
            .set_hashed(&mem, b"a", 1, 0)
            .unwrap()
            .set_hashed(&mem, b"b", 2, 16)
            .unwrap()
            .set_hashed(&mem, b"c", 3, 1)
            .unwrap();
        let remove_key = |map: &Map<i32>, key: &[u8], hash| Map {
            root: remove(&mem, map.root.as_ref().unwrap(), key, hash, 0).unwrap(),
            len: map.len - 1,
        };
        assert_eq!(base.get_hashed(b"missing", 2), None);
        let one_branch = remove_key(&base, b"c", 1); // one child, not leaf
        assert!(matches!(
            &**one_branch.root.as_ref().unwrap(),
            MapNode::Branch(_)
        ));
        let two_leaves = remove_key(&base, b"b", 16); // two children, first leaf
        assert!(matches!(
            &**two_leaves.root.as_ref().unwrap(),
            MapNode::Branch(_)
        ));
        let one_leaf = remove_key(&two_leaves, b"c", 1); // one child and leaf
        assert!(matches!(
            &**one_leaf.root.as_ref().unwrap(),
            MapNode::Leaf { .. }
        ));
        assert_eq!(one_leaf.get_hashed(b"a", 0), Some(&1));
        assert_eq!(base.get_hashed(b"b", 16), Some(&2));
        assert_eq!(one_branch.get_hashed(b"b", 16), Some(&2));
    }
    #[test]
    fn list_capacity_shrink_bounds_and_tail_clear_have_controls() {
        let allocator =
            std::sync::Arc::new(crate::memory::test_allocator::TestAllocator::default());
        let mem = allocator.memory(crate::Limits {
            max_entries: 258,
            ..Default::default()
        });
        assert!(matches!(List::<u64>::new().pop(&mem), Err(Error::Bounds)));
        let mut list = List::new();
        for n in 0..258 {
            list = list.push(&mem, n).unwrap();
            if [1, 2, 16, 17, 18, 256, 257, 258].contains(&list.len()) {
                let old = list.clone();
                let popped = if list.len() == 1 {
                    let empty = allocator.with_refusal(0, || list.pop(&mem)).unwrap();
                    assert!(empty.root.is_none());
                    empty
                } else {
                    list.pop(&mem).unwrap()
                };
                if list.len() == 17 || list.len() == 18 {
                    let cleared =
                        list_clear(&mem, list.root.as_ref().unwrap(), 1, list.len() - 1).unwrap();
                    let ListNode::Branch(children) = &*cleared else {
                        panic!("clear returned the wrong root kind");
                    };
                    if list.len() == 17 {
                        assert!(children[1].is_none());
                    } else {
                        let Some(ListNode::Leaf(items)) = children[1].as_deref() else {
                            panic!("clear removed a partially occupied leaf");
                        };
                        assert_eq!(items[0], Some(16));
                        assert_eq!(items[1], None);
                    }
                }
                assert_eq!(popped.len(), list.len() - 1);
                assert_eq!(
                    popped.height,
                    if popped.len() <= 16 {
                        0
                    } else if popped.len() <= 256 {
                        1
                    } else {
                        2
                    }
                );
                for i in 0..popped.len() {
                    assert_eq!(popped.get(i), Some(&(i as u64)));
                }
                assert!(popped.get(popped.len()).is_none());
                assert_eq!(old.get(old.len() - 1), Some(&n));
            }
        }
        assert!(matches!(list.push(&mem, 999), Err(Error::Limit)));
        assert!(matches!(list.set(&mem, 258, 999), Err(Error::Bounds)));
        assert_eq!(list.set(&mem, 257, 999).unwrap().get(257), Some(&999));
    }
    #[test]
    fn visit_error_stops_before_later_items_and_empty_visit_never_calls() {
        let mem = Memory::new(Default::default()).unwrap();
        Map::<u64>::new()
            .visit(&mut |_, _| panic!("empty visit"))
            .unwrap();
        for hashes in [[0, 0], [0, 1]] {
            let map = Map::new()
                .set_hashed(&mem, b"a", 1, hashes[0])
                .unwrap()
                .set_hashed(&mem, b"b", 2, hashes[1])
                .unwrap();
            let mut calls = 0;
            assert_eq!(
                map.visit(&mut |_, _| {
                    calls += 1;
                    Err(Error::Io)
                }),
                Err(Error::Io)
            );
            assert_eq!(calls, 1);
            let mut calls = 0;
            map.visit(&mut |_, _| {
                calls += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(calls, 2);
        }
    }
}
