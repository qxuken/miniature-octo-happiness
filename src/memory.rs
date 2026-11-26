use crate::{Error, Result};
#[cfg(test)]
use std::sync::Arc;
use std::{
    alloc::{AllocError, Allocator, Global, Layout},
    marker::PhantomData,
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

// Copied from a C descriptor before the first allocator call.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AllocatorCallbacks {
    pub context: *mut std::ffi::c_void,
    pub allocate:
        Option<unsafe extern "C" fn(*mut std::ffi::c_void, usize, usize) -> *mut std::ffi::c_void>,
    pub deallocate:
        Option<unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, usize, usize)>,
    pub retain_context: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    pub release_context: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
}

pub(crate) struct ForeignAllocator(AllocatorCallbacks);

// SAFETY: the C allocator interface requires concurrent calls to be safe.
unsafe impl Send for ForeignAllocator {}
unsafe impl Sync for ForeignAllocator {}

impl ForeignAllocator {
    /// The callback functions and their context must satisfy the C allocator contract.
    pub(crate) unsafe fn new(callbacks: AllocatorCallbacks) -> Result<Self> {
        if callbacks.allocate.is_none()
            || callbacks.deallocate.is_none()
            || callbacks.retain_context.is_some() != callbacks.release_context.is_some()
        {
            return Err(Error::Invalid);
        }
        if let Some(retain) = callbacks.retain_context {
            unsafe { retain(callbacks.context) };
        }
        Ok(Self(callbacks))
    }
}

impl Drop for ForeignAllocator {
    fn drop(&mut self) {
        if let Some(release) = self.0.release_context {
            unsafe { release(self.0.context) };
        }
    }
}

// SAFETY: the C contract supplies layout-compatible blocks and matching frees.
// The context is retained until all allocations using this adapter are freed.
unsafe impl Allocator for ForeignAllocator {
    fn allocate(&self, layout: Layout) -> std::result::Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Global.allocate(layout);
        }
        let allocate = self.0.allocate.expect("validated allocator");
        let ptr = unsafe { allocate(self.0.context, layout.size(), layout.align()) };
        NonNull::new(ptr.cast())
            .map(|ptr| NonNull::slice_from_raw_parts(ptr, layout.size()))
            .ok_or(AllocError)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            let deallocate = self.0.deallocate.expect("validated allocator");
            unsafe {
                deallocate(
                    self.0.context,
                    ptr.as_ptr().cast(),
                    layout.size(),
                    layout.align(),
                )
            };
        }
    }
}

/// A shared allocator for core storage and opaque handles. The default uses Global.
#[derive(Default)]
pub struct AllocatorHandle(Option<NonNull<AllocatorOwner>>);

struct AllocatorOwner {
    refs: AtomicUsize,
    vtable: &'static AllocatorVtable,
}

struct AllocatorVtable {
    provider: unsafe fn(NonNull<AllocatorOwner>) -> NonNull<dyn Allocator>,
    free: unsafe fn(NonNull<AllocatorOwner>),
}

// A pointer to the first field is also the base pointer of the whole block.
#[repr(C)]
struct OwnedAllocator<A> {
    owner: AllocatorOwner,
    provider: A,
}

impl<A: Allocator + Send + Sync + 'static> OwnedAllocator<A> {
    const VTABLE: AllocatorVtable = AllocatorVtable {
        provider: Self::provider,
        free: Self::free,
    };

    unsafe fn provider(owner: NonNull<AllocatorOwner>) -> NonNull<dyn Allocator> {
        let block = owner.cast::<Self>().as_ptr();
        // SAFETY: this vtable belongs to the concrete type stored in the block.
        NonNull::from(unsafe { &(*block).provider })
    }

    unsafe fn free(owner: NonNull<AllocatorOwner>) {
        let block = owner.cast::<Self>();
        // SAFETY: only the last owner reaches this method. Move the provider
        // out so it can free the block before its own destructor runs.
        let provider = unsafe { (&raw const (*block.as_ptr()).provider).read() };
        unsafe { provider.deallocate(block.cast(), Layout::new::<Self>()) };
    }
}

