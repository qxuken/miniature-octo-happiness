use super::*;
use std::sync::atomic::AtomicBool;

#[derive(Default)]
pub(crate) struct TestAllocator {
    fail: AtomicBool,
    live: AtomicUsize,
}

impl TestAllocator {
    pub(crate) fn fail_next(&self) {
        self.fail.store(true, Ordering::Relaxed);
    }

    pub(crate) fn live(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }

    pub(crate) fn memory(self: &Arc<Self>, limits: Limits) -> Memory {
        Memory::with_options(DatabaseOptions::from(limits).with_allocator(self.clone())).unwrap()
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
        Ok(block)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { Global.deallocate(ptr, layout) };
        self.live.fetch_sub(1, Ordering::Relaxed);
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> std::result::Result<NonNull<[u8]>, AllocError> {
        self.check()?;
        unsafe { Global.grow(ptr, old, new) }
    }
}

#[test]
fn retained_data_and_values_keep_their_database_allocator_alive() {
    use crate::{Database, storage::SimulatedDisk};

    let disk = SimulatedDisk::new().unwrap();
    let allocator = Arc::new(TestAllocator::default());
    let weak = Arc::downgrade(&allocator);
    let options = || DatabaseOptions::default().with_allocator(allocator.clone());
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
    let custom =
        crate::Database::with_options(DatabaseOptions::default().with_allocator(allocator.clone()))
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
