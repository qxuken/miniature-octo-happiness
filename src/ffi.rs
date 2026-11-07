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
    Database, Error, Kind, Result, Snapshot, Transaction, Validation, Value, engine::Rows,
    memory::free_raw,
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
type CScan = Handle<Rows>;
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
unsafe fn db<'a>(ptr: *mut CDatabase) -> Result<&'a CDatabase> {
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
export!(wdb_open(out: *mut *mut CDatabase) {
    output(out)?;
    let database = Database::new()?;
    let mem = database.mem.clone();
    *out = mem.raw(CDatabase { db: database, handles: AtomicUsize::new(0) })?;
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
            let mem=database.mem.clone();*out=mem.raw(CDatabase {db:database,handles:AtomicUsize::new(0)})?;Ok(())
        }
        #[cfg(not(any(target_os="macos",target_os="linux",windows)))]
        {let _=(bytes,len);Err(Error::UnsupportedPlatform)}
    });};
}
file_factory!(wdb_file_create, create_file);
file_factory!(wdb_file_open, open_file);
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
    let ids=db(database)?.db.retained_ids()?;*required=ids.len();
    if capacity<ids.len() {return Err(Error::Buffer);}
    if !ids.is_empty() {ptr::copy_nonoverlapping(ids.as_ptr(),buffer,ids.len());}Ok(())
});
export!(wdb_close(database: *mut CDatabase) {
    if db(database)?.handles.load(Ordering::Acquire) != 0 { return Err(Error::Busy); }
    free_raw(database); Ok(())
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
        if !value.is_null() { free_raw(value); } Ok(())
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
    output(out)?; let map = handle(map)?; make_handle(map.owner, map.value.map_entries()?, out)
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
    make_handle(snapshot.owner, snapshot.value.scan(db(snapshot.owner)?.db.collection_id(id))?, out)
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
    let rows = tx.value.scan(db(tx.owner)?.db.collection_id(id))?; make_handle(tx.owner, rows, out)
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

#[cfg(test)]
mod tests {
    use super::*;
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
        unsafe {
            let mut database = ptr::null_mut();
            assert_eq!(wdb_open(&mut database), 0);
            for budget in 0..3 {
                (*database).db.fail_allocations_after(Some(budget));
                let mut value = ptr::null_mut();
                let status = wdb_value_make(database, 0, 0, ptr::null(), 0, &mut value);
                (*database).db.fail_allocations_after(None);
                if status == 0 {
                    assert_eq!(wdb_value_release(value), 0);
                } else {
                    assert_eq!(status, Error::OutOfMemory as i32);
                    assert!(value.is_null());
                }
                assert_eq!((*database).handles.load(Ordering::Acquire), 0);
            }
            assert_eq!(wdb_close(database), 0);
        }
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