// SAFETY: try_new requires a thread-safe provider. The strong count keeps the
// provider alive and synchronizes final destruction across all handles.
unsafe impl Send for AllocatorHandle {}
unsafe impl Sync for AllocatorHandle {}

impl AllocatorHandle {
    /// Move a provider into one shared block allocated by that provider.
    /// Cloning the handle does not allocate.
    pub fn try_new<A: Allocator + Send + Sync + 'static>(provider: A) -> Result<Self> {
        let block = provider
            .allocate(Layout::new::<OwnedAllocator<A>>())
            .map_err(|_| Error::OutOfMemory)?
            .cast::<OwnedAllocator<A>>();
        // SAFETY: the provider returned a unique block with this layout. The
        // Allocator contract preserves ownership when the provider is moved.
        unsafe {
            block.as_ptr().write(OwnedAllocator {
                owner: AllocatorOwner {
                    refs: AtomicUsize::new(1),
                    vtable: &OwnedAllocator::<A>::VTABLE,
                },
                provider,
            });
        }
        Ok(Self(Some(block.cast())))
    }

    fn as_allocator(&self) -> &dyn Allocator {
        match self.0 {
            // SAFETY: self keeps the block alive for the returned borrow.
            Some(owner) => unsafe { (owner.as_ref().vtable.provider)(owner).as_ref() },
            None => &Global,
        }
    }
}

impl Clone for AllocatorHandle {
    fn clone(&self) -> Self {
        if let Some(owner) = self.0 {
            retain_ref(unsafe { &owner.as_ref().refs });
        }
        Self(self.0)
    }
}

impl Drop for AllocatorHandle {
    fn drop(&mut self) {
        if let Some(owner) = self.0
            && release_ref(unsafe { &owner.as_ref().refs })
        {
            let free = unsafe { owner.as_ref().vtable.free };
            // SAFETY: the last decrement transfers exclusive ownership.
            unsafe { free(owner) };
        }
    }
}

fn try_box_in<T, A: Allocator>(value: T, allocator: A) -> Result<Box<T, A>> {
    let layout = Layout::new::<T>();
    if layout.size() == 0 {
        return Ok(Box::new_in(value, allocator));
    }
    let ptr = allocator
        .allocate(layout)
        .map_err(|_| Error::OutOfMemory)?
        .cast::<T>();
    // SAFETY: the unique block came from this allocator with Layout<T>.
    unsafe {
        ptr.as_ptr().write(value);
        Ok(Box::from_raw_in(ptr.as_ptr(), allocator))
    }
}

// SAFETY: cloned handles share the provider until the final block is freed.
// Report the requested layout even when the provider offers additional capacity.
unsafe impl Allocator for AllocatorHandle {
    fn allocate(&self, layout: Layout) -> std::result::Result<NonNull<[u8]>, AllocError> {
        self.as_allocator()
            .allocate(layout)
            .map(|block| NonNull::slice_from_raw_parts(block.cast(), layout.size()))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        unsafe { self.as_allocator().deallocate(ptr, layout) };
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old: Layout,
        new: Layout,
    ) -> std::result::Result<NonNull<[u8]>, AllocError> {
        unsafe { self.as_allocator().grow(ptr, old, new) }
            .map(|block| NonNull::slice_from_raw_parts(block.cast(), new.size()))
    }
}

/// Requested core allocation layouts, including in-flight reservations. Excludes
/// allocator overhead, backend/application/foreign-handle buffers and the fixed
/// budget and allocator ownership blocks. The peak includes old and new buffers
/// during growth.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryUsage {
    pub live_bytes: usize,
    pub peak_bytes: usize,
    pub limit_bytes: usize,
}

/// Resource limits are checked before allocation or publication.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum requested bytes of live core heap allocations; excludes application outputs,
    /// backend storage, foreign handles, allocator overhead and the fixed budget and allocator
    /// ownership blocks.
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

fn retain_ref(refs: &AtomicUsize) {
    // The live source handle keeps the value alive. Abort well below wraparound.
    if refs.fetch_add(1, Ordering::Relaxed) >= isize::MAX as usize {
        std::process::abort();
    }
}

fn release_ref(refs: &AtomicUsize) -> bool {
    // Acquire prior owners' accesses before destruction. AcqRel on every
    // decrement is also understood by ThreadSanitizer.
    refs.fetch_sub(1, Ordering::AcqRel) == 1
}

