#[path = "support/allocator.rs"]
pub mod allocator;
use allocator::TestAllocator;
use std::sync::Arc;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use weekend_db as allocator_engine;
use weekend_db::{
    AllocatorHandle, Database, DatabaseOptions, Error, Limits, Result,
    storage::{ByteStore, Fault, IoEvent, IoKind, SimulatedDisk, export_logical_on, inspect_on},
};

#[derive(Clone, Copy)]
struct GlobalRequests {
    count: usize,
    allowed: usize,
}

thread_local! {
    static GLOBAL_REQUESTS: Cell<Option<GlobalRequests>> = const { Cell::new(None) };
}

struct CountingGlobal;

#[global_allocator]
static GLOBAL: CountingGlobal = CountingGlobal;

fn refuse_request() -> bool {
    GLOBAL_REQUESTS
        .try_with(|requests| {
            let Some(mut state) = requests.get() else {
                return false;
            };
            state.count = state.count.saturating_add(1);
            requests.set(Some(state));
            state.count > state.allowed
        })
        .unwrap_or(false)
}

// SAFETY: System performs every allocation and deallocation with its supplied
// layout. The counter uses const thread-local storage and never allocates.
unsafe impl GlobalAlloc for CountingGlobal {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if refuse_request() {
            return std::ptr::null_mut();
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if refuse_request() {
            return std::ptr::null_mut();
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if refuse_request() {
            return std::ptr::null_mut();
        }
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
}

fn global_requests<T>(allowed: usize, operation: impl FnOnce() -> T) -> (T, usize) {
    assert!(GLOBAL_REQUESTS.get().is_none());
    GLOBAL_REQUESTS.set(Some(GlobalRequests { count: 0, allowed }));
    let result = operation();
    (result, GLOBAL_REQUESTS.replace(None).unwrap().count)
}

fn sweep_global_refusal<T>(mut operation: impl FnMut() -> Result<T>) -> T {
    for allowed in 0..512 {
        let (result, count) = global_requests(allowed, &mut operation);
        match result {
            Ok(output) => {
                assert!(allowed > 0, "the operation never used Global");
                assert_eq!(count, allowed);
                return output;
            }
            Err(error) => {
                assert_eq!(error, Error::OutOfMemory, "allowance {allowed}");
                assert_eq!(count, allowed + 1, "allowance {allowed}");
            }
        }
    }
    panic!("Global refusal sweep did not reach success");
}

#[test]
fn snapshot_scan_iterator_owns_ordered_rows_without_global_allocations() -> Result<()> {
    let options = DatabaseOptions::default().with_allocator(AllocatorHandle::try_new(System)?);
    let db = Database::with_options(options)?;
    let collection = db.create_collection()?;
    let mut tx = db.begin();
    tx.put(collection, b"z", &db.integer(9)?)?;
    tx.put(collection, b"a\0", &db.integer(1)?)?;
    tx.commit()?;
    drop(tx);

    let snapshot = db.snapshot();
    let before = db.memory_usage().live_bytes;
    let (rows, requests) = global_requests(0, || snapshot.scan_iter(collection));
    assert_eq!(requests, 0);
    let mut rows = rows?;
    assert!(db.memory_usage().live_bytes > before);
    drop(snapshot);
    drop(db);

    for (key, value) in [(b"a\0".as_slice(), 1), (b"z".as_slice(), 9)] {
        let (actual_key, actual_value) = rows.next().unwrap();
        assert_eq!(actual_key.as_ref(), key);
        assert_eq!(actual_value.as_i64()?, value);
    }
    assert!(rows.next().is_none());
    Ok(())
}

#[test]
fn oversized_nested_map_scan_is_rejected_before_allocating_output() {
    let allocator = Arc::new(TestAllocator::default());
    let options = allocator.options().with_limits(Limits {
        max_transaction_bytes: 0,
        ..Limits::default()
    });
    let db = Database::with_options(options).unwrap();
    let key = [b'x'; 1024];
    let map = db
        .map()
        .unwrap()
        .map_set(&key, &db.none().unwrap())
        .unwrap();
    let ((result, selected), global) =
        global_requests(0, || allocator.count_with_refusal(0, || map.map_entries()));
    assert_eq!(result, Err(Error::Limit));
    assert_eq!((selected, global), (0, 0));
    assert_eq!(
        map.map_get(&key).unwrap().unwrap().kind(),
        weekend_db::Kind::None
    );
}

// Preallocate the test backend before measurement. Its I/O never grows a buffer.
struct Image {
    bytes: Vec<u8>,
    length: usize,
}

impl Image {
    fn empty() -> Self {
        Self {
            bytes: vec![0; 128 * 1024],
            length: 0,
        }
    }
}

impl ByteStore for Image {
    fn lock(&mut self) -> Result<()> {
        Ok(())
    }

    fn unlock(&mut self) {}

    fn length(&mut self) -> Result<u64> {
        Ok(self.length as u64)
    }

    fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
        let start = usize::try_from(offset)
            .map_err(|_| Error::Bounds)?
            .min(self.length);
        let len = out.len().min(self.length - start);
        out[..len].copy_from_slice(&self.bytes[start..start + len]);
        Ok(len)
    }

    fn write(&mut self, offset: u64, input: &[u8]) -> Result<usize> {
        let start = usize::try_from(offset).map_err(|_| Error::Limit)?;
        let end = start.checked_add(input.len()).ok_or(Error::Limit)?;
        self.bytes
            .get_mut(start..end)
            .ok_or(Error::Limit)?
            .copy_from_slice(input);
        self.length = self.length.max(end);
        Ok(input.len())
    }

