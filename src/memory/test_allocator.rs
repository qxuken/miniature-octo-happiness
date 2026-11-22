use super::*;
use crate::DatabaseOptions;
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct TestAllocator {
    fail_at: AtomicUsize,
    live: AtomicUsize,
    bytes: AtomicUsize,
    requests: AtomicUsize,
    last_request_size: AtomicUsize,
}

impl TestAllocator {
    pub(crate) fn fail_next(&self) {
        self.fail_after(0);
    }

    pub(crate) fn fail_after(&self, allowed: usize) {
        self.fail_at
            .store(self.requests() + allowed + 1, Ordering::Relaxed);
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

    fn requests(&self) -> usize {
        self.requests.load(Ordering::Relaxed)
    }

    fn handle(self: &Arc<Self>) -> AllocatorHandle {
        AllocatorHandle::try_new(TestProvider(self.clone())).unwrap()
    }

    fn check(&self, layout: Layout) -> std::result::Result<(), AllocError> {
        let request = self.requests.fetch_add(1, Ordering::Relaxed) + 1;
        self.last_request_size
            .store(layout.size(), Ordering::Relaxed);
        if self
            .fail_at
            .compare_exchange(request, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return Err(AllocError);
        }
        Ok(())
    }
}

// SAFETY: delegates ownership and layouts to Global, with fallible injection
// before allocation and growth.
unsafe impl Allocator for TestAllocator {
    fn allocate(&self, layout: Layout) -> std::result::Result<NonNull<[u8]>, AllocError> {
        self.check(layout)?;
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
        self.check(new)?;
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
fn allocator_handle_owns_one_block_allocated_by_its_provider() {
    let allocator = Arc::new(TestAllocator::default());
    allocator.fail_next();
    assert!(matches!(
        AllocatorHandle::try_new(TestProvider(allocator.clone())),
        Err(Error::OutOfMemory)
    ));
    assert_eq!(Arc::strong_count(&allocator), 1);
    assert_eq!(allocator.requests(), 1);
    assert_eq!(allocator.live(), 0);

    let handle = allocator.handle();
    assert_eq!(std::mem::size_of_val(&handle), std::mem::size_of::<usize>());
    assert_eq!(allocator.requests(), 2);
    assert_eq!(allocator.live(), 1);
    assert_eq!(Arc::strong_count(&allocator), 2);
    let other = handle.clone();
    drop(handle);
    assert_eq!(allocator.requests(), 2);
    assert_eq!(allocator.live(), 1);
    std::thread::spawn(move || {
        let layout = Layout::new::<[u64; 8]>();
        let block = other.allocate(layout).unwrap();
        unsafe { other.deallocate(block.cast(), layout) };
    })
    .join()
    .unwrap();
    assert_eq!(allocator.requests(), 3);
    assert_eq!(allocator.live(), 0);
    assert_eq!(allocator.live_bytes(), 0);
    assert_eq!(Arc::strong_count(&allocator), 1);
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
    let requests = allocator.requests();
    allocator.fail_next();
    default.create_collection().unwrap();
    assert_eq!(allocator.requests(), requests);
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
    let provider_bytes = allocator.live_bytes();
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
        provider_bytes + db.memory_usage().live_bytes + second_before.live_bytes
    );
    drop(rows);
    db.release_retained(id).unwrap();
    drop(snapshot);
    drop(db);
    assert_eq!(
        allocator.live_bytes(),
        provider_bytes + second_before.live_bytes
    );
    drop(second);
    assert_eq!(allocator.live_bytes(), provider_bytes);
    drop(handle);
    assert_eq!(allocator.live(), 0);
    assert_eq!(allocator.live_bytes(), 0);
}

#[cfg(all(not(miri), any(target_os = "macos", target_os = "linux")))]
#[test]
fn native_path_allocations_are_fallible_and_preserve_the_file_lease() {
    use crate::{Database, storage::FileStore};
    use std::{
        fs,
        os::unix::fs::symlink,
        path::{Path, PathBuf},
        time::SystemTime,
    };

    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("wdb-path-{}-{stamp}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let directory = Directory(directory);
    let mut nested = directory.0.clone();
    for _ in 0..6 {
        nested.push("x".repeat(70));
    }
    let parent = nested.join("actual");
    fs::create_dir_all(&parent).unwrap();
    let alias = nested.join("alias");
    symlink(&parent, &alias).unwrap();

    let leaf = "allocator-leaf.wdb";
    let path = alias.join(leaf);
    let parent_size = alias.as_os_str().as_encoded_bytes().len() + 1;
    assert!(parent_size > 384);
    let path_requests = [parent_size, leaf.len() + 1];
    let allocator = Arc::new(TestAllocator::default());
    let handle = allocator.handle();
    let provider_bytes = allocator.live_bytes();
    let options = || DatabaseOptions::default().with_allocator(handle.clone());

    let check_refusal = |open: fn(&Path, DatabaseOptions) -> Result<Database>, allowed, size| {
        let requests = allocator.requests();
        allocator.fail_after(allowed);
        assert!(matches!(open(&path, options()), Err(Error::OutOfMemory)));
        assert_eq!(allocator.requests() - requests, allowed + 1);
        assert_eq!(allocator.last_request_size.load(Ordering::Relaxed), size);
        assert_eq!(allocator.live_bytes(), provider_bytes);
    };
    for (allowed, size) in path_requests.into_iter().enumerate() {
        check_refusal(
            |path, opts| Database::create_file_with_options(path, opts),
            allowed,
            size,
        );
        assert!(
            matches!(fs::metadata(&path), Err(err) if err.kind() == std::io::ErrorKind::NotFound)
        );
    }

    let check_live = |db: &Database| {
        assert_eq!(
            allocator.live_bytes(),
            provider_bytes
                + db.memory_usage().live_bytes
                + std::mem::size_of::<FileStore>()
                + leaf.len()
                + 1
        );
    };
    let db = Database::create_file_with_options(&path, options()).unwrap();
    assert!(matches!(
        Database::open_file_with_options(parent.join(leaf), options()),
        Err(Error::Busy)
    ));
    check_live(&db);
    let collection = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(collection, b"key", &db.bytes(b"durable").unwrap())
        .unwrap();
    tx.commit().unwrap();
    drop(tx);
    let id = db.retain_snapshot(&db.snapshot()).unwrap();
    check_live(&db);
    drop(db);
    assert_eq!(allocator.live_bytes(), provider_bytes);

    let requests = allocator.requests();
    let store = FileStore::open_in(&path, false, handle.clone()).unwrap();
    assert_eq!(allocator.requests() - requests, 2);
    assert_eq!(allocator.live_bytes(), provider_bytes + leaf.len() + 1);
    drop(store);
    assert_eq!(allocator.live_bytes(), provider_bytes);

    let image = fs::read(&path).unwrap();
    for (allowed, size) in path_requests.into_iter().enumerate() {
        check_refusal(
            |path, opts| Database::open_file_with_options(path, opts),
            allowed,
            size,
        );
        assert_eq!(fs::read(&path).unwrap(), image);
    }

    let db = Database::open_file_with_options(&path, options()).unwrap();
    check_live(&db);
    let retained = db.retained_snapshot(id).unwrap();
    let value = retained.get(db.collection_id(1), b"key").unwrap().unwrap();
    assert_eq!(value.as_bytes().unwrap(), b"durable");
    drop(value);
    drop(retained);
    drop(db);
    assert_eq!(allocator.live_bytes(), provider_bytes);
    drop(handle);
    assert_eq!(allocator.live(), 0);
}

#[cfg(all(windows, not(miri)))]
#[test]
fn native_windows_path_allocations_are_fallible_and_preserve_the_file_lease() {
    use crate::{Database, storage::FileStore};
    use std::{
        ffi::OsString,
        fs,
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::PathBuf,
        time::SystemTime,
    };

    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wdb-path-{}-{stamp}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let root = Directory(root);
    let mut parent = root.0.clone();
    for _ in 0..6 {
        parent.push("x".repeat(70));
    }
    fs::create_dir_all(&parent).unwrap();
    let verbatim = fs::canonicalize(parent).unwrap();
    let units: Vec<_> = verbatim.as_os_str().encode_wide().collect();
    let ordinary = PathBuf::from(OsString::from_wide(
        units.strip_prefix(&[92, 92, 63, 92]).unwrap(),
    ));
    let allocator = Arc::new(TestAllocator::default());
    let handle = allocator.handle();
    let provider_bytes = allocator.live_bytes();
    let options = || DatabaseOptions::default().with_allocator(handle.clone());

    for (parent, alias) in [(&ordinary, &verbatim), (&verbatim, &ordinary)] {
        let leaf = "allocator-λ-🦀.wdb";
        let path = parent.join(leaf);
        assert!(parent.as_os_str().encode_wide().count() > 384);
        let requests = allocator.requests();
        let probe = FileStore::open_in(&path, true, handle.clone()).unwrap();
        let path_requests = allocator.requests() - requests;
        assert!(path_requests >= 2);
        assert_eq!(allocator.live_bytes(), provider_bytes);
        drop(probe);
        fs::remove_file(&path).unwrap();

        for allowed in 0..path_requests {
            let requests = allocator.requests();
            allocator.fail_after(allowed);
            assert!(matches!(
                Database::create_file_with_options(&path, options()),
                Err(Error::OutOfMemory)
            ));
            assert_eq!(allocator.requests() - requests, allowed + 1);
            assert_eq!(allocator.live_bytes(), provider_bytes);
            assert!(
                matches!(fs::metadata(&path), Err(err) if err.kind() == std::io::ErrorKind::NotFound)
            );
        }

        let db = Database::create_file_with_options(&path, options()).unwrap();
        assert!(matches!(
            Database::open_file_with_options(alias.join(leaf), options()),
            Err(Error::Busy)
        ));
        assert_eq!(
            allocator.live_bytes(),
            provider_bytes + db.memory_usage().live_bytes + std::mem::size_of::<FileStore>()
        );
        let collection = db.create_collection().unwrap();
        let mut tx = db.begin();
        tx.put(collection, b"key", &db.bytes(b"durable").unwrap())
            .unwrap();
        tx.commit().unwrap();
        drop(tx);
        let id = db.retain_snapshot(&db.snapshot()).unwrap();
        drop(db);
        assert_eq!(allocator.live_bytes(), provider_bytes);

        let image = fs::read(&path).unwrap();
        for allowed in 0..path_requests {
            let requests = allocator.requests();
            allocator.fail_after(allowed);
            assert!(matches!(
                Database::open_file_with_options(&path, options()),
                Err(Error::OutOfMemory)
            ));
            assert_eq!(allocator.requests() - requests, allowed + 1);
            assert_eq!(allocator.live_bytes(), provider_bytes);
            assert_eq!(fs::read(&path).unwrap(), image);
        }

        let db = Database::open_file_with_options(&path, options()).unwrap();
        let snapshot = db.retained_snapshot(id).unwrap();
        let value = snapshot.get(db.collection_id(1), b"key").unwrap().unwrap();
        assert_eq!(value.as_bytes().unwrap(), b"durable");
        drop(value);
        drop(snapshot);
        drop(db);
        assert_eq!(allocator.live_bytes(), provider_bytes);
        fs::remove_file(&path).unwrap();
    }
    drop(handle);
    assert_eq!(allocator.live(), 0);
}
