// SPDX-License-Identifier: GPL-2.0
//! Kernel linkage for the same bounded QueueCore used by native tests.
#![deny(unsafe_op_in_unsafe_fn)]
#[cfg(not(feature = "native-host"))]
use kernel as _;
#[path = "../../libs/typedio/src/notification.rs"]
pub mod notification;
#[path = "../../libs/typedio/src/queue.rs"]
pub mod queue;
#[path = "../../libs/typedio/src/service.rs"]
pub mod service;
use queue::{QueueCore, Reject, RequestError, RequestId, RequestTag};
use service::{Record, MAX_METADATA, MAX_QUEUE_DEPTH};
type Core = QueueCore<Record, Record, MAX_QUEUE_DEPTH>;
#[no_mangle]
pub extern "C" fn klet_tio_queue_size() -> usize {
    core::mem::size_of::<Core>()
}
/// # Safety
/// C supplies fresh aligned storage of the reported size.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_queue_init(ptr: *mut Core, generation: u64, depth: usize) -> i32 {
    let Some(state) = Core::with_capacity(generation, depth) else {
        return -22;
    };
    // SAFETY: the C queue allocator owns this uninitialized storage.
    unsafe { ptr.write(state) };
    0
}
/// # Safety
/// C retains the queue, holds its lock and supplies a live host-private record.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_queue_op(
    ptr: *mut Core,
    operation: u32,
    record: *mut Record,
) -> i32 {
    // SAFETY: host-private allocations are retained and serialized by C.
    let state = unsafe { &mut *ptr };
    let r = unsafe { &mut *record };
    // SAFETY: identity stays confined to the authenticated queue and is checked.
    let id = unsafe { RequestId::__from_record(r.generation, r.sequence) };
    match operation {
        service::SUBMIT => {
            if r.len as usize > MAX_METADATA {
                return -22;
            }
            match state.try_submit(*r, RequestTag(r.tag)) {
                Ok(id) => {
                    (r.generation, r.sequence) = id.__parts();
                    0
                }
                Err(e) => match e.reason {
                    Reject::WouldBlock => -11,
                    Reject::DuplicateTag => -17,
                    Reject::Removed => -19,
                    Reject::Exhausted => -75,
                },
            }
        }
        107 => {
            if state.reject_before_dispatch(id).is_ok() {
                0
            } else {
                -22
            }
        }
        100 => match state.take_for_dispatch(id) {
            Some(message) => {
                let (generation, sequence) = (r.generation, r.sequence);
                *r = message;
                r.generation = generation;
                r.sequence = sequence;
                0
            }
            None => -2,
        },
        101 => {
            if r.len as usize > MAX_METADATA {
                return -22;
            }
            let result = match r.status {
                0 => Ok(*r),
                1 => Err(RequestError::Cancelled),
                2 => Err(RequestError::Removed),
                3 => Err(RequestError::Backend),
                4 => Err(RequestError::Invalid),
                5 => Err(RequestError::Permission),
                6 => Err(RequestError::ResourceLimit),
                _ => return -22,
            };
            if state.finish(id, result).is_ok() {
                0
            } else {
                -22
            }
        }
        service::POP => match state.try_pop_completion() {
            Some(done) => {
                let (generation, sequence) = done.id.__parts();
                let (mut value, status) = match done.result {
                    Ok(value) => (value, 0),
                    Err(error) => (Record::default(), error as u32),
                };
                value.generation = generation;
                value.sequence = sequence;
                value.tag = done.tag.0;
                value.status = status;
                *r = value;
                0
            }
            None => 1,
        },
        service::BIND => {
            if r.generation == 0 || r.sequence >= 64 {
                return -22;
            }
            state.bind_irq(notification::Binding {
                line: r.sequence as u32,
                vcpu: r.tag as u32,
                generation: r.generation,
            });
            0
        }
        105 => {
            state.unbind_irq();
            0
        }
        service::CANCEL => {
            if state.cancel(id).is_ok() {
                0
            } else {
                -2
            }
        }
        service::EVENTS => state.take_events() as i32,
        service::REARM => state.rearm_irq() as i32,
        102 => {
            state.remove();
            0
        }
        103 => state.used() as i32,
        106 => state.cancellation_requested(id) as i32,
        _ => -22,
    }
}
/// # Safety
/// C has withdrawn the queue and retained no accepted backend operations.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_queue_destroy(ptr: *mut Core) {
    // SAFETY: every backend reference ended before the final queue put.
    unsafe { core::ptr::drop_in_place(ptr) };
}

/// # Safety
/// C holds the queue lock and supplies writable host-private binding storage.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_queue_delivery(
    ptr: *mut Core,
    output: *mut notification::Binding,
) -> bool {
    // SAFETY: C owns the live core and serializes this operation.
    match unsafe { &mut *ptr }.take_delivery() {
        Some(binding) => {
            // SAFETY: output is a live C-owned binding record.
            unsafe { output.write(binding) };
            true
        }
        None => false,
    }
}
