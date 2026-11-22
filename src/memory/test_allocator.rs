use super::*;
use crate::DatabaseOptions;
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Default)]
pub(crate) struct TestAllocator {
    fail: AtomicBool,
    live: AtomicUsize,
    bytes: AtomicUsize,
}

impl TestAllocator {
    pub(crate) fn fail_next(&self) {
        self.fail.store(true, Ordering::Relaxed);
    }

    pub(crate) fn live(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }

    pub(crate) fn memory(self: &Arc<Self>, limits: Limits) -> Memory {
        Memory::with_allocator(limits, self.handle()).unwrap()
    }

    pub(crate) fn live_bytes(&self) -> usize {
        self.bytes.load(Ordering::Relaxed)
    }

    fn handle(self: &Arc<Self>) -> AllocatorHandle {
        AllocatorHandle::try_new(TestProvider(self.clone())).unwrap()
    }

    fn check(&self) -> std::result::Result<(), AllocError> {
        if self.fail.swap(false, Ordering::Relaxed) {
            return Err(AllocError);
        }
        Ok(())
    }
}

// SAFETY: delegates ownership and layouts to Global, with fallible injection
// before allocation and growth.
unsafe impl Allocator for TestAllocator {
    fn allocate(&self, layout: Layout) -> std::result::Result<NonNull<[u8]>, AllocError> {
        self.check()?;
        let block = Global.allocate(layout)?;
        self.live.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(layout.size(), Ordering::Relaxed);
        Ok(block)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { Global.deallocate(ptr, layout) };
        self.live.fetch_sub(1, Ordering::Relaxed);
        self.bytes.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> std::result::Result<NonNull<[u8]>, AllocError> {
        self.check()?;
        let block = unsafe { Global.grow(ptr, old, new)? };
        self.bytes
            .fetch_add(new.size() - old.size(), Ordering::Relaxed);
        Ok(block)
    }
}

// Tests retain an independent observer of allocator calls and drop timing.
struct TestProvider(Arc<TestAllocator>);

// SAFETY: forwarding preserves allocation ownership and the provider lifetime.
unsafe impl Allocator for TestProvider {
    fn allocate(&self, layout: Layout) -> std::result::Result<NonNull<[u8]>, AllocError> {
        self.0.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { self.0.deallocate(ptr, layout) }
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> std::result::Result<NonNull<[u8]>, AllocError> {
        unsafe { self.0.grow(ptr, old, new) }
    }
}

#[test]
fn retained_data_and_values_keep_their_database_allocator_alive() {
    use crate::{Database, storage::SimulatedDisk};

    let disk = SimulatedDisk::new().unwrap();
    let allocator = Arc::new(TestAllocator::default());
    let weak = Arc::downgrade(&allocator);
    let options = || DatabaseOptions::default().with_allocator(allocator.handle());
    let db = Database::create_on_with_options(disk.clone(), [3; 16], options()).unwrap();
    let collection = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(collection, b"key", &db.bytes(b"retained").unwrap())
        .unwrap();
    tx.commit().unwrap();
    drop(tx);

    let captured = db.snapshot();
    let durable = disk.durable_image().unwrap();
    let charged = db.memory_usage().live_bytes;
    let blocks = allocator.live();
    allocator.fail_next();
    assert_eq!(db.retain_snapshot(&captured), Err(Error::OutOfMemory));
    assert_eq!(db.memory_usage().live_bytes, charged);
    assert_eq!(allocator.live(), blocks);
    assert_eq!(disk.durable_image().unwrap(), durable);
    let id = db.retain_snapshot(&captured).unwrap();
    drop(captured);
    drop(db);
    assert_eq!(allocator.live(), 0);

    let db = Database::open_on_with_options(disk, options()).unwrap();
    assert_eq!(db.retained_ids().unwrap(), [id]);
    let captured = db.retained_snapshot(id).unwrap();
    db.release_retained(id).unwrap();
    let value = captured.get(db.collection_id(1), b"key").unwrap().unwrap();
    assert_eq!(value.as_bytes().unwrap(), b"retained");
    drop(captured);
    drop(db);
    assert!(allocator.live() > 0);
    drop(allocator);
    assert!(weak.upgrade().is_some());
    assert_eq!(value.as_bytes().unwrap(), b"retained");
    drop(value);
    assert!(weak.upgrade().is_none());
}

#[test]
fn allocator_refusal_does_not_affect_another_database() {
    let allocator = Arc::new(TestAllocator::default());
    let custom = crate::Database::with_options(
        DatabaseOptions::default().with_allocator(allocator.handle()),
    )
    .unwrap();
    let default = crate::Database::new().unwrap();
    allocator.fail_next();
    default.create_collection().unwrap();
    assert_eq!(custom.create_collection(), Err(Error::OutOfMemory));
    assert_eq!(custom.snapshot().revision(), 0);
    custom.create_collection().unwrap();
    drop(custom);
    assert_eq!(allocator.live(), 0);
}

#[test]
fn budgets_are_independent_when_databases_share_one_allocator() {
    use crate::Database;

    let allocator = Arc::new(TestAllocator::default());
    let handle = allocator.handle();
    let options = || DatabaseOptions::default().with_allocator(handle.clone());
    let db = Database::with_options(options()).unwrap();
    let second = Database::with_options(options()).unwrap();
    let second_before = second.memory_usage();
    let denied = options().with_limits(Limits {
        max_live_bytes: 0,
        ..Limits::default()
    });
    assert!(matches!(Database::with_options(denied), Err(Error::Limit)));
    let collection = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(collection, b"key", &db.bytes(b"bytes").unwrap())
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    let snapshot = db.snapshot();
    let id = db.retain_snapshot(&snapshot).unwrap();
    let rows = snapshot.scan_owned(collection).unwrap();
    assert_eq!(rows[0].0.as_slice(), b"key");
    assert_eq!(second.memory_usage(), second_before);
    assert_eq!(
        allocator.live_bytes(),
        db.memory_usage().live_bytes + second_before.live_bytes
    );
    drop(rows);
    db.release_retained(id).unwrap();
    drop(snapshot);
    drop(db);
    assert_eq!(allocator.live_bytes(), second_before.live_bytes);
    drop(second);
    assert_eq!(allocator.live(), 0);
    assert_eq!(allocator.live_bytes(), 0);
}
