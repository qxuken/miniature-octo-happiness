use crate::{Error, Result};
use std::{
    alloc::{Layout, alloc, dealloc},
    marker::PhantomData,
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

#[cfg(test)]
thread_local! {
    static FAIL_SYSTEM_ALLOCATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Tests replace only this thread's next allocation result.
unsafe fn allocate(layout: Layout) -> *mut u8 {
    #[cfg(test)]
    if FAIL_SYSTEM_ALLOCATION.with(|fail| fail.replace(false)) {
        return std::ptr::null_mut();
    }
    unsafe { alloc(layout) }
}

/// Resource limits are checked before allocation or publication.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_value_bytes: usize,
    pub max_depth: usize,
    pub max_entries: usize,
    pub max_writes: usize,
    pub max_observations: usize,
    pub max_transaction_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024,
            max_value_bytes: 16 * 1024 * 1024,
            max_depth: 64,
            max_entries: 4096,
            max_writes: 1024,
            max_observations: 4096,
            max_transaction_bytes: 32 * 1024 * 1024,
        }
    }
}
impl Limits {
    pub(crate) fn validate(self) -> Result<Self> {
        if self.max_depth == 0
            || self.max_depth > 64
            || self.max_entries > 1_000_000
            || self.max_writes > 1_000_000
            || self.max_observations > 1_000_000
            || self.max_bytes > isize::MAX as usize
            || self.max_value_bytes > isize::MAX as usize
            || self.max_transaction_bytes > isize::MAX as usize
        {
            return Err(Error::Invalid);
        }
        Ok(self)
    }
}

// Fallible atomic reference counting. No allocation occurs on clone.
// All unsafe ownership code is contained here (and in the foreign boundary).
struct SharedBox<T> {
    refs: AtomicUsize,
    value: T,
}
pub(crate) struct Shared<T> {
    ptr: NonNull<SharedBox<T>>,
    _owns: PhantomData<SharedBox<T>>,
}
// SAFETY: shared ownership exposes only &T and synchronizes final destruction.
// Both bounds are necessary: another owner may read or destroy T on a thread.
unsafe impl<T: Send + Sync> Send for Shared<T> {}
unsafe impl<T: Send + Sync> Sync for Shared<T> {}
impl<T> Shared<T> {
    fn allocate(value: T) -> Result<Self> {
        let layout = Layout::new::<SharedBox<T>>();
        // SharedBox always has a nonzero size due to the count.
        let ptr = NonNull::new(unsafe { allocate(layout) }.cast::<SharedBox<T>>())
            .ok_or(Error::OutOfMemory)?;
        unsafe {
            ptr.as_ptr().write(SharedBox {
                refs: AtomicUsize::new(1),
                value,
            });
        }
        Ok(Self {
            ptr,
            _owns: PhantomData,
        })
    }
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        let refs = unsafe { &self.ptr.as_ref().refs };
        // The live source handle already keeps the value alive. Cloning only
        // needs an atomic increment, not publication of the immutable contents.
        // Abort well below wraparound, including deliberately leaked handles.
        if refs.fetch_add(1, Ordering::Relaxed) >= isize::MAX as usize {
            std::process::abort();
        }
        Self {
            ptr: self.ptr,
            _owns: PhantomData,
        }
    }
}
impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &self.ptr.as_ref().value }
    }
}
impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        let refs = unsafe { &self.ptr.as_ref().refs };
        // Release prior accesses; acquire the release sequence of previous
        // owners before the last owner's destruction. AcqRel on every decrement
        // is deliberately stronger than Arc's final acquire fence and is also
        // understood by ThreadSanitizer.
        if refs.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        unsafe {
            std::ptr::drop_in_place(self.ptr.as_ptr());
            dealloc(self.ptr.as_ptr().cast(), Layout::new::<SharedBox<T>>());
        }
    }
}

