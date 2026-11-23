//! C callers own every returned handle and release it exactly once. Non-null
//! pointers must be aligned and valid for their declared types/lengths. Output
//! storage cannot alias inputs. Database and immutable value/snapshot/scan handles
//! may be shared. A transaction requires exclusive ownership (transfer is allowed).
//! Release requires exclusive access to that handle. Close requires external
//! quiescence: no calls on the database or its handles may overlap close.
//! Database close rejects live handles; this protects the extended lifetimes of
//! the snapshot/transaction borrows stored behind the opaque boundary.
#![allow(clippy::missing_safety_doc)]
use crate::{
    AllocatorHandle, Database, DatabaseOptions, Error, Kind, Limits, MemoryUsage, Result, Snapshot,
    Transaction, Validation, Value,
    engine::OwnedRows,
    memory::{AllocatorCallbacks, ForeignAllocator},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct CDatabase {
    db: Database,
    handles: AtomicUsize,
}
pub struct Handle<T> {
    owner: *mut CDatabase,
    value: T,
}
type CValue = Handle<Value>;
type CSnapshot = Handle<Snapshot<'static>>;
type CTransaction = Handle<Transaction<'static>>;
type CScan = Handle<OwnedRows>;
impl<T> Drop for Handle<T> {
    fn drop(&mut self) {
        // Close cannot free this owner until every handle has been destroyed.
        let owner = unsafe { &*self.owner };
        owner.handles.fetch_sub(1, Ordering::AcqRel);
    }
}
fn boundary(f: impl FnOnce() -> Result<()>) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => e as i32,
        Err(_) => Error::Panic as i32,
    }
}
unsafe fn db<'a>(ptr: *const CDatabase) -> Result<&'a CDatabase> {
    unsafe { ptr.as_ref() }.ok_or(Error::Invalid)
}
unsafe fn handle<'a, T>(ptr: *const Handle<T>) -> Result<&'a Handle<T>> {
    unsafe { ptr.as_ref() }.ok_or(Error::Invalid)
}
unsafe fn handle_mut<'a, T>(ptr: *mut Handle<T>) -> Result<&'a mut Handle<T>> {
    unsafe { ptr.as_mut() }.ok_or(Error::Invalid)
}
unsafe fn output<T>(out: *mut *mut T) -> Result<()> {
    if out.is_null() {
        return Err(Error::Invalid);
    }
    unsafe {
        *out = ptr::null_mut();
    }
    Ok(())
}
unsafe fn input<'a>(owner: *mut CDatabase, bytes: *const u8, len: usize) -> Result<&'a [u8]> {
    if len > unsafe { db(owner)? }.db.limits().max_bytes {
        return Err(Error::Limit);
    }
    if len == 0 {
        return Ok(&[]);
    }
    if bytes.is_null() {
        return Err(Error::Invalid);
    }
    Ok(unsafe { slice::from_raw_parts(bytes, len) })
}
unsafe fn make_handle<T>(owner: *mut CDatabase, value: T, out: *mut *mut Handle<T>) -> Result<()> {
    let db = unsafe { db(owner)? };
    db.handles
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| Error::Limit)?;
    // On failure the temporary Handle drops and rolls back the count.
    let raw = db.db.mem.raw(Handle { owner, value })?;
    unsafe {
        *out = raw;
    }
    Ok(())
}
fn same(a: *mut CDatabase, b: *mut CDatabase) -> Result<()> {
    if a == b {
        Ok(())
    } else {
        Err(Error::ForeignStore)
    }
}
unsafe fn copy_bytes(
    bytes: &[u8],
    buffer: *mut u8,
    capacity: usize,
    required: *mut usize,
) -> Result<()> {
    if required.is_null() {
        return Err(Error::Invalid);
    }
    unsafe {
        *required = bytes.len();
    }
    if buffer.is_null() && capacity != 0 {
        return Err(Error::Invalid);
    }
    if capacity < bytes.len() {
        return Err(Error::Buffer);
    }
    if !bytes.is_empty() {
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
        }
    }
    Ok(())
}
macro_rules! export {
    ($name:ident($($arg:ident : $ty:ty),*) $body:block) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($($arg:$ty),*) -> i32 {
            boundary(|| unsafe { $body })
        }
    }
}
fn into_database_handle(database: Database) -> Result<*mut CDatabase> {
    let mem = database.mem.clone();
    mem.raw(CDatabase {
        db: database,
        handles: AtomicUsize::new(0),
    })
}

#[repr(C)]
pub struct CDatabaseOptions {
    limits: Limits,
    allocator: *const AllocatorCallbacks,
}

unsafe fn database_options(raw: *const CDatabaseOptions) -> Result<DatabaseOptions> {
    let Some(raw) = (unsafe { raw.as_ref() }) else {
        return Ok(DatabaseOptions::default());
    };
    let options = DatabaseOptions::from(raw.limits.validate()?);
    let Some(callbacks) = (unsafe { raw.allocator.as_ref() }) else {
        return Ok(options);
    };
    let provider = unsafe { ForeignAllocator::new(*callbacks)? };
    Ok(options.with_allocator(AllocatorHandle::try_new(provider)?))
}

export!(wdb_database_options_init(out: *mut CDatabaseOptions) {
    if out.is_null() { return Err(Error::Invalid); }
    *out = CDatabaseOptions { limits: Limits::default(), allocator: ptr::null() };
    Ok(())
});
export!(wdb_database_get_limits(database: *const CDatabase, out: *mut Limits) {
    if out.is_null() { return Err(Error::Invalid); }
    *out = db(database)?.db.limits();
    Ok(())
});
export!(wdb_database_get_memory_usage(database: *const CDatabase, out: *mut MemoryUsage) {
    if out.is_null() { return Err(Error::Invalid); }
    *out = db(database)?.db.memory_usage();
    Ok(())
});

