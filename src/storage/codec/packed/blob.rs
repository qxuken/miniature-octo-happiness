//! A format-4 leaf protects the offset, length and expected object CRC. Only
//! a witnessed read-only open may defer reading the referenced object's bytes.
use super::*;
pub(in crate::storage::codec) const BLOB: u32 = 6;
const TAG: u8 = 8;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::storage::codec) struct Blob {
    pub offset: u64,
    len: u64,
    digest: u64,
}
impl Blob {
    pub fn parse(raw: &[u8]) -> Result<Option<Self>> {
        if raw.first() != Some(&TAG) {
            return Ok(None);
        }
        if raw.len() != 25 {
            return Err(Error::Corrupt);
        }
        Ok(Some(Self {
            offset: u64_at(raw, 1),
            len: u64_at(raw, 9),
            digest: u64_at(raw, 17),
        }))
    }
    pub fn bytes(self) -> [u8; 25] {
        let mut row = [0; 25];
        row[0] = TAG;
        put64(&mut row, 1, self.offset);
        put64(&mut row, 9, self.len);
        put64(&mut row, 17, self.digest);
        row
    }
    pub fn write(encoder: &mut Encoder<'_>, raw: &[u8], born: u64) -> Result<Self> {
        let (offset, _, digest) = encoder.object_with_crc(BLOB, raw, born)?;
        Ok(Self {
            offset,
            len: raw.len() as u64,
            digest,
        })
    }
    pub fn read(self, io: &mut dyn ByteStore, mem: &Memory) -> Result<(u64, ChargedVec<u8>)> {
        let len = usize::try_from(self.len).map_err(|_| Error::Limit)?;
        Value::bytes_weight(mem, len)?;
        let size = align(
            self.len
                .checked_add(OBJECT_HEADER as u64)
                .ok_or(Error::Corrupt)?,
        )? as usize;
        let mut bytes = ChargedVec::new();
        mem.reserve(&mut bytes, size)?;
        bytes.resize(size, 0);
        read_all(io, self.offset, &mut bytes)?;
        if u32_at(&bytes, 8) != BLOB
            || u64_at(&bytes, 16) != self.len
            || u64_at(&bytes, 48) != self.digest
            || checksum(&bytes, 48) != self.digest
            || bytes[OBJECT_HEADER + len..].iter().any(|b| *b != 0)
        {
            return Err(Error::Corrupt);
        }
        let born = u64_at(&bytes, 24);
        bytes.copy_within(OBJECT_HEADER..OBJECT_HEADER + len, 0);
        bytes.truncate(len);
        Ok((born, bytes))
    }
}
impl Reader<'_> {
    pub(super) fn verify_blob(&mut self, blob: Blob, parent: u64, row_revision: u64) -> Result<()> {
        if !self.packed.extended() || blob.offset >= parent || blob.len <= LEAF_BYTES as u64 {
            return Err(Error::Corrupt);
        }
        let len = usize::try_from(blob.len).map_err(|_| Error::Limit)?;
        Value::bytes_weight(self.mem, len)?;
        if let Some((known, end, born)) = self.blobs.get(blob.offset) {
            return if *known == blob && *end <= parent && *born <= row_revision {
                Ok(())
            } else {
                Err(Error::Corrupt)
            };
        }
        let obj = self.object(blob.offset, BLOB, parent)?;
        if obj.payload_len != len
            || obj.digest != blob.digest
            || obj.born == 0
            || obj.born > row_revision
        {
            return Err(Error::Corrupt);
        }
        self.blobs
            .insert(self.mem, blob.offset, (blob, obj.end, obj.born))?;
        self.scratch = obj.payload;
        Ok(())
    }
}
