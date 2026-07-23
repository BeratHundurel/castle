use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

struct TrackingAllocator;

static CURRENT_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
static TOTAL_ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

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
        CURRENT_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if new_pointer.is_null() {
            return new_pointer;
        }

        match new_size.cmp(&layout.size()) {
            std::cmp::Ordering::Greater => record_allocation(new_size - layout.size()),
            std::cmp::Ordering::Less => {
                CURRENT_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
            std::cmp::Ordering::Equal => {}
        }

        new_pointer
    }
}

fn record_allocation(bytes: usize) {
    TOTAL_ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    let current = CURRENT_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK_BYTES.fetch_max(current, Ordering::Relaxed);
}

pub(crate) struct AllocationSnapshot {
    baseline_bytes: usize,
    allocated_bytes: usize,
}

pub(crate) struct AllocationDelta {
    pub(crate) peak_growth_bytes: usize,
    pub(crate) retained_growth_bytes: usize,
    pub(crate) allocated_bytes: usize,
}

pub(crate) fn start_measurement() -> AllocationSnapshot {
    let baseline_bytes = CURRENT_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(baseline_bytes, Ordering::Relaxed);

    AllocationSnapshot {
        baseline_bytes,
        allocated_bytes: TOTAL_ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

impl AllocationSnapshot {
    pub(crate) fn finish(self) -> AllocationDelta {
        AllocationDelta {
            peak_growth_bytes: PEAK_BYTES
                .load(Ordering::Relaxed)
                .saturating_sub(self.baseline_bytes),
            retained_growth_bytes: CURRENT_BYTES
                .load(Ordering::Relaxed)
                .saturating_sub(self.baseline_bytes),
            allocated_bytes: TOTAL_ALLOCATED_BYTES
                .load(Ordering::Relaxed)
                .saturating_sub(self.allocated_bytes),
        }
    }
}
