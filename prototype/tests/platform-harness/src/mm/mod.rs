// SPDX-License-Identifier: MPL-2.0
//! Native memory primitive fixture for production bounded byte-copy code.
pub type Paddr = usize;
pub type Vaddr = usize;
pub const PAGE_SIZE: usize = 4096;
pub fn paddr_to_vaddr(p: Paddr) -> Vaddr {
    p
}
#[derive(Clone, Copy)]
pub struct PageFlags;
impl PageFlags {
    pub const W: Self = Self;
    pub fn contains(&self, _: Self) -> bool {
        true
    }
}
pub mod page_table {
    pub struct Leaf {
        pub flags: super::PageFlags,
        pub paddr: usize,
    }
    std::thread_local! {static MAPPINGS:std::cell::RefCell<std::collections::BTreeMap<(usize,usize),usize>>=const{std::cell::RefCell::new(std::collections::BTreeMap::new())};}
    pub fn walk(root: usize, va: usize) -> Option<Leaf> {
        MAPPINGS.with(|m| {
            m.borrow().get(&(root, va)).copied().map(|paddr| Leaf {
                flags: super::PageFlags,
                paddr,
            })
        })
    }
    pub fn map(root: usize, va: usize, paddr: usize) {
        MAPPINGS.with(|m| {
            m.borrow_mut().insert((root, va), paddr);
        });
    }
    pub fn clear() {
        MAPPINGS.with(|m| m.borrow_mut().clear());
    }
}
pub mod rcu {
    pub fn read<T>(f: impl FnOnce() -> T) -> T {
        let _pin = crate::task::disable_preempt();
        f()
    }
}
#[path = "../../../../vostd/src/mm/io.rs"]
pub mod io;
#[path = "../../../../vostd/src/boot/memory_region.rs"]
pub mod memory_region;

pub use io::*;
pub trait HasPaddr {
    fn paddr(&self) -> Paddr;
}
pub trait HasSize {
    fn size(&self) -> usize;
}
#[path = "../../../../vostd/src/mm/frame.rs"]
pub mod frame;

pub use frame::{allocator::FrameAllocOptions, segment::USegment};
pub type Daddr = usize;
pub trait HasDaddr {
    fn daddr(&self) -> Daddr;
}
pub trait Split: Sized {
    fn split(self, offset: usize) -> (Self, Self);
}
#[path = "../../../../vostd/src/mm/dma.rs"]
pub mod dma;

pub const MAX_USERSPACE_VADDR: usize = 0x0000_8000_0000_0000 - 4096;
pub const KERNEL_VADDR_RANGE: core::ops::Range<usize> = 0xffff_8000_0000_0000..usize::MAX;
pub mod vm_space {
    std::thread_local! {static ROOT:std::cell::Cell<Option<usize>>=const{std::cell::Cell::new(None)};}
    pub(crate) fn current_root() -> Option<usize> {
        ROOT.with(std::cell::Cell::get)
    }
    pub fn activate_for_test(root: Option<usize>) {
        ROOT.with(|r| r.set(root));
    }
}
#[path = "../../../../vostd/src/mm/fault.rs"]
pub mod fault;
