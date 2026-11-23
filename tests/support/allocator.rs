use super::{AllocatorHandle, Error};
use std::{
    alloc::{AllocError, Allocator, Global, Layout},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Default)]
pub struct TestAllocator {
    fail_at: AtomicUsize,
    fail_from: AtomicUsize,
    live: AtomicUsize,
    bytes: AtomicUsize,
    requests: AtomicUsize,
    last_request_size: AtomicUsize,
}

impl TestAllocator {
    pub fn fail_next(&self) {
        self.fail_after(0);
    }

    pub fn fail_after(&self, allowed: usize) {
        self.fail_at
            .store(self.requests() + allowed + 1, Ordering::Relaxed);
    }

    /// Refuse every request after the allowance within one synchronous operation.
    /// Check that the operation propagates a real refusal as OutOfMemory.
    pub fn with_refusal<T>(
        &self,
        allowed: usize,
        operation: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Error> {
        let (result, actual) = self.count_with_refusal(allowed, operation);
        assert_eq!(
            actual > allowed,
            matches!(&result, Err(Error::OutOfMemory)),
            "allowed {allowed} requests, observed {actual}"
        );
        result
    }

    /// Return the number of requests, including refusals, for callers that
    /// translate allocation errors at a storage boundary.
    pub fn count_with_refusal<T>(
        &self,
        allowed: usize,
        operation: impl FnOnce() -> T,
    ) -> (T, usize) {
        let before = self.requests();
        self.fail_from
            .store(before + allowed + 1, Ordering::Relaxed);
        let result = operation();
        self.fail_from.store(0, Ordering::Relaxed);
        (result, self.requests() - before)
    }

    pub fn live(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }

    pub fn live_bytes(&self) -> usize {
        self.bytes.load(Ordering::Relaxed)
    }

    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::Relaxed)
    }

    pub fn try_handle(self: &Arc<Self>) -> Result<AllocatorHandle, Error> {
        AllocatorHandle::try_new(TestProvider(self.clone()))
    }

    pub fn handle(self: &Arc<Self>) -> AllocatorHandle {
        self.try_handle().unwrap()
    }

    pub fn last_request_size(&self) -> usize {
        self.last_request_size.load(Ordering::Relaxed)
    }

    fn check(&self, layout: Layout) -> std::result::Result<(), AllocError> {
        let request = self.requests.fetch_add(1, Ordering::Relaxed) + 1;
        self.last_request_size
            .store(layout.size(), Ordering::Relaxed);
        let threshold = self.fail_from.load(Ordering::Relaxed);
        if (threshold != 0 && request >= threshold)
            || self
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
