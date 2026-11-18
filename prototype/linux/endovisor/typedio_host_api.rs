// SPDX-License-Identifier: GPL-2.0
//! Audited host-local wrappers. Only the private FFI capsule constructs host
//! requests or capability owners. Backend policy uses safe, typed operations.
use crate::{
    message::{Decoder, Encoder, TransferError},
    queue::RequestError,
    service::{Descriptor, Record, Resource},
    Message, Protocol, SchemaDigest,
};
use core::{ffi::c_void, ptr::NonNull};
extern "C" {
    fn klet_tio_request_message(p: *mut c_void) -> *const Record;
    fn klet_tio_request_queue_generation(p: *mut c_void) -> u64;
    fn klet_tio_request_host_owner(p: *mut c_void) -> u64;
    fn klet_tio_request_caller(p: *mut c_void) -> u64;
    fn klet_tio_request_cancelled(p: *mut c_void) -> bool;
    fn klet_tio_request_finish(p: *mut c_void, r: *const Record);
    fn klet_tio_endpoint_drop(e: EndpointKey, owner: u64);
    fn klet_tio_pipe_account_create(owner: u64, bytes: u64, pipes: u32) -> *mut c_void;
    fn klet_tio_pipe_account_put(p: *mut c_void);
    fn klet_tio_role_pair(
        account: *mut c_void,
        owner: u64,
        capacity: u64,
        control: *mut *mut c_void,
        keys: *mut EndpointKey,
    ) -> i32;
    fn klet_tio_pipe_put(p: *mut c_void);
    fn klet_tio_pipe_quiescent(p: *mut c_void) -> bool;
    fn klet_tio_pipe_close(p: *mut c_void, operation: u32);
    fn klet_tio_resource_notify_all();
    fn klet_tio_backend_lock();
    fn klet_tio_backend_unlock();
}
/// Originating instance/attachment protection identity. Generations are never reused.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub attachment_generation: u64,
}
struct Token(NonNull<c_void>);
// SAFETY: this is an independently retained host record, not a guest address
// or an arbitrary Message. C protects its immutable metadata and retirement.
unsafe impl Send for Token {}
pub struct HostRequest<R> {
    token: Option<Token>,
    message: R,
    caller: Caller,
}
impl<R: Message> HostRequest<R> {
    /// # Safety
    /// Only the framework worker may give its unique accepted request token.
    pub unsafe fn capture(pointer: *mut c_void) -> Result<Self, RequestError> {
        let token = Token(NonNull::new(pointer).ok_or(RequestError::Invalid)?);
        // SAFETY: worker reference pins this captured immutable record.
        let record = unsafe { &*klet_tio_request_message(pointer) };
        if record.resource_count > 4 || record.len as usize > R::__MAX_SIZE {
            return Err(RequestError::Invalid);
        }
        let bytes = record
            .data
            .get(..record.len as usize)
            .ok_or(RequestError::Invalid)?;
        let owner = unsafe { klet_tio_request_host_owner(pointer) };
        // SAFETY: admission transferred all listed tickets to this host scope.
        let mut decoder = unsafe {
            Decoder::with_resources(
                bytes,
                &record.resources[..record.resource_count as usize],
                crate::message::ResourceContext {
                    attachment: 0,
                    owner,
                },
            )
        };
        let message = R::__decode(&mut decoder).map_err(|_| RequestError::Invalid)?;
        if decoder.remaining() != 0 || !decoder.resources_complete() {
            return Err(RequestError::Invalid);
        }
        let caller = Caller {
            attachment_generation: unsafe { klet_tio_request_caller(pointer) },
        };
        Ok(Self {
            token: Some(token),
            message,
            caller,
        })
    }
}
impl<R> HostRequest<R> {
    pub fn message(&self) -> &R {
        &self.message
    }
    pub fn caller(&self) -> Caller {
        self.caller
    }
    pub fn queue_generation(&self) -> u64 {
        unsafe { klet_tio_request_queue_generation(self.token.as_ref().unwrap().0.as_ptr()) }
    }
    pub fn cancelled(&self) -> bool {
        unsafe { klet_tio_request_cancelled(self.token.as_ref().unwrap().0.as_ptr()) }
    }
    pub fn finish<T: Message>(mut self, result: Result<T, RequestError>) {
        let mut record = Record::default();
        match result {
            Err(error) => record.status = error as u32,
            Ok(mut value) => {
                let mut encoder = Encoder::with_resources(&mut record.data, &mut record.resources);
                let encoded = value.__encode(&mut encoder).is_ok();
                match if encoded { Ok(()) } else { Err(()) } {
                    Ok(()) => {
                        record.len = encoder.position() as u32;
                        record.resource_count = encoder.resource_count() as u32;
                    }
                    Err(_) => record.status = RequestError::Invalid as u32,
                }
                // C transfers ownership before this value's endpoint drops.
                unsafe { klet_tio_request_finish(self.token.take().unwrap().0.as_ptr(), &record) };
                if encoded {
                    value.__committed();
                }
                return;
            }
        }
        unsafe { klet_tio_request_finish(self.token.take().unwrap().0.as_ptr(), &record) };
    }
}
impl<R> Drop for HostRequest<R> {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            let error = Record {
                status: RequestError::Backend as u32,
                ..Record::default()
            };
            unsafe { klet_tio_request_finish(token.0.as_ptr(), &error) }
        }
    }
}
pub trait Backend<R>: Send + Sync {
    fn handle(&self, request: HostRequest<R>);
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EndpointKey {
    slot: u64,
    generation: u64,
}
pub struct Producer(Resource, bool);
pub struct Consumer(Resource, bool);
macro_rules! endpoint {
    ($ty:ident,$kind:expr,$name:expr) => {
        unsafe impl Message for $ty {
            const SCHEMA: SchemaDigest = $name;
            const __MAX_SIZE: usize = 4;
            fn __committed(&mut self) {
                self.1 = false;
            }
            fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
                out.resource(self.0)
            }
            fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
                let (_, resource) = input.resource($kind)?;
                Ok(Self(resource, true))
            }
        }
        impl Drop for $ty {
            fn drop(&mut self) {
                if !self.1 {
                    return;
                }
                unsafe {
                    klet_tio_endpoint_drop(
                        EndpointKey {
                            slot: self.0.slot,
                            generation: self.0.generation,
                        },
                        self.0.owner,
                    )
                }
            }
        }
    };
}
endpoint!(Producer, 1, crate::message::PIPE_PRODUCER_SCHEMA);
endpoint!(Consumer, 2, crate::message::PIPE_CONSUMER_SCHEMA);
pub struct PipeControl(NonNull<c_void>);
unsafe impl Send for PipeControl {}
impl PipeControl {
    pub fn quiescent(&self) -> bool {
        unsafe { klet_tio_pipe_quiescent(self.0.as_ptr()) }
    }
    pub fn close_producer(&self) {
        unsafe {
            klet_tio_pipe_close(self.0.as_ptr(), 0);
            klet_tio_resource_notify_all();
        }
    }
    pub fn close_consumer(&self) {
        unsafe {
            klet_tio_pipe_close(self.0.as_ptr(), 1);
            klet_tio_resource_notify_all();
        }
    }
    pub fn reset(&self) {
        unsafe {
            klet_tio_pipe_close(self.0.as_ptr(), 2);
            klet_tio_resource_notify_all();
        }
    }
}
impl Drop for PipeControl {
    fn drop(&mut self) {
        unsafe { klet_tio_pipe_put(self.0.as_ptr()) }
    }
}
pub struct PipeAccount {
    pointer: NonNull<c_void>,
    owner: u64,
}
unsafe impl Send for PipeAccount {}
impl PipeAccount {
    pub fn new(owner: u64, bytes: u64, pipes: u32) -> Result<Self, RequestError> {
        let pointer = unsafe { klet_tio_pipe_account_create(owner, bytes, pipes) };
        if (pointer as usize) >= usize::MAX - 4094 {
            return Err(RequestError::ResourceLimit);
        }
        Ok(Self {
            pointer: NonNull::new(pointer).ok_or(RequestError::ResourceLimit)?,
            owner,
        })
    }
    pub fn create(&self, capacity: u64) -> Result<(Producer, Consumer, PipeControl), RequestError> {
        let mut keys = [EndpointKey::default(); 2];
        let mut control = core::ptr::null_mut();
        if unsafe {
            klet_tio_role_pair(
                self.pointer.as_ptr(),
                self.owner,
                capacity,
                &mut control,
                keys.as_mut_ptr(),
            )
        } != 0
        {
            return Err(RequestError::ResourceLimit);
        }
        let resource = |i: usize, kind: u32| Resource {
            slot: keys[i].slot,
            generation: keys[i].generation,
            owner: self.owner,
            kind,
            reserved: 0,
        };
        Ok((
            Producer(resource(0, 1), true),
            Consumer(resource(1, 2), true),
            PipeControl(NonNull::new(control).unwrap()),
        ))
    }
}
impl Drop for PipeAccount {
    fn drop(&mut self) {
        unsafe { klet_tio_pipe_account_put(self.pointer.as_ptr()) }
    }
}

