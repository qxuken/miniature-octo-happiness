//! Sorted blocks of reachable offsets. File holes consume no index entries.
use super::{ChargedVec, Error, Memory, Result};

const BLOCK: usize = 128;
pub(super) struct Offsets<T>(ChargedVec<ChargedVec<(u64, T)>>);
impl<T: Clone> Offsets<T> {
    pub(super) fn new() -> Self {
        Self(ChargedVec::new())
    }
    fn position(&self, off: u64) -> (usize, usize) {
        let b = self
            .0
            .partition_point(|block| block[0].0 <= off)
            .saturating_sub(1);
        let i = self
            .0
            .get(b)
            .map_or(0, |block| block.partition_point(|(start, _)| *start < off));
        (b, i)
    }
    pub(super) fn get(&self, off: u64) -> Option<&T> {
        let (b, i) = self.position(off);
        self.0
            .get(b)?
            .get(i)
            .filter(|(o, _)| *o == off)
            .map(|(_, v)| v)
    }
    pub(super) fn insert(&mut self, mem: &Memory, off: u64, v: T) -> Result<()> {
        let (mut b, mut i) = self.position(off);
        if self.0.is_empty() {
            mem.reserve_geometric(&mut self.0, 1, usize::MAX)?;
            self.0.push(ChargedVec::new());
        }
        if self.0[b].get(i).is_some_and(|x| x.0 == off) {
            return Err(Error::Corrupt);
        }
        if self.0[b].len() == BLOCK {
            let half = BLOCK / 2;
            let mut next = ChargedVec::new();
            mem.reserve(&mut next, half + 1)?;
            next.extend_from_slice(&self.0[b][half..]);
            mem.reserve_geometric(&mut self.0, 1, usize::MAX)?;
            self.0[b].truncate(half);
            self.0.push(next);
            self.0[b + 1..].rotate_right(1);
            if i >= half {
                b += 1;
                i -= half;
            }
        }
        let block = &mut self.0[b];
        mem.reserve_geometric(block, 1, BLOCK)?;
        block.push((off, v));
        block[i..].rotate_right(1);
        Ok(())
    }
}
impl Offsets<u64> {
    pub(super) fn check_free(&self, start: u64, end: u64) -> Result<()> {
        let (b, i) = self.position(start);
        let Some(block) = self.0.get(b) else {
            return Ok(());
        };
        let prev = if i > 0 {
            block.get(i - 1)
        } else {
            b.checked_sub(1).map(|p| self.0[p].last().unwrap())
        };
        let next = block.get(i).or_else(|| self.0.get(b + 1).map(|p| &p[0]));
        if prev.is_some_and(|p| p.1 > start) || next.is_some_and(|n| end > n.0) {
            return Err(Error::Corrupt);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_blocks_reject_overlaps_from_both_neighbors() {
        let mem = Memory::new(crate::Limits::default()).unwrap();
        let mut ix = Offsets::new();
        for j in 0..512u64 {
            let i = (j * 193) % 512;
            let start = (1u64 << 36) + i * 16;
            ix.check_free(start, start + 8).unwrap();
            ix.insert(&mem, start, start + 8).unwrap();
        }
        for i in 0..512 {
            let off = (1u64 << 36) + i * 16;
            assert_eq!(ix.get(off), Some(&(off + 8)));
            assert_eq!(ix.check_free(off - 1, off + 1), Err(Error::Corrupt));
            assert_eq!(ix.check_free(off + 7, off + 10), Err(Error::Corrupt));
            assert_eq!(ix.check_free(off, off + 8), Err(Error::Corrupt));
            assert_eq!(ix.check_free(off + 8, off + 16), Ok(()));
        }
    }
}
