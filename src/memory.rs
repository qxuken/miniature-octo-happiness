use crate::{Error, Result};
use std::{
    alloc::{Layout, alloc, dealloc, realloc},
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

// Same test seam as allocate: realloc failure must leave its input allocation live.
unsafe fn reallocate(ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
    #[cfg(test)]
    if FAIL_SYSTEM_ALLOCATION.with(|fail| fail.replace(false)) {
        return std::ptr::null_mut();
    }
    unsafe { realloc(ptr, layout, size) }
}

/// Requested core allocation layouts, including in-flight reservations. Excludes
/// allocator overhead, backend/application/foreign-handle buffers and the fixed
/// budget control block. The peak includes old and new buffers during growth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryUsage {
    pub live_bytes: usize,
    pub peak_bytes: usize,
    pub limit_bytes: usize,
}

/// Resource limits are checked before allocation or publication.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum requested bytes of live core heap allocations; excludes application outputs,
    /// backend storage, foreign handles, allocator overhead and the budget control block.
    pub max_live_bytes: usize,
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
            max_live_bytes: usize::MAX,
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
    charge: Option<Charge>,
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
    fn allocate(value: T, charge: Option<Charge>) -> Result<Self> {
        let layout = Layout::new::<SharedBox<T>>();
        // SharedBox always has a nonzero size due to the count.
        let ptr = NonNull::new(unsafe { allocate(layout) }.cast::<SharedBox<T>>())
            .ok_or(Error::OutOfMemory)?;
        unsafe {
            ptr.as_ptr().write(SharedBox {
                refs: AtomicUsize::new(1),
                value,
                charge,
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
        // Keep the charge until destruction AND deallocation, including unwind.
        let charge = unsafe { std::ptr::read(&self.ptr.as_ref().charge) };
        let _allocation = Allocation {
            ptr: self.ptr.as_ptr().cast(),
            layout: Layout::new::<SharedBox<T>>(),
            _charge: charge,
        };
        unsafe { std::ptr::drop_in_place(&mut (*self.ptr.as_ptr()).value) };
    }
}

struct Charge {
    owner: Memory,
    bytes: usize,
}
impl Drop for Charge {
    fn drop(&mut self) {
        self.owner.0.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
// Drop body deallocates before Rust drops the charge field. Also used when an
// element destructor unwinds; allocated capacity must never become uncharged.
struct Allocation {
    ptr: *mut u8,
    layout: Layout,
    _charge: Option<Charge>,
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) };
    }
}

/// Internal capacity with exact requested allocation layout. No method exposes
/// a mutable Vec or grows implicitly; callers must use Memory::reserve first.
pub(crate) struct ChargedVec<T> {
    vec: Vec<T>,
    charge: Option<Charge>,
}
impl<T> ChargedVec<T> {
    pub(crate) fn new() -> Self {
        Self {
            vec: Vec::new(),
            charge: None,
        }
    }
    fn reserve(&mut self, mem: &Memory, extra: usize) -> Result<()> {
        let needed = self.vec.len().checked_add(extra).ok_or(Error::Limit)?;
        if needed <= self.vec.capacity() {
            return Ok(());
        }
        let layout = Layout::array::<T>(needed).map_err(|_| Error::Limit)?;
        let charge = mem.admit(layout.size())?;
        mem.claim()?;
        let len = self.vec.len();
        let capacity = self.vec.capacity();
        // SAFETY: every nonempty buffer originated here with this exact layout.
        // ZSTs have capacity usize::MAX and return before this branch. Realloc
        // keeps alignment and preserves the old allocation on null. All bytes
        // containing initialized elements remain valid in the returned buffer.
        let raw = unsafe {
            if capacity == 0 {
                allocate(layout)
            } else {
                let old = Layout::array::<T>(capacity).expect("existing exact layout");
                reallocate(self.vec.as_mut_ptr().cast(), old, layout.size())
            }
        };
        let ptr = NonNull::new(raw.cast::<T>()).ok_or(Error::OutOfMemory)?;
        // After successful realloc the old pointer must never be accessed or
        // deallocated. These metadata-only operations cannot unwind. Disarm
        // the old Vec and transfer sole ownership to the new exact capacity.
        let new = unsafe { Vec::from_raw_parts(ptr.as_ptr(), len, needed) };
        std::mem::forget(std::mem::replace(&mut self.vec, new));
        self.charge = Some(charge); // release old charge only after successful realloc
        Ok(())
    }
    pub(crate) fn as_slice(&self) -> &[T] {
        &self.vec
    }
    pub(crate) fn push(&mut self, value: T) {
        assert!(self.vec.len() < self.vec.capacity(), "reserve before push");
        self.vec.push(value);
    }
    pub(crate) fn extend(&mut self, values: impl IntoIterator<Item = T>) {
        for value in values {
            self.push(value);
        }
    }
    pub(crate) fn extend_from_slice(&mut self, values: &[T])
    where
        T: Clone,
    {
        assert!(
            values.len() <= self.vec.capacity() - self.vec.len(),
            "reserve before extend"
        );
        self.vec.extend_from_slice(values);
    }
    pub(crate) fn resize(&mut self, len: usize, value: T)
    where
        T: Clone,
    {
        assert!(len <= self.vec.capacity(), "reserve before resize");
        self.vec.resize(len, value);
    }
    pub(crate) fn truncate(&mut self, len: usize) {
        self.vec.truncate(len);
    }
    pub(crate) fn remove(&mut self, index: usize) -> T {
        self.vec.remove(index)
    }
    pub(crate) fn clear(&mut self) {
        self.vec.clear();
    }
    pub(crate) fn dedup_by_key<K: PartialEq>(&mut self, key: impl FnMut(&mut T) -> K) {
        self.vec.dedup_by_key(key);
    }
    /// Explicit ownership transfer to an uncharged application-output buffer.
    pub(crate) fn into_external(self) -> Vec<T> {
        self.vec
    }
}
impl<T> std::ops::Deref for ChargedVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.vec
    }
}
impl<T> std::ops::DerefMut for ChargedVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.vec
    }
}
impl<'a, T> IntoIterator for &'a ChargedVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.vec.iter()
    }
}
pub(crate) struct ChargedIntoIter<T> {
    iter: std::vec::IntoIter<T>,
    _charge: Option<Charge>,
}
impl<T> Iterator for ChargedIntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.iter.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
impl<T> IntoIterator for ChargedVec<T> {
    type Item = T;
    type IntoIter = ChargedIntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        ChargedIntoIter {
            iter: self.vec.into_iter(),
            _charge: self.charge,
        }
    }
}