struct MemoryState {
    limits: Limits,
    #[cfg(any(test, feature = "fault-injection"))]
    remaining: AtomicUsize,
}
#[derive(Clone)]
pub(crate) struct Memory(Shared<MemoryState>);
impl Memory {
    pub(crate) fn new(limits: Limits) -> Result<Self> {
        Ok(Self(Shared::allocate(MemoryState {
            limits: limits.validate()?,
            #[cfg(any(test, feature = "fault-injection"))]
            remaining: AtomicUsize::new(usize::MAX),
        })?))
    }
    pub(crate) fn limits(&self) -> Limits {
        self.0.limits
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
    pub(crate) fn claim(&self) -> Result<()> {
        #[cfg(any(test, feature = "fault-injection"))]
        self.0
            .remaining
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                match n {
                    usize::MAX => Some(n), // unlimited
                    0 => None,
                    _ => Some(n - 1),
                }
            })
            .map_err(|_| Error::OutOfMemory)?;
        Ok(())
    }
    #[cfg(any(test, feature = "fault-injection"))]
    pub(crate) fn fail_after(&self, n: Option<usize>) {
        self.0
            .remaining
            .store(n.unwrap_or(usize::MAX), Ordering::Relaxed);
    }
    pub(crate) fn shared<T>(&self, value: T) -> Result<Shared<T>> {
        self.claim()?;
        Shared::allocate(value)
    }
    pub(crate) fn reserve<T>(&self, vec: &mut Vec<T>, extra: usize) -> Result<()> {
        let needed = vec.len().checked_add(extra).ok_or(Error::Limit)?;
        if needed > vec.capacity() {
            self.claim()?;
            vec.try_reserve_exact(extra)
                .map_err(|_| Error::OutOfMemory)?;
        }
        Ok(())
    }
    pub(crate) fn copy(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        if bytes.len() > isize::MAX as usize {
            return Err(Error::Limit);
        }
        let mut vec = Vec::new();
        self.reserve(&mut vec, bytes.len())?;
        vec.extend_from_slice(bytes);
        Ok(vec)
    }
    /// Allocate a foreign handle without the infallible Box::new path.
    pub(crate) fn boxed<T>(&self, value: T) -> Result<Box<T>> {
        if std::mem::size_of::<T>() == 0 {
            return Ok(Box::new(value));
        }
        // raw() and Box use the same global allocator and Layout<T>. Transfer
        // ownership exactly once so ordinary Box destruction can release it.
        Ok(unsafe { Box::from_raw(self.raw(value)?) })
    }
    pub(crate) fn raw<T>(&self, value: T) -> Result<*mut T> {
        self.claim()?;
        let layout = Layout::new::<T>();
        assert!(layout.size() != 0);
        let ptr =
            NonNull::new(unsafe { allocate(layout) }.cast::<T>()).ok_or(Error::OutOfMemory)?;
        unsafe {
            ptr.as_ptr().write(value);
        }
        Ok(ptr.as_ptr())
    }
}
/// Safety: ptr must be a live allocation returned by Memory::raw, consumed once.
pub(crate) unsafe fn free_raw<T>(ptr: *mut T) {
    unsafe {
        std::ptr::drop_in_place(ptr);
        dealloc(ptr.cast(), Layout::new::<T>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Barrier, Mutex},
        thread,
    };
    #[test]
    fn null_system_allocation_drops_inputs_and_allows_retry() {
        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let mem = Memory::new(Limits::default()).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        for kind in 0..3 {
            FAIL_SYSTEM_ALLOCATION.with(|flag| flag.set(true));
            let input = Probe(drops.clone());
            let result = match kind {
                0 => mem.shared(input).map(drop),
                1 => mem.boxed(input).map(drop),
                _ => mem.raw(input).map(|ptr| unsafe { free_raw(ptr) }),
            };
            assert_eq!(result, Err(Error::OutOfMemory));
            assert_eq!(drops.load(Ordering::Relaxed), kind + 1);
            assert!(!FAIL_SYSTEM_ALLOCATION.with(|flag| flag.get()));
        }
        drop(mem.shared(Probe(drops.clone())).unwrap());
        assert_eq!(drops.load(Ordering::Relaxed), 4);
    }
    #[test]
    #[cfg(not(miri))]
    fn shared_clone_aborts_at_the_refcount_threshold_in_a_subprocess() {
        const CHILD: &str = "WEEKEND_DB_REFCOUNT_ABORT_CHILD";
        if let Some(mode) = std::env::var_os(CHILD) {
            let shared = Shared::allocate(7u8).unwrap();
            let before = if mode == "below" {
                isize::MAX as usize - 1
            } else {
                isize::MAX as usize
            };
            // Only this child fabricates the otherwise impractical boundary.
            unsafe { shared.ptr.as_ref() }
                .refs
                .store(before, Ordering::Relaxed);
            let cloned = shared.clone();
            let after = unsafe { shared.ptr.as_ref() }.refs.load(Ordering::Relaxed);
            // Restore the two real owners before assertions or destruction.
            unsafe { shared.ptr.as_ref() }
                .refs
                .store(2, Ordering::Relaxed);
            assert_eq!(mode, "below", "clone returned at the abort threshold");
            assert_eq!(after, isize::MAX as usize);
            assert_eq!(*cloned, 7);
            return;
        }
        for mode in ["below", "at"] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "memory::tests::shared_clone_aborts_at_the_refcount_threshold_in_a_subprocess",
                ])
                .env(CHILD, mode)
                .output()
                .unwrap();
            if mode == "below" {
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
            } else {
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    assert_eq!(result.status.signal(), Some(6), "expected SIGABRT");
                }
                #[cfg(not(unix))]
                assert!(!result.status.success() && result.status.code() != Some(101));
            }
        }
    }
    #[test]
    fn reservation_checks_capacity_overflow_and_budget_independently() {
        let mem = Memory::new(Limits::default()).unwrap();
        let mut bytes = Vec::<u8>::with_capacity(4);
        bytes.push(7);
        mem.fail_after(Some(0));
        assert_eq!(mem.reserve(&mut bytes, 3), Ok(()));
        assert_eq!(mem.reserve(&mut bytes, 4), Err(Error::OutOfMemory));
        assert_eq!(mem.reserve(&mut bytes, usize::MAX), Err(Error::Limit));
        assert_eq!(bytes, [7]);
        mem.fail_after(Some(1));
        assert_eq!(mem.reserve(&mut bytes, 4), Ok(()));
        assert_eq!(mem.claim(), Err(Error::OutOfMemory));
        assert_eq!(bytes, [7]);
        mem.fail_after(None);
        assert_eq!(mem.claim(), Ok(()));
        assert_eq!(mem.claim(), Ok(()));
        // A representable element count can still exceed Vec's byte limit.
        let mut words = Vec::<u64>::new();
        assert_eq!(
            mem.reserve(&mut words, isize::MAX as usize),
            Err(Error::OutOfMemory)
        );
        assert!(words.is_empty());
    }

    #[test]
    fn allocation_ownership_transfers_or_drops_exactly_once() {
        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let mem = Memory::new(Limits::default()).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        for allocation in 0..3 {
            mem.fail_after(Some(0));
            let probe = Probe(drops.clone());
            let result = match allocation {
                0 => mem.shared(probe).map(drop),
                1 => mem.boxed(probe).map(drop),
                _ => mem.raw(probe).map(|ptr| unsafe { free_raw(ptr) }),
            };
            assert_eq!(result, Err(Error::OutOfMemory));
            assert_eq!(drops.load(Ordering::Relaxed), allocation + 1);
        }
        // A zero-sized box has no allocation and consumes no budget.
        assert_eq!(mem.boxed(()).map(|value| *value), Ok(()));
        assert_eq!(mem.copy(b""), Ok(Vec::new()));
        assert_eq!(mem.copy(b"x"), Err(Error::OutOfMemory));
        mem.fail_after(Some(1));
        let shared = mem.shared(Probe(drops.clone())).unwrap();
        let cloned = shared.clone();
        assert!(shared.ptr_eq(&cloned));
        drop(shared);
        assert_eq!(drops.load(Ordering::Relaxed), 3);
        assert_eq!(mem.claim(), Err(Error::OutOfMemory));
        drop(cloned);
        assert_eq!(drops.load(Ordering::Relaxed), 4);
        mem.fail_after(Some(2));
        drop(mem.boxed(Probe(drops.clone())).unwrap());
        let raw = mem.raw(Probe(drops.clone())).unwrap();
        unsafe { free_raw(raw) };
        assert_eq!(drops.load(Ordering::Relaxed), 6);
        assert_eq!(mem.claim(), Err(Error::OutOfMemory));
    }

    #[test]
    fn last_shared_owner_destroys_once_after_cross_thread_accesses() {
        struct Probe {
            visits: Mutex<usize>,
            drops: Arc<AtomicUsize>,
        }
        impl Drop for Probe {
            fn drop(&mut self) {
                assert_eq!(*self.visits.get_mut().unwrap(), 8000);
                self.drops.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let shared = Shared::allocate(Probe {
            visits: Mutex::new(0),
            drops: drops.clone(),
        })
        .unwrap();
        let start = Barrier::new(8);
        thread::scope(|s| {
            for _ in 0..8 {
                let (value, start) = (shared.clone(), &start);
                s.spawn(move || {
                    start.wait();
                    for _ in 0..1000 {
                        *value.clone().visits.lock().unwrap() += 1;
                    }
                });
            }
            drop(shared);
        });
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }
}
