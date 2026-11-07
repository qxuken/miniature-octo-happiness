#![allow(dead_code)]
use std::collections::BTreeMap;
use weekend_db::{Kind, Value};

/// Deliberately copying logical model, independent of the engine's trees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Model {
    None,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(u64),
    Bytes(Vec<u8>),
    Map(BTreeMap<Vec<u8>, Model>),
    List(Vec<Model>),
}
impl Model {
    pub fn read(v: &Value) -> Self {
        match v.kind() {
            Kind::None => Self::None,
            Kind::Boolean => Self::Bool(v.as_bool().unwrap()),
            Kind::Integer => Self::Int(v.as_i64().unwrap()),
            Kind::Unsigned => Self::UInt(v.as_u64().unwrap()),
            Kind::Float => Self::Float(v.as_f64().unwrap().to_bits()),
            Kind::Bytes => Self::Bytes(v.as_bytes().unwrap().to_vec()),
            Kind::Map => Self::Map(
                v.map_entries()
                    .unwrap()
                    .into_iter()
                    .map(|(k, v)| (k, Self::read(&v)))
                    .collect(),
            ),
            Kind::List => Self::List(
                (0..v.len().unwrap())
                    .map(|i| Self::read(&v.list_get(i).unwrap()))
                    .collect(),
            ),
        }
    }
}
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn n(&mut self, n: usize) -> usize {
        self.next() as usize % n
    }
}
