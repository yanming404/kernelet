// SPDX-License-Identifier: MPL-2.0
//! Native CPU identity fixture; algorithms use the exact production local.rs.
use core::sync::atomic::{AtomicUsize, Ordering};
use std::cell::Cell;
pub const MAX_CPUS: usize = 8;
static NR_CPUS: AtomicUsize = AtomicUsize::new(1);
thread_local! {static ID:Cell<u32>=const{Cell::new(0)};}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CpuId(u32);
impl CpuId {
    pub fn current_racy() -> Self {
        Self(ID.with(Cell::get))
    }
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}
impl From<CpuId> for u32 {
    fn from(id: CpuId) -> Self {
        id.0
    }
}
pub fn num_cpus() -> usize {
    NR_CPUS.load(Ordering::Relaxed)
}
pub fn all_cpus() -> impl Iterator<Item = CpuId> {
    (0..num_cpus() as u32).map(CpuId)
}
pub unsafe trait PinCurrentCpu {
    fn current_cpu(&self) -> CpuId {
        CpuId::current_racy()
    }
}
unsafe impl<T: crate::task::atomic_mode::InAtomicMode + ?Sized> PinCurrentCpu for T {}
pub fn select(id: u32) {
    crate::task::assert_quiescent();
    ID.with(|v| v.set(id));
}
pub fn set_count(n: usize) {
    assert!(n <= MAX_CPUS);
    NR_CPUS.store(n, Ordering::Relaxed);
}
#[path = "../../../../vostd/src/cpu/local.rs"]
pub mod local;
pub use local::{AtomicCpuSet, CpuSet};
