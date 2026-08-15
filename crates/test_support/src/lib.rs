//! Shared measurement primitives for Castle's test binaries.
//!
//! Add this crate only as a development dependency. A final test binary may
//! contain only one global allocator, so consumers must remove any local
//! `#[global_allocator]` declaration when adopting this crate.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    marker::PhantomData,
    rc::Rc,
};

/// System allocator wrapper that tracks allocations independently per thread.
pub struct TrackingAllocator;

#[derive(Clone, Copy)]
struct AllocationState {
    current_bytes: usize,
    peak_bytes: usize,
    total_allocated_bytes: usize,
}

thread_local! {
    // Rust tests share one allocator process while running on separate test threads.
    static ALLOCATION_STATE: Cell<AllocationState> = const { Cell::new(AllocationState {
        current_bytes: 0,
        peak_bytes: 0,
        total_allocated_bytes: 0,
    }) };
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        record_deallocation(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if new_pointer.is_null() {
            return new_pointer;
        }

        match new_size.cmp(&layout.size()) {
            std::cmp::Ordering::Greater => record_allocation(new_size - layout.size()),
            std::cmp::Ordering::Less => record_deallocation(layout.size() - new_size),
            std::cmp::Ordering::Equal => {}
        }

        new_pointer
    }
}

fn record_allocation(bytes: usize) {
    let _ = ALLOCATION_STATE.try_with(|cell| {
        let mut state = cell.get();
        state.total_allocated_bytes = state.total_allocated_bytes.saturating_add(bytes);
        state.current_bytes = state.current_bytes.saturating_add(bytes);
        state.peak_bytes = state.peak_bytes.max(state.current_bytes);
        cell.set(state);
    });
}

fn record_deallocation(bytes: usize) {
    let _ = ALLOCATION_STATE.try_with(|cell| {
        let mut state = cell.get();
        state.current_bytes = state.current_bytes.saturating_sub(bytes);
        cell.set(state);
    });
}

/// The allocation growth observed between the beginning and end of a measurement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocationDelta {
    /// Maximum growth in live bytes over the measurement baseline.
    pub peak_growth_bytes: usize,
    /// Live bytes remaining above the measurement baseline at completion.
    pub retained_growth_bytes: usize,
    /// Sum of newly allocated bytes, including allocations later freed.
    pub allocated_bytes: usize,
}

/// An in-progress allocation measurement bound to its originating thread.
#[must_use = "call finish to obtain the allocation delta"]
pub struct AllocationSnapshot {
    baseline_bytes: usize,
    allocated_bytes: usize,
    _current_thread: PhantomData<Rc<()>>,
}

/// Starts a measurement for the calling thread.
///
/// Allocations on parallel test threads are excluded. Measurements must not be
/// nested on the same thread because starting one resets that thread's peak.
pub fn start_measurement() -> AllocationSnapshot {
    ALLOCATION_STATE.with(|cell| {
        let mut state = cell.get();
        state.peak_bytes = state.current_bytes;
        cell.set(state);

        AllocationSnapshot {
            baseline_bytes: state.current_bytes,
            allocated_bytes: state.total_allocated_bytes,
            _current_thread: PhantomData,
        }
    })
}

impl AllocationSnapshot {
    /// Completes the measurement on its originating thread.
    #[must_use]
    pub fn finish(self) -> AllocationDelta {
        ALLOCATION_STATE.with(|state| {
            let state = state.get();
            AllocationDelta {
                peak_growth_bytes: state.peak_bytes.saturating_sub(self.baseline_bytes),
                retained_growth_bytes: state.current_bytes.saturating_sub(self.baseline_bytes),
                allocated_bytes: state
                    .total_allocated_bytes
                    .saturating_sub(self.allocated_bytes),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::start_measurement;
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    #[test]
    fn reports_allocated_peak_and_retained_bytes() {
        const ALLOCATED_BYTES: usize = 64 * 1024;

        let measurement = start_measurement();
        let retained = vec![0_u8; ALLOCATED_BYTES];
        std::hint::black_box(&retained);
        let allocation = measurement.finish();

        assert!(allocation.allocated_bytes >= ALLOCATED_BYTES);
        assert!(allocation.peak_growth_bytes >= ALLOCATED_BYTES);
        assert!(allocation.retained_growth_bytes >= ALLOCATED_BYTES);
    }

    #[test]
    fn reports_freed_memory_as_not_retained() {
        const ALLOCATED_BYTES: usize = 64 * 1024;

        let measurement = start_measurement();
        {
            let released = vec![0_u8; ALLOCATED_BYTES];
            std::hint::black_box(&released);
        }
        let allocation = measurement.finish();

        assert!(allocation.allocated_bytes >= ALLOCATED_BYTES);
        assert!(allocation.peak_growth_bytes >= ALLOCATED_BYTES);
        assert_eq!(allocation.retained_growth_bytes, 0);
    }

    #[test]
    fn measurement_ignores_allocations_from_parallel_test_threads() {
        const BACKGROUND_BYTES: usize = 8 * 1024 * 1024;
        const LOCAL_BYTES: usize = 64 * 1024;

        let start = Arc::new(Barrier::new(2));
        let allocated = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker = thread::spawn({
            let start = start.clone();
            let allocated = allocated.clone();
            let release = release.clone();
            move || {
                start.wait();
                let background = vec![0_u8; BACKGROUND_BYTES];
                std::hint::black_box(&background);
                allocated.wait();
                release.wait();
            }
        });

        let measurement = start_measurement();
        start.wait();
        allocated.wait();
        let local = vec![0_u8; LOCAL_BYTES];
        std::hint::black_box(&local);
        let allocation = measurement.finish();
        release.wait();
        worker
            .join()
            .expect("allocation worker should finish without panicking");

        assert!(allocation.allocated_bytes >= LOCAL_BYTES);
        assert!(allocation.peak_growth_bytes >= LOCAL_BYTES);
        assert!(allocation.retained_growth_bytes >= LOCAL_BYTES);
        assert!(allocation.allocated_bytes < BACKGROUND_BYTES / 2);
        assert!(allocation.peak_growth_bytes < BACKGROUND_BYTES / 2);
        assert!(allocation.retained_growth_bytes < BACKGROUND_BYTES / 2);
    }
}
