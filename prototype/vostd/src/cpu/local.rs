// SPDX-License-Identifier: MPL-2.0

//! CPU-local storage, and sets of virtual CPUs.
//!
//! Real OSTD gives each processor a private copy of a CPU-local variable through
//! the `gs` segment, so that access is one instruction. Here `gs` belongs to the
//! host's own per-processor area, and a vCPU is a host task rather than a
//! processor, so vOSTD keeps one slot per virtual CPU in an ordinary array and
//! indexes it by [`CpuId::current_racy`]. The macro surface is real OSTD's.
//!
//! The one CPU-local word that *must* be updated with a single instruction —
//! OSTD's preemption word, which a virtual interrupt may interrupt and which the
//! endovisor reads — does not live here. It lives in the vCPU record and
//! `task::preempt::cpu_local` updates it the way real OSTD does, with one
//! read-modify-write instruction.

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU32, Ordering},
};

use super::{num_cpus, CpuId, MAX_CPUS};

/// One copy of `T` per virtual CPU, accessed through a guard that pins the
/// current one.
pub struct CpuLocal<T> {
    slots: UnsafeCell<[T; MAX_CPUS]>,
}

// SAFETY: each virtual CPU only ever touches its own slot, and a caller must
// hold a guard that pins it to that virtual CPU.
unsafe impl<T: Send> Sync for CpuLocal<T> {}

impl<T> CpuLocal<T> {
    /// Creates the variable with one copy of `init` per virtual CPU.
    pub const fn new(slots: [T; MAX_CPUS]) -> Self {
        Self {
            slots: UnsafeCell::new(slots),
        }
    }

    /// Returns this virtual CPU's copy.
    pub fn get_with<'a, G: super::PinCurrentCpu>(&'a self, guard: &'a G) -> &'a T {
        let index = guard.current_cpu().as_usize();
        // SAFETY: the guard pins the current task to this virtual CPU, so
        // nothing else touches this slot.
        unsafe { &(*self.slots.get())[index] }
    }
}

/// One `Copy` word per virtual CPU.
pub struct CpuLocalCell<T: Copy> {
    slots: UnsafeCell<[T; MAX_CPUS]>,
}

// SAFETY: as for `CpuLocal`.
unsafe impl<T: Copy> Sync for CpuLocalCell<T> {}

impl<T: Copy> CpuLocalCell<T> {
    /// Creates the variable with `init` on every virtual CPU.
    /// # Safety
    /// Used only for static per-vCPU initializers whose copied initial values
    /// do not introduce cross-vCPU aliases to unsynchronized objects.
    #[doc(hidden)]
    pub const unsafe fn __new(init: T) -> Self {
        Self {
            slots: UnsafeCell::new([init; MAX_CPUS]),
        }
    }

    #[inline(always)]
    fn slot(&self) -> *mut T {
        let index = CpuId::current_racy().as_usize();
        // SAFETY: `index` is below `MAX_CPUS`.
        unsafe { (self.slots.get() as *mut T).add(index) }
    }

    /// Reads this virtual CPU's copy.
    #[inline(always)]
    pub fn load(&self) -> T {
        // SAFETY: only this virtual CPU writes this slot.
        unsafe { core::ptr::read_volatile(self.slot()) }
    }

    /// Writes this virtual CPU's copy.
    #[inline(always)]
    pub fn store(&self, value: T) {
        // SAFETY: as above.
        unsafe { core::ptr::write_volatile(self.slot(), value) }
    }
}

