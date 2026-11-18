// SPDX-License-Identifier: MPL-2.0
//! Typed guest endpoints backed by the authenticated host metadata service.
//! Queue ownership never substitutes for the host's independent request record.

#[path = "frontend/pipe.rs"]
mod pipe;
use super::{invoke, irq_route};
use crate::irq::IrqLine;
use ::typedio::{
    message::{self, TransferError},
    service::{self, Call, Descriptor},
    Message, Protocol,
};
use alloc::vec::Vec;
use core::marker::PhantomData;
pub use pipe::*;

pub use ::typedio::queue::{
    Completion, RequestError, RequestId, RequestTag, CAPACITY_AVAILABLE, COMPLETION_AVAILABLE,
    REMOVED,
};
pub use ::typedio::{ProtocolId, SchemaDigest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoverError {
    Host(i64),
    Schema,
    Transfer(TransferError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    Host(i64),
    InvalidOptions,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitReason {
    WouldBlock,
    DuplicateTag,
    Removed,
    Transfer(TransferError),
    Host(i64),
}
pub struct SubmitError<R> {
    pub reason: SubmitReason,
    pub request: R,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CancelError(pub i64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindError(pub i64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueOptions {
    pub depth: usize,
    pub features: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueEvents(u8);
impl QueueEvents {
    pub fn bits(self) -> u8 {
        self.0
    }
    pub const COMPLETION_AVAILABLE: Self = Self(COMPLETION_AVAILABLE);
    pub const CAPACITY_AVAILABLE: Self = Self(CAPACITY_AVAILABLE);
    pub const REMOVED: Self = Self(REMOVED);
    pub const fn empty() -> Self {
        Self(0)
    }
    pub fn contains(self, mask: Self) -> bool {
        self.0 & mask.0 == mask.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

pub struct Device<P: Protocol> {
    slot: u64,
    generation: u64,
    descriptor: Descriptor,
    config: P::Config,
}
pub struct Queue<P: Protocol> {
    device: u64,
    device_generation: u64,
    slot: u64,
    generation: u64,
    protocol: PhantomData<P>,
}

pub fn discover<P: Protocol>() -> Result<Vec<Device<P>>, DiscoverError> {
    let mut devices = Vec::new();
    for slot in 0..service::MAX_ATTACHMENTS {
        let mut call = Call {
            operation: service::DISCOVER,
            handle: slot as u64,
            ..Call::default()
        };
        let result = invoke(&mut call);
        if result == -19 {
            continue;
        }
        if result < 0 {
            return Err(DiscoverError::Host(result));
        }
        let d = call.descriptor;
        if d.protocol != P::ID.0 {
            continue;
        }
        if d.abi_version != service::ABI_VERSION
            || d.protocol_version != 1
            || d.reserved != 0
            || d.schemas
                != [
                    P::Config::SCHEMA.0,
                    P::Request::SCHEMA.0,
                    P::Reply::SCHEMA.0,
                ]
        {
            return Err(DiscoverError::Schema);
        }
        let bytes = d
            .config
            .get(..d.config_len as usize)
            .ok_or(DiscoverError::Schema)?;
        let config = message::decode::<P::Config>(P::Config::SCHEMA, bytes)
            .map_err(DiscoverError::Transfer)?;
        devices.push(Device {
            slot: slot as u64,
            generation: call.generation,
            descriptor: d,
            config,
        });
    }
    Ok(devices)
}
impl<P: Protocol> Device<P> {
    pub fn config(&self) -> &P::Config {
        &self.config
    }
    pub fn open(&self, options: QueueOptions) -> Result<Queue<P>, OpenError> {
        if options.depth == 0
            || options.depth > service::MAX_QUEUE_DEPTH
            || options.features & !self.descriptor.features != 0
        {
            return Err(OpenError::InvalidOptions);
        }
        let mut call = Call {
            operation: service::OPEN,
            handle: self.slot,
            generation: self.generation,
            arg0: options.depth as u64,
            descriptor: self.descriptor,
            ..Call::default()
        };
        call.descriptor.features = options.features;
        let result = invoke(&mut call);
        if result < 0 {
            return Err(OpenError::Host(result));
        }
        Ok(Queue {
            device: self.slot,
            device_generation: self.generation,
            slot: call.arg0,
            generation: call.arg1,
            protocol: PhantomData,
        })
    }
}
impl<P: Protocol> Queue<P> {
    fn call(&self, operation: u32) -> Call {
        Call {
            operation,
            handle: self.device,
            generation: self.device_generation,
            arg0: self.slot,
            arg1: self.generation,
            ..Call::default()
        }
    }
    pub fn try_submit(
        &self,
        mut request: P::Request,
        tag: RequestTag,
    ) -> Result<RequestId, SubmitError<P::Request>> {
        let mut call = self.call(service::SUBMIT);
        call.record.tag = tag.0;
        let mut encoder =
            message::Encoder::with_resources(&mut call.record.data, &mut call.record.resources);
        let encoded = request
            .__encode(&mut encoder)
            .map(|()| (encoder.position(), encoder.resource_count()));
        let (length, resource_count) = match encoded {
            Ok(n) => n,
            Err(error) => {
                return Err(SubmitError {
                    reason: SubmitReason::Transfer(error),
                    request,
                })
            }
        };
        call.record.len = length as u32;
        call.record.resource_count = resource_count as u32;
        let result = invoke(&mut call);
        if result < 0 {
            return Err(SubmitError {
                reason: match result {
                    -11 => SubmitReason::WouldBlock,
                    -17 => SubmitReason::DuplicateTag,
                    -19 => SubmitReason::Removed,
                    other => SubmitReason::Host(other),
                },
                request,
            });
        }
        request.__committed();
        // SAFETY: the host accepted into this fixed queue generation.
        Ok(unsafe { RequestId::__from_record(call.record.generation, call.record.sequence) })
    }
    pub fn try_pop_completion(&self) -> Option<Completion<P::Reply>> {
        let mut call = self.call(service::POP);
        if invoke(&mut call) != 0 {
            return None;
        }
        let result = if call.record.generation != self.generation {
            Err(RequestError::Invalid)
        } else {
            match call.record.status {
                0 => call
                    .record
                    .data
                    .get(..call.record.len as usize)
                    .ok_or(RequestError::Invalid)
                    .and_then(|bytes| {
                        if call.record.resource_count > 4 {
                            return Err(RequestError::Invalid);
                        }
                        let resources =
                            &call.record.resources[..call.record.resource_count as usize];
                        // SAFETY: POP transferred these authenticated tickets
                        // into this queue's attachment protection scope.
                        let mut decoder = unsafe {
                            message::Decoder::with_resources(
                                bytes,
                                resources,
                                message::ResourceContext {
                                    attachment: self.device,
                                    owner: self.device_generation,
                                },
                            )
                        };
                        let value =
                            P::Reply::__decode(&mut decoder).map_err(|_| RequestError::Invalid)?;
                        if decoder.remaining() != 0 || !decoder.resources_complete() {
                            return Err(RequestError::Invalid);
                        }
                        Ok(value)
                    }),
                1 => Err(RequestError::Cancelled),
                2 => Err(RequestError::Removed),
                3 => Err(RequestError::Backend),
                4 => Err(RequestError::Invalid),
                5 => Err(RequestError::Permission),
                6 => Err(RequestError::ResourceLimit),
                _ => Err(RequestError::Invalid),
            }
        };
        if result.is_err() {
            for resource in call
                .record
                .resources
                .iter()
                .take((call.record.resource_count as usize).min(4))
            {
                let mut drop_call = Call {
                    operation: 20,
                    handle: self.device,
                    generation: self.device_generation,
                    arg0: resource.slot,
                    arg1: resource.generation,
                    ..Call::default()
                };
                let _ = invoke(&mut drop_call);
            }
        }
        // SAFETY: correlation remains confined to this authenticated queue.
        let id = unsafe { RequestId::__from_record(call.record.generation, call.record.sequence) };
        Some(Completion {
            id,
            tag: RequestTag(call.record.tag),
            result,
        })
    }
    pub fn cancel(&self, id: RequestId) -> Result<(), CancelError> {
        let mut call = self.call(service::CANCEL);
        (call.record.generation, call.record.sequence) = id.__parts();
        let result = invoke(&mut call);
        if result < 0 {
            Err(CancelError(result))
        } else {
            Ok(())
        }
    }
    pub fn take_events(&self) -> QueueEvents {
        let mut call = self.call(service::EVENTS);
        let result = invoke(&mut call);
        QueueEvents(if result < 0 { REMOVED } else { result as u8 })
    }
    pub fn rearm_irq(&self) -> bool {
        invoke(&mut self.call(service::REARM)) == 1
    }
    pub fn bind_irq(&self, line: &IrqLine) -> Result<(), BindError> {
        let route = irq_route(line)?;
        let mut call = self.call(service::BIND);
        call.record.generation = route.generation;
        call.record.sequence = route.line as u64;
        call.record.tag = route.vcpu as u64;
        let result = invoke(&mut call);
        if result < 0 {
            Err(BindError(result))
        } else {
            Ok(())
        }
    }
}
impl<P: Protocol> Drop for Queue<P> {
    fn drop(&mut self) {
        let _ = invoke(&mut self.call(service::CLOSE));
    }
}

/// Device protocol definitions use the common Message/Queue/Pipe mechanisms.
pub mod vsock {
    pub use ::typedio::vsock::{
        Address, AttemptId, ContextId, Flow, Port, ShutdownDirection, VsockConfig,
        VsockControlKind, VsockControlMessage, VsockRequest,
    };
    pub type VsockEvent = ::typedio::vsock::VsockEvent<super::Producer, super::Consumer>;
    pub type VsockReply = ::typedio::vsock::VsockReply<super::Producer, super::Consumer>;
    pub type VsockProtocol = ::typedio::vsock::VsockProtocol<super::Producer, super::Consumer>;
}

impl core::ops::BitOr for QueueEvents {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
