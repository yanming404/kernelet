// SPDX-License-Identifier: MPL-2.0

//! The raw user context, which is also the shape the endovisor copies to and
//! from `task_pt_regs(current)`.

use super::cpu::context::GeneralRegs;

/// Ring-3 code/data selectors used and checked by the Linux x86-64 carrier.
pub const USER_CS_VALUE: usize = 0x33;
pub const USER_SS_VALUE: usize = 0x2b;

/// Userspace context.
///
/// On bare metal the trailing fields are filled in by the trap handler from the
/// hardware. Here the endovisor fills them in from Linux's `struct pt_regs` and
/// its page-fault information. `fault_addr` is an addition to real OSTD's
/// layout, which reads CR2 instead.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawUserContext {
    pub(crate) general: GeneralRegs,
    pub(crate) trap_num: usize,
    pub(crate) error_code: usize,
    pub(crate) fault_addr: usize,
}

impl RawUserContext {
    /// Enters user mode and returns the reason it came back: 0 a system call,
    /// 1 an exception, 2 an event.
    pub(crate) fn run(&mut self) -> u64 {
        // There is nothing to save around the call: what the carrier carries is
        // a virtual CPU, and the current OSTD task of that vCPU is OSTD's own
        // per-CPU state, which the carrier keeps across a trip to user mode.
        crate::sync::r2::assert_no_spin_lock("user_run");
        // SAFETY: the service is given a valid context to read and write.
        unsafe { (crate::abi::svc().user_run)(self) }
    }
}

/// The real register snapshot supplied to a virtual IRQ callback.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TrapFrame {
    pub rax: usize,
    pub rbx: usize,
    pub rcx: usize,
    pub rdx: usize,
    pub rsi: usize,
    pub rdi: usize,
    pub rbp: usize,
    pub r8: usize,
    pub r9: usize,
    pub r10: usize,
    pub r11: usize,
    pub r12: usize,
    pub r13: usize,
    pub r14: usize,
    pub r15: usize,
    pub trap_num: usize,
    pub error_code: usize,
    pub rip: usize,
    pub cs: usize,
    pub rflags: usize,
    pub rsp: usize,
    pub ss: usize,
}
impl TrapFrame {
    /// # Safety
    /// `saved` names the 16 words saved by the audited IRQ entry assembly.
    pub(crate) unsafe fn from_saved(saved: *const usize, rip: usize) -> Self {
        // SAFETY: assembly saved 15 GPRs and RFLAGS in this exact order.
        let r = unsafe { &*(saved as *const [usize; 16]) };
        let cs: u16;
        let ss: u16;
        // SAFETY: reading the current selectors does not modify CPU state.
        unsafe {
            core::arch::asm!("mov {0:x}, cs",out(reg)cs,options(nomem,nostack,preserves_flags));
            core::arch::asm!("mov {0:x}, ss",out(reg)ss,options(nomem,nostack,preserves_flags));
        }
        Self {
            rax: r[14],
            rcx: r[13],
            rdx: r[12],
            rbx: r[11],
            rbp: r[10],
            rsi: r[9],
            rdi: r[8],
            r8: r[7],
            r9: r[6],
            r10: r[5],
            r11: r[4],
            r12: r[3],
            r13: r[2],
            r14: r[1],
            r15: r[0],
            rflags: r[15],
            rsp: saved as usize + 17 * 8,
            rip,
            cs: cs as usize,
            ss: ss as usize,
            trap_num: 0,
            error_code: 0,
        }
    }
}
