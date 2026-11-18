// SPDX-License-Identifier: MPL-2.0
//! Scheduler/atomic-mode primitive fixtures; lock/RCU algorithms are production.
use std::sync::{Condvar, Mutex as NativeMutex};
pub struct WaitQueue(NativeMutex<()>, Condvar);
impl WaitQueue {
    pub const fn new() -> Self {
        Self(NativeMutex::new(()), Condvar::new())
    }
    pub fn wait_until<R>(&self, mut f: impl FnMut() -> Option<R>) -> R {
        let mut state = self.0.lock().unwrap();
        loop {
            if let Some(r) = f() {
                return r;
            }
            state = self.1.wait(state).unwrap();
        }
    }
    pub fn wake_one(&self) -> bool {
        let _state = self.0.lock().unwrap();
        self.1.notify_one();
        true
    }
    pub fn wake_all(&self) -> usize {
        let _state = self.0.lock().unwrap();
        self.1.notify_all();
        1
    }
}
pub mod guard {
    pub trait GuardTransfer {
        fn transfer_to(&mut self) -> Self;
    }
    pub trait SpinGuardian {
        type Guard: crate::task::atomic_mode::AsAtomicModeGuard + GuardTransfer;
        type ReadGuard: crate::task::atomic_mode::AsAtomicModeGuard + GuardTransfer;
        fn guard() -> Self::Guard;
        fn read_guard() -> Self::ReadGuard;
    }
}
pub enum PreemptDisabled {}
impl guard::SpinGuardian for PreemptDisabled {
    type Guard = crate::task::DisabledPreemptGuard;
    type ReadGuard = crate::task::DisabledPreemptGuard;
    fn guard() -> Self::Guard {
        crate::task::disable_preempt()
    }
    fn read_guard() -> Self::ReadGuard {
        crate::task::disable_preempt()
    }
}
pub struct SpinLock<T>(NativeMutex<T>);
impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self(NativeMutex::new(value))
    }
    pub fn disable_irq(&self) -> &Self {
        self
    }
    pub fn lock(&self) -> SpinGuard<'_, T> {
        SpinGuard {
            value: self.0.lock().unwrap(),
            _pin: crate::task::disable_preempt(),
        }
    }
}
pub struct SpinGuard<'a, T> {
    value: std::sync::MutexGuard<'a, T>,
    _pin: crate::task::DisabledPreemptGuard,
}
impl<T> core::ops::Deref for SpinGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T> core::ops::DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}
impl<T> crate::task::atomic_mode::AsAtomicModeGuard for SpinGuard<'_, T> {
    fn as_atomic_mode_guard(&self) -> &dyn crate::task::atomic_mode::InAtomicMode {
        &self._pin
    }
}
#[path = "../../../../vostd/src/sync/mutex.rs"]
mod mutex;
#[path = "../../../../vostd/src/sync/rcu/mod.rs"]
mod rcu;
#[path = "../../../../vostd/src/sync/rwarc.rs"]
mod rwarc;
#[path = "../../../../vostd/src/sync/rwlock.rs"]
mod rwlock;
#[path = "../../../../vostd/src/sync/rwmutex.rs"]
mod rwmutex;
pub use mutex::Mutex;
pub use rcu::{non_null, RcuOption};
pub use rwarc::RwArc;
pub use rwlock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
pub use rwmutex::RwMutex;
pub fn init() {
    rcu::init();
}
pub fn quiescent() {
    crate::task::assert_quiescent();
    unsafe { rcu::finish_grace_period() }
}