// Fallible atomic reference counting for core and budget allocations.
struct SharedBox<T> {
    refs: AtomicUsize,
    value: T,
    ownership: AllocationOwner,
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
    fn allocate(value: T, allocator: AllocatorHandle, charge: Option<Charge>) -> Result<Self> {
        let layout = Layout::new::<SharedBox<T>>();
        // SharedBox always has a nonzero size due to the count.
        let ptr = allocator
            .as_allocator()
            .allocate(layout)
            .map_err(|_| Error::OutOfMemory)?
            .cast::<SharedBox<T>>();
        unsafe {
            ptr.as_ptr().write(SharedBox {
                refs: AtomicUsize::new(1),
                value,
                ownership: AllocationOwner {
                    allocator,
                    _charge: charge,
                },
            });
        }
        Ok(Self {
            ptr,
            _owns: PhantomData,
        })
    }
    pub(crate) fn get_mut(&mut self) -> Option<&mut T> {
        // Shared has no weak references. A unique strong owner excludes concurrent access.
        if unsafe { self.ptr.as_ref().refs.load(Ordering::Acquire) } == 1 {
            Some(unsafe { &mut self.ptr.as_mut().value })
        } else {
            None
        }
    }
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
    // Address-only identity: never converted back into a pointer. A live Shared
    // owner must outlive any index entry using this key.
    pub(crate) fn identity(&self) -> usize {
        self.ptr.as_ptr().addr()
    }
}
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        retain_ref(unsafe { &self.ptr.as_ref().refs });
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
        if !release_ref(unsafe { &self.ptr.as_ref().refs }) {
            return;
        }
        // Keep the allocator and any charge through destruction and deallocation.
        let ownership = unsafe { std::ptr::read(&self.ptr.as_ref().ownership) };
        let _allocation = Allocation {
            ptr: self.ptr.cast(),
            layout: Layout::new::<SharedBox<T>>(),
            ownership,
        };
        unsafe { std::ptr::drop_in_place(&mut (*self.ptr.as_ptr()).value) };
    }
}

