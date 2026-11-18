// SPDX-License-Identifier: MPL-2.0
//! Host-heap metadata retained until its Grant frame's final handle disappears.
pub use super::AnyFrameMeta;
use crate::mm::{paddr_to_vaddr, Infallible, Paddr, VmReader, PAGE_SIZE};
use alloc::boxed::Box;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};
/// Same admission limits as the current Asterinas 64-byte metadata slot.
pub const FRAME_METADATA_MAX_SIZE: usize = 40;
pub const FRAME_METADATA_MAX_ALIGN: usize = 64;
#[repr(align(64))]
struct Slot {
    ready: AtomicBool,
    value: UnsafeCell<Option<Box<dyn AnyFrameMeta>>>,
}
// SAFETY: allocator bitmap exclusivity serializes initialization and final
// destruction. Published metadata is immutable; only its own synchronized fields
// may mutate. Every borrower must pin a counted frame reference.
unsafe impl Sync for Slot {}
static SLOTS: [Slot; 4096] = [const {
    Slot {
        ready: AtomicBool::new(false),
        value: UnsafeCell::new(None),
    }
}; 4096];
pub(super) fn identity(paddr: Paddr) -> core::ptr::NonNull<u8> {
    let index = super::allocator::index(paddr).expect("frame identity outside Grant");
    core::ptr::NonNull::from(&SLOTS[index]).cast()
}
pub(super) fn from_identity(pointer: core::ptr::NonNull<u8>) -> Paddr {
    let offset = (pointer.as_ptr() as usize)
        .checked_sub(SLOTS.as_ptr() as usize)
        .expect("frame identity outside metadata table");
    assert_eq!(
        offset % core::mem::size_of::<Slot>(),
        0,
        "unaligned metadata identity"
    );
    let index = offset / core::mem::size_of::<Slot>();
    let (base, count) = super::allocator::bounds();
    assert!(index < count, "frame identity outside active Grant");
    base + index * PAGE_SIZE
}
pub(super) fn publish<M: AnyFrameMeta>(paddr: Paddr, value: M) {
    const {
        assert!(core::mem::size_of::<M>() <= FRAME_METADATA_MAX_SIZE);
    }
    const {
        assert!(core::mem::align_of::<M>() <= FRAME_METADATA_MAX_ALIGN);
    }
    let slot = &SLOTS[super::allocator::index(paddr).expect("metadata outside Grant")];
    assert!(!slot.ready.load(Ordering::Acquire));
    // Allocate before borrowing the slot, so an allocation failure cannot leave
    // a partially published object. The owning Frame/Segment handles roll back.
    let value: Box<dyn AnyFrameMeta> = Box::new(value);
    // SAFETY: only the newly allocated frame's exclusive initializer reaches here.
    unsafe {
        let target = &mut *slot.value.get();
        assert!(target.is_none());
        *target = Some(value);
    }
    slot.ready.store(true, Ordering::Release);
}
/// # Safety
/// Caller must keep a counted frame reference for the returned borrow's lifetime.
pub(super) unsafe fn borrow<'a>(paddr: Paddr) -> &'a dyn AnyFrameMeta {
    let slot = &SLOTS[super::allocator::index(paddr).expect("metadata outside Grant")];
    assert!(slot.ready.load(Ordering::Acquire), "metadata not published");
    // SAFETY: caller pins the frame, so the immutable Box cannot be removed.
    unsafe {
        (&*slot.value.get())
            .as_deref()
            .expect("published metadata missing")
    }
}
pub(super) fn release(paddr: Paddr) {
    let slot = &SLOTS[super::allocator::index(paddr).expect("metadata outside Grant")];
    slot.ready.store(false, Ordering::Release);
    // SAFETY: the final counted reference has been released; the bitmap still
    // reserves this frame. There can be no metadata borrowers or new initializer.
    let value = unsafe { (&mut *slot.value.get()).take() };
    if let Some(mut value) = value {
        // SAFETY: physical backing is retained until this hook and Drop finish.
        // AnyFrameMeta's unsafe contract governs whether its hook may read bytes.
        let mut reader: VmReader<Infallible> =
            unsafe { VmReader::from_kernel_space(paddr_to_vaddr(paddr) as *const u8, PAGE_SIZE) };
        value.on_drop(&mut reader);
        drop(value);
    }
}
#[macro_export]
macro_rules! check_frame_meta_layout {
    ($t:ty) => {
        const _: () =
            assert!(core::mem::size_of::<$t>() <= $crate::mm::frame::meta::FRAME_METADATA_MAX_SIZE);
        const _: () = assert!(
            core::mem::align_of::<$t>() <= $crate::mm::frame::meta::FRAME_METADATA_MAX_ALIGN
        );
    };
}
#[macro_export]
macro_rules! impl_frame_meta_for {
    ($t:ty) => {
        unsafe impl $crate::mm::frame::meta::AnyFrameMeta for $t {}
        $crate::check_frame_meta_layout!($t);
    };
}
#[macro_export]
macro_rules! impl_untyped_frame_meta_for {
    ($t:ty) => {
        unsafe impl $crate::mm::frame::meta::AnyFrameMeta for $t {
            fn is_untyped(&self) -> bool {
                true
            }
        }
        impl $crate::mm::frame::untyped::AnyUFrameMeta for $t {}
        $crate::check_frame_meta_layout!($t);
    };
    ($t:ty,$body:expr) => {
        unsafe impl $crate::mm::frame::meta::AnyFrameMeta for $t {
            fn is_untyped(&self) -> bool {
                true
            }
            fn on_drop(&mut self, reader: &mut $crate::mm::VmReader<$crate::mm::Infallible>) {
                $body(reader)
            }
        }
        impl $crate::mm::frame::untyped::AnyUFrameMeta for $t {}
        $crate::check_frame_meta_layout!($t);
    };
}
