use crate::memory::ChargedVec;
use crate::{
    Error, Result,
    memory::{Memory, Shared},
    tree::{List, Map},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Kind {
    None = 0,
    Boolean = 1,
    Integer = 2,
    Unsigned = 3,
    Float = 4,
    Bytes = 5,
    Map = 6,
    List = 7,
}
enum Data {
    None,
    Boolean(bool),
    Integer(i64),
    Unsigned(u64),
    Float(u64),
    Bytes(ChargedVec<u8>),
    Map(Map<Value>),
    List(List<Value>),
}
struct Node {
    data: Data,
    depth: usize,
    weight: usize,
}

/// An explicitly owned immutable value. Clone shares storage and cannot allocate.
/// Values keep their allocations alive even after the database is dropped.
#[derive(Clone)]
pub struct Value {
    pub(crate) mem: Memory,
    node: Shared<Node>,
}
impl Value {
    /// The originating database's shared budget, valid after the database drops.
    /// See [`crate::MemoryUsage`] for the accounted boundary.
    pub fn memory_usage(&self) -> crate::MemoryUsage {
        self.mem.public_usage()
    }

    pub(crate) fn same_node(&self, other: &Self) -> bool {
        self.node.ptr_eq(&other.node)
    }
    pub(crate) fn from_map(mem: &Memory, map: Map<Value>) -> Result<Self> {
        let mut depth = 1;
        let mut weight = 16usize;
        map.visit(&mut |key, value| {
            depth = depth.max(value.node.depth + 1);
            weight = weight
                .checked_add(key.len())
                .and_then(|n| n.checked_add(value.weight()))
                .ok_or(Error::Limit)?;
            Ok(())
        })?;
        Self::new(mem, Data::Map(map), depth, weight)
    }
    pub(crate) fn from_list(mem: &Memory, list: List<Value>) -> Result<Self> {
        let mut depth = 1;
        let mut weight = 16usize;
        for i in 0..list.len() {
            let value = list.get(i).unwrap();
            depth = depth.max(value.node.depth + 1);
            weight = weight.checked_add(value.weight()).ok_or(Error::Limit)?;
        }
        Self::new(mem, Data::List(list), depth, weight)
    }
    fn new(mem: &Memory, data: Data, depth: usize, weight: usize) -> Result<Self> {
        if depth > mem.limits().max_depth || weight > mem.limits().max_value_bytes {
            return Err(Error::Limit);
        }
        Ok(Self {
            mem: mem.clone(),
            node: mem.shared(Node {
                data,
                depth,
                weight,
            })?,
        })
    }
    pub(crate) fn scalar(mem: &Memory, kind: Kind, bits: u64) -> Result<Self> {
        let data = match kind {
            Kind::None if bits == 0 => Data::None,
            Kind::Boolean if bits <= 1 => Data::Boolean(bits != 0),
            Kind::Integer => Data::Integer(bits as i64),
            Kind::Unsigned => Data::Unsigned(bits),
            Kind::Float => Data::Float(bits),
            Kind::Map if bits == 0 => Data::Map(Map::new()),
            Kind::List if bits == 0 => Data::List(List::new()),
            _ => return Err(Error::Invalid),
        };
        Self::new(mem, data, 1, 16)
    }
    pub(crate) fn bytes_new(mem: &Memory, bytes: &[u8]) -> Result<Self> {
        if bytes.len() > mem.limits().max_bytes {
            return Err(Error::Limit);
        }
        let weight = bytes.len().checked_add(16).ok_or(Error::Limit)?;
        if weight > mem.limits().max_value_bytes {
            return Err(Error::Limit);
        }
        Self::new(mem, Data::Bytes(mem.copy(bytes)?), 1, weight)
    }
    pub(crate) fn weight(&self) -> usize {
        self.node.weight
    }
    pub fn kind(&self) -> Kind {
        match &self.node.data {
            Data::None => Kind::None,
            Data::Boolean(_) => Kind::Boolean,
            Data::Integer(_) => Kind::Integer,
            Data::Unsigned(_) => Kind::Unsigned,
            Data::Float(_) => Kind::Float,
            Data::Bytes(_) => Kind::Bytes,
            Data::Map(_) => Kind::Map,
            Data::List(_) => Kind::List,
        }
    }
    pub fn as_bool(&self) -> Result<bool> {
        match self.node.data {
            Data::Boolean(v) => Ok(v),
            _ => Err(Error::Type),
        }
    }
    pub fn as_i64(&self) -> Result<i64> {
        match self.node.data {
            Data::Integer(v) => Ok(v),
            _ => Err(Error::Type),
        }
    }
    pub fn as_u64(&self) -> Result<u64> {
        match self.node.data {
            Data::Unsigned(v) => Ok(v),
            _ => Err(Error::Type),
        }
    }
    pub fn as_f64(&self) -> Result<f64> {
        match self.node.data {
            Data::Float(v) => Ok(f64::from_bits(v)),
            _ => Err(Error::Type),
        }
    }
    pub fn as_bytes(&self) -> Result<&[u8]> {
        match &self.node.data {
            Data::Bytes(v) => Ok(v),
            _ => Err(Error::Type),
        }
    }
    pub fn len(&self) -> Result<usize> {
        match &self.node.data {
            Data::Bytes(v) => Ok(v.len()),
            Data::Map(v) => Ok(v.len()),
            Data::List(v) => Ok(v.len()),
            _ => Err(Error::Type),
        }
    }
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
    fn same(&self, other: &Self) -> Result<()> {
        if self.mem.same(&other.mem) {
            Ok(())
        } else {
            Err(Error::ForeignStore)
        }
    }
    fn key(&self, key: &[u8]) -> Result<()> {
        if key.len() > self.mem.limits().max_bytes {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
    pub fn map_get(&self, key: &[u8]) -> Result<Option<Self>> {
        self.key(key)?;
        let Data::Map(map) = &self.node.data else {
            return Err(Error::Type);
        };
        Ok(map.get(key).cloned())
    }
    pub fn map_set(&self, key: &[u8], value: &Self) -> Result<Self> {
        self.same(value)?;
        self.key(key)?;
        let Data::Map(map) = &self.node.data else {
            return Err(Error::Type);
        };
        let old = map.get(key);
        let removed = old.map_or(0, |v| v.weight());
        let added_key = if old.is_some() { 0 } else { key.len() };
        let weight = self
            .weight()
            .checked_sub(removed)
            .and_then(|n| n.checked_add(value.weight()))
            .and_then(|n| n.checked_add(added_key))
            .ok_or(Error::Limit)?;
        if weight > self.mem.limits().max_value_bytes
            || value.node.depth >= self.mem.limits().max_depth
        {
            return Err(Error::Limit);
        }
        let map = map.set(&self.mem, key, value.clone())?;
        let depth = map_depth(&map)?;
        Self::new(&self.mem, Data::Map(map), depth, weight)
    }
    pub fn map_remove(&self, key: &[u8]) -> Result<Self> {
        self.key(key)?;
        let Data::Map(map) = &self.node.data else {
            return Err(Error::Type);
        };
        let Some(old) = map.get(key) else {
            return Ok(self.clone());
        };
        let weight = self.weight() - old.weight() - key.len();
        let map = map.remove(&self.mem, key)?;
        let depth = map_depth(&map)?;
        Self::new(&self.mem, Data::Map(map), depth, weight)
    }
    /// Binary-key order. The returned vector owns keys and value handles.
    /// Buffer entries plus copied keys must fit `max_transaction_bytes`; child
    /// values are shared. Oversized results return Limit before allocation.
    pub fn map_entries(&self) -> Result<Vec<(Vec<u8>, Value)>> {
        let Data::Map(map) = &self.node.data else {
            return Err(Error::Type);
        };
        map.materialize(&self.mem, Clone::clone)
    }

    pub fn list_get(&self, index: usize) -> Result<Self> {
        let Data::List(list) = &self.node.data else {
            return Err(Error::Type);
        };
        list.get(index).cloned().ok_or(Error::Bounds)
    }
    pub fn list_set(&self, index: usize, value: &Self) -> Result<Self> {
        self.same(value)?;
        let Data::List(list) = &self.node.data else {
            return Err(Error::Type);
        };
        let old = list.get(index).ok_or(Error::Bounds)?;
        let weight = (self.weight() - old.weight())
            .checked_add(value.weight())
            .ok_or(Error::Limit)?;
        if weight > self.mem.limits().max_value_bytes
            || value.node.depth >= self.mem.limits().max_depth
        {
            return Err(Error::Limit);
        }
        let list = list.set(&self.mem, index, value.clone())?;
        let depth = list_depth(&list);
        Self::new(&self.mem, Data::List(list), depth, weight)
    }
    pub fn list_push(&self, value: &Self) -> Result<Self> {
        self.same(value)?;
        let Data::List(list) = &self.node.data else {
            return Err(Error::Type);
        };
        let weight = self
            .weight()
            .checked_add(value.weight())
            .ok_or(Error::Limit)?;
        if weight > self.mem.limits().max_value_bytes
            || value.node.depth >= self.mem.limits().max_depth
        {
            return Err(Error::Limit);
        }
        let list = list.push(&self.mem, value.clone())?;
        Self::new(
            &self.mem,
            Data::List(list),
            self.node.depth.max(value.node.depth + 1),
            weight,
        )
    }
    /// Rebuilds the affected suffix with path copying: O((len-index) log16 len).
    pub fn list_insert(&self, index: usize, value: &Self) -> Result<Self> {
        self.same(value)?;
        let Data::List(list) = &self.node.data else {
            return Err(Error::Type);
        };
        if index > list.len() {
            return Err(Error::Bounds);
        }
        if index == list.len() {
            return self.list_push(value);
        }
        let weight = self
            .weight()
            .checked_add(value.weight())
            .ok_or(Error::Limit)?;
        if weight > self.mem.limits().max_value_bytes
            || value.node.depth >= self.mem.limits().max_depth
        {
            return Err(Error::Limit);
        }
        let mut next = list.push(&self.mem, list.get(list.len() - 1).unwrap().clone())?;
        for i in (index..list.len() - 1).rev() {
            next = next.set(&self.mem, i + 1, list.get(i).unwrap().clone())?;
        }
        next = next.set(&self.mem, index, value.clone())?;
        let depth = self.node.depth.max(value.node.depth + 1);
        Self::new(&self.mem, Data::List(next), depth, weight)
    }
    /// Rebuilds the affected suffix; no positional operation is replayed at commit.
    pub fn list_remove(&self, index: usize) -> Result<Self> {
        let Data::List(list) = &self.node.data else {
            return Err(Error::Type);
        };
        let old = list.get(index).ok_or(Error::Bounds)?;
        let mut next = list.clone();
        for i in index..list.len() - 1 {
            next = next.set(&self.mem, i, list.get(i + 1).unwrap().clone())?;
        }
        next = next.pop(&self.mem)?;
        let depth = list_depth(&next);
        Self::new(
            &self.mem,
            Data::List(next),
            depth,
            self.weight() - old.weight(),
        )
    }
}
fn map_depth(map: &Map<Value>) -> Result<usize> {
    let mut depth = 1;
    map.visit(&mut |_, v| {
        depth = depth.max(v.node.depth + 1);
        Ok(())
    })?;
    Ok(depth)
}
fn list_depth(list: &List<Value>) -> usize {
    (0..list.len())
        .map(|i| list.get(i).unwrap().node.depth + 1)
        .max()
        .unwrap_or(1)
}
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        if self.node.ptr_eq(&other.node) {
            return true;
        }
        match (&self.node.data, &other.node.data) {
            (Data::None, Data::None) => true,
            (Data::Boolean(a), Data::Boolean(b)) => a == b,
            (Data::Integer(a), Data::Integer(b)) => a == b,
            (Data::Unsigned(a), Data::Unsigned(b)) => a == b,
            (Data::Float(a), Data::Float(b)) => a == b,
            (Data::Bytes(a), Data::Bytes(b)) => a.as_slice() == b.as_slice(),
            (Data::List(a), Data::List(b)) => {
                a.len() == b.len() && (0..a.len()).all(|i| a.get(i) == b.get(i))
            }
            (Data::Map(a), Data::Map(b)) if a.len() == b.len() => {
                let mut equal = true;
                a.visit(&mut |k, v| {
                    if b.get(k) != Some(v) {
                        equal = false;
                    }
                    Ok(())
                })
                .unwrap();
                equal
            }
            _ => false,
        }
    }
}
impl Eq for Value {}
impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Value")
            .field("kind", &self.kind())
            .field("depth", &self.node.depth)
            .field("weight", &self.weight())
            .finish()
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;
    #[test]
    fn constructor_depth_weight_or_and_scalar_bits_have_controls() {
        let mem = Memory::new(crate::Limits {
            max_depth: 2,
            max_value_bytes: 32,
            ..Default::default()
        })
        .unwrap();
        for (depth, weight, valid) in [(2, 32, true), (1, 32, false), (2, 31, false)] {
            let mem = Memory::new(crate::Limits {
                max_depth: depth,
                max_value_bytes: weight,
                ..Default::default()
            })
            .unwrap();
            let leaf = Value::scalar(&mem, Kind::None, 0).unwrap();
            let list = List::new().push(&mem, leaf).unwrap();
            let result = Value::from_list(&mem, list);
            assert_eq!(result.is_ok(), valid);
            if !valid {
                assert_eq!(result, Err(Error::Limit));
            }
        }
        for kind in [Kind::None, Kind::Map, Kind::List] {
            assert!(Value::scalar(&mem, kind, 0).is_ok());
            assert_eq!(Value::scalar(&mem, kind, 1), Err(Error::Invalid));
        }
        for bits in [0, 1] {
            assert!(Value::scalar(&mem, Kind::Boolean, bits).is_ok());
        }
        assert_eq!(Value::scalar(&mem, Kind::Boolean, 2), Err(Error::Invalid));
        assert_eq!(Value::scalar(&mem, Kind::Bytes, 0), Err(Error::Invalid));
        for kind in [Kind::Integer, Kind::Unsigned, Kind::Float] {
            assert!(Value::scalar(&mem, kind, u64::MAX).is_ok());
        }
    }
}

