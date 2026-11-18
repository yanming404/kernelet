// SPDX-License-Identifier: MPL-2.0
//! Measures native allocator use on the same queue fast-path implementation.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use typedio::queue::{QueueCore, RequestTag};

thread_local! {static WATCH:Cell<bool>=const{Cell::new(false)};static ALLOCATIONS:Cell<usize>=const{Cell::new(0)};}
struct Counting;
// SAFETY: all allocation/deallocation behavior is delegated unchanged to System.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = WATCH.try_with(|watch| {
            if watch.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
        // SAFETY: the allocator forwards the caller's original valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout are forwarded to the same backing allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
#[test]
fn queue_admission_completion_cancel_and_drain_allocate_nothing() {
    let mut q = QueueCore::<u64, u64, 8>::new(1).unwrap();
    ALLOCATIONS.with(|count| count.set(0));
    WATCH.with(|watch| watch.set(true));
    for n in 0..1000 {
        let id = q.try_submit(n, RequestTag(n)).unwrap();
        let value = q.take_for_dispatch(id).unwrap();
        q.cancel(id).unwrap();
        q.finish(id, Ok(value)).unwrap();
        let result = q.try_pop_completion().unwrap();
        assert_eq!(result.result, Ok(n));
        q.take_events();
        q.rearm_irq();
    }
    WATCH.with(|watch| watch.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
}
