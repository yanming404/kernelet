// SPDX-License-Identifier: GPL-2.0

//! Kernel linkage for the same safe metadata code tested by Cargo.
//! C owns allocations, per-pipe locks and retained backing. Only this thin
//! FFI layer dereferences those host-private records; it never sees a guest
//! reference, callback, allocator, native enum, or payload slice.

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(not(feature = "native-host"))]
use kernel as _;

#[cfg(not(feature = "native-host"))]
#[path = "../../libs/typedio/src/notification.rs"]
pub mod notification;
#[cfg(feature = "native-host")]
pub use crate::notification;
#[path = "../../libs/typedio/src/pipe.rs"]
pub mod pipe;

use pipe::{PipeCore, PipeError, Role};

#[path = "../../libs/typedio/src/abi.rs"]
pub mod records;
use records::ReservationRecord;

fn role(value: u32) -> Result<Role, PipeError> {
    match value {
        0 => Ok(Role::Producer),
        1 => Ok(Role::Consumer),
        _ => Err(PipeError::Invalid),
    }
}
fn error(value: PipeError) -> i32 {
    match value {
        PipeError::WouldBlock => -11,
        PipeError::Closed => -32,
        PipeError::Reset => -104,
        PipeError::Invalid => -22,
        PipeError::Busy => -16,
        PipeError::Exhausted => -75,
        PipeError::Permission => -1,
    }
}

/// Returns the allocation size required for the host-private metadata core.
#[no_mangle]
pub extern "C" fn klet_tio_pipe_core_size() -> usize {
    core::mem::size_of::<PipeCore>()
}

/// Initializes host-private, aligned storage, allocated by the C pipe creator.
///
/// # Safety
/// `ptr` owns at least `klet_tio_pipe_core_size()` bytes aligned for PipeCore.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_init(
    ptr: *mut PipeCore,
    capacity: usize,
    producer: u64,
    consumer: u64,
) -> i32 {
    let Some(value) = PipeCore::new(capacity, producer, consumer) else {
        return -22;
    };
    // SAFETY: the creator supplies fresh, correctly sized and aligned storage.
    unsafe { ptr.write(value) };
    0
}

/// # Safety
/// All functions below require the C caller to hold this pipe's metadata lock
/// and retain its host allocation. `output` is host-private writable storage.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_reserve(
    ptr: *mut PipeCore,
    owner: u64,
    which: u32,
    limit: usize,
    output: *mut ReservationRecord,
) -> i32 {
    // SAFETY: this function's C-side lock/allocation contract.
    let state = unsafe { &mut *ptr };
    let result = match role(which) {
        Ok(Role::Producer) => state.reserve_producer(owner, limit).map(Some),
        Ok(Role::Consumer) => state.reserve_consumer(owner, limit),
        Err(e) => return error(e),
    };
    match result {
        Ok(Some(r)) => {
            // SAFETY: output is a live host-owned transfer record.
            unsafe {
                output.write(ReservationRecord {
                    token: r.token(),
                    offset: r.offset() as u64,
                    length: r.length() as u64,
                    progress: r.progress() as u64,
                })
            };
            0
        }
        Ok(None) => 1,
        Err(e) => error(e),
    }
}

/// # Safety
/// `ptr` is a live host-private pipe and its metadata lock is held.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_finalize(
    ptr: *mut PipeCore,
    owner: u64,
    which: u32,
    token: u64,
    bytes: usize,
    operation: u32,
) -> i32 {
    // SAFETY: C retains and locks the pipe across this call.
    let state = unsafe { &mut *ptr };
    let which = match role(which) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let r = match state.reservation(which, owner, token) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let result = match operation {
        0 => state.record_progress(owner, r, bytes).map(|_| ()),
        1 => state.commit(owner, r, bytes),
        2 => state.cancel(owner, r),
        _ => Err(PipeError::Invalid),
    };
    match result {
        Ok(()) => 0,
        Err(e) => error(e),
    }
}

