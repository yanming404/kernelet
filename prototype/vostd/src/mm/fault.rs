// SPDX-License-Identifier: MPL-2.0
//! Recoverable faults in copies of this task's authorized user address space.
use super::{Paddr, Vaddr, MAX_USERSPACE_VADDR};
use crate::arch::cpu::context::{CpuException, PageFaultErrorCode, RawPageFaultInfo};
pub type UserPageFaultHandler = fn(&CpuException) -> Result<(), ()>;
static HANDLER: spin::Once<UserPageFaultHandler> = spin::Once::new();
pub fn inject_user_page_fault_handler(handler: UserPageFaultHandler) {
    HANDLER.call_once(|| handler);
}

// Called after the table-walk/copy RCU section is released, so the VMAR handler
// can allocate or sleep. Alien roots and atomic contexts cannot use this hook.
pub(crate) fn resolve(root: Paddr, address: Vaddr, write: bool, present: bool) -> bool {
    let Some(handler) = HANDLER.get() else {
        return false;
    };
    if address >= MAX_USERSPACE_VADDR
        || !crate::irq::is_local_enabled()
        || crate::task::preempt::cpu_local::get_guard_count() != 0
        || super::vm_space::current_root() != Some(root)
    {
        return false;
    }
    let flags = (if present { 1 } else { 0 }) | (if write { 2 } else { 0 });
    let exception = CpuException::PageFault(RawPageFaultInfo {
        addr: address,
        error_code: PageFaultErrorCode::from_bits_truncate(flags),
    });
    handler(&exception).is_ok()
}