/// Sleepable kernel mutex capsule. No backend code is invoked under a spin
/// lock; the closure cannot keep a reference into the protected state.
pub struct Serialized<T>(core::cell::UnsafeCell<T>);
// SAFETY: every access is serialized by the dedicated Linux backend mutex.
unsafe impl<T: Send> Sync for Serialized<T> {}
impl<T> Serialized<T> {
    pub const fn new(value: T) -> Self {
        Self(core::cell::UnsafeCell::new(value))
    }
    pub fn with<U>(&self, run: impl FnOnce(&mut T) -> U) -> U {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { klet_tio_backend_unlock() }
            }
        }
        unsafe { klet_tio_backend_lock() };
        let _guard = Guard;
        // SAFETY: this capsule is private to the unique backend mutex.
        run(unsafe { &mut *self.0.get() })
    }
}
pub fn descriptor<P: Protocol>() -> Descriptor {
    Descriptor {
        protocol: P::ID.0,
        schemas: [
            P::Config::SCHEMA.0,
            P::Request::SCHEMA.0,
            P::Reply::SCHEMA.0,
        ],
        ..Descriptor::default()
    }
}

pub unsafe fn reject(p: *mut c_void, r: &Record) {
    unsafe { klet_tio_request_finish(p, r) }
}

