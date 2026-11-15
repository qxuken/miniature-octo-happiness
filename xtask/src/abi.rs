//! Independent dynamic C-ABI smoke consumer. No database crate is linked here.
use crate::support::Scratch;
#[cfg(not(unix))]
use anyhow::Context;
use anyhow::{Result, ensure};
use libloading::Library;
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
};
type Ptr = *mut c_void;
type Open = unsafe extern "C" fn(*mut Ptr) -> i32;
type Unary = unsafe extern "C" fn(Ptr) -> i32;
type Version = unsafe extern "C" fn() -> u32;
type Text = unsafe extern "C" fn(*mut u8, usize, *mut usize) -> i32;
type Id = unsafe extern "C" fn(Ptr, *mut u64) -> i32;
type Make = unsafe extern "C" fn(Ptr, i32, u64, *const u8, usize, *mut Ptr) -> i32;
type Begin = unsafe extern "C" fn(Ptr, i32, *mut Ptr) -> i32;
type Put = unsafe extern "C" fn(Ptr, u64, *const u8, usize, Ptr) -> i32;
type Acquire = unsafe extern "C" fn(Ptr, *mut Ptr) -> i32;
type Get = unsafe extern "C" fn(Ptr, u64, *const u8, usize, *mut Ptr) -> i32;
type FileOpen = unsafe extern "C" fn(*const u8, usize, *mut Ptr) -> i32;
type Compact = unsafe extern "C" fn(*const u8, usize, *mut u64, *mut u64) -> i32;
struct Api {
    library: Library,
}
impl Api {
    // SAFETY: callers name documented header exports and the matching function
    // pointer type. The Library outlives every copied pointer and live handle.
    unsafe fn symbol<T: Copy>(&self, name: &[u8]) -> Result<T> {
        Ok(*unsafe { self.library.get::<T>(name) }?)
    }
    fn status(&self, actual: i32, expected: i32, name: &str) -> Result<()> {
        ensure!(
            actual == expected,
            "{name}: ABI status {actual}, expected {expected}"
        );
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Kind {
    Database,
    Value,
    Transaction,
    Snapshot,
}
struct Handle<'a> {
    api: &'a Api,
    raw: Ptr,
    kind: Kind,
}
impl<'a> Handle<'a> {
    fn new(api: &'a Api, raw: Ptr, kind: Kind) -> Result<Self> {
        ensure!(!raw.is_null(), "successful ABI call returned a null handle");
        Ok(Self { api, raw, kind })
    }
    fn release(&mut self) -> Result<()> {
        if self.raw.is_null() {
            return Ok(());
        }
        let name: &[u8] = match self.kind {
            Kind::Database => b"wdb_close\0",
            Kind::Value => b"wdb_value_release\0",
            Kind::Transaction => b"wdb_tx_release\0",
            Kind::Snapshot => b"wdb_snapshot_release\0",
        };
        // SAFETY: this guard owns the live handle, is never cloned and is dropped
        // before Api. Database is declared before its subordinate guards.
        let code = unsafe { self.api.symbol::<Unary>(name)?(self.raw) };
        self.api.status(code, 0, "release")?;
        self.raw = std::ptr::null_mut();
        Ok(())
    }
}
impl Drop for Handle<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.release() {
            eprintln!("xtask ABI cleanup: {e:#}");
        }
    }
}
#[derive(clap::Args)]
pub struct Options {
    pub library: PathBuf,
    #[arg(long)]
    pub expected_source: Option<String>,
    #[arg(long)]
    pub expected_features: Option<u32>,
}
pub fn run(options: Options) -> Result<()> {
    smoke(
        &options.library,
        options.expected_source.as_deref(),
        options.expected_features,
    )
}
fn smoke(
    library: &Path,
    expected_source: Option<&str>,
    expected_features: Option<u32>,
) -> Result<()> {
    let library = dunce::canonicalize(library)?;
    // SAFETY: the user requests execution of this exact locally-built shared
    // artifact. Symbols below follow include/weekend_db.h, with no Rust ABI use.
    let api = Api {
        library: unsafe { Library::new(&library) }?,
    };
    // SAFETY: all pointers/lengths below refer to live, aligned local buffers or
    // handles produced by this library. Null outputs test documented validation.
    unsafe {
        for (name, expected) in [
            (b"wdb_abi_version\0".as_slice(), 1),
            (b"wdb_format_version\0".as_slice(), 2),
        ] {
            ensure!(
                api.symbol::<Version>(name)?() == expected,
                "ABI/format version mismatch"
            );
        }
        let actual_features = api.symbol::<Version>(b"wdb_features\0")?();
        ensure!(actual_features & !7 == 0, "unknown feature mask");
        if let Some(expected) = expected_features {
            ensure!(
                actual_features == expected,
                "feature mask mismatch: actual {actual_features}, expected {expected}"
            );
        }
        for name in [
            b"wdb_package_version\0".as_slice(),
            b"wdb_source_id\0".as_slice(),
        ] {
            let function = api.symbol::<Text>(name)?;
            let mut required = 0;
            api.status(
                function(std::ptr::null_mut(), 0, &mut required),
                5,
                "identity size",
            )?;
            ensure!(required > 0 && required < 256, "identity length bound");
            let mut buffer = vec![0; required];
            api.status(
                function(buffer.as_mut_ptr(), buffer.len(), &mut required),
                0,
                "identity",
            )?;
            let actual = std::str::from_utf8(&buffer)?;
            if name == b"wdb_source_id\0"
                && let Some(expected) = expected_source
            {
                ensure!(
                    actual == expected,
                    "source_id mismatch: {actual:?}, expected {expected:?}"
                );
            }
            api.status(
                function(buffer.as_mut_ptr(), buffer.len(), std::ptr::null_mut()),
                1,
                "identity required pointer",
            )?;
        }
        let mut raw = std::ptr::null_mut();
        api.status(api.symbol::<Open>(b"wdb_open\0")?(&mut raw), 0, "open")?;
        let mut db = Handle::new(&api, raw, Kind::Database)?;
        let mut collection = 0;
        api.status(
            api.symbol::<Id>(b"wdb_collection_create\0")?(db.raw, &mut collection),
            0,
            "collection_create",
        )?;
        raw = std::ptr::null_mut();
        api.status(
            api.symbol::<Make>(b"wdb_value_make\0")?(db.raw, 3, 123, std::ptr::null(), 0, &mut raw),
            0,
            "value_make",
        )?;
        let mut value = Handle::new(&api, raw, Kind::Value)?;
        raw = std::ptr::null_mut();
        api.status(
            api.symbol::<Begin>(b"wdb_tx_begin\0")?(db.raw, 0, &mut raw),
            0,
            "tx_begin",
        )?;
        let mut tx = Handle::new(&api, raw, Kind::Transaction)?;
        let key = b"binary\0key";
        api.status(
            api.symbol::<Put>(b"wdb_tx_put\0")?(
                tx.raw,
                collection,
                key.as_ptr(),
                key.len(),
                value.raw,
            ),
            0,
            "tx_put",
        )?;
        let mut revision = 0;
        api.status(
            api.symbol::<Id>(b"wdb_tx_commit\0")?(tx.raw, &mut revision),
            0,
            "tx_commit",
        )?;
        ensure!(revision == 2, "unexpected committed revision");
        tx.release()?;
        raw = std::ptr::null_mut();
        api.status(
            api.symbol::<Acquire>(b"wdb_snapshot_acquire\0")?(db.raw, &mut raw),
            0,
            "snapshot_acquire",
        )?;
        let mut snapshot = Handle::new(&api, raw, Kind::Snapshot)?;
        raw = std::ptr::null_mut();
        api.status(
            api.symbol::<Get>(b"wdb_snapshot_get\0")?(
                snapshot.raw,
                collection,
                key.as_ptr(),
                key.len(),
                &mut raw,
            ),
            0,
            "snapshot_get",
        )?;
        let mut found = Handle::new(&api, raw, Kind::Value)?;
        let mut bits = 0;
        api.status(
            api.symbol::<Id>(b"wdb_value_scalar\0")?(found.raw, &mut bits),
            0,
            "value_scalar",
        )?;
        ensure!(bits == 123, "snapshot scalar mismatch");
        api.status(
            api.symbol::<Unary>(b"wdb_close\0")?(db.raw),
            6,
            "close busy",
        )?;
        found.release()?;
        value.release()?;
        snapshot.release()?;
        db.release()?;
        let can_replace = cfg!(target_os = "macos")
            || cfg!(all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ));
        let file_open = api.symbol::<FileOpen>(b"wdb_file_open\0")?;
        let compact = api.symbol::<Compact>(b"wdb_file_compact\0")?;
        if !can_replace && !cfg!(windows) {
            raw = std::ptr::null_mut();
            api.status(
                file_open(key.as_ptr(), key.len(), &mut raw),
                20,
                "unsupported file_open",
            )?;
            ensure!(raw.is_null(), "unsupported factory output not reset");
            let (mut before, mut after) = (1, 1);
            api.status(
                compact(key.as_ptr(), key.len(), &mut before, &mut after),
                20,
                "unsupported compact",
            )?;
            ensure!(
                before == 0 && after == 0,
                "unsupported byte counts not reset"
            );
        } else {
            let mut scratch = Scratch::new(&std::env::temp_dir(), "ffi")?;
            let file = scratch.path().join("native-λ.db");
            #[cfg(unix)]
            let bytes = {
                use std::os::unix::ffi::OsStrExt;
                file.as_os_str().as_bytes().to_vec()
            };
            #[cfg(not(unix))]
            let bytes = file
                .to_str()
                .context("native path must be UTF-8")?
                .as_bytes()
                .to_vec();
            raw = std::ptr::null_mut();
            api.status(
                api.symbol::<FileOpen>(b"wdb_file_create\0")?(
                    bytes.as_ptr(),
                    bytes.len(),
                    &mut raw,
                ),
                0,
                "file_create",
            )?;
            let mut file_db = Handle::new(&api, raw, Kind::Database)?;
            let mut other = std::ptr::null_mut();
            api.status(
                file_open(bytes.as_ptr(), bytes.len(), &mut other),
                6,
                "file lease busy",
            )?;
            ensure!(other.is_null(), "busy factory output not reset");
            file_db.release()?;
            let (mut before, mut after) = (1, 1);
            api.status(
                compact(bytes.as_ptr(), bytes.len(), &mut before, &mut after),
                if can_replace { 0 } else { 20 },
                "file_compact",
            )?;
            if can_replace {
                ensure!(before >= after && after > 0, "invalid compact byte counts");
            } else {
                ensure!(
                    before == 0 && after == 0,
                    "unsupported byte counts not reset"
                );
                let invalid = [0xff];
                raw = std::ptr::null_mut();
                api.status(
                    file_open(invalid.as_ptr(), 1, &mut raw),
                    1,
                    "invalid UTF-8 path",
                )?;
                ensure!(raw.is_null(), "invalid factory output not reset");
            }
            raw = std::ptr::null_mut();
            api.status(
                file_open(bytes.as_ptr(), bytes.len(), &mut raw),
                0,
                "file_open",
            )?;
            Handle::new(&api, raw, Kind::Database)?.release()?;
            scratch.close()?;
        }
    }
    println!("Shared-library C ABI smoke passed: {}", library.display());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_library_rejects_without_linking_engine() {
        assert!(
            smoke(
                Path::new("/definitely/missing/weekend_db-library"),
                None,
                None
            )
            .is_err()
        );
    }
}