/// # Safety
/// `ptr` is a live host-private pipe and its metadata lock is held.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_close(ptr: *mut PipeCore, operation: u32) {
    // SAFETY: C retains and locks the pipe across this call.
    let state = unsafe { &mut *ptr };
    match operation {
        0 => state.close_producer(),
        1 => state.close_consumer(),
        _ => state.reset(),
    }
}

/// # Safety
/// `ptr` is a live host-private pipe and its metadata lock is held.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_reclaimable(ptr: *mut PipeCore) -> bool {
    // SAFETY: C retains and locks the pipe across this call.
    unsafe { &*ptr }.can_release_storage()
}

/// # Safety
/// C holds the live pipe's metadata lock and supplies a valid output record.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_access(
    ptr: *mut PipeCore,
    owner: u64,
    which: u32,
    token: u64,
    output: *mut ReservationRecord,
    bytes: usize,
    ending: bool,
) -> i32 {
    // SAFETY: C retains and locks the pipe across this call.
    let state = unsafe { &mut *ptr };
    let which = match role(which) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    if ending {
        return match state.end_transfer(which, owner, token, bytes) {
            Ok(()) => 0,
            Err(e) => error(e),
        };
    }
    match state.begin_transfer(which, owner, token) {
        Ok(r) => {
            // SAFETY: the C wrapper passes a live host-private output record.
            unsafe {
                output.write(ReservationRecord {
                    token: r.token(),
                    offset: r.offset() as u64,
                    length: r.length() as u64,
                    progress: r.progress() as u64,
                })
            };
            0
        }
        Err(e) => error(e),
    }
}

/// Destroys a fully initialized core after every endpoint/access reference ends.
///
/// # Safety
/// `ptr` owns a live PipeCore and no future access to it is possible.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_destroy(ptr: *mut PipeCore) {
    // SAFETY: the C wrapper calls this only on the last independently retained
    // pipe reference; no reservation or payload operation can remain active.
    unsafe { core::ptr::drop_in_place(ptr) };
}

/// # Safety
/// C retains the pipe and holds its metadata lock across validation/transfer.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_transfer(
    ptr: *mut PipeCore,
    owner: u64,
    which: u32,
    target: u64,
    commit: bool,
) -> i32 {
    // SAFETY: C's host-private pipe lock serializes this unique view.
    let state = unsafe { &mut *ptr };
    let which = match role(which) {
        Ok(value) => value,
        Err(e) => return error(e),
    };
    let result = if commit {
        state.transfer_role(which, owner, target)
    } else {
        state.can_transfer_role(which, owner)
    };
    match result {
        Ok(()) => 0,
        Err(e) => error(e),
    }
}

/// Operates on retained pipe notification state without invoking guest code.
/// # Safety
/// The C owner retains the pipe and holds its metadata lock; binding points
/// to a live host-private record, never to guest callbacks.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_pipe_core_notify(
    ptr: *mut PipeCore,
    owner: u64,
    which: u32,
    operation: u32,
    value: u32,
    binding: *mut notification::Binding,
) -> i32 {
    // SAFETY: the wrapper serializes access to this retained allocation.
    let state = unsafe { &mut *ptr };
    let role = match role(which) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let result = match operation {
        21 => state.bind_irq(role, owner, unsafe { *binding }).map(|()| 0),
        22 => state.set_interest(role, owner, value as u8).map(|()| 0),
        23 => state.take_events(role, owner).map(|v| v as i32),
        24 => state.rearm_irq(role, owner).map(|v| v as i32),
        26 => state.current_events(role, owner).map(|v| v as i32),
        25 => {
            if let Some(b) = state.take_delivery(role) {
                unsafe { binding.write(b) };
                Ok(1)
            } else {
                Ok(0)
            }
        }
        _ => Err(pipe::PipeError::Invalid),
    };
    match result {
        Ok(n) => n,
        Err(e) => error(e),
    }
}
