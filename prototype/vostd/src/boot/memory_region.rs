// SPDX-License-Identifier: MPL-2.0
//! Descriptions of sandbox memory; these records grant no access authority.
use crate::mm::{Paddr, PAGE_SIZE};
/// OSTD boot region classification.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MemoryRegionType {
    BadMemory = 0,
    Unknown = 1,
    NonVolatileSleep = 2,
    Reserved = 3,
    Kernel = 4,
    Module = 5,
    Framebuffer = 6,
    Reclaimable = 7,
    Usable = 8,
}
impl MemoryRegionType {
    pub fn is_physical(self) -> bool {
        !matches!(
            self,
            Self::BadMemory | Self::Unknown | Self::Reserved | Self::Framebuffer
        )
    }
}
/// A checked physical-memory description, never an arbitrary-address mapping.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MemoryRegion {
    base: Paddr,
    len: usize,
    typ: MemoryRegionType,
}
impl MemoryRegion {
    pub const fn new(base: Paddr, len: usize, typ: MemoryRegionType) -> Self {
        assert!(base.checked_add(len).is_some(), "boot region overflows");
        assert!(
            base % PAGE_SIZE == 0 && len % PAGE_SIZE == 0,
            "boot region is not page aligned"
        );
        Self { base, len, typ }
    }
    pub fn base(&self) -> Paddr {
        self.base
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn end(&self) -> Paddr {
        self.base + self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn typ(&self) -> MemoryRegionType {
        self.typ
    }
}
