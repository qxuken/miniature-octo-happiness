//! Persisted capacity controls append admission independently of the reader's live-byte budget.
use super::{MAX_IMAGE_BYTES, REGION};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileFormat {
    #[default]
    V2,
    Packed,
    Extended(FileCapacity),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCapacity {
    MiB64,
    MiB256,
    GiB1,
    /// Grow to the host's signed file-offset ceiling, subject to disk and memory limits.
    Unbounded,
}
impl FileFormat {
    pub fn version(self) -> u32 {
        match self {
            Self::V2 => 2,
            Self::Packed => 3,
            Self::Extended(_) => 4,
        }
    }
    pub fn capacity(self) -> u64 {
        match self {
            Self::V2 | Self::Packed | Self::Extended(FileCapacity::MiB64) => MAX_IMAGE_BYTES as u64,
            Self::Extended(FileCapacity::MiB256) => 256 << 20,
            Self::Extended(FileCapacity::GiB1) => 1 << 30,
            Self::Extended(FileCapacity::Unbounded) => {
                (i64::MAX as u64 / REGION as u64) * REGION as u64
            }
        }
    }
    pub(crate) fn extended(self) -> bool {
        matches!(self, Self::Extended(_))
    }
    pub(crate) fn from_supported_header(version: u32, cap: u64) -> Result<Self> {
        // The reader checks the supported version before interpreting its fields.
        Ok(match version {
            2 => Self::V2,
            3 => Self::Packed,
            4 => Self::Extended(match cap {
                67108864 => FileCapacity::MiB64,
                268435456 => FileCapacity::MiB256,
                1073741824 => FileCapacity::GiB1,
                n if n == Self::Extended(FileCapacity::Unbounded).capacity() => {
                    FileCapacity::Unbounded
                }
                _ => return Err(Error::Corrupt),
            }),
            _ => return Err(Error::Corrupt),
        })
    }
}