struct Charge {
    owner: Budget,
    bytes: usize,
}
impl Drop for Charge {
    fn drop(&mut self) {
        self.owner.0.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
struct AllocationOwner {
    allocator: AllocatorHandle,
    _charge: Option<Charge>,
}

// Deallocate before releasing the allocator and any charge, including when a
// value destructor unwinds.
struct Allocation {
    ptr: NonNull<u8>,
    layout: Layout,
    ownership: AllocationOwner,
}
impl Drop for Allocation {
    fn drop(&mut self) {
        let allocator = self.ownership.allocator.as_allocator();
        unsafe { allocator.deallocate(self.ptr, self.layout) };
    }
}

/// Internal capacity with exact requested allocation layout. No method exposes
/// a mutable Vec or grows implicitly; callers must reserve through Memory first.
pub(crate) struct ChargedVec<T> {
    vec: Vec<T, AllocatorHandle>,
    charge: Option<Charge>,
}
impl<T> ChargedVec<T> {
    pub(crate) fn new() -> Self {
        Self {
            vec: Vec::new_in(AllocatorHandle::default()),
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
        if self.vec.capacity() == 0 {
            self.vec = Vec::new_in(mem.allocator());
        }
        self.vec
            .try_reserve_exact(extra)
            .map_err(|_| Error::OutOfMemory)?;
        self.charge = Some(charge); // release old charge after successful growth
        Ok(())
    }
    /// Bounded power-of-two growth for append-heavy scratch/cache buffers.
    /// Keep the exact allocator/accounting path, and fall back to exact size
    /// only when speculative slack is rejected by the live/layout limit.
    fn reserve_geometric(&mut self, mem: &Memory, extra: usize, maximum: usize) -> Result<()> {
        let needed = self.vec.len().checked_add(extra).ok_or(Error::Limit)?;
        if needed > maximum {
            return Err(Error::Limit);
        }
        if needed <= self.vec.capacity() {
            return Ok(());
        }
        let capacity = needed
            .checked_next_power_of_two()
            .unwrap_or(maximum)
            .min(maximum);
        match self.reserve(mem, capacity - self.vec.len()) {
            Err(Error::Limit) if capacity != needed => self.reserve(mem, extra),
            result => result,
        }
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
    pub(crate) fn into_external(self) -> Result<Vec<T>> {
        if self.vec.allocator().0.is_some() {
            if self.vec.is_empty() {
                return Ok(Vec::new());
            }
            let mut output = Vec::new();
            output
                .try_reserve_exact(self.vec.len())
                .map_err(|_| Error::OutOfMemory)?;
            output.extend(self.vec);
            return Ok(output);
        }
        let (ptr, len, capacity, _) = self.vec.into_raw_parts_with_allocator();
        // SAFETY: this buffer uses Global and transfers with its exact layout.
        Ok(unsafe { Vec::from_raw_parts(ptr, len, capacity) })
    }
}
impl<T> AsRef<[T]> for ChargedVec<T> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
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
    iter: <Vec<T, AllocatorHandle> as IntoIterator>::IntoIter,
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

struct BudgetState {
    limits: Limits,
    used: AtomicUsize,
    peak: AtomicUsize,
}

#[derive(Clone)]
struct Budget(Shared<BudgetState>);
impl Budget {
    fn new(limits: Limits, allocator: &AllocatorHandle) -> Result<Self> {
        Ok(Self(Shared::allocate(
            BudgetState {
                limits: limits.validate()?,
                used: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
            },
            allocator.clone(),
            None,
        )?))
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
            limit_bytes: self.0.limits.max_live_bytes,
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
}

#[derive(Clone)]
pub(crate) struct Memory {
    budget: Budget,
    allocator: AllocatorHandle,
}

impl Memory {
    pub(crate) fn new(limits: Limits) -> Result<Self> {
        Self::with_allocator(limits, AllocatorHandle::default())
    }
    pub(crate) fn with_allocator(limits: Limits, allocator: AllocatorHandle) -> Result<Self> {
        Ok(Self {
            budget: Budget::new(limits, &allocator)?,
            allocator,
        })
    }
    pub(crate) fn allocator(&self) -> AllocatorHandle {
        self.allocator.clone()
    }
    pub(crate) fn limits(&self) -> Limits {
        self.budget.0.limits
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.budget.0.ptr_eq(&other.budget.0)
    }
    fn admit(&self, bytes: usize) -> Result<Charge> {
        self.budget.admit(bytes)
    }
    pub(crate) fn public_usage(&self) -> MemoryUsage {
        self.budget.public_usage()
    }
    #[cfg(test)]
    pub(crate) fn usage(&self) -> (usize, usize) {
        self.budget.usage()
    }
    pub(crate) fn shared<T>(&self, value: T) -> Result<Shared<T>> {
        let charge = self.admit(Layout::new::<SharedBox<T>>().size())?;
        Shared::allocate(value, self.allocator(), Some(charge))
    }
    pub(crate) fn reserve_external<T, A: Allocator>(
        &self,
        vec: &mut Vec<T, A>,
        extra: usize,
    ) -> Result<()> {
        vec.len().checked_add(extra).ok_or(Error::Limit)?;
        vec.try_reserve_exact(extra).map_err(|_| Error::OutOfMemory)
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
    pub(crate) fn reserve<T>(&self, vec: &mut ChargedVec<T>, extra: usize) -> Result<()> {
        vec.reserve(self, extra)
    }
    pub(crate) fn reserve_geometric<T>(
        &self,
        vec: &mut ChargedVec<T>,
        extra: usize,
        maximum: usize,
    ) -> Result<()> {
        vec.reserve_geometric(self, extra, maximum)
    }
    pub(crate) fn copy(&self, bytes: &[u8]) -> Result<ChargedVec<u8>> {
        let mut vec = ChargedVec::new();
        self.reserve(&mut vec, bytes.len())?;
        vec.extend_from_slice(bytes);
        Ok(vec)
    }
    /// Allocate ownership fallibly. Zero-sized boxes require no provider request.
    pub(crate) fn boxed<T>(&self, value: T) -> Result<Box<T, AllocatorHandle>> {
        try_box_in(value, self.allocator())
    }
    pub(crate) fn raw<T>(&self, value: T) -> Result<*mut T> {
        assert!(std::mem::size_of::<T>() != 0);
        Ok(Box::into_raw_with_allocator(self.boxed(value)?).0)
    }

    /// Safety: ptr is consumed once and must come from this Memory's raw().
    pub(crate) unsafe fn free_raw<T>(&self, ptr: *mut T) {
        unsafe { drop(Box::from_raw_in(ptr, self.allocator())) };
    }
}

#[cfg(test)]
pub(crate) mod test_allocator;

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
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let bootstrap_bytes = allocator.live_bytes();
        let drops = Arc::new(AtomicUsize::new(0));
        for kind in 0..3 {
            allocator.fail_next();
            let input = Probe(drops.clone());
            let result = match kind {
                0 => mem.shared(input).map(drop),
                1 => mem.boxed(input).map(drop),
                _ => mem.raw(input).map(|ptr| unsafe { mem.free_raw(ptr) }),
            };
            assert_eq!(result, Err(Error::OutOfMemory));
            assert_eq!(drops.load(Ordering::Relaxed), kind + 1);
            assert_eq!(allocator.live_bytes(), bootstrap_bytes);
        }
        drop(mem.shared(Probe(drops.clone())).unwrap());
        assert_eq!(drops.load(Ordering::Relaxed), 4);
    }
    #[test]
    #[cfg(not(miri))]
    fn shared_clone_aborts_at_the_refcount_threshold_in_a_subprocess() {
        const CHILD: &str = "WEEKEND_DB_REFCOUNT_ABORT_CHILD";
        if let Some(mode) = std::env::var_os(CHILD) {
            let shared = Shared::allocate(7u8, AllocatorHandle::default(), None).unwrap();
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
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: 0,
            ..Limits::default()
        });
        let mut bytes = Vec::<u8, _>::with_capacity_in(4, mem.allocator());
        bytes.push(7);
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve_external(&mut bytes, 3)),
            Ok(())
        );
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve_external(&mut bytes, 4)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve_external(&mut bytes, usize::MAX)),
            Err(Error::Limit)
        );
        assert_eq!(bytes, [7]);
        assert_eq!(bytes.capacity(), 4);
        let (result, requests) =
            allocator.count_with_refusal(1, || mem.reserve_external(&mut bytes, 4));
        assert_eq!(result, Ok(()));
        assert_eq!(requests, 1);
        assert_eq!(bytes, [7]);