/// Declares CPU-local variables reached through a pinning guard.
///
/// The initialiser is written out [`MAX_CPUS`] times because the value need not
/// be `Copy` (real OSTD's timer callback list is a `RefCell<Vec<_>>`), so the
/// array-repeat syntax cannot be used. The assertion below keeps the count and
/// `MAX_CPUS` in step.
#[macro_export]
macro_rules! cpu_local {
    ($( $(#[$attr:meta])* $vis:vis static $name:ident : $t:ty = $init:expr; )*) => {
        $(
            $(#[$attr])*
            $vis static $name: $crate::cpu::local::CpuLocal<$t> =
                $crate::cpu::local::CpuLocal::new(
                    [$init, $init, $init, $init, $init, $init, $init, $init]
                );
        )*
    };
}

const _: () = assert!(
    MAX_CPUS == 8,
    "the cpu_local! macro writes out 8 initialisers"
);

/// Declares CPU-local words.
#[macro_export]
macro_rules! cpu_local_cell {
    ($( $(#[$attr:meta])* $vis:vis static $name:ident : $t:ty = $init:expr; )*) => {
        $(
            $(#[$attr])*
            $vis static $name: $crate::cpu::local::CpuLocalCell<$t> =
                // SAFETY: static initializer, with one independent word per vCPU.
                unsafe { $crate::cpu::local::CpuLocalCell::__new($init) };
        )*
    };
}

/// A set of virtual CPU IDs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuSet(u32);

impl CpuSet {
    /// The empty set.
    pub const fn new_empty() -> Self {
        Self(0)
    }

    /// Every virtual CPU of the sandbox.
    pub fn new_full() -> Self {
        Self((1u32 << num_cpus()) - 1)
    }

    /// Adds a virtual CPU.
    pub fn add(&mut self, cpu: CpuId) {
        self.0 |= 1 << cpu.as_usize();
    }

    /// Removes a virtual CPU.
    pub fn remove(&mut self, cpu: CpuId) {
        self.0 &= !(1 << cpu.as_usize());
    }

    /// Whether the set holds `cpu`.
    pub fn contains(&self, cpu: CpuId) -> bool {
        self.0 & (1 << cpu.as_usize()) != 0
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Iterates over the set.
    pub fn iter(&self) -> impl Iterator<Item = CpuId> + '_ {
        let bits = self.0;
        super::all_cpus().filter(move |cpu| bits & (1 << cpu.as_usize()) != 0)
    }
}

impl From<CpuId> for CpuSet {
    fn from(cpu: CpuId) -> Self {
        Self(1 << cpu.as_usize())
    }
}

/// A [`CpuSet`] with concurrent access.
#[derive(Debug)]
pub struct AtomicCpuSet(AtomicU32);

impl AtomicCpuSet {
    /// Creates a new instance.
    pub const fn new(set: CpuSet) -> Self {
        Self(AtomicU32::new(set.0))
    }

    /// Adds a virtual CPU.
    pub fn add(&self, cpu: CpuId, order: Ordering) {
        self.0.fetch_or(1 << cpu.as_usize(), order);
    }

    /// Whether the set holds `cpu`.
    pub fn contains(&self, cpu: CpuId, order: Ordering) -> bool {
        self.0.load(order) & (1 << cpu.as_usize()) != 0
    }
}

/// Static CPU-local storage uses per-vCPU instances rather than host GS offsets.
pub type StaticCpuLocal<T> = CpuLocal<T>;
impl<T: Sync> CpuLocal<T> {
    /// Remote read is safe only for values that admit concurrent shared access.
    pub fn get_on_cpu(&self, cpu: CpuId) -> &T {
        // SAFETY: CpuId is checked; T: Sync permits shared cross-vCPU access.
        unsafe { &(*self.slots.get())[cpu.as_usize()] }
    }
}
impl CpuSet {
    pub fn is_full(&self) -> bool {
        self.0 == Self::new_full().0
    }
    pub fn iter_in<'a, R: core::ops::RangeBounds<CpuId> + 'a>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = CpuId> + 'a {
        self.iter().filter(move |cpu| range.contains(cpu))
    }
}
impl AtomicCpuSet {
    pub fn load(&self, order: Ordering) -> CpuSet {
        CpuSet(self.0.load(order))
    }
    pub fn store(&self, set: &CpuSet, order: Ordering) {
        self.0.store(set.0, order);
    }
}

#[path = "local/dynamic.rs"]
mod dynamic;
pub use dynamic::{DynCpuLocalChunk, DynamicCpuLocal};

// Each operation is one x86 memory RMW. No load/modify/store sequence can lose
// an update when an IRQ touches the same CPU-local word between instructions.
macro_rules! cell_rmw {
    ($t:ty,$width:literal,$modifier:literal)=>{
        impl CpuLocalCell<$t>{
            pub fn add_assign(&self,value:$t){unsafe{core::arch::asm!(concat!("add ",$width," ptr [{p}],{v",$modifier,"}"),p=in(reg)self.slot(),v=in(reg)value,options(nostack));}}
            pub fn sub_assign(&self,value:$t){unsafe{core::arch::asm!(concat!("sub ",$width," ptr [{p}],{v",$modifier,"}"),p=in(reg)self.slot(),v=in(reg)value,options(nostack));}}
            pub fn bitor_assign(&self,value:$t){unsafe{core::arch::asm!(concat!("or ",$width," ptr [{p}],{v",$modifier,"}"),p=in(reg)self.slot(),v=in(reg)value,options(nostack));}}
            pub fn bitand_assign(&self,value:$t){unsafe{core::arch::asm!(concat!("and ",$width," ptr [{p}],{v",$modifier,"}"),p=in(reg)self.slot(),v=in(reg)value,options(nostack));}}
        }
    };
}
cell_rmw!(u16, "word", ":x");
cell_rmw!(u32, "dword", ":e");
cell_rmw!(u64, "qword", "");
cell_rmw!(usize, "qword", "");
impl CpuLocalCell<u8> {
    pub fn add_assign(&self, value: u8) {
        unsafe {
            core::arch::asm!("add byte ptr [{p}],{v}",p=in(reg)self.slot(),v=in(reg_byte)value,options(nostack));
        }
    }
    pub fn sub_assign(&self, value: u8) {
        unsafe {
            core::arch::asm!("sub byte ptr [{p}],{v}",p=in(reg)self.slot(),v=in(reg_byte)value,options(nostack));
        }
    }
    pub fn bitor_assign(&self, value: u8) {
        unsafe {
            core::arch::asm!("or byte ptr [{p}],{v}",p=in(reg)self.slot(),v=in(reg_byte)value,options(nostack));
        }
    }
    pub fn bitand_assign(&self, value: u8) {
        unsafe {
            core::arch::asm!("and byte ptr [{p}],{v}",p=in(reg)self.slot(),v=in(reg_byte)value,options(nostack));
        }
    }
}