export!(wdb_open(out: *mut *mut CDatabase) {
    output(out)?;
    *out = into_database_handle(Database::new()?)?;
    Ok(())
});
export!(wdb_open_with_options(options: *const CDatabaseOptions, out: *mut *mut CDatabase) {
    output(out)?;
    *out = into_database_handle(Database::with_options(database_options(options)?)?)?;
    Ok(())
});
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
unsafe fn path<'a>(bytes: *const u8, len: usize) -> Result<&'a std::path::Path> {
    if bytes.is_null() || len == 0 {
        return Err(Error::Invalid);
    }
    if len > 1024 * 1024 {
        return Err(Error::Limit);
    }
    let bytes = unsafe { slice::from_raw_parts(bytes, len) };
    if bytes.contains(&0) {
        return Err(Error::Invalid);
    }
    #[cfg(windows)]
    let name = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    #[cfg(not(windows))]
    let name = {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(bytes)
    };
    Ok(std::path::Path::new(name))
}
macro_rules! file_factory {
    ($name:ident,$method:ident)=>{export!($name(bytes:*const u8,len:usize,out:*mut *mut CDatabase) {
        output(out)?;
        #[cfg(any(target_os="macos",target_os="linux",windows))]
        {
            let database=Database::$method(path(bytes,len)?,crate::Limits::default())?;
            *out = into_database_handle(database)?; Ok(())
        }
        #[cfg(not(any(target_os="macos",target_os="linux",windows)))]
        {let _=(bytes,len);Err(Error::UnsupportedPlatform)}
    });};
}
file_factory!(wdb_file_create, create_file);
file_factory!(wdb_file_open, open_file);
macro_rules! file_factory_with_options {
    ($name:ident,$method:ident) => { export!($name(
        bytes:*const u8,len:usize,options:*const CDatabaseOptions,out:*mut *mut CDatabase
    ) {
        output(out)?;
        #[cfg(any(target_os="macos",target_os="linux",windows))]
        {
            let name = path(bytes,len)?;
            let database = Database::$method(name, database_options(options)?)?;
            *out = into_database_handle(database)?; Ok(())
        }
        #[cfg(not(any(target_os="macos",target_os="linux",windows)))]
        {let _=(bytes,len,options);Err(Error::UnsupportedPlatform)}
    }); };
}
file_factory_with_options!(wdb_file_create_with_options, create_file_with_options);
file_factory_with_options!(wdb_file_open_with_options, open_file_with_options);
export!(wdb_file_compact(bytes: *const u8, len: usize, before: *mut u64, after: *mut u64) {
    if before.is_null() || after.is_null() {return Err(Error::Invalid);}
    *before = 0; *after = 0;
    #[cfg(any(target_os="macos",target_os="linux"))]
    {
        let stats = Database::compact_file(path(bytes,len)?, crate::Limits::default())?;
        *before = stats.before_bytes; *after = stats.after_bytes; Ok(())
    }
    #[cfg(not(any(target_os="macos",target_os="linux")))]
    {let _ = (bytes,len); Err(Error::UnsupportedPlatform)}
});
export!(wdb_snapshot_retain(database:*mut CDatabase,snapshot:*const CSnapshot,out:*mut u64) {
    if out.is_null() {return Err(Error::Invalid);}*out=0;
    let snapshot=handle(snapshot)?;same(database,snapshot.owner)?;
    *out=db(database)?.db.retain_snapshot(&snapshot.value)?;Ok(())
});
export!(wdb_retained_release(database:*mut CDatabase,id:u64) {db(database)?.db.release_retained(id)});
export!(wdb_retained_open(database:*mut CDatabase,id:u64,out:*mut *mut CSnapshot) {
    output(out)?;make_handle(database,db(database)?.db.retained_snapshot(id)?,out)
});
export!(wdb_retained_ids(database:*mut CDatabase,buffer:*mut u64,capacity:usize,required:*mut usize) {
    if required.is_null() {return Err(Error::Invalid);}*required=0;
    if buffer.is_null() && capacity!=0 {return Err(Error::Invalid);}
    let ids=db(database)?.db.retained_ids_owned()?;*required=ids.len();
    if capacity<ids.len() {return Err(Error::Buffer);}
    if !ids.is_empty() {ptr::copy_nonoverlapping(ids.as_ptr(),buffer,ids.len());}Ok(())
});
export!(wdb_close(database: *mut CDatabase) {
    if db(database)?.handles.load(Ordering::Acquire) != 0 { return Err(Error::Busy); }
    let mem = db(database)?.db.mem.clone();
    mem.free_raw(database); Ok(())
});
export!(wdb_collection_create(database: *mut CDatabase, out: *mut u64) {
    if out.is_null() { return Err(Error::Invalid); } *out = 0;
    *out = db(database)?.db.create_collection()?.as_u64(); Ok(())
});
export!(wdb_collection_drop(database: *mut CDatabase, id: u64) {
    let db = db(database)?; db.db.drop_collection(db.db.collection_id(id))
});
export!(wdb_value_make(database: *mut CDatabase, kind: i32, bits: u64, bytes: *const u8, len: usize, out: *mut *mut CValue) {
    output(out)?; let db = db(database)?;
    let value = match kind {
        5 if bits == 0 => db.db.bytes(input(database, bytes, len)?)?,
        0..=7 if len == 0 => {
            let kind = match kind { 0 => Kind::None, 1 => Kind::Boolean, 2 => Kind::Integer,
                3 => Kind::Unsigned, 4 => Kind::Float, 6 => Kind::Map, 7 => Kind::List, _ => return Err(Error::Invalid) };
            Value::scalar(&db.db.mem, kind, bits)?
        }
        _ => return Err(Error::Invalid),
    };
    make_handle(database, value, out)
});
export!(wdb_value_clone(value: *const CValue, out: *mut *mut CValue) {
    output(out)?; let value = handle(value)?; make_handle(value.owner, value.value.clone(), out)
});
macro_rules! release {
    ($name:ident, $ty:ty) => { export!($name(value: *mut $ty) {
        if !value.is_null() {
            let owner = handle(value)?.owner;
            db(owner)?.db.mem.free_raw(value);
        }
        Ok(())
    }); };
}
release!(wdb_value_release, CValue);
release!(wdb_snapshot_release, CSnapshot);
release!(wdb_tx_release, CTransaction);
release!(wdb_scan_release, CScan);
export!(wdb_value_kind(value: *const CValue, out: *mut i32) {
    if out.is_null() { return Err(Error::Invalid); } *out = -1;
    *out = handle(value)?.value.kind() as i32; Ok(())
});
export!(wdb_value_scalar(value: *const CValue, out: *mut u64) {
    if out.is_null() { return Err(Error::Invalid); } *out = 0;
    let value = &handle(value)?.value;
    *out = match value.kind() { Kind::None => 0, Kind::Boolean => u64::from(value.as_bool()?),
        Kind::Integer => value.as_i64()? as u64, Kind::Unsigned => value.as_u64()?,
        Kind::Float => value.as_f64()?.to_bits(), _ => return Err(Error::Type) };
    Ok(())
});
export!(wdb_value_len(value: *const CValue, out: *mut usize) {
    if out.is_null() { return Err(Error::Invalid); } *out = 0;
    *out = handle(value)?.value.len()?; Ok(())
});
export!(wdb_value_bytes(value: *const CValue, buffer: *mut u8, capacity: usize, required: *mut usize) {
    if required.is_null() { return Err(Error::Invalid); } *required = 0;
    copy_bytes(handle(value)?.value.as_bytes()?, buffer, capacity, required)
});
export!(wdb_map_set(map: *const CValue, key: *const u8, len: usize, value: *const CValue, out: *mut *mut CValue) {
    output(out)?; let map = handle(map)?; let value = handle(value)?; same(map.owner, value.owner)?;
    make_handle(map.owner, map.value.map_set(input(map.owner, key, len)?, &value.value)?, out)
});
export!(wdb_map_get(map: *const CValue, key: *const u8, len: usize, out: *mut *mut CValue) {
    output(out)?; let map = handle(map)?;
    make_handle(map.owner, map.value.map_get(input(map.owner, key, len)?)?.ok_or(Error::NotFound)?, out)
});
export!(wdb_map_remove(map: *const CValue, key: *const u8, len: usize, out: *mut *mut CValue) {
    output(out)?; let map = handle(map)?;
    make_handle(map.owner, map.value.map_remove(input(map.owner, key, len)?)?, out)
});
export!(wdb_map_scan(map: *const CValue, out: *mut *mut CScan) {
    output(out)?; let map = handle(map)?; make_handle(map.owner, map.value.map_entries_owned()?, out)
});
export!(wdb_list_push(list: *const CValue, value: *const CValue, out: *mut *mut CValue) {
    output(out)?; let list = handle(list)?; let value = handle(value)?; same(list.owner, value.owner)?;
    make_handle(list.owner, list.value.list_push(&value.value)?, out)
});
macro_rules! indexed_edit {
    ($name:ident, $method:ident) => { export!($name(list: *const CValue, index: usize, value: *const CValue, out: *mut *mut CValue) {
        output(out)?; let list = handle(list)?; let value = handle(value)?; same(list.owner, value.owner)?;
        make_handle(list.owner, list.value.$method(index, &value.value)?, out)
    }); };
}
indexed_edit!(wdb_list_set, list_set);
indexed_edit!(wdb_list_insert, list_insert);
export!(wdb_list_remove(list: *const CValue, index: usize, out: *mut *mut CValue) {
    output(out)?; let list = handle(list)?; make_handle(list.owner, list.value.list_remove(index)?, out)
});
export!(wdb_list_get(list: *const CValue, index: usize, out: *mut *mut CValue) {
    output(out)?; let list = handle(list)?; make_handle(list.owner, list.value.list_get(index)?, out)
});
export!(wdb_snapshot_acquire(database: *mut CDatabase, out: *mut *mut CSnapshot) {
    output(out)?; make_handle(database, db(database)?.db.snapshot(), out)
});
export!(wdb_snapshot_revision(snapshot: *const CSnapshot, out: *mut u64) {
    if out.is_null() { return Err(Error::Invalid); } *out = 0;
    *out = handle(snapshot)?.value.revision(); Ok(())
});
export!(wdb_snapshot_get(snapshot: *const CSnapshot, id: u64, key: *const u8, len: usize, out: *mut *mut CValue) {
    output(out)?; let snapshot = handle(snapshot)?; let owner = db(snapshot.owner)?;
    let value = snapshot.value.get(owner.db.collection_id(id), input(snapshot.owner, key, len)?)?.ok_or(Error::NotFound)?;
    make_handle(snapshot.owner, value, out)
});
export!(wdb_snapshot_scan(snapshot: *const CSnapshot, id: u64, out: *mut *mut CScan) {
    output(out)?; let snapshot = handle(snapshot)?;
    make_handle(snapshot.owner, snapshot.value.scan_owned(db(snapshot.owner)?.db.collection_id(id))?, out)
});
export!(wdb_tx_begin(database: *mut CDatabase, validation: i32, out: *mut *mut CTransaction) {
    output(out)?;
    let mode = match validation { 0 => Validation::Records, 1 => Validation::WholeDatabase, _ => return Err(Error::Invalid) };
    make_handle(database, db(database)?.db.begin_with_validation(mode), out)
});
export!(wdb_tx_get(tx: *mut CTransaction, id: u64, key: *const u8, len: usize, out: *mut *mut CValue) {
    output(out)?; let tx = handle_mut(tx)?;
    let value = tx.value.get(db(tx.owner)?.db.collection_id(id), input(tx.owner, key, len)?)?.ok_or(Error::NotFound)?;
    make_handle(tx.owner, value, out)
});
export!(wdb_tx_put(tx: *mut CTransaction, id: u64, key: *const u8, len: usize, value: *const CValue) {
    let tx = handle_mut(tx)?; let value = handle(value)?; same(tx.owner, value.owner)?;
    tx.value.put(db(tx.owner)?.db.collection_id(id), input(tx.owner, key, len)?, &value.value)
});
export!(wdb_tx_delete(tx: *mut CTransaction, id: u64, key: *const u8, len: usize) {
    let tx = handle_mut(tx)?;
    tx.value.delete(db(tx.owner)?.db.collection_id(id), input(tx.owner, key, len)?)
});
export!(wdb_tx_scan(tx: *mut CTransaction, id: u64, out: *mut *mut CScan) {
    output(out)?; let tx = handle_mut(tx)?;
    let rows = tx.value.scan_owned(db(tx.owner)?.db.collection_id(id))?; make_handle(tx.owner, rows, out)
});
export!(wdb_tx_commit(tx: *mut CTransaction, out: *mut u64) {
    if out.is_null() { return Err(Error::Invalid); } *out = 0;
    *out = handle_mut(tx)?.value.commit()?; Ok(())
});
export!(wdb_tx_abort(tx: *mut CTransaction) { handle_mut(tx)?.value.abort() });
export!(wdb_scan_len(scan: *const CScan, out: *mut usize) {
    if out.is_null() { return Err(Error::Invalid); } *out = 0;
    *out = handle(scan)?.value.len(); Ok(())
});
export!(wdb_scan_key(scan: *const CScan, index: usize, buffer: *mut u8, capacity: usize, required: *mut usize) {
    if required.is_null() { return Err(Error::Invalid); } *required = 0;
    let scan = handle(scan)?; let (key, _) = scan.value.get(index).ok_or(Error::Bounds)?;
    copy_bytes(key, buffer, capacity, required)
});
export!(wdb_scan_value(scan: *const CScan, index: usize, out: *mut *mut CValue) {
    output(out)?; let scan = handle(scan)?; let (_, value) = scan.value.get(index).ok_or(Error::Bounds)?;
    make_handle(scan.owner, value.clone(), out)
});

