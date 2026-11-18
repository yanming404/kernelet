// SPDX-License-Identifier: MPL-2.0
//! Production platform algorithms under explicit native CPU/scheduler primitives.
//! This fixture does not boot Asterinas or execute the privileged entry chain.
//!
//! CPU-local references must not escape the pinning guard:
//! ```compile_fail
//! use kernelet_platform_harness::{cpu, task};
//! let local = cpu::local::CpuLocal::new([0usize; 8]);
//! let escaped = {
//!     let guard = task::disable_preempt();
//!     local.get_with(&guard)
//! };
//! assert_eq!(*escaped, 0);
//! ```
#![feature(negative_impls)]
extern crate alloc;
extern crate self as ostd;
#[path = "../../../../../asterinas/kernel/core/src/process/posix_thread/cpu_sync.rs"]
pub(crate) mod cpu_sync;
#[path = "../../../vostd/src/error.rs"]
mod error;
pub use error::Error;
pub mod prelude {
    pub type Result<T, E = crate::Error> = core::result::Result<T, E>;
    pub(crate) use alloc::{boxed::Box, sync::Arc, vec::Vec};
}
pub mod cpu;
#[path = "../../../vostd/src/util/mod.rs"]
pub mod util;
pub mod task {
    use std::{cell::Cell, marker::PhantomData, rc::Rc};
    thread_local! {static DEPTH: Cell<usize> = const {Cell::new(0)};}
    pub mod atomic_mode {
        pub unsafe trait InAtomicMode: core::fmt::Debug {}
        pub trait AsAtomicModeGuard {
            fn as_atomic_mode_guard(&self) -> &dyn InAtomicMode;
        }
        impl<T: InAtomicMode> AsAtomicModeGuard for T {
            fn as_atomic_mode_guard(&self) -> &dyn InAtomicMode {
                self
            }
        }
    }
    #[derive(Debug)]
    pub struct DisabledPreemptGuard(PhantomData<Rc<()>>);
    unsafe impl atomic_mode::InAtomicMode for DisabledPreemptGuard {}
    impl crate::sync::guard::GuardTransfer for DisabledPreemptGuard {
        fn transfer_to(&mut self) -> Self {
            disable_preempt()
        }
    }
    pub fn disable_preempt() -> DisabledPreemptGuard {
        DEPTH.with(|d| d.set(d.get() + 1));
        DisabledPreemptGuard(PhantomData)
    }
    impl Drop for DisabledPreemptGuard {
        fn drop(&mut self) {
            DEPTH.with(|d| d.set(d.get() - 1));
        }
    }
    pub fn assert_quiescent() {
        DEPTH.with(|d| assert_eq!(d.get(), 0));
    }
    pub fn local_enabled() -> bool {
        DEPTH.with(|d| d.get() == 0)
    }
    pub mod preempt {
        pub mod cpu_local {
            pub fn get_guard_count() -> u32 {
                super::super::DEPTH.with(|d| d.get() as u32)
            }
        }
    }
}
pub mod panic {
    #[derive(Debug)]
    pub struct PanicGuard;
    impl PanicGuard {
        pub fn new() -> Self {
            Self
        }
        pub fn forget(self) {
            core::mem::forget(self)
        }
    }
    impl Drop for PanicGuard {
        fn drop(&mut self) {
            panic!("guarded destructor escaped")
        }
    }
}
pub mod mm;
pub mod sync;
#[cfg(test)]
mod tests;

/// Native IRQ guard fixture; no privileged IRQ instruction is executed.
pub mod irq {
    pub type DisabledLocalIrqGuard = crate::task::DisabledPreemptGuard;
    pub fn is_local_enabled() -> bool {
        crate::task::local_enabled()
    }
    pub fn disable_local() -> DisabledLocalIrqGuard {
        crate::task::disable_preempt()
    }
}
pub mod abi {
    use std::cell::Cell;
    thread_local! {static TLS: [Cell<u64>;2]=const{[Cell::new(0),Cell::new(0)]};}
    pub struct Services {
        pub user_tls: unsafe extern "C" fn(u64, u64, *mut u64) -> i64,
    }
    static SERVICES: Services = Services { user_tls: service };
    pub fn svc() -> &'static Services {
        &SERVICES
    }
    unsafe extern "C" fn service(which: u64, operation: u64, value: *mut u64) -> i64 {
        if which > 1 || operation > 1 {
            return -22;
        }
        TLS.with(|tls| {
            if operation == 0 {
                *value = tls[which as usize].get();
            } else {
                tls[which as usize].set(*value);
            }
        });
        0
    }
    pub fn simulate_userspace_fsgsbase(which: usize, value: u64) {
        TLS.with(|tls| tls[which].set(value));
    }
}
#[path = "../../../vostd/src/arch/cpu/context/user_tls.rs"]
pub mod user_tls;

/// CPU exception metadata fixture used by the production fault-dispatch policy.
/// No host trap entry or page-table hardware is simulated by these value types.
pub mod arch {
    pub mod cpu {
        pub mod context {
            pub use crate::user_tls::{FsBase, GsBase};
            /// Opaque native fixture solely for compiling CpuSync's FPU impl.
            /// These methods deliberately cannot simulate privileged FPU work.
            pub struct FpuContext;
            impl FpuContext {
                pub fn save(&mut self) {
                    panic!("FPU hardware is outside this fixture")
                }
                pub fn load(&self) {
                    panic!("FPU hardware is outside this fixture")
                }
            }
            #[derive(Clone, Copy, Debug)]
            pub struct PageFaultErrorCode(usize);
            impl PageFaultErrorCode {
                pub fn from_bits_truncate(v: usize) -> Self {
                    Self(v)
                }
                pub fn bits(&self) -> usize {
                    self.0
                }
            }
            #[derive(Clone, Copy, Debug)]
            pub struct RawPageFaultInfo {
                pub addr: usize,
                pub error_code: PageFaultErrorCode,
            }
            #[derive(Clone, Copy, Debug)]
            pub enum CpuException {
                PageFault(RawPageFaultInfo),
            }
        }
    }
}
