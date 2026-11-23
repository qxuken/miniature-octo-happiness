use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use weekend_db::{AllocatorHandle, Database, DatabaseOptions, Error, Result, storage::ByteStore};

thread_local! {
    static GLOBAL_REQUESTS: Cell<Option<usize>> = const { Cell::new(None) };
}

struct CountingGlobal;

#[global_allocator]
static GLOBAL: CountingGlobal = CountingGlobal;

fn count_request() {
    let _ = GLOBAL_REQUESTS.try_with(|requests| {
        if let Some(count) = requests.get() {
            requests.set(Some(count.saturating_add(1)));
        }
    });
}

// SAFETY: System performs every allocation and deallocation with its supplied
// layout. The counter uses const thread-local storage and never allocates.
unsafe impl GlobalAlloc for CountingGlobal {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_request();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_request();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_request();
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
}

fn global_requests<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    assert_eq!(GLOBAL_REQUESTS.replace(Some(0)), None);
    let result = operation();
    (result, GLOBAL_REQUESTS.replace(None).unwrap())
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

    let (entries, requests) = global_requests(|| child.map_entries());
    assert_eq!(entries.unwrap().len(), 2);
    assert!(
        requests > 0,
        "public map output should exercise the counter"
    );

    let mut tx = db.begin();
    tx.put(collection, b"row", &parent).unwrap();
    let (commit, requests) = global_requests(|| tx.commit());
    commit.unwrap();
    assert_eq!(requests, 0, "durable commit allocated through Global");
    drop(tx);

    let destination = Image::empty();
    let (compact, requests) = global_requests(|| db.compact_to(destination));
    compact.unwrap();
    assert_eq!(requests, 0, "compaction allocated through Global");
}
