//! A hard bound on live Rust allocation bytes in the extractor. Darwin rejects
//! finite AS/DATA limits on some systems; lopdf and its decoders allocate through
//! this allocator. OS limits additionally cover mappings/native allocations
//! where supported. Outside the extractor the budget remains unlimited.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static BUDGET: AtomicUsize = AtomicUsize::new(usize::MAX);

#[global_allocator]
static ALLOCATOR: Bounded = Bounded;
struct Bounded;

pub(super) fn set(bytes: usize) {
    BUDGET.fetch_min(bytes, Ordering::Relaxed);
}

fn reserve(bytes: usize) -> bool {
    if bytes == 0 {
        return true;
    }
    let mut live = LIVE.load(Ordering::Relaxed);
    loop {
        let Some(next) = live
            .checked_add(bytes)
            .filter(|next| *next <= BUDGET.load(Ordering::Relaxed))
        else {
            return false;
        };
        match LIVE.compare_exchange_weak(live, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(actual) => live = actual,
        }
    }
}

// SAFETY: pointers and layouts are passed unchanged to System, which owns all
// storage. Only successfully reserved bytes are counted, including realloc's
// growth; failed operations roll back without consuming or freeing the pointer.
unsafe impl GlobalAlloc for Bounded {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return null_mut();
        }
        // SAFETY: the caller supplies GlobalAlloc's valid nonzero layout.
        let pointer = unsafe { System.alloc(layout) };
        if pointer.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !reserve(layout.size()) {
            return null_mut();
        }
        // SAFETY: the caller supplies GlobalAlloc's valid nonzero layout.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if pointer.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: this allocator issued the pointer with the matching layout.
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let growth = new_size.saturating_sub(layout.size());
        if !reserve(growth) {
            return null_mut();
        }
        // SAFETY: the caller supplies this allocator's live pointer, original
        // layout and a nonzero size satisfying GlobalAlloc's size requirements.
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if replacement.is_null() {
            LIVE.fetch_sub(growth, Ordering::Relaxed);
        } else if new_size < layout.size() {
            LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
        }
        replacement
    }
}
