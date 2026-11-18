// SPDX-License-Identifier: MPL-2.0

//! Synchronization primitives.
//!
//! The pieces the task layer and the injected scheduler need, ported from real
//! OSTD. Mutexes and read/write locks use its original algorithms with virtual
//! IRQ/preemption guards and the real vOSTD scheduler wait queues. RCU support
//! for the complete Asterinas dependency graph is handled separately.

mod guard;
mod spin;
pub(crate) use self::spin::r2;
mod wait;

pub use self::{
    guard::{GuardTransfer, LocalIrqDisabled, PreemptDisabled, SpinGuardian, WriteIrqDisabled},
    spin::{SpinLock, SpinLockGuard},
    wait::{WaitQueue, Waiter, Waker},
};

mod mutex;
mod rwarc;
mod rwlock;
mod rwmutex;
pub use self::{
    mutex::{Mutex, MutexGuard},
    rwarc::{RoArc, RwArc},
    rwlock::{RwLock, RwLockReadGuard, RwLockUpgradeableGuard, RwLockWriteGuard},
    rwmutex::{RwMutex, RwMutexReadGuard, RwMutexUpgradeableGuard, RwMutexWriteGuard},
};

mod rcu;
pub(crate) use self::rcu::finish_grace_period;
pub use self::rcu::{non_null, Rcu, RcuDrop, RcuOption, RcuOptionReadGuard, RcuReadGuard};
pub(crate) fn init() {
    rcu::init();
}
