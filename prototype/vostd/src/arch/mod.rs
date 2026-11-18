// SPDX-License-Identifier: MPL-2.0

//! Architecture-specific code. Only x86-64 exists in this prototype.

pub mod cpu;
pub(crate) mod irq;
pub mod mm;
pub mod task;
pub mod trap;

pub use crate::time::read_tsc;

/// Counter frequency advertised by the authenticated host boot table.
pub fn tsc_freq() -> u64 {
    crate::time::tsc_khz()
        .checked_mul(1000)
        .expect("TSC frequency overflow")
}
/// Unprivileged architectural randomness, with CPUID admission and carry check.
pub fn read_random() -> Option<u64> {
    if core::arch::x86_64::__cpuid(1).ecx & (1 << 30) == 0 {
        return None;
    }
    let mut value = 0;
    if unsafe { core::arch::x86_64::_rdrand64_step(&mut value) } == 1 {
        Some(value)
    } else {
        None
    }
}
pub mod power {
    pub fn try_poweroff(code: crate::power::ExitCode) {
        crate::power::stop_instance(code)
    }
    pub fn try_restart(code: crate::power::ExitCode) {
        crate::power::stop_instance(code)
    }
}
/// A Kernelet is not a hardware TDX guest; only the ordinary branch is compiled.
#[macro_export]
macro_rules! if_tdx_enabled {
    ($yes:block else $no:block) => {
        $no
    };
    ($yes:block) => {};
}
pub use crate::if_tdx_enabled;
