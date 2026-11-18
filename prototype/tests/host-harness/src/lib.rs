// SPDX-License-Identifier: MPL-2.0
//! Native linkage of the exact production queue metadata implementation.
#[path = "../../../linux/endovisor/typedio_queue.rs"]
mod queue_link;
pub use queue_link::{notification, queue, service};

#[path = "../../../linux/endovisor/typedio_core.rs"]
mod pipe_link;
pub use pipe_link::pipe;

extern crate self as typedio;
#[path = "../../../libs/typedio/src/api.rs"]
mod sdk;
pub use sdk::{
    flow, message, schema, vsock, BoundedVec, Message, Protocol, ProtocolId, SchemaDigest,
};
#[path = "../../../linux/endovisor/typedio_host_api.rs"]
mod host_api;
#[path = "../../../linux/endovisor/typedio_vsock.rs"]
mod vsock_backend;

extern crate alloc;
#[path = "../../../vostd/src/error.rs"]
mod error;
pub use error::Error;
pub mod prelude {
    pub type Result<T, E = crate::Error> = core::result::Result<T, E>;
}
pub mod irq {
    use std::sync::{Arc, Condvar, Mutex, OnceLock};
    pub struct IrqLine;
    type Callback = Arc<dyn Fn(&()) + Send + Sync>;
    static CALLBACKS: OnceLock<Mutex<Vec<Callback>>> = OnceLock::new();
    static PROMPT: OnceLock<Arc<(Mutex<bool>, Condvar)>> = OnceLock::new();
    impl IrqLine {
        pub fn alloc() -> Result<Self, crate::Error> {
            CALLBACKS.get_or_init(|| Mutex::new(Vec::new()));
            PROMPT.get_or_init(|| {
                let prompt = Arc::new((Mutex::new(false), Condvar::new()));
                let thread_prompt = prompt.clone();
                std::thread::spawn(move || loop {
                    let mut pending = thread_prompt.0.lock().unwrap();
                    while !*pending {
                        pending = thread_prompt.1.wait(pending).unwrap()
                    }
                    *pending = false;
                    drop(pending);
                    for slot in 0..crate::service::MAX_ATTACHMENTS {
                        loop {
                            let mut call = crate::service::Call {
                                operation: crate::service::IRQ_POLL,
                                handle: slot as u64,
                                ..crate::service::Call::default()
                            };
                            if unsafe { crate::guest::service(&mut call) } != 0 {
                                break;
                            }
                            if call.record.sequence == 7 && call.record.generation == 1 {
                                let callbacks = CALLBACKS.get().unwrap().lock().unwrap().clone();
                                for callback in callbacks {
                                    callback(&());
                                }
                            }
                        }
                    }
                });
                prompt
            });
            Ok(Self)
        }
        pub fn on_active(&mut self, callback: impl Fn(&()) + Send + Sync + 'static) {
            CALLBACKS
                .get()
                .unwrap()
                .lock()
                .unwrap()
                .push(Arc::new(callback));
        }
        pub fn num(&self) -> u32 {
            7
        }
        pub fn generation(&self) -> u64 {
            1
        }
        pub fn target_vcpu(&self) -> u32 {
            0
        }
    }
    #[no_mangle]
    pub extern "C" fn native_irq_prompt() {
        if let Some(prompt) = PROMPT.get() {
            *prompt.0.lock().unwrap() = true;
            prompt.1.notify_one();
        }
    }
}

pub mod abi {
    pub struct Services {
        pub typedio: unsafe extern "C" fn(*mut crate::service::Call) -> i64,
    }
    static SERVICES: Services = Services {
        typedio: crate::guest::service,
    };
    pub fn svc() -> &'static Services {
        &SERVICES
    }
}
pub mod mm {
    pub type Paddr = usize;
    pub type Vaddr = usize;
    pub const PAGE_SIZE: usize = 4096;
    pub const MAX_USERSPACE_VADDR: usize = 0x0000_8000_0000_0000 - 4096;
    pub const KERNEL_VADDR_RANGE: core::ops::Range<usize> = 0xffff_8000_0000_0000..usize::MAX;
    // This fixture tests transport/fault-progress, not a complete guest VMAR.
    pub mod vm_space {
        pub(crate) fn current_root() -> Option<usize> {
            None
        }
    }
    pub mod fault {
        pub(crate) fn resolve(_: usize, _: usize, _: bool, _: bool) -> bool {
            false
        }
    }

    pub fn paddr_to_vaddr(p: usize) -> usize {
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
        pub fn walk(_: usize, _: usize) -> Option<Leaf> {
            None
        }
    }
    pub mod rcu {
        pub fn read<T>(f: impl FnOnce() -> T) -> T {
            f()
        }
    }
    #[path = "../../../../vostd/src/mm/io.rs"]
    pub mod io;
    pub use io::*;
}
pub mod guest;

#[path = "../../../linux/endovisor/typedio_host_socket.rs"]
mod typedio_host_socket;
