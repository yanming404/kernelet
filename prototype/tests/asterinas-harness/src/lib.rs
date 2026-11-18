// SPDX-License-Identifier: MPL-2.0
//! Actual Asterinas transport source, production vOSTD Queue/Pipe/VmIO and
//! production Host Framework/Vsock/File sources. OS scheduling/locks are mocked.
extern crate self as aster_softirq;
extern crate self as ostd;
pub use typedio_host_harness::Error;
pub mod irq {
    pub use typedio_host_harness::irq::*;
}
pub mod mm {
    pub use typedio_host_harness::mm::*;
}
pub mod typedio {
    pub use typedio_host_harness::guest::frontend::*;
}
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex as StdMutex};
pub struct BottomHalfDisabled;
pub struct SpinLock<T, C = BottomHalfDisabled> {
    inner: StdMutex<T>,
    _kind: std::marker::PhantomData<fn() -> C>,
}
impl<T, C> SpinLock<T, C> {
    pub fn new(value: T) -> Self {
        Self {
            inner: StdMutex::new(value),
            _kind: std::marker::PhantomData,
        }
    }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
        self.inner.lock().unwrap()
    }
}
pub struct Taskless {
    pending: AtomicBool,
    wake: Condvar,
    lock: StdMutex<()>,
}
impl Taskless {
    pub fn new(run: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        let task = Arc::new(Self {
            pending: AtomicBool::new(false),
            wake: Condvar::new(),
            lock: StdMutex::new(()),
        });
        let worker = task.clone();
        std::thread::spawn(move || loop {
            let mut guard = worker.lock.lock().unwrap();
            while !worker.pending.swap(false, Ordering::AcqRel) {
                guard = worker.wake.wait(guard).unwrap()
            }
            drop(guard);
            run();
        });
        task
    }
    pub fn schedule(&self) {
        let _guard = self.lock.lock().unwrap();
        self.pending.store(true, Ordering::Release);
        self.wake.notify_one();
    }
}
pub mod events {
    bitflags::bitflags! {#[derive(Clone,Copy,Debug,PartialEq,Eq)]pub struct IoEvents:u32{const IN=1;const OUT=4;const ERR=8;const HUP=16;const RDHUP=8192;}}
}
pub mod process {
    pub mod signal {
        #[derive(Clone)]
        pub struct Pollee;
        impl Pollee {
            pub fn new() -> Self {
                Self
            }
            pub fn invalidate(&self) {}
            pub fn notify(&self, _: crate::events::IoEvents) {}
        }
    }
}
pub mod prelude {
    pub use crate::{return_errno_with_message, SpinLock};
    pub use core::fmt::Debug;
    pub use std::{
        boxed::Box,
        collections::{BTreeMap, VecDeque},
        sync::{Arc, Weak},
        vec::Vec,
    };
    pub type Result<T, E = KernelError> = core::result::Result<T, E>;
    #[derive(Clone, Debug)]
    pub struct KernelError {
        pub errno: Errno,
        pub message: &'static str,
    }
    pub use KernelError as Error;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Errno {
        EADDRNOTAVAIL,
        EOVERFLOW,
        EIO,
        EAGAIN,
        EPIPE,
        ENOBUFS,
        EADDRINUSE,
        EACCES,
        ENETUNREACH,
        ECONNRESET,
        ETIMEDOUT,
        ENODEV,
    }
    impl KernelError {
        pub fn with_message(errno: Errno, message: &'static str) -> Self {
            Self { errno, message }
        }
    }
    impl From<crate::Error> for KernelError {
        fn from(_: crate::Error) -> Self {
            Self::with_message(Errno::EIO, "memory fault")
        }
    }
}
#[macro_export]
macro_rules! return_errno_with_message {
    ($errno:expr,$message:expr) => {
        return Err(crate::prelude::Error::with_message($errno, $message))
    };
}
pub mod util {
    pub trait MultiRead {
        fn sum_lens(&self) -> usize;
        fn read(
            &mut self,
            dst: &mut crate::mm::VmWriter<'_, crate::mm::Infallible>,
        ) -> Result<usize, (crate::Error, usize)>;
    }
    pub trait MultiWrite {
        fn sum_lens(&self) -> usize;
        fn write(
            &mut self,
            src: &mut crate::mm::VmReader<'_, crate::mm::Infallible>,
        ) -> Result<usize, (crate::Error, usize)>;
    }
}
pub mod time {
    use super::*;
    use std::time::{Duration, Instant};
    pub struct Timer {
        callback: Box<dyn for<'a> Fn(timer::TimerGuard<'a>) + Send + Sync>,
        deadline: StdMutex<Option<Instant>>,
    }
    impl Timer {
        pub fn lock(&self) -> timer::TimerGuard<'_> {
            timer::TimerGuard { timer: self }
        }
    }
    pub mod timer {
        use super::*;
        pub enum Timeout {
            After(Duration),
        }
        pub struct TimerGuard<'a> {
            pub(super) timer: &'a Timer,
        }
        impl TimerGuard<'_> {
            pub fn set_timeout(&mut self, timeout: Timeout) {
                let Timeout::After(d) = timeout;
                *self.timer.deadline.lock().unwrap() = Some(Instant::now() + d)
            }
        }
    }
    pub struct Manager;
    impl Manager {
        pub fn create_timer(
            &self,
            callback: impl for<'a> Fn(timer::TimerGuard<'a>) + Send + Sync + 'static,
        ) -> Arc<Timer> {
            let t = Arc::new(Timer {
                callback: Box::new(callback),
                deadline: StdMutex::new(None),
            });
            let weak = Arc::downgrade(&t);
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(1));
                let Some(t) = weak.upgrade() else { break };
                let fire = {
                    let mut d = t.deadline.lock().unwrap();
                    if d.is_some_and(|d| d <= Instant::now()) {
                        *d = None;
                        true
                    } else {
                        false
                    }
                };
                if fire {
                    (t.callback)(timer::TimerGuard { timer: &t });
                }
            });
            t
        }
    }
    pub mod clocks {
        pub static JIFFIES_TIMER_MANAGER: spin::Once<super::Manager> = spin::Once::new();
    }
}
pub mod net {
    pub mod socket {
        pub mod util {
            #[derive(Clone, Copy)]
            pub struct SendFlags;
            #[derive(Clone, Copy)]
            pub struct RecvFlags(pub bool);
            impl RecvFlags {
                pub fn receive_behavior(self) -> ReceiveBehavior {
                    ReceiveBehavior(self.0)
                }
            }
            pub struct ReceiveBehavior(bool);
            impl ReceiveBehavior {
                pub fn will_consume_data(&self) -> bool {
                    !self.0
                }
            }
            #[derive(Clone, Copy)]
            pub struct SockShutdownCmd(pub u8);
            impl SockShutdownCmd {
                pub const SHUT_WR: Self = Self(2);
                pub fn shut_read(&self) -> bool {
                    self.0 & 1 != 0
                }
                pub fn shut_write(&self) -> bool {
                    self.0 & 2 != 0
                }
            }
        }
        pub mod vsock {
            pub mod addr {
                #[derive(Clone, Copy, Debug, PartialEq, Eq)]
                pub struct VsockSocketAddr {
                    pub cid: u32,
                    pub port: u32,
                }
                pub const VMADDR_CID_ANY: u32 = u32::MAX;
                pub const VMADDR_PORT_ANY: u32 = u32::MAX;
            }
            pub mod socket_transport {
                include!(concat!(env!("OUT_DIR"), "/adapter.rs"));
                mod checks;
                pub fn run(fd: i32) {
                    checks::run(fd)
                }
            }
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn native_asterinas_roundtrip(fd: i32) {
    time::clocks::JIFFIES_TIMER_MANAGER.call_once(|| time::Manager);
    net::socket::vsock::socket_transport::run(fd);
}