struct MemoryState {
    limits: Limits,
    used: AtomicUsize,
    peak: AtomicUsize,
    #[cfg(any(test, feature = "fault-injection"))]
    remaining: AtomicUsize,
}
#[derive(Clone)]
pub(crate) struct Memory(Shared<MemoryState>);
impl Memory {
    pub(crate) fn new(limits: Limits) -> Result<Self> {
        Ok(Self(Shared::allocate(
            MemoryState {
                limits: limits.validate()?,
                used: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
                #[cfg(any(test, feature = "fault-injection"))]
                remaining: AtomicUsize::new(usize::MAX),
            },
            None,
        )?))
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
        let charge = self.admit(Layout::new::<SharedBox<T>>().size())?;
        Shared::allocate(value, Some(charge))
    }
    pub(crate) fn reserve_external<T>(&self, vec: &mut Vec<T>, extra: usize) -> Result<()> {
        let needed = vec.len().checked_add(extra).ok_or(Error::Limit)?;
        if needed > vec.capacity() {
            self.claim()?;
            vec.try_reserve_exact(extra)
                .map_err(|_| Error::OutOfMemory)?;
        }
        Ok(())
    }
    pub(crate) fn copy_external(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        if bytes.len() > isize::MAX as usize {
            return Err(Error::Limit);
        }
        let mut vec = Vec::new();
        self.reserve_external(&mut vec, bytes.len())?;
        vec.extend_from_slice(bytes);
        Ok(vec)
    }
    fn admit(&self, bytes: usize) -> Result<Charge> {
        let previous = self
            .0
            .used
            .try_update(Ordering::AcqRel, Ordering::Relaxed, |used| {
                used.checked_add(bytes)
                    .filter(|n| *n <= self.0.limits.max_live_bytes)
            })
            .map_err(|_| Error::Limit)?;
        self.0.peak.fetch_max(previous + bytes, Ordering::Relaxed);
        Ok(Charge {
            owner: self.clone(),
            bytes,
        })
    }
    pub(crate) fn public_usage(&self) -> MemoryUsage {
        let (live_bytes, peak_bytes) = self.usage();
        MemoryUsage {
            live_bytes,
            peak_bytes,
            limit_bytes: self.limits().max_live_bytes,
        }
    }
    pub(crate) fn usage(&self) -> (usize, usize) {
        let live = self.0.used.load(Ordering::Acquire);
        // Admission updates the counters separately. Persist a sampled high
        // water mark even if the admitting thread has not updated peak yet;
        // later queries must not forget it after another old owner drops.
        (
            live,
            self.0.peak.fetch_max(live, Ordering::Relaxed).max(live),
        )
    }
    pub(crate) fn reserve<T>(&self, vec: &mut ChargedVec<T>, extra: usize) -> Result<()> {
        vec.reserve(self, extra)
    }
    pub(crate) fn copy(&self, bytes: &[u8]) -> Result<ChargedVec<u8>> {
        let mut vec = ChargedVec::new();
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
            let shared = Shared::allocate(7u8, None).unwrap();
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
        assert_eq!(mem.reserve_external(&mut bytes, 3), Ok(()));
        assert_eq!(mem.reserve_external(&mut bytes, 4), Err(Error::OutOfMemory));
        assert_eq!(
            mem.reserve_external(&mut bytes, usize::MAX),
            Err(Error::Limit)
        );
        assert_eq!(bytes, [7]);
        mem.fail_after(Some(1));
        assert_eq!(mem.reserve_external(&mut bytes, 4), Ok(()));
        assert_eq!(mem.claim(), Err(Error::OutOfMemory));
        assert_eq!(bytes, [7]);
        mem.fail_after(None);
        assert_eq!(mem.claim(), Ok(()));
        assert_eq!(mem.claim(), Ok(()));
        // A representable element count can still exceed Vec's byte limit.
        let mut words = Vec::<u64>::new();
        assert_eq!(
            mem.reserve_external(&mut words, isize::MAX as usize),
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
        assert_eq!(mem.copy_external(b""), Ok(Vec::new()));
        assert_eq!(mem.copy_external(b"x"), Err(Error::OutOfMemory));
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
        let shared = Shared::allocate(
            Probe {
                visits: Mutex::new(0),
                drops: drops.clone(),
            },
            None,
        )
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

#[cfg(test)]
mod live_budget_tests {
    use super::*;
    fn memory(cap: usize) -> Memory {
        Memory::new(Limits {
            max_live_bytes: cap,
            ..Limits::default()
        })
        .unwrap()
    }
    #[test]
    fn growth_reserves_both_buffers_and_rolls_back_failures() {
        let mem = memory(23);
        let mut bytes = mem.copy(&[1; 8]).unwrap();
        assert_eq!(mem.usage().0, 8);
        assert_eq!(mem.reserve(&mut bytes, 8), Err(Error::Limit));
        assert_eq!(bytes.as_slice(), &[1; 8]);
        assert_eq!(mem.usage().0, 8);
        drop(bytes);
        assert_eq!(mem.usage().0, 0);
        let mem = memory(24);
        let mut bytes = mem.copy(&[1; 8]).unwrap();
        mem.fail_after(Some(0));
        assert_eq!(mem.reserve(&mut bytes, 8), Err(Error::OutOfMemory));
        assert_eq!(mem.usage().0, 8);
        mem.fail_after(None);
        FAIL_SYSTEM_ALLOCATION.with(|fail| fail.set(true));
        assert_eq!(mem.reserve(&mut bytes, 8), Err(Error::OutOfMemory));
        assert_eq!(mem.usage().0, 8);
        mem.reserve(&mut bytes, 8).unwrap();
        assert_eq!(mem.usage(), (16, 24));
        let mut iter = bytes.into_iter();
        assert_eq!(iter.next(), Some(1));
        assert_eq!(mem.usage().0, 16);
        drop(iter);
        assert_eq!(mem.usage().0, 0);
    }
    #[test]
    fn shared_clones_keep_one_charge_and_unwind_reclaims_allocation() {
        let size = Layout::new::<SharedBox<u64>>().size();
        let mem = memory(size);
        let value = mem.shared(1u64).unwrap();
        let cloned = value.clone();
        assert_eq!(mem.usage().0, size);
        assert!(matches!(mem.shared(2u64), Err(Error::Limit)));
        drop(value);
        assert_eq!(mem.usage().0, size);
        drop(cloned);
        assert_eq!(mem.usage().0, 0);
        struct Panics(Memory);
        impl Drop for Panics {
            fn drop(&mut self) {
                assert!(self.0.usage().0 > 0);
                panic!("intentional destructor panic");
            }
        }
        let mem = memory(4096);
        let value = mem.shared(Panics(mem.clone())).unwrap();
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(value))).is_err());
        assert_eq!(mem.usage().0, 0);
        let mut values = ChargedVec::new();
        mem.reserve(&mut values, 1).unwrap();
        values.push(Panics(mem.clone()));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(values.into_iter())))
                .is_err()
        );
        assert_eq!(mem.usage().0, 0);
    }
    #[test]
    fn concurrent_reservations_cannot_exceed_cap() {
        let mem = memory(32);
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let mem = &mem;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        let reservation = mem.copy(&[0; 32]);
                        barrier.wait();
                        reservation.is_ok()
                    })
                })
                .collect();
            assert_eq!(
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .filter(|ok| *ok)
                    .count(),
                1
            );
        });
        assert_eq!(mem.usage(), (0, 32));
    }
    #[test]
    fn admission_limit_and_overflow_decision_pairs() {
        for (cap, accepts) in [(8, true), (7, false)] {
            let mem = memory(cap);
            let baseline = mem.admit(7).unwrap();
            let extra = mem.admit(1);
            assert_eq!(extra.is_ok(), accepts);
            if !accepts {
                assert!(matches!(extra, Err(Error::Limit)));
            }
            drop(extra);
            drop(baseline);
            assert_eq!(mem.usage().0, 0);
        }
        let mem = memory(usize::MAX);
        let baseline = mem.admit(usize::MAX - 1).unwrap();
        let last = mem.admit(1).unwrap();
        drop(last);
        drop(baseline);
        let full = mem.admit(usize::MAX).unwrap();
        assert!(matches!(mem.admit(1), Err(Error::Limit)));
        drop(full);
        assert_eq!(mem.usage().0, 0);
        let mem = memory(0);
        drop(mem.admit(0).unwrap());
        assert!(matches!(mem.admit(1), Err(Error::Limit)));
        assert_eq!(mem.usage(), (0, 0));
    }
    #[test]
    fn charged_reserve_capacity_count_and_layout_decision_pairs() {
        let mem = memory(usize::MAX);
        let mut bytes = mem.copy(&[1]).unwrap();
        mem.fail_after(Some(0));
        assert_eq!(mem.reserve(&mut bytes, 0), Ok(()));
        assert_eq!(mem.reserve(&mut bytes, 1), Err(Error::OutOfMemory));
        assert_eq!(mem.reserve(&mut bytes, usize::MAX), Err(Error::Limit));
        assert_eq!(bytes.as_slice(), &[1]);
        let mut words = ChargedVec::<u64>::new();
        assert_eq!(mem.reserve(&mut words, 1), Err(Error::OutOfMemory));
        // Addition is representable; array byte size/layout is not.
        assert_eq!(
            mem.reserve(&mut words, isize::MAX as usize),
            Err(Error::Limit)
        );
        assert_eq!(mem.usage().0, 1);
    }
    #[test]
    fn usage_bounds_peak_during_admission_update_window() {
        let mem = memory(8);
        // Model the interval between admission CAS and fetch_max.
        mem.0.used.store(8, Ordering::Release);
        assert_eq!(mem.public_usage().peak_bytes, 8);
        // Another old owner can drop before the admitting thread updates peak.
        // A later query must not forget high water already reported above.
        mem.0.used.store(4, Ordering::Release);
        assert_eq!(mem.public_usage().peak_bytes, 8);
        mem.0.peak.fetch_max(8, Ordering::Relaxed);
        mem.0.used.store(0, Ordering::Release);
        assert_eq!(mem.usage(), (0, 8));
    }
    #[test]
    fn zero_sized_elements_and_output_transfer() {
        let mem = memory(0);
        let mut values = ChargedVec::new();
        mem.reserve(&mut values, 3).unwrap();
        values.extend([(), (), ()]);
        assert_eq!(values.len(), 3);
        assert_eq!(mem.usage(), (0, 0));
        let mem = memory(8);
        let output = mem.copy(&[7; 8]).unwrap().into_external();
        assert_eq!(mem.usage().0, 0);
        assert_eq!(output, [7; 8]);
    }
}