        // A representable element count can still exceed Vec's byte limit.
        let mut words = Vec::<u64, _>::new_in(mem.allocator());
        let (result, requests) = allocator
            .count_with_refusal(0, || mem.reserve_external(&mut words, isize::MAX as usize));
        assert_eq!(result, Err(Error::OutOfMemory));
        assert_eq!(requests, 0);
        assert!(words.is_empty());
        assert_eq!(mem.usage(), (0, 0));
    }

    #[test]
    fn allocation_ownership_transfers_or_drops_exactly_once() {
        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits::default());
        let bootstrap_bytes = allocator.live_bytes();
        let drops = Arc::new(AtomicUsize::new(0));

        assert_eq!(
            allocator.with_refusal(0, || mem.boxed(()).map(|value| *value)),
            Ok(())
        );
        assert_eq!(mem.copy_external(b""), Ok(Vec::new()));

        let shared = mem.shared(Probe(drops.clone())).unwrap();
        let cloned = shared.clone();
        assert!(shared.ptr_eq(&cloned));
        drop(shared);
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        drop(cloned);
        assert_eq!(drops.load(Ordering::Relaxed), 1);

        drop(mem.boxed(Probe(drops.clone())).unwrap());
        let raw = mem.raw(Probe(drops.clone())).unwrap();
        unsafe { mem.free_raw(raw) };
        assert_eq!(drops.load(Ordering::Relaxed), 3);
        assert_eq!(allocator.live_bytes(), bootstrap_bytes);
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
            AllocatorHandle::default(),
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
    fn geometric_growth_is_bounded_and_uses_logarithmic_allocation_requests() {
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: 16384,
            ..Limits::default()
        });
        let mut bytes = ChargedVec::new();
        // Capacities 1, 2, 4, ..., 4096 require thirteen requests.
        let ((), requests) = allocator.count_with_refusal(13, || {
            for i in 0..4096 {
                mem.reserve_geometric(&mut bytes, 1, 4096).unwrap();
                bytes.push((i % 251) as u8);
            }
        });
        assert_eq!(requests, 13);
        assert_eq!(bytes.vec.capacity(), 4096);
        assert_eq!(mem.usage(), (4096, 6144));
        for (i, b) in bytes.iter().enumerate() {
            assert_eq!(*b, (i % 251) as u8);
        }
        assert_eq!(
            mem.reserve_geometric(&mut bytes, 1, 4096),
            Err(Error::Limit)
        );
        assert_eq!(
            mem.reserve_geometric(&mut bytes, usize::MAX, usize::MAX),
            Err(Error::Limit)
        );
        drop(bytes);
        assert_eq!(mem.usage().0, 0);
        let mut bytes = mem.copy(&[1, 2, 3]).unwrap();
        mem.reserve_geometric(&mut bytes, 3, 6).unwrap();
        assert_eq!(bytes.vec.capacity(), 6); // clamp a non-power-of-two maximum
        assert_eq!(&*bytes, &[1, 2, 3]);
        let ((), requests) = allocator.count_with_refusal(0, || {
            mem.reserve_geometric(&mut bytes, 3, 6).unwrap(); // spare capacity
            assert_eq!(mem.reserve_geometric(&mut bytes, 4, 6), Err(Error::Limit));
            let mut zst = ChargedVec::<()>::new();
            mem.reserve_geometric(&mut zst, 8, 8).unwrap();
            zst.resize(8, ());
            assert_eq!(mem.reserve_geometric(&mut zst, 1, 8), Err(Error::Limit));
            let mut empty = ChargedVec::<u8>::new();
            mem.reserve_geometric(&mut empty, 0, 0).unwrap();
            assert_eq!(mem.reserve_geometric(&mut empty, 1, 0), Err(Error::Limit));
            // Generic overflow/layout guards precede allocation, even though codec
            // callers' 64 MiB/16,384-entry bounds cannot approach these sizes.
            assert_eq!(
                mem.reserve_geometric(&mut empty, usize::MAX / 2 + 2, usize::MAX),
                Err(Error::Limit)
            );
        });
        assert_eq!(requests, 0);
    }

    #[test]
    fn geometric_growth_exact_fallback_preserves_budget_and_failure_atomicity() {
        for cap in [8, 9, 12] {
            let mem = memory(cap);
            let mut bytes = ChargedVec::<u8>::new();
            mem.reserve_geometric(&mut bytes, 3, 16).unwrap();
            bytes.extend_from_slice(&[1, 2, 3]);
            assert_eq!(mem.usage(), (4, 4));
            let result = mem.reserve_geometric(&mut bytes, 2, 16);
            assert_eq!(result, if cap == 8 { Err(Error::Limit) } else { Ok(()) });
            assert_eq!(&*bytes, &[1, 2, 3]);
            assert_eq!(
                bytes.vec.capacity(),
                match cap {
                    8 => 4,
                    9 => 5,
                    _ => 8,
                }
            );
            assert_eq!(
                mem.usage(),
                match cap {
                    8 => (4, 4),
                    9 => (5, 9),
                    _ => (8, 12),
                }
            );
            drop(bytes);
            assert_eq!(mem.usage().0, 0);
        }
        // Hold Error::Limit fixed while toggling capacity != needed: with
        // needed=8, rounding is already exact, so there is no fallback.
        // At needed=5, rounded Limit falls back to exact 5.
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: 9,
            ..Limits::default()
        });
        let mut bytes = mem.copy(&[1, 2, 3, 4]).unwrap();
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve_geometric(&mut bytes, 4, 16)),
            Err(Error::Limit)
        );
        assert_eq!(&*bytes, &[1, 2, 3, 4]);
        assert_eq!(bytes.vec.capacity(), 4);
        assert_eq!(mem.usage(), (4, 4));
        drop(bytes);
        assert_eq!(mem.usage().0, 0);
        // Rounding fails admission, exact admission succeeds, then its injected
        // allocation failure must propagate without changing the original Vec.
        let mut bytes = mem.copy(&[1, 2, 3, 4]).unwrap();
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve_geometric(&mut bytes, 1, 16)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(allocator.last_request_size(), 5);
        assert_eq!(&*bytes, &[1, 2, 3, 4]);
        assert_eq!(mem.usage(), (4, 9));
        mem.reserve_geometric(&mut bytes, 1, 16).unwrap();
        assert_eq!(bytes.vec.capacity(), 5);
        // A real allocator-null result on the rounded request must not be
        // hidden by retrying the smaller request after OutOfMemory.
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: 32,
            ..Limits::default()
        });
        let mut bytes = mem.copy(&[7; 4]).unwrap();
        allocator.fail_next();
        assert_eq!(
            mem.reserve_geometric(&mut bytes, 1, 16),
            Err(Error::OutOfMemory)
        );
        assert_eq!(&*bytes, &[7; 4]);
        assert_eq!(bytes.vec.capacity(), 4);
        assert_eq!(mem.usage(), (4, 12));
        mem.reserve_geometric(&mut bytes, 1, 16).unwrap();
        assert_eq!(bytes.vec.capacity(), 8);
    }
    #[test]
    fn growth_reserves_both_buffers_and_rolls_back_failures() {
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: 23,
            ..Limits::default()
        });
        let mut bytes = ChargedVec::<u8>::new();
        allocator.fail_next();
        assert_eq!(mem.reserve(&mut bytes, 8), Err(Error::OutOfMemory));
        assert_eq!(bytes.vec.capacity(), 0);
        assert_eq!(mem.usage(), (0, 8));
        let mut bytes = mem.copy(&[1; 8]).unwrap();
        assert_eq!(mem.usage().0, 8);
        assert_eq!(mem.reserve(&mut bytes, 8), Err(Error::Limit));
        assert_eq!(bytes.as_slice(), &[1; 8]);
        assert_eq!(mem.usage().0, 8);
        drop(bytes);
        assert_eq!(mem.usage().0, 0);
        let mem = allocator.memory(Limits {
            max_live_bytes: 24,
            ..Limits::default()
        });
        let mut bytes = mem.copy(&[1; 8]).unwrap();
        allocator.fail_next();
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
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits {
            max_live_bytes: usize::MAX,
            ..Limits::default()
        });
        let mut bytes = mem.copy(&[1]).unwrap();
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve(&mut bytes, 0)),
            Ok(())
        );
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve(&mut bytes, 1)),
            Err(Error::OutOfMemory)
        );
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve(&mut bytes, usize::MAX)),
            Err(Error::Limit)
        );
        assert_eq!(bytes.as_slice(), &[1]);
        let mut words = ChargedVec::<u64>::new();
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve(&mut words, 1)),
            Err(Error::OutOfMemory)
        );
        // The element count fits, while its byte layout exceeds the limit.
        assert_eq!(
            allocator.with_refusal(0, || mem.reserve(&mut words, isize::MAX as usize)),
            Err(Error::Limit)
        );
        assert_eq!(mem.usage().0, 1);
    }
    #[test]
    fn usage_bounds_peak_during_admission_update_window() {
        let mem = memory(8);
        // Model the interval between admission CAS and fetch_max.
        mem.budget.0.used.store(8, Ordering::Release);
        assert_eq!(mem.public_usage().peak_bytes, 8);
        // Another old owner can drop before the admitting thread updates peak.
        // A later query must not forget high water already reported above.
        mem.budget.0.used.store(4, Ordering::Release);
        assert_eq!(mem.public_usage().peak_bytes, 8);
        mem.budget.0.peak.fetch_max(8, Ordering::Relaxed);
        mem.budget.0.used.store(0, Ordering::Release);
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
        let bytes = mem.copy(&[7; 8]).unwrap();
        let addr = bytes.as_ptr().addr();
        let output = bytes.into_external().unwrap();
        assert_eq!(mem.usage().0, 0);
        assert_eq!(output.as_ptr().addr(), addr);
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
        let allocator = Arc::new(test_allocator::TestAllocator::default());
        let mem = allocator.memory(Limits::default());
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
        allocator.fail_next();
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