#[unsafe(no_mangle)]
pub extern "C" fn wdb_abi_version() -> u32 {
    crate::version::ABI_VERSION
}
#[unsafe(no_mangle)]
pub extern "C" fn wdb_format_version() -> u32 {
    crate::version::FORMAT_VERSION
}
#[unsafe(no_mangle)]
pub extern "C" fn wdb_features() -> u32 {
    crate::version::FEATURES
}
export!(wdb_package_version(buffer: *mut u8, capacity: usize, required: *mut usize) {
    copy_bytes(crate::version::PACKAGE_VERSION.as_bytes(), buffer, capacity, required)
});
export!(wdb_source_id(buffer: *mut u8, capacity: usize, required: *mut usize) {
    copy_bytes(crate::version::SOURCE_ID.as_bytes(), buffer, capacity, required)
});

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_commit_contains_adapter_panic_and_requires_recovery() {
        use crate::storage::{ByteStore, SimulatedDisk};
        use std::sync::{Arc, atomic::AtomicBool};

        struct PanickingStore {
            disk: SimulatedDisk,
            armed: Arc<AtomicBool>,
        }
        impl ByteStore for PanickingStore {
            fn lock(&mut self) -> Result<()> {
                self.disk.lock()
            }
            fn unlock(&mut self) {
                self.disk.unlock();
            }
            fn length(&mut self) -> Result<u64> {
                self.disk.length()
            }
            fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
                self.disk.read(offset, out)
            }
            fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
                self.disk.write(offset, bytes)
            }
            fn sync(&mut self) -> Result<()> {
                self.disk.sync()?;
                assert!(
                    !self.armed.swap(false, Ordering::Relaxed),
                    "intentional sync panic"
                );
                Ok(())
            }
        }
        let disk = SimulatedDisk::new().unwrap();
        let armed = Arc::new(AtomicBool::new(false));
        let db = Database::create_on(
            PanickingStore {
                disk: disk.clone(),
                armed: armed.clone(),
            },
            *b"ffi-panic-test01",
            crate::Limits::default(),
        )
        .unwrap();
        let mem = db.mem.clone();
        let owner = mem
            .raw(CDatabase {
                db,
                handles: AtomicUsize::new(0),
            })
            .unwrap();
        unsafe {
            let mut tx = ptr::null_mut();
            assert_eq!(wdb_tx_begin(owner, 0, &mut tx), 0);
            armed.store(true, Ordering::Relaxed);
            let mut revision = u64::MAX;
            assert_eq!(wdb_tx_commit(tx, &mut revision), Error::Panic as i32);
            assert_eq!(revision, 0);
            assert_eq!(
                wdb_tx_commit(tx, &mut revision),
                Error::RecoveryRequired as i32
            );
            assert_eq!(revision, 0);
            assert_eq!(wdb_tx_release(tx), 0);
            assert_eq!(wdb_close(owner), 0);
        }
        let reopened = Database::open_on(disk, crate::Limits::default()).unwrap();
        assert_eq!(reopened.snapshot().revision(), 0);
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[test]
    fn file_path_null_length_and_nul_guards_reset_output() {
        unsafe {
            for (bytes, len, expected) in [
                (ptr::null(), 1, Error::Invalid),
                (b"x".as_ptr(), 0, Error::Invalid),
                (b"x".as_ptr(), 1024 * 1024 + 1, Error::Limit),
                (b"x\0y".as_ptr(), 3, Error::Invalid),
            ] {
                let mut database = ptr::dangling_mut();
                assert_eq!(wdb_file_open(bytes, len, &mut database), expected as i32);
                assert!(database.is_null());
            }
            // A valid byte span reaches path conversion without dereferencing
            // any output pointer or touching the filesystem.
            assert_eq!(path(b"x".as_ptr(), 1).unwrap(), std::path::Path::new("x"));
        }
    }
    #[test]
    fn byte_buffers_distinguish_null_capacity_and_required_length() {
        unsafe {
            let mut database = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            for bytes in [b"".as_slice(), b"x".as_slice(), b"abc".as_slice()] {
                let mut value = ptr::null_mut();
                assert_eq!(
                    wdb_value_make(database, 5, 0, bytes.as_ptr(), bytes.len(), &mut value),
                    0
                );
                for (null_buffer, capacity) in
                    [(true, 0), (true, 3), (false, 0), (false, 2), (false, 3)]
                {
                    let mut buffer = [0xa5; 3];
                    let destination = if null_buffer {
                        ptr::null_mut()
                    } else {
                        buffer.as_mut_ptr()
                    };
                    let mut required = usize::MAX;
                    let expected = if null_buffer && capacity != 0 {
                        Error::Invalid as i32
                    } else if capacity < bytes.len() {
                        Error::Buffer as i32
                    } else {
                        0
                    };
                    assert_eq!(
                        wdb_value_bytes(value, destination, capacity, &mut required),
                        expected
                    );
                    assert_eq!(required, bytes.len());
                    let mut expected_buffer = [0xa5; 3];
                    if expected == 0 {
                        expected_buffer[..bytes.len()].copy_from_slice(bytes);
                    }
                    assert_eq!(buffer, expected_buffer);
                }
                let mut untouched = [0xa5; 3];
                assert_eq!(
                    wdb_value_bytes(value, untouched.as_mut_ptr(), 3, ptr::null_mut()),
                    Error::Invalid as i32
                );
                assert_eq!(untouched, [0xa5; 3]);
                assert_eq!(wdb_value_release(value), 0);
            }
            let mut required = usize::MAX;
            assert_eq!(
                wdb_value_bytes(ptr::null(), ptr::null_mut(), 0, &mut required),
                Error::Invalid as i32
            );
            assert_eq!(required, 0);
            // The version export reaches the shared guard directly.
            let mut untouched = [0xa5; 3];
            assert_eq!(
                wdb_package_version(untouched.as_mut_ptr(), untouched.len(), ptr::null_mut()),
                Error::Invalid as i32
            );
            assert_eq!(untouched, [0xa5; 3]);
            assert_eq!(wdb_close(database), 0);
        }
    }

    #[test]
    fn input_length_kind_and_output_guards_preserve_handle_ownership() {
        unsafe {
            let mut database = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            let byte = [7u8];
            for (kind, bits, bytes, len, expected) in [
                (5, 0, ptr::null(), 0, 0),
                (5, 0, byte.as_ptr(), 1, 0),
                (5, 0, ptr::null(), 1, Error::Invalid as i32),
                (5, 0, ptr::null(), 1024 * 1024 + 1, Error::Limit as i32),
                (5, 1, ptr::null(), 0, Error::Invalid as i32),
                (2, 7, ptr::null(), 0, 0),
                (2, 7, byte.as_ptr(), 1, Error::Invalid as i32),
                (-1, 0, ptr::null(), 0, Error::Invalid as i32),
                (8, 0, ptr::null(), 0, Error::Invalid as i32),
                (1, 2, ptr::null(), 0, Error::Invalid as i32),
            ] {
                // The sentinel is output-only and is never dereferenced.
                let mut value = ptr::dangling_mut();
                assert_eq!(
                    wdb_value_make(database, kind, bits, bytes, len, &mut value),
                    expected
                );
                if expected == 0 {
                    assert!(!value.is_null());
                    assert_eq!(wdb_value_release(value), 0);
                } else {
                    assert!(value.is_null());
                }
                assert_eq!((*database).handles.load(Ordering::Acquire), 0);
            }
            assert_eq!(
                wdb_value_make(database, 0, 0, ptr::null(), 0, ptr::null_mut()),
                Error::Invalid as i32
            );
            let mut output = ptr::dangling_mut();
            assert_eq!(
                wdb_value_clone(ptr::null(), &mut output),
                Error::Invalid as i32
            );
            assert!(output.is_null());
            assert_eq!(wdb_value_release(ptr::null_mut()), 0);
            assert_eq!(wdb_snapshot_release(ptr::null_mut()), 0);
            assert_eq!(wdb_scan_release(ptr::null_mut()), 0);
            assert_eq!(wdb_tx_release(ptr::null_mut()), 0);
            assert_eq!(wdb_close(database), 0);
        }
    }

    #[test]
    fn optional_database_options_and_byte_input_preserve_the_inclusive_limit() {
        unsafe {
            let mut database = ptr::dangling_mut();
            assert_eq!(wdb_open_with_options(ptr::null(), &mut database), 0);
            let mut limits = Limits {
                max_bytes: 0,
                ..Limits::default()
            };
            assert_eq!(wdb_database_get_limits(database, &mut limits), 0);
            assert_eq!(limits, Limits::default());
            assert_eq!(wdb_close(database), 0);

            let options = CDatabaseOptions {
                limits: Limits {
                    max_bytes: 2,
                    ..Limits::default()
                },
                allocator: ptr::null(),
            };
            assert_eq!(wdb_open_with_options(&options, &mut database), 0);
            assert_eq!(wdb_database_get_limits(database, &mut limits), 0);
            assert_eq!(limits, options.limits);
            for bytes in [b"".as_slice(), b"x".as_slice(), b"xy".as_slice()] {
                let input = if bytes.is_empty() {
                    ptr::null()
                } else {
                    bytes.as_ptr()
                };
                let mut value = ptr::dangling_mut();
                assert_eq!(
                    wdb_value_make(database, 5, 0, input, bytes.len(), &mut value),
                    0
                );
                let mut output = [0xa5; 2];
                let mut required = usize::MAX;
                assert_eq!(
                    wdb_value_bytes(value, output.as_mut_ptr(), output.len(), &mut required),
                    0
                );
                assert_eq!(required, bytes.len());
                assert_eq!(&output[..required], bytes);
                assert_eq!(wdb_value_release(value), 0);
            }
            let mut value = ptr::dangling_mut();
            assert_eq!(
                wdb_value_make(database, 5, 0, b"xyz".as_ptr(), 3, &mut value),
                Error::Limit as i32
            );
            assert!(value.is_null());
            assert_eq!(wdb_close(database), 0);
        }
    }

    #[test]
    fn null_owner_is_rejected_before_constructing_a_slice_or_handle() {
        let value = Database::new().unwrap().unsigned(7).unwrap();
        let mut out = ptr::null_mut();
        unsafe {
            assert_eq!(
                input(ptr::null_mut(), b"x".as_ptr(), 1),
                Err(Error::Invalid)
            );
            assert_eq!(
                make_handle(ptr::null_mut(), value, &mut out),
                Err(Error::Invalid)
            );
        }
        assert!(out.is_null());
    }

    #[test]
    fn foreign_allocator_owns_context_through_snapshot_and_handle_release() {
        use crate::memory::test_allocator::TestAllocator;
        use std::{
            alloc::{Allocator, Layout},
            ffi::c_void,
            sync::Arc,
        };

        unsafe extern "C" fn allocate(
            context: *mut c_void,
            size: usize,
            align: usize,
        ) -> *mut c_void {
            let state = unsafe { &*context.cast::<TestAllocator>() };
            match state.allocate(Layout::from_size_align(size, align).unwrap()) {
                Ok(block) => block.cast::<u8>().as_ptr().cast(),
                Err(_) => ptr::null_mut(),
            }
        }
        unsafe extern "C" fn deallocate(
            context: *mut c_void,
            block: *mut c_void,
            size: usize,
            align: usize,
        ) {
            let state = unsafe { &*context.cast::<TestAllocator>() };
            let block = std::ptr::NonNull::new(block.cast::<u8>()).unwrap();
            unsafe { state.deallocate(block, Layout::from_size_align(size, align).unwrap()) };
        }
        unsafe extern "C" fn retain(context: *mut c_void) {
            unsafe { Arc::increment_strong_count(context.cast::<TestAllocator>()) };
        }
        unsafe extern "C" fn release(context: *mut c_void) {
            let state = unsafe { &*context.cast::<TestAllocator>() };
            assert_eq!(state.live(), 0);
            unsafe { Arc::decrement_strong_count(context.cast::<TestAllocator>()) };
        }

        let state = Arc::new(TestAllocator::default());
        let weak = Arc::downgrade(&state);
        let callbacks = AllocatorCallbacks {
            context: Arc::as_ptr(&state).cast_mut().cast(),
            allocate: Some(allocate),
            deallocate: Some(deallocate),
            retain_context: Some(retain),
            release_context: Some(release),
        };
        unsafe {
            let invalid = AllocatorCallbacks {
                release_context: None,
                ..callbacks
            };
            let mut database = ptr::dangling_mut();
            assert_eq!(
                wdb_open_with_options(
                    &CDatabaseOptions {
                        limits: Limits::default(),
                        allocator: &invalid
                    },
                    &mut database
                ),
                Error::Invalid as i32
            );
            assert!(database.is_null());
            assert_eq!(Arc::strong_count(&state), 1);

            let mut options = std::mem::MaybeUninit::uninit();
            assert_eq!(wdb_database_options_init(options.as_mut_ptr()), 0);
            let mut options = options.assume_init();
            assert_eq!(options.limits, Limits::default());
            assert!(options.allocator.is_null());
            options.limits.max_bytes = 3;
            options.allocator = &callbacks;
            let invalid_limits = CDatabaseOptions {
                limits: Limits {
                    max_depth: 0,
                    ..options.limits
                },
                allocator: options.allocator,
            };
            let before = state.requests();
            database = ptr::dangling_mut();
            assert_eq!(
                wdb_open_with_options(&invalid_limits, &mut database),
                Error::Invalid as i32
            );
            assert!(database.is_null());
            assert_eq!(state.requests(), before);
            assert_eq!(Arc::strong_count(&state), 1);
            // Refuse the provider, budget, initial core head, and C handle blocks.
            for allowed in 0..4 {
                state.fail_after(allowed);
                assert_eq!(
                    wdb_open_with_options(&options, &mut database),
                    Error::OutOfMemory as i32
                );
                assert!(database.is_null());
                assert_eq!(Arc::strong_count(&state), 1);
                assert_eq!(state.live(), 0);
            }

            assert_eq!(wdb_open_with_options(&options, &mut database), 0);
            assert!(state.live() > 0);
            let mut limits = Limits::default();
            assert_eq!(wdb_database_get_limits(database, &mut limits), 0);
            assert_eq!(limits, options.limits);
            let mut value = ptr::dangling_mut();
            assert_eq!(
                wdb_value_make(database, 5, 0, b"four".as_ptr(), 4, &mut value),
                Error::Limit as i32
            );
            assert!(value.is_null());
            drop(state);
            assert_eq!(weak.strong_count(), 1);

            let mut snapshot = ptr::dangling_mut();
            weak.upgrade().unwrap().fail_next();
            assert_eq!(
                wdb_snapshot_acquire(database, &mut snapshot),
                Error::OutOfMemory as i32
            );
            assert!(snapshot.is_null());
            assert_eq!((*database).handles.load(Ordering::Acquire), 0);

            assert_eq!(wdb_snapshot_acquire(database, &mut snapshot), 0);
            let mut id = 0;
            assert_eq!(wdb_snapshot_retain(database, snapshot, &mut id), 0);
            assert_eq!(wdb_snapshot_release(snapshot), 0);
            let mut retained = ptr::null_mut();
            assert_eq!(wdb_retained_open(database, id, &mut retained), 0);
            assert_eq!(wdb_retained_release(database, id), 0);
            assert_eq!(wdb_close(database), Error::Busy as i32);
            let mut revision = u64::MAX;
            assert_eq!(wdb_snapshot_revision(retained, &mut revision), 0);
            assert_eq!(revision, 0);
            assert_eq!(wdb_snapshot_release(retained), 0);

            let state = weak.upgrade().unwrap();
            let mut collection = 0;
            let mut tx = ptr::null_mut();
            assert_eq!(wdb_collection_create(database, &mut collection), 0);
            assert_eq!(wdb_tx_begin(database, 0, &mut tx), 0);
            assert_eq!(
                wdb_value_make(database, 5, 0, b"val".as_ptr(), 3, &mut value),
                0
            );
            assert_eq!(wdb_tx_put(tx, collection, b"k".as_ptr(), 1, value), 0);
            assert_eq!(wdb_tx_commit(tx, &mut revision), 0);
            assert_eq!(wdb_tx_release(tx), 0);
            assert_eq!(wdb_value_release(value), 0);
            assert_eq!(wdb_snapshot_acquire(database, &mut snapshot), 0);
            let before = state.live_bytes();
            let mut usage = std::mem::MaybeUninit::uninit();
            assert_eq!(
                wdb_database_get_memory_usage(database, usage.as_mut_ptr()),
                0
            );
            let before_charge = usage.assume_init_read().live_bytes;
            let mut scan = ptr::null_mut();
            assert_eq!(wdb_snapshot_scan(snapshot, collection, &mut scan), 0);
            assert_eq!(
                wdb_database_get_memory_usage(database, usage.as_mut_ptr()),
                0
            );
            let added_charge = usage.assume_init().live_bytes - before_charge;
            assert!(added_charge > 0);
            assert_eq!(
                state.live_bytes() - before,
                added_charge + std::mem::size_of::<CScan>()
            );
            assert_eq!(wdb_scan_release(scan), 0);
            assert_eq!(state.live_bytes(), before);
            assert_eq!(wdb_snapshot_release(snapshot), 0);
            assert_eq!(wdb_close(database), 0);
            assert_eq!(state.live(), 0);
            assert_eq!(Arc::strong_count(&state), 1);
            drop(state);
            assert!(weak.upgrade().is_none());
        }
    }

    #[test]
    fn retained_id_buffers_and_foreign_snapshots_preserve_outputs() {
        unsafe {
            let mut database = ptr::null_mut();
            let mut foreign = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            assert_eq!(wdb_open(&mut foreign), 0);
            let mut snapshot = ptr::null_mut();
            assert_eq!(wdb_snapshot_acquire(database, &mut snapshot), 0);
            let mut id = u64::MAX;
            assert_eq!(
                wdb_snapshot_retain(foreign, snapshot, &mut id),
                Error::ForeignStore as i32
            );
            assert_eq!(id, 0);
            assert_eq!(
                wdb_snapshot_retain(database, snapshot, ptr::null_mut()),
                Error::Invalid as i32
            );
            for populated in [false, true] {
                if populated {
                    assert_eq!(wdb_snapshot_retain(database, snapshot, &mut id), 0);
                    assert_eq!(id, 1);
                }
                for (null_buffer, capacity) in [(true, 0), (true, 1), (false, 0), (false, 1)] {
                    let mut buffer = u64::MAX;
                    let destination = if null_buffer {
                        ptr::null_mut()
                    } else {
                        &mut buffer
                    };
                    let mut required = usize::MAX;
                    let expected = if null_buffer && capacity != 0 {
                        Error::Invalid as i32
                    } else if populated && capacity == 0 {
                        Error::Buffer as i32
                    } else {
                        0
                    };
                    assert_eq!(
                        wdb_retained_ids(database, destination, capacity, &mut required),
                        expected
                    );
                    assert_eq!(
                        required,
                        usize::from(populated && expected != Error::Invalid as i32)
                    );
                    assert_eq!(
                        buffer,
                        if populated && expected == 0 {
                            1
                        } else {
                            u64::MAX
                        }
                    );
                }
            }
            let mut buffer = u64::MAX;
            assert_eq!(
                wdb_retained_ids(database, &mut buffer, 1, ptr::null_mut()),
                Error::Invalid as i32
            );
            assert_eq!(buffer, u64::MAX);
            assert_eq!(wdb_snapshot_release(snapshot), 0);
            assert_eq!(wdb_close(foreign), 0);
            assert_eq!(wdb_close(database), 0);
        }
    }

    #[test]
    fn handle_count_exhaustion_rejects_clone_without_losing_source() {
        unsafe {
            let mut database = ptr::null_mut();
            let mut value = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            assert_eq!(
                wdb_value_make(database, 2, 7, ptr::null(), 0, &mut value),
                0
            );
            // Construct only the otherwise impractical counter boundary.
            (*database).handles.store(usize::MAX, Ordering::Relaxed);
            let mut clone = ptr::dangling_mut();
            assert_eq!(wdb_value_clone(value, &mut clone), Error::Limit as i32);
            assert!(clone.is_null());
            assert_eq!((*database).handles.load(Ordering::Relaxed), usize::MAX);
            (*database).handles.store(1, Ordering::Relaxed);
            assert_eq!(wdb_value_clone(value, &mut clone), 0);
            assert_eq!(wdb_value_release(clone), 0);
            let mut bits = 0;
            assert_eq!(wdb_value_scalar(value, &mut bits), 0);
            assert_eq!(bits, 7);
            assert_eq!(wdb_value_release(value), 0);
            assert_eq!(wdb_close(database), 0);
        }
    }
    #[test]
    fn scan_bounds_and_invalid_transaction_modes_reset_outputs() {
        unsafe {
            let mut database = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            let mut map = ptr::null_mut();
            let mut item = ptr::null_mut();
            let mut populated = ptr::null_mut();
            assert_eq!(wdb_value_make(database, 6, 0, ptr::null(), 0, &mut map), 0);
            assert_eq!(wdb_value_make(database, 2, 7, ptr::null(), 0, &mut item), 0);
            assert_eq!(wdb_map_set(map, b"k".as_ptr(), 1, item, &mut populated), 0);
            let mut scan = ptr::null_mut();
            assert_eq!(wdb_map_scan(populated, &mut scan), 0);
            for index in [0, 1, usize::MAX] {
                let mut key = [0xa5];
                let mut required = usize::MAX;
                let mut value = ptr::dangling_mut();
                let expected = if index == 0 { 0 } else { Error::Bounds as i32 };
                assert_eq!(
                    wdb_scan_key(scan, index, key.as_mut_ptr(), 1, &mut required),
                    expected
                );
                assert_eq!(wdb_scan_value(scan, index, &mut value), expected);
                if index == 0 {
                    assert_eq!(key, *b"k");
                    assert_eq!(required, 1);
                    assert_eq!(wdb_value_release(value), 0);
                } else {
                    assert_eq!(key, [0xa5]);
                    assert_eq!(required, 0);
                    assert!(value.is_null());
                }
            }
            for mode in [-1, 0, 1, 2] {
                let mut tx = ptr::dangling_mut();
                let status = wdb_tx_begin(database, mode, &mut tx);
                if (0..=1).contains(&mode) {
                    assert_eq!(status, 0);
                    assert_eq!(wdb_tx_abort(tx), 0);
                    let mut revision = u64::MAX;
                    assert_eq!(wdb_tx_commit(tx, &mut revision), Error::Closed as i32);
                    assert_eq!(revision, 0);
                    assert_eq!(wdb_tx_release(tx), 0);
                } else {
                    assert_eq!(status, Error::Invalid as i32);
                    assert!(tx.is_null());
                }
            }
            assert_eq!(wdb_scan_release(scan), 0);
            for value in [map, item, populated] {
                assert_eq!(wdb_value_release(value), 0);
            }
            assert_eq!(wdb_close(database), 0);
        }
    }
    #[test]
    fn shared_foreign_handles_count_concurrent_acquisition_and_release() {
        unsafe {
            let mut database = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            let mut shared = ptr::null_mut();
            assert_eq!(
                wdb_value_make(
                    database,
                    Kind::Unsigned as i32,
                    7,
                    ptr::null(),
                    0,
                    &mut shared
                ),
                0
            );
            // The raw C contract requires explicit lifetime/quiescence. Keep
            // the database/source handle alive until every worker has joined.
            let (address, source) = (database as usize, shared as usize);
            std::thread::scope(|s| {
                for _ in 0..8 {
                    s.spawn(move || {
                        for _ in 0..1000 {
                            let mut value = ptr::null_mut();
                            let mut snapshot = ptr::null_mut();
                            assert_eq!(wdb_value_clone(source as *const CValue, &mut value), 0);
                            assert_eq!(
                                wdb_snapshot_acquire(address as *mut CDatabase, &mut snapshot),
                                0
                            );
                            let mut bits = 0;
                            assert_eq!(wdb_value_scalar(value, &mut bits), 0);
                            assert_eq!(bits, 7);
                            assert_eq!(wdb_value_release(value), 0);
                            assert_eq!(wdb_snapshot_release(snapshot), 0);
                        }
                    });
                }
            });
            assert_eq!((*database).handles.load(Ordering::Acquire), 1);
            assert_eq!(wdb_close(database), Error::Busy as i32);
            assert_eq!(wdb_value_release(shared), 0);
            assert_eq!(wdb_close(database), 0);
        }
    }
    #[test]
    fn foreign_handle_allocation_failure_rolls_back_live_count() {
        use crate::memory::test_allocator::TestAllocator;
        use std::sync::Arc;

        let allocator = Arc::new(TestAllocator::default());
        unsafe {
            let database =
                into_database_handle(Database::with_options(allocator.options()).unwrap()).unwrap();
            let live = allocator.live_bytes();
            for budget in 0..3 {
                let mut value = ptr::null_mut();
                let (status, actual) = allocator.count_with_refusal(budget, || {
                    wdb_value_make(database, 0, 0, ptr::null(), 0, &mut value)
                });
                assert_eq!(actual, (budget + 1).min(2));
                assert_eq!(status == 0, budget == 2);
                if status == 0 {
                    assert_eq!(wdb_value_release(value), 0);
                } else {
                    assert_eq!(status, Error::OutOfMemory as i32);
                    assert!(value.is_null());
                }
                assert_eq!((*database).handles.load(Ordering::Acquire), 0);
                assert_eq!(allocator.live_bytes(), live);
            }
            assert_eq!(wdb_close(database), 0);
        }
        assert_eq!(allocator.live(), 0);
    }
    #[test]
    fn concurrent_handle_allocation_refusal_keeps_only_the_successful_clone() {
        use crate::memory::test_allocator::TestAllocator;
        use std::sync::{Arc, Barrier, atomic::AtomicPtr};

        let allocator = Arc::new(TestAllocator::default());
        unsafe {
            let database =
                into_database_handle(Database::with_options(allocator.options()).unwrap()).unwrap();
            let mut source = ptr::null_mut();
            assert_eq!(
                wdb_value_make(
                    database,
                    Kind::Unsigned as i32,
                    7,
                    ptr::null(),
                    0,
                    &mut source
                ),
                0
            );
            let live = allocator.live();
            // The source stays live and immutable until both calls have returned.
            let shared = AtomicPtr::new(source);
            let outputs = [const { AtomicPtr::new(ptr::null_mut()) }; 2];
            let start = Barrier::new(outputs.len());
            let (statuses, requests) = allocator.count_with_refusal(1, || {
                std::thread::scope(|scope| {
                    let workers = outputs.each_ref().map(|output| {
                        let (shared, start) = (&shared, &start);
                        scope.spawn(move || {
                            start.wait();
                            let mut cloned = ptr::null_mut();
                            let status =
                                wdb_value_clone(shared.load(Ordering::Relaxed), &mut cloned);
                            output.store(cloned, Ordering::Relaxed);
                            status
                        })
                    });
                    workers.map(|worker| worker.join().unwrap())
                })
            });
            assert_eq!(requests, 2);
            assert_eq!(statuses.iter().filter(|&&status| status == 0).count(), 1);
            assert_eq!((*database).handles.load(Ordering::Acquire), 2);
            assert_eq!(allocator.live(), live + 1);
            assert_eq!(wdb_close(database), Error::Busy as i32);
            for (status, output) in statuses.into_iter().zip(outputs) {
                let cloned = output.into_inner();
                if status == 0 {
                    let mut bits = 0;
                    assert_eq!(wdb_value_scalar(cloned, &mut bits), 0);
                    assert_eq!(bits, 7);
                    assert_eq!(wdb_value_release(cloned), 0);
                } else {
                    assert_eq!(status, Error::OutOfMemory as i32);
                    assert!(cloned.is_null());
                }
            }
            assert_eq!((*database).handles.load(Ordering::Acquire), 1);
            assert_eq!(allocator.live(), live);
            assert_eq!(wdb_value_release(source), 0);
            assert_eq!(wdb_close(database), 0);
        }
        assert_eq!(allocator.live(), 0);
    }

    #[test]
    fn invalid_commit_output_cannot_publish_and_live_handles_block_close() {
        unsafe {
            let mut database = ptr::null_mut();
            let mut tx = ptr::null_mut();
            let mut revision = 0;
            assert_eq!(wdb_open(&mut database), 0);
            assert_eq!(wdb_tx_begin(database, 0, &mut tx), 0);
            assert_eq!(wdb_close(database), Error::Busy as i32);
            assert_eq!(wdb_tx_commit(tx, ptr::null_mut()), Error::Invalid as i32);
            assert_eq!((*database).db.snapshot().revision(), 0);
            assert_eq!(wdb_tx_commit(tx, &mut revision), 0);
            assert_eq!(revision, 1);
            assert_eq!(wdb_tx_release(tx), 0);
            assert_eq!(wdb_close(database), 0);
        }
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    #[test]
    fn scalar_arms_and_output_failures_preserve_valid_handles() {
        unsafe {
            let mut database = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            for (kind, bits) in [
                (0, 0),
                (1, 1),
                (2, u64::MAX),
                (3, u64::MAX),
                (4, f64::NAN.to_bits()),
                (5, 0),
                (6, 0),
                (7, 0),
            ] {
                let mut value = ptr::null_mut();
                assert_eq!(
                    wdb_value_make(database, kind, bits, ptr::null(), 0, &mut value),
                    0
                );
                let mut actual_kind = -9;
                assert_eq!(wdb_value_kind(value, &mut actual_kind), 0);
                assert_eq!(actual_kind, kind);
                assert_eq!(
                    wdb_value_kind(value, ptr::null_mut()),
                    Error::Invalid as i32
                );
                let mut scalar = 99;
                assert_eq!(
                    wdb_value_scalar(value, &mut scalar),
                    if kind <= 4 { 0 } else { Error::Type as i32 }
                );
                assert_eq!(scalar, if kind <= 4 { bits } else { 0 });
                assert_eq!(
                    wdb_value_scalar(value, ptr::null_mut()),
                    Error::Invalid as i32
                );
                let mut len = 99;
                assert_eq!(
                    wdb_value_len(value, &mut len),
                    if kind >= 5 { 0 } else { Error::Type as i32 }
                );
                assert_eq!(len, 0);
                assert_eq!(wdb_value_len(value, ptr::null_mut()), Error::Invalid as i32);
                assert_eq!(wdb_value_release(value), 0);
            }
            let mut kind = 99;
            let mut bits = 99;
            let mut len = 99;
            assert_eq!(
                wdb_value_kind(ptr::null(), &mut kind),
                Error::Invalid as i32
            );
            assert_eq!(kind, -1);
            assert_eq!(
                wdb_value_scalar(ptr::null(), &mut bits),
                Error::Invalid as i32
            );
            assert_eq!(bits, 0);
            assert_eq!(wdb_value_len(ptr::null(), &mut len), Error::Invalid as i32);
            assert_eq!(len, 0);
            assert_eq!(wdb_close(database), 0);
        }
    }

    #[test]
    fn null_inputs_and_scalar_outputs_do_not_publish_or_acquire_handles() {
        unsafe {
            let mut id = 99;
            assert_eq!(
                wdb_collection_create(ptr::null_mut(), &mut id),
                Error::Invalid as i32
            );
            assert_eq!(id, 0);
            let mut revision = 99;
            assert_eq!(
                wdb_tx_commit(ptr::null_mut(), &mut revision),
                Error::Invalid as i32
            );
            assert_eq!(revision, 0);
            assert_eq!(wdb_tx_abort(ptr::null_mut()), Error::Invalid as i32);
            assert_eq!(wdb_close(ptr::null_mut()), Error::Invalid as i32);
            let mut len = 99;
            assert_eq!(wdb_scan_len(ptr::null(), &mut len), Error::Invalid as i32);
            assert_eq!(len, 0);
            assert_eq!(
                wdb_snapshot_revision(ptr::null(), &mut revision),
                Error::Invalid as i32
            );
            assert_eq!(revision, 0);
            let mut database = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            assert_eq!(
                wdb_collection_create(database, ptr::null_mut()),
                Error::Invalid as i32
            );
            assert_eq!(wdb_collection_create(database, &mut id), 0);
            let mut snapshot = ptr::null_mut();
            assert_eq!(wdb_snapshot_acquire(database, &mut snapshot), 0);
            assert_eq!(
                wdb_snapshot_revision(snapshot, ptr::null_mut()),
                Error::Invalid as i32
            );
            assert_eq!(wdb_snapshot_revision(snapshot, &mut revision), 0);
            assert_eq!(revision, 1);
            let mut scan = ptr::null_mut();
            assert_eq!(wdb_snapshot_scan(snapshot, id, &mut scan), 0);
            assert_eq!(wdb_scan_len(scan, ptr::null_mut()), Error::Invalid as i32);
            assert_eq!(wdb_scan_len(scan, &mut len), 0);
            assert_eq!(len, 0);
            assert_eq!(wdb_scan_release(scan), 0);
            assert_eq!(wdb_snapshot_release(snapshot), 0);
            assert_eq!(wdb_close(database), 0);
        }
    }

    #[test]
    fn compaction_output_guards_are_independent_before_path_validation() {
        unsafe {
            let mut before = 99;
            let mut after = 99;
            assert_eq!(
                wdb_file_compact(ptr::null(), 0, ptr::null_mut(), &mut after),
                Error::Invalid as i32
            );
            assert_eq!(after, 99);
            assert_eq!(
                wdb_file_compact(ptr::null(), 0, &mut before, ptr::null_mut()),
                Error::Invalid as i32
            );
            assert_eq!(before, 99);
            let result = wdb_file_compact(ptr::null(), 0, &mut before, &mut after);
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            assert_eq!(result, Error::Invalid as i32);
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            assert_eq!(result, Error::UnsupportedPlatform as i32);
            assert_eq!((before, after), (0, 0));
            assert_eq!(
                wdb_scan_key(ptr::null(), 0, ptr::null_mut(), 0, ptr::null_mut()),
                Error::Invalid as i32
            );
        }
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn native_path_length_boundary_and_encoding_contract() {
        let bytes = vec![b'x'; 1024 * 1024];
        unsafe {
            assert_eq!(path(ptr::null(), 1).err(), Some(Error::Invalid));
            assert_eq!(path(b"x".as_ptr(), 0).err(), Some(Error::Invalid));
            assert_eq!(path(b"x\0y".as_ptr(), 3).err(), Some(Error::Invalid));
            assert_eq!(path(b"x".as_ptr(), 1).unwrap(), std::path::Path::new("x"));
            assert!(path(bytes.as_ptr(), bytes.len()).is_ok());
            assert_eq!(
                path(bytes.as_ptr(), bytes.len() + 1).err(),
                Some(Error::Limit)
            );
            #[cfg(windows)]
            assert_eq!(path([0xff].as_ptr(), 1).err(), Some(Error::Invalid));
            #[cfg(not(windows))]
            assert!(path([0xff].as_ptr(), 1).is_ok());
        }
    }
}
