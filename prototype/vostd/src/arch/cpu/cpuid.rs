// SPDX-License-Identifier: MPL-2.0
//! Bounded unprivileged CPUID queries, as visible to this carrier's user mode.
use core::arch::x86_64::{__cpuid_count, CpuidResult};
pub fn cpuid(leaf: u32, subleaf: u32) -> Option<CpuidResult> {
    let base = if leaf < 0x40000000 {
        0
    } else if leaf < 0x80000000 {
        0x40000000
    } else {
        0x80000000
    };
    if leaf > __cpuid_count(base, 0).eax {
        None
    } else {
        Some(__cpuid_count(leaf, subleaf))
    }
}
