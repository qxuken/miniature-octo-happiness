//! Experimental thread-safe persistent values, transactions, byte codec,
//! macOS/Linux files, offline compaction, durable retained snapshots, and optional
//! schema access conventions. See examples/typed.rs for the application layer.
//! See DESIGN.md for implemented contracts and the remaining durability work.
mod engine;
mod ffi;
mod memory;
pub mod schema;
pub mod storage;
mod tree;
mod value;

pub use engine::{
    CollectionId, Database, MAX_RETAINED_SNAPSHOTS, Rows, Snapshot, Transaction, TransactionState,
    Validation,
};
pub use memory::Limits;
pub use value::{Kind, Value};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Error {
    Invalid = 1,
    Type = 2,
    NotFound = 3,
    Bounds = 4,
    Buffer = 5,
    Busy = 6,
    Limit = 7,
    OutOfMemory = 8,
    Panic = 9,
    Conflict = 10,
    Closed = 11,
    ForeignStore = 12,
    Exhausted = 13,
    Io = 14,
    OutcomeUnknown = 15,
    RecoveryRequired = 16,
    Corrupt = 17,
    UnsupportedFormat = 18,
    AlreadyExists = 19,
    UnsupportedPlatform = 20,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