    fn sync(&mut self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn nested_map_serialization_uses_no_global_allocations() {
    let allocator = AllocatorHandle::try_new(System).unwrap();
    let options = DatabaseOptions::default().with_allocator(allocator);
    let mut db = Database::create_on_with_options(Image::empty(), [1; 16], options).unwrap();
    let collection = db.create_collection().unwrap();
    let scalar = db.unsigned(1).unwrap();
    let child = db
        .map()
        .unwrap()
        .map_set(b"\xff", &scalar)
        .unwrap()
        .map_set(b"\0", &scalar)
        .unwrap();
    let parent = db.map().unwrap().map_set(b"child", &child).unwrap();

    let (entries, requests) = global_requests(usize::MAX, || child.map_entries());
    assert_eq!(entries.unwrap().len(), 2);
    assert!(
        requests > 0,
        "public map output should exercise the counter"
    );

    let mut tx = db.begin();
    tx.put(collection, b"row", &parent).unwrap();
    let (commit, requests) = global_requests(usize::MAX, || tx.commit());
    commit.unwrap();
    assert_eq!(requests, 0, "durable commit allocated through Global");
    drop(tx);

    let destination = Image::empty();
    let (compact, requests) = global_requests(usize::MAX, || db.compact_to(destination));
    compact.unwrap();
    assert_eq!(requests, 0, "compaction allocated through Global");
}

#[test]
fn rust_outputs_propagate_real_global_refusals_and_preserve_roots() {
    let options =
        DatabaseOptions::default().with_allocator(AllocatorHandle::try_new(System).unwrap());
    let db = Database::with_options(options).unwrap();
    let collection = db.create_collection().unwrap();
    let one = db.unsigned(1).unwrap();
    let two = db.unsigned(2).unwrap();
    let map = db
        .map()
        .unwrap()
        .map_set(b"b", &two)
        .unwrap()
        .map_set(b"a", &one)
        .unwrap();
    {
        let mut seed = db.begin();
        seed.put(collection, b"b", &two).unwrap();
        seed.put(collection, b"a", &one).unwrap();
        seed.commit().unwrap();
    }
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let snapshot = db.snapshot();
    let usage = db.memory_usage().live_bytes;
    let expected = [(b"a".to_vec(), one), (b"b".to_vec(), two)];
    assert_eq!(sweep_global_refusal(|| map.map_entries()), expected);
    assert_eq!(sweep_global_refusal(|| snapshot.scan(collection)), expected);
    assert_eq!(sweep_global_refusal(|| db.retained_ids()), [retained]);
    {
        let mut draft = db.begin();
        assert_eq!(sweep_global_refusal(|| draft.scan(collection)), expected);
        assert_eq!(draft.state(), weekend_db::TransactionState::Active);
    }
    assert_eq!(db.memory_usage().live_bytes, usage);
    assert_eq!(db.snapshot().revision(), snapshot.revision());
    assert_eq!(
        db.retained_snapshot(retained)
            .unwrap()
            .scan(collection)
            .unwrap(),
        expected
    );
}

#[test]
fn simulator_output_refusal_preserves_images_trace_and_lease() {
    let allocator = AllocatorHandle::try_new(System).unwrap();
    let mut disk = SimulatedDisk::from_image_with_allocator(&[], allocator).unwrap();
    disk.lock().unwrap();
    let (trace, count) = global_requests(0, || disk.trace());
    assert!(trace.unwrap().is_empty());
    assert_eq!(count, 0);
    let (image, count) = global_requests(0, || disk.durable_image());
    assert!(image.unwrap().is_empty());
    assert_eq!(count, 0);

    assert_eq!(disk.write(0, b"seed"), Ok(4));
    disk.sync().unwrap();
    let trace = [
        IoEvent {
            kind: IoKind::Write,
            offset: 0,
            len: 4,
        },
        IoEvent {
            kind: IoKind::Sync,
            offset: 0,
            len: 0,
        },
    ];
    assert_eq!(sweep_global_refusal(|| disk.trace()), trace);
    assert_eq!(sweep_global_refusal(|| disk.durable_image()), b"seed");
    assert_eq!(disk.clone().lock(), Err(Error::Busy));
    assert_eq!(disk.write(0, b"next"), Ok(4));
    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), b"next");
}

#[test]
fn read_only_image_outputs_propagate_global_refusals_without_writes() {
    let allocator = AllocatorHandle::try_new(System).unwrap();
    let disk = SimulatedDisk::from_image_with_allocator(&[], allocator).unwrap();
    let db = Database::create_on(disk.clone(), [7; 16], Limits::default()).unwrap();
    let collection = db.create_collection().unwrap();
    {
        let mut draft = db.begin();
        draft
            .put(collection, b"a", &db.unsigned(1).unwrap())
            .unwrap();
        draft.commit().unwrap();
    }
    let retained = db.retain_snapshot(&db.snapshot()).unwrap();
    let revision = db.snapshot().revision();
    drop(db);
    let image = disk.durable_image().unwrap();
    let logical = export_logical_on(disk.clone(), Limits::default()).unwrap();
    disk.arm(usize::MAX, Fault::Error);

    let info = sweep_global_refusal(|| inspect_on(disk.clone(), Limits::default()));
    assert_eq!(info.revision, revision);
    assert_eq!(info.retained.len(), 1);
    assert_eq!(info.retained[0].id, retained);
    assert_eq!(
        sweep_global_refusal(|| export_logical_on(disk.clone(), Limits::default())),
        logical
    );
    assert!(
        disk.trace()
            .unwrap()
            .iter()
            .all(|event| matches!(event.kind, IoKind::Length | IoKind::Read))
    );
    assert_eq!(disk.durable_image().unwrap(), image);
    let mut observer = disk.clone();
    observer.lock().unwrap();
    observer.unlock();
}
