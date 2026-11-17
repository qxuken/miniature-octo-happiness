//! Runtime identity for matching a consumer to its delivered artifact.
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ABI_VERSION: u32 = 1;
pub const FORMAT_VERSION: u32 = 2;
/// Release tooling sets this to the full Git commit. Ordinary builds say unknown.
pub const SOURCE_ID: &str = match option_env!("WEEKEND_DB_SOURCE_ID") {
    Some(value) => value,
    None => "unknown",
};
/// Bit 0 is replication, bit 1 is CRDT, and bit 2 is fault injection.
pub const FEATURES: u32 = (cfg!(feature = "replication") as u32)
    | ((cfg!(feature = "crdt") as u32) << 1)
    | ((cfg!(feature = "fault-injection") as u32) << 2);
