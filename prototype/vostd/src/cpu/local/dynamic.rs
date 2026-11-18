// SPDX-License-Identifier: MPL-2.0
//! Dynamic per-vCPU objects backed by the instance-accounted Host heap.
//! Chunk identity and live-slot accounting survive individual object movement.
use crate::{
    cpu::{all_cpus, num_cpus, CpuId, PinCurrentCpu},
    Error,
};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    mem::{align_of, size_of},
    sync::atomic::{AtomicUsize, Ordering},
};

/// A dynamically allocated CPU-local object. Its storage is owned, never copied.
pub struct DynamicCpuLocal<T> {
    slots: Box<[T]>,
    owner: Arc<AtomicUsize>,
}
// SAFETY: a pinned caller accesses only its own vCPU slot; remote access
// requires T: Sync. Send permits moving each slot between carrier executions.
unsafe impl<T: Send> Sync for DynamicCpuLocal<T> {}
impl<T> DynamicCpuLocal<T> {
    /// Creates an independently owned object with one initialized value per vCPU.
    pub fn try_new(mut init: impl FnMut(CpuId) -> T) -> Result<Self, Error> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(num_cpus())
            .map_err(|_| Error::NoMemory)?;
        for cpu in all_cpus() {
            values.push(init(cpu));
        }
        Ok(Self {
            slots: values.into_boxed_slice(),
            owner: Arc::new(AtomicUsize::new(1)),
        })
    }

    pub fn get_with<'a, G: PinCurrentCpu>(&'a self, guard: &'a G) -> &'a T {
        &self.slots[guard.current_cpu().as_usize()]
    }
}
impl<T: Sync> DynamicCpuLocal<T> {
    pub fn get_on_cpu(&self, cpu: CpuId) -> &T {
        &self.slots[cpu.as_usize()]
    }
}
impl<T> Drop for DynamicCpuLocal<T> {
    fn drop(&mut self) {
        // No access may outlive the exclusive owner being dropped. Box drops
        // every initialized T; the accounting identity remains valid via Arc.
        let old = self.owner.fetch_sub(1, Ordering::AcqRel);
        assert!(old > 0, "CPU-local chunk accounting underflow");
    }
}
/// Bounded group of dynamic objects, with the OSTD allocation/deallocation API.
/// Each object owns exactly num_cpus() initialized values; no Grant metadata or
/// host GS register is borrowed to manufacture per-CPU references.
pub struct DynCpuLocalChunk<const ITEM_SIZE: usize> {
    live: Arc<AtomicUsize>,
}
impl<const ITEM_SIZE: usize> DynCpuLocalChunk<ITEM_SIZE> {
    const CAPACITY: usize = 4096 / ITEM_SIZE;
    pub fn new() -> Result<Self, Error> {
        if !ITEM_SIZE.is_power_of_two() || ITEM_SIZE > 4096 {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            live: Arc::new(AtomicUsize::new(0)),
        })
    }
    pub fn alloc<T>(&mut self, init: &mut impl FnMut(CpuId) -> T) -> Option<DynamicCpuLocal<T>> {
        assert!(size_of::<T>() <= ITEM_SIZE && align_of::<T>() <= ITEM_SIZE);
        if self.is_full() {
            return None;
        }
        // Build all values before publishing the live slot. Partial initializer
        // unwinding drops the Vec, without leaving an occupied slot behind.
        let mut values = Vec::new();
        values.try_reserve_exact(num_cpus()).ok()?;
        for cpu in all_cpus() {
            values.push(init(cpu));
        }
        self.live.fetch_add(1, Ordering::Release);
        Some(DynamicCpuLocal {
            slots: values.into_boxed_slice(),
            owner: self.live.clone(),
        })
    }
    pub fn try_dealloc<T>(&mut self, value: DynamicCpuLocal<T>) -> Result<(), DynamicCpuLocal<T>> {
        if !Arc::ptr_eq(&self.live, &value.owner) {
            return Err(value);
        }
        drop(value);
        Ok(())
    }
    pub fn is_full(&self) -> bool {
        self.live.load(Ordering::Acquire) >= Self::CAPACITY
    }
    pub fn is_empty(&self) -> bool {
        self.live.load(Ordering::Acquire) == 0
    }
}
impl<const ITEM_SIZE: usize> Drop for DynCpuLocalChunk<ITEM_SIZE> {
    fn drop(&mut self) {
        assert!(
            self.is_empty(),
            "dropping CPU-local chunk with live objects"
        );
    }
}