#[cfg(test)]
mod arithmetic_decision_tests {
    use super::*;
    #[test]
    fn shared_children_make_aggregate_overflow_feasible_without_large_allocations() {
        let mem = Memory::new(crate::Limits {
            max_value_bytes: isize::MAX as usize,
            max_depth: 64,
            ..Default::default()
        })
        .unwrap();
        assert!(
            Value::from_map(&mem, Map::<Value>::new())
                .unwrap()
                .is_empty()
                .unwrap()
        );
        assert!(
            Value::from_list(&mem, List::<Value>::new())
                .unwrap()
                .is_empty()
                .unwrap()
        );
        let mut child = Value::scalar(&mem, Kind::None, 0).unwrap();
        let mut previous = child.clone();
        for _ in 0..usize::BITS - 6 {
            previous = child.clone();
            let pair = List::new()
                .push(&mem, child.clone())
                .unwrap()
                .push(&mem, child)
                .unwrap();
            child = Value::from_list(&mem, pair).unwrap();
        }
        assert_eq!(child.weight(), isize::MAX as usize - 15);
        let map = Map::new()
            .set(&mem, b"", previous.clone())
            .unwrap()
            .set(&mem, b"x", previous)
            .unwrap();
        let heavy = Value::from_map(&mem, map).unwrap();
        assert_eq!(heavy.weight(), isize::MAX as usize - 14);
        assert_eq!(heavy.map_set(&[7; 32], &child), Err(Error::Limit));
        let map = Map::new()
            .set(&mem, b"a", child.clone())
            .unwrap()
            .set(&mem, b"b", child.clone())
            .unwrap()
            .set(&mem, b"c", child.clone())
            .unwrap();
        assert_eq!(Value::from_map(&mem, map), Err(Error::Limit));
        let list = List::new()
            .push(&mem, child.clone())
            .unwrap()
            .push(&mem, child.clone())
            .unwrap()
            .push(&mem, child)
            .unwrap();
        assert_eq!(Value::from_list(&mem, list), Err(Error::Limit));
    }
}