#[cfg(test)]
mod allocation_layout_tests {
    use super::*;
    #[repr(align(256))]
    struct Aligned(std::sync::Arc<AtomicUsize>, u64);
    impl Drop for Aligned {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn aligned_growth_partial_iteration_and_null_shared_cleanup() {
        let mem = Memory::new(Limits::default()).unwrap();
        let drops = std::sync::Arc::new(AtomicUsize::new(0));
        let mut values = ChargedVec::new();
        for i in 0..32 {
            mem.reserve(&mut values, 1).unwrap();
            values.push(Aligned(drops.clone(), i));
            assert_eq!(values.as_ptr() as usize % 256, 0);
            assert_eq!(mem.usage().0, values.len() * 256);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        let mut iter = values.into_iter();
        let first = iter.next().unwrap();
        assert_eq!(first.1, 0);
        assert_eq!(mem.usage().0, 8192);
        drop(iter);
        assert_eq!(drops.load(Ordering::SeqCst), 31);
        assert_eq!(mem.usage().0, 0);
        drop(first);
        assert_eq!(drops.load(Ordering::SeqCst), 32);
        FAIL_SYSTEM_ALLOCATION.with(|v| v.set(true));
        assert!(matches!(
            mem.shared(Aligned(drops.clone(), 0)),
            Err(Error::OutOfMemory)
        ));
        assert_eq!(mem.usage().0, 0);
        assert_eq!(drops.load(Ordering::SeqCst), 33);
    }
    #[test]
    fn admission_overflow_rolls_back_exactly() {
        let mem = Memory::new(Limits::default()).unwrap();
        let hold = mem.admit(usize::MAX).unwrap();
        assert!(matches!(mem.admit(1), Err(Error::Limit)));
        assert_eq!(mem.usage().0, usize::MAX);
        drop(hold);
        assert_eq!(mem.usage().0, 0);
    }
}
