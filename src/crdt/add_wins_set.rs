use super::{Actor, Record};
use crate::{Error, Result};

const IDS: usize = 64;
const MARK_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Mark {
    added: u64,
    removed: u64,
}

/// An observed-remove set for two actors and IDs from 0 through 63. A
/// removal covers the additions visible to the actor performing it.
/// A concurrent addition survives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddWinsSet([[Mark; 2]; IDS]);

impl Default for AddWinsSet {
    fn default() -> Self {
        Self([[Mark::default(); 2]; IDS])
    }
}

impl AddWinsSet {
    /// The payload stores an add sequence and a removal watermark for each
    /// ID and actor.
    pub const SIZE: usize = IDS * 2 * MARK_BYTES;

    /// Record a fresh addition even if this ID is already visible.
    pub fn insert(&mut self, id: u8, actor: Actor) -> Result<()> {
        let marks = self.0.get_mut(usize::from(id)).ok_or(Error::Bounds)?;
        let mark = &mut marks[actor.index()];
        mark.added = mark.added.checked_add(1).ok_or(Error::Exhausted)?;
        Ok(())
    }

    /// Remove the additions visible for this ID and report whether it was present.
    pub fn remove(&mut self, id: u8) -> Result<bool> {
        let marks = self.0.get_mut(usize::from(id)).ok_or(Error::Bounds)?;
        let present = Self::visible(marks);
        for mark in marks {
            mark.removed = mark.added;
        }
        Ok(present)
    }

    pub fn contains(&self, id: u8) -> bool {
        self.0.get(usize::from(id)).is_some_and(Self::visible)
    }

    pub fn iter(&self) -> impl Iterator<Item = u8> + '_ {
        (0..IDS as u8).filter(|&id| self.contains(id))
    }

    fn visible(marks: &[Mark; 2]) -> bool {
        marks.iter().any(|mark| mark.added > mark.removed)
    }

    pub fn join(&mut self, other: Self) {
        for (local, incoming) in self
            .0
            .iter_mut()
            .flatten()
            .zip(other.0.into_iter().flatten())
        {
            local.added = local.added.max(incoming.added);
            local.removed = local.removed.max(incoming.removed);
        }
    }
}

impl Record for AddWinsSet {
    const KEY: &'static [u8] = b"add-wins-set";
    const FORMAT: &'static [u8; 8] = b"WDBASET\x01";
    const SIZE: usize = AddWinsSet::SIZE;

    fn encode(self, bytes: &mut [u8]) -> Result<()> {
        if bytes.len() != Self::SIZE {
            return Err(Error::Buffer);
        }
        let (marks, _) = bytes.as_chunks_mut::<MARK_BYTES>();
        for (mark, bytes) in self.0.iter().flatten().zip(marks) {
            bytes[..8].copy_from_slice(&mark.added.to_le_bytes());
            bytes[8..].copy_from_slice(&mark.removed.to_le_bytes());
        }
        Ok(())
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != Self::SIZE {
            return Err(Error::Corrupt);
        }
        let mut set = Self::default();
        let (marks, _) = bytes.as_chunks::<MARK_BYTES>();
        for (mark, bytes) in set.0.iter_mut().flatten().zip(marks) {
            let added = u64::from_le_bytes(bytes[..8].try_into().unwrap());
            let removed = u64::from_le_bytes(bytes[8..].try_into().unwrap());
            if removed > added {
                return Err(Error::Corrupt);
            }
            *mark = Mark { added, removed };
        }
        Ok(set)
    }

    fn merge(&mut self, other: Self) {
        self.join(other);
    }

    fn check_peer(self, incoming: Self, local_actor: Actor) -> Result<()> {
        let actor = local_actor.index();
        if self
            .0
            .iter()
            .zip(incoming.0)
            .any(|(local, incoming)| incoming[actor].added > local[actor].added)
        {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}
