use super::{Actor, Record};
use crate::{Error, Result};
use std::cmp::Ordering;

const ENTRY_BYTES: usize = 17;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Entry {
    sequence: u64,
    value: Option<u64>,
}

/// A register of unsigned integers shared by two actors. An assignment
/// supersedes the values it observes. Concurrent assignments stay visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MultiValueRegister([Entry; 2]);

impl MultiValueRegister {
    pub const SIZE: usize = 2 * ENTRY_BYTES;

    /// Replace the observed choices with a new assignment by this actor.
    pub fn assign(&mut self, actor: Actor, value: u64) -> Result<()> {
        let sequence = self.0[actor.index()]
            .sequence
            .checked_add(1)
            .ok_or(Error::Exhausted)?;
        for entry in &mut self.0 {
            entry.value = None;
        }
        self.0[actor.index()] = Entry {
            sequence,
            value: Some(value),
        };
        Ok(())
    }

    /// Return the distinct visible choices in actor order.
    pub fn values(self) -> impl Iterator<Item = u64> {
        let [first, second] = self.0.map(|entry| entry.value);
        first
            .into_iter()
            .chain(second.filter(|&value| Some(value) != first))
    }

    /// Read an effective value using the field's resolution policy.
    /// The policy receives two distinct choices in actor order. Use the same
    /// deterministic policy on every peer that reads this field.
    pub fn resolve_with(self, policy: impl FnOnce(u64, u64) -> u64) -> Option<u64> {
        let mut values = self.values();
        let first = values.next()?;
        Some(match values.next() {
            Some(second) => policy(first, second),
            None => first,
        })
    }

    /// Merge state from the same enrolled history. Each actor has one
    /// writer, so a given actor and sequence identify one assignment.
    pub fn join(&mut self, other: Self) {
        for (local, incoming) in self.0.iter_mut().zip(other.0) {
            match local.sequence.cmp(&incoming.sequence) {
                Ordering::Less => *local = incoming,
                Ordering::Equal => {
                    // A retired assignment stays retired at this sequence.
                    // A conflicting value for the same dot is rejected by
                    // check_peer before a replica merges it.
                    local.value = local.value.zip(incoming.value).map(|(a, b)| a.max(b));
                }
                Ordering::Greater => {}
            }
        }
    }
}

impl Record for MultiValueRegister {
    const KEY: &'static [u8] = b"multi-value-register";
    const FORMAT: &'static [u8; 8] = b"WDBMVRG\x01";
    const SIZE: usize = MultiValueRegister::SIZE;

    fn encode(self, bytes: &mut [u8]) -> Result<()> {
        if bytes.len() != Self::SIZE {
            return Err(Error::Buffer);
        }
        let (entries, _) = bytes.as_chunks_mut::<ENTRY_BYTES>();
        for (entry, bytes) in self.0.into_iter().zip(entries) {
            bytes[..8].copy_from_slice(&entry.sequence.to_le_bytes());
            bytes[8] = u8::from(entry.value.is_some());
            bytes[9..].copy_from_slice(&entry.value.unwrap_or_default().to_le_bytes());
        }
        Ok(())
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != Self::SIZE {
            return Err(Error::Corrupt);
        }
        let (entries, _) = bytes.as_chunks::<ENTRY_BYTES>();
        let mut state = Self::default();
        for (entry, bytes) in state.0.iter_mut().zip(entries) {
            let sequence = u64::from_le_bytes(bytes[..8].try_into().unwrap());
            let value = u64::from_le_bytes(bytes[9..].try_into().unwrap());
            let value = match bytes[8] {
                0 if value == 0 => None,
                1 if sequence != 0 => Some(value),
                _ => return Err(Error::Corrupt),
            };
            *entry = Entry { sequence, value };
        }
        Ok(state)
    }

    fn merge(&mut self, other: Self) {
        self.join(other);
    }

    fn check_peer(self, incoming: Self, local_actor: Actor) -> Result<()> {
        let actor = local_actor.index();
        if incoming.0[actor].sequence > self.0[actor].sequence {
            return Err(Error::Conflict);
        }
        for (local, remote) in self.0.into_iter().zip(incoming.0) {
            if local.sequence == remote.sequence
                && let (Some(a), Some(b)) = (local.value, remote.value)
                && a != b
            {
                return Err(Error::Conflict);
            }
        }
        Ok(())
    }
}
