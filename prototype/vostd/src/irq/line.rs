// SPDX-License-Identifier: MPL-2.0
//! Local IRQ callback ownership; the host receives only line identity/generation.
use crate::{arch::trap::TrapFrame, cpu::CpuId, sync::SpinLock, Error};
use ::typedio::service::{self, Call};
use alloc::{
    boxed::Box,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::{
    array,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

const LINES: usize = 64;
const CALLBACKS: usize = 8;
struct Callback {
    function: Box<dyn Fn(&TrapFrame) + Send + Sync>,
    running: AtomicUsize,
    removed: AtomicBool,
}
struct State {
    number: u8,
    generation: u64,
    target: u32,
    callbacks: SpinLock<[Option<Arc<Callback>>; CALLBACKS]>,
}
struct Slot {
    generation: u64,
    state: Weak<State>,
}
static REGISTRY: SpinLock<Option<[Slot; LINES]>> = SpinLock::new(None);
static ACTIVE: [AtomicUsize; crate::cpu::MAX_CPUS] =
    [const { AtomicUsize::new(0) }; crate::cpu::MAX_CPUS];

/// An IRQ identity whose original handle owns its callback registrations.
pub struct IrqLine {
    state: Arc<State>,
    callbacks: Vec<Arc<Callback>>,
}
impl IrqLine {
    pub fn alloc() -> Result<Self, Error> {
        Self::allocate(None)
    }
    pub fn alloc_specific(number: u8) -> Result<Self, Error> {
        Self::allocate(Some(number))
    }
    fn allocate(number: Option<u8>) -> Result<Self, Error> {
        let mut state = Arc::new(State {
            number: 0,
            generation: 0,
            target: CpuId::current_racy().into(),
            callbacks: SpinLock::new(array::from_fn(|_| None)),
        });
        let callbacks = Vec::with_capacity(CALLBACKS);
        let mut registry = REGISTRY.lock();
        let slots = registry.get_or_insert_with(|| {
            array::from_fn(|_| Slot {
                generation: 0,
                state: Weak::new(),
            })
        });
        let index = match number {
            Some(n) if (n as usize) < LINES => n as usize,
            Some(_) => return Err(Error::InvalidArgs),
            None => slots
                .iter()
                .position(|s| s.state.strong_count() == 0 && s.generation != u64::MAX)
                .ok_or(Error::NotEnoughResources)?,
        };
        if slots[index].state.strong_count() != 0 || slots[index].generation == u64::MAX {
            return Err(Error::NotEnoughResources);
        }
        slots[index].generation += 1;
        let unique = Arc::get_mut(&mut state).expect("new IRQ state is uniquely owned");
        unique.number = index as u8;
        unique.generation = slots[index].generation;
        slots[index].state = Arc::downgrade(&state);
        Ok(Self { state, callbacks })
    }
    pub fn num(&self) -> u8 {
        self.state.number
    }
    pub(crate) fn generation(&self) -> u64 {
        self.state.generation
    }
    pub(crate) fn target_vcpu(&self) -> u32 {
        self.state.target
    }
    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }
    pub fn on_active<F>(&mut self, function: F)
    where
        F: Fn(&TrapFrame) + Send + Sync + 'static,
    {
        let callback = Arc::new(Callback {
            function: Box::new(function),
            running: AtomicUsize::new(0),
            removed: AtomicBool::new(false),
        });
        let mut entries = self.state.callbacks.lock();
        let slot = entries
            .iter_mut()
            .find(|e| e.is_none())
            .expect("IRQ callback quota exhausted");
        *slot = Some(callback.clone());
        drop(entries);
        self.callbacks.push(callback);
    }
}
impl Clone for IrqLine {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            callbacks: Vec::new(),
        }
    }
}
impl Drop for IrqLine {
    fn drop(&mut self) {
        for callback in self.callbacks.drain(..) {
            let pointer = Arc::as_ptr(&callback) as usize;
            assert_ne!(
                ACTIVE[CpuId::current_racy().as_usize()].load(Ordering::Acquire),
                pointer,
                "IRQ callback cannot unregister itself"
            );
            {
                let mut entries = self.state.callbacks.lock();
                callback.removed.store(true, Ordering::Release);
                for entry in entries.iter_mut() {
                    if entry.as_ref().is_some_and(|e| Arc::ptr_eq(e, &callback)) {
                        *entry = None;
                    }
                }
            }
            while callback.running.load(Ordering::Acquire) != 0 {
                // SAFETY: no source lock is held; this bounded service helps
                // a descheduled sibling finish its in-flight callback.
                unsafe { (crate::abi::svc().vcpu_on_spin)() };
            }
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        for slot in 0..service::MAX_ATTACHMENTS {
            let mut call = Call {
                operation: service::UNBIND_LINE,
                handle: slot as u64,
                ..Call::default()
            };
            call.record.sequence = self.number as u64;
            call.record.generation = self.generation;
            crate::typedio::invoke(&mut call);
        }
    }
}
pub(crate) fn dispatch(frame: &TrapFrame) {
    for slot in 0..service::MAX_ATTACHMENTS {
        loop {
            let mut call = Call {
                operation: service::IRQ_POLL,
                handle: slot as u64,
                ..Call::default()
            };
            if crate::typedio::invoke(&mut call) != 0 {
                break;
            }
            let state = {
                let registry = REGISTRY.lock();
                registry
                    .as_ref()
                    .and_then(|slots| slots.get(call.record.sequence as usize))
                    .and_then(|slot| slot.state.upgrade())
                    .filter(|state| state.generation == call.record.generation)
            };
            let Some(state) = state else { continue };
            let callbacks: [Option<Arc<Callback>>; CALLBACKS] = {
                let entries = state.callbacks.lock();
                array::from_fn(|i| {
                    entries[i]
                        .as_ref()
                        .filter(|c| !c.removed.load(Ordering::Acquire))
                        .cloned()
                })
            };
            for callback in callbacks.into_iter().flatten() {
                {
                    let _entries = state.callbacks.lock();
                    if callback.removed.load(Ordering::Acquire) {
                        continue;
                    }
                    callback.running.fetch_add(1, Ordering::AcqRel);
                }
                let cpu = CpuId::current_racy().as_usize();
                ACTIVE[cpu].store(Arc::as_ptr(&callback) as usize, Ordering::Release);
                (callback.function)(frame);
                ACTIVE[cpu].store(0, Ordering::Release);
                callback.running.fetch_sub(1, Ordering::Release);
            }
            crate::irq::finish_bottom_half(state.number);
        }
    }
}
