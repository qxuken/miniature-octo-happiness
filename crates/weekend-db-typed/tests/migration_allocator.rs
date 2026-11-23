use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use weekend_db_typed::{
    Model, Record, Result, Store,
    engine::{AllocatorHandle, Database, DatabaseOptions},
};

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

#[derive(Model)]
#[weekend(type_id = 924, version = 1)]
struct Old {
    #[weekend(id = 1, unique)]
    code: u64,
}

#[derive(Model)]
#[weekend(type_id = 924, version = 2)]
struct Current {
    #[weekend(id = 1, unique)]
    code: u64,
    #[weekend(id = 2, default = 0)]
    priority: u64,
}

#[test]
fn indexed_migrations_use_the_selected_allocator_for_internal_scratch() -> Result<()> {
    for ids in [None, Some([2, 1])] {
        let options = DatabaseOptions::default().with_allocator(AllocatorHandle::try_new(System)?);
        let mut store = Store::initialize(Database::with_options(options)?)?;
        let old = store.table::<Old>("jobs")?;
        store.write(|w| {
            w.insert(&old, 1, &Old { code: 5 })?;
            w.insert(&old, 2, &Old { code: 6 })
        })?;
        let current = store.table::<Current>("jobs")?;

        let mut visited = [0; 2];
        let mut calls = 0;
        let mut convert = |_: &Record, job: &mut Current| {
            visited[calls] = job.code;
            calls += 1;
            job.code += 10;
            job.priority = 9;
            Ok(())
        };
        let (result, requests) = global_requests(|| {
            store.write(|w| match &ids {
                Some(ids) => w.migrate_ids(&current, ids, |_, raw, job| convert(raw, job)),
                None => w.migrate(&current, &mut convert),
            })
        });
        assert_eq!(result?, 2);
        assert_eq!(requests, 0, "migration of {ids:?} allocated through Global");
        assert_eq!(visited, if ids.is_some() { [6, 5] } else { [5, 6] });

        store.read(|r| {
            assert!(r.find(&current, Current::CODE, &5)?.is_none());
            assert!(r.find(&current, Current::CODE, &6)?.is_none());
            for (id, code) in [(1, 15), (2, 16)] {
                let job = r.find(&current, Current::CODE, &code)?.unwrap();
                assert_eq!((job.id(), job.version(), job.priority), (id, 2, 9));
            }
            assert!(r.check_indexes(&current)?.is_consistent());
            Ok(())
        })?;
    }
    Ok(())
}