/// Reserve every role needed by both event destinations before publication.
pub fn reserve_deliveries(
    mut first: (Producer, Consumer, u64),
    mut second: (Producer, Consumer, u64),
) -> Result<((Producer, Consumer), (Producer, Consumer)), RequestError> {
    extern "C" {
        fn klet_tio_deliver_roles(resources: *mut Resource, targets: *const u64, count: u32)
            -> i32;
    }
    let mut resources = [first.0 .0, first.1 .0, second.0 .0, second.1 .0];
    let targets = [first.2, first.2, second.2, second.2];
    // SAFETY: unique leaves own these capabilities; C validates all target
    // generations and reserves every quota before moving any role.
    if unsafe { klet_tio_deliver_roles(resources.as_mut_ptr(), targets.as_ptr(), 4) } != 0 {
        return Err(RequestError::ResourceLimit);
    }
    first.0 .0 = resources[0];
    first.1 .0 = resources[1];
    second.0 .0 = resources[2];
    second.1 .0 = resources[3];
    Ok(((first.0, first.1), (second.0, second.1)))
}

pub fn queue_alive(owner: u64, generation: u64) -> bool {
    extern "C" {
        fn klet_tio_queue_alive(owner: u64, generation: u64) -> bool;
    }
    unsafe { klet_tio_queue_alive(owner, generation) }
}

pub fn into_delivery(mut tx: Producer, mut rx: Consumer) -> [Resource; 2] {
    let result = [tx.0, rx.0];
    tx.__committed();
    rx.__committed();
    result
}

pub fn account_budget(capacity: u64, pipes: u32) -> u64 {
    extern "C" {
        fn klet_tio_pipe_account_budget(capacity: u64, pipes: u32) -> u64;
    }
    unsafe { klet_tio_pipe_account_budget(capacity, pipes) }
}
