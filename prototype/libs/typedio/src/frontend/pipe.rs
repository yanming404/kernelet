// SPDX-License-Identifier: MPL-2.0
//! Move-only, host-authorized byte-pipe roles. Reservations retain backing in
//! host records even when a caller forgets its local Rust borrow.
use super::{invoke, irq_route, BindError};
use crate::{
    irq::IrqLine,
    mm::{FallibleVmRead, Infallible, VmReader, VmWriter},
    Error,
};
pub use ::typedio::pipe::PipeError;
use ::typedio::{
    message::{Decoder, Encoder, ResourceContext, TransferError},
    service::{Call, Resource},
    Message, SchemaDigest,
};

struct Endpoint {
    context: ResourceContext,
    resource: Resource,
}
impl Endpoint {
    fn call(&self, operation: u32) -> Call {
        Call {
            operation,
            handle: self.context.attachment,
            generation: self.context.owner,
            arg0: self.resource.slot,
            arg1: self.resource.generation,
            ..Call::default()
        }
    }
    fn reserve(&mut self, limit: usize) -> Result<Option<Reservation<'_>>, PipeError> {
        let mut call = self.call(16);
        call.record.len = limit.try_into().map_err(|_| PipeError::Invalid)?;
        call.record.status = self.resource.kind - 1;
        let result = invoke(&mut call);
        if result == 1 {
            return Ok(None);
        }
        check(result)?;
        Ok(Some(Reservation {
            endpoint: self,
            token: call.record.generation,
            address: call.record.sequence as usize,
            length: call.record.len as usize,
            progress: 0,
            done: false,
        }))
    }
    fn bind(&mut self, line: &IrqLine) -> Result<(), BindError> {
        let route = irq_route(line)?;
        let mut call = self.call(21);
        call.record.sequence = route.line as u64;
        call.record.generation = route.generation;
        call.record.tag = route.vcpu as u64;
        let result = invoke(&mut call);
        if result < 0 {
            Err(BindError(result))
        } else {
            Ok(())
        }
    }
    fn interest(&mut self, mask: u8) -> Result<(), PipeError> {
        let mut c = self.call(22);
        c.record.status = mask as u32;
        check(invoke(&mut c))
    }
    fn events(&mut self) -> u8 {
        let mut c = self.call(23);
        let r = invoke(&mut c);
        if r < 0 {
            6
        } else {
            r as u8
        }
    }
    fn rearm(&mut self) -> bool {
        let mut c = self.call(24);
        invoke(&mut c) == 1
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = invoke(&mut self.call(20));
    }
}
fn check(result: i64) -> Result<(), PipeError> {
    if result >= 0 {
        return Ok(());
    }
    Err(match result {
        -11 => PipeError::WouldBlock,
        -16 => PipeError::Busy,
        -32 => PipeError::Closed,
        -104 => PipeError::Reset,
        -116 => PipeError::Permission,
        _ => PipeError::Invalid,
    })
}

/// The unique producer of one direction of a connection.
pub struct Producer(Endpoint);
/// The unique consumer of one direction of a connection.
pub struct Consumer(Endpoint);
macro_rules! endpoint_message {
    ($t:ident,$kind:expr,$name:expr) => {
        // SAFETY: ticket export/import is admitted only by the framework's
        // transactional resource context; ordinary metadata decoding fails closed.
        unsafe impl Message for $t {
            const SCHEMA: SchemaDigest = $name;
            const __MAX_SIZE: usize = 4;
            fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
                out.resource(self.0.resource)
            }
            fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
                let (context, resource) = input.resource($kind)?;
                Ok(Self(Endpoint { context, resource }))
            }
        }
    };
}
endpoint_message!(Producer, 1, ::typedio::message::PIPE_PRODUCER_SCHEMA);
endpoint_message!(Consumer, 2, ::typedio::message::PIPE_CONSUMER_SCHEMA);

#[derive(Clone, Copy)]
pub struct ProducerInterest(u8);
#[derive(Clone, Copy)]
pub struct ConsumerInterest(u8);
impl ProducerInterest {
    pub const SPACE_AVAILABLE: Self = Self(1);
    pub const fn empty() -> Self {
        Self(0)
    }
}
impl ConsumerInterest {
    pub const DATA_AVAILABLE: Self = Self(1);
    pub const fn empty() -> Self {
        Self(0)
    }
}
#[derive(Clone, Copy)]
pub struct ProducerEvents(u8);
#[derive(Clone, Copy)]
pub struct ConsumerEvents(u8);
macro_rules! events {
    ($t:ident,$ready:ident) => {
        impl $t {
            pub const $ready: Self = Self(1);
            pub const CLOSED: Self = Self(2);
            pub const RESET: Self = Self(4);
            pub fn bits(self) -> u8 {
                self.0
            }
            pub fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }
        }
    };
}
events!(ProducerEvents, SPACE_AVAILABLE);
events!(ConsumerEvents, DATA_AVAILABLE);
macro_rules! notifications {
    ($t:ident,$i:ident,$e:ident) => {
        impl $t {
            pub fn bind_irq(&mut self, line: &IrqLine) -> Result<(), BindError> {
                self.0.bind(line)
            }
            pub fn set_interest(&mut self, interest: $i) -> Result<(), PipeError> {
                self.0.interest(interest.0)
            }
            pub fn take_events(&mut self) -> $e {
                $e(self.0.events())
            }
            pub fn rearm_irq(&mut self) -> bool {
                self.0.rearm()
            }
        }
    };
}
notifications!(Producer, ProducerInterest, ProducerEvents);
notifications!(Consumer, ConsumerInterest, ConsumerEvents);

struct Reservation<'a> {
    endpoint: &'a mut Endpoint,
    token: u64,
    address: usize,
    length: usize,
    progress: usize,
    done: bool,
}
impl Reservation<'_> {
    fn finish(&mut self, operation: u32, bytes: usize) -> Result<(), PipeError> {
        let mut call = self.endpoint.call(operation);
        call.record.generation = self.token;
        call.record.len = bytes.try_into().map_err(|_| PipeError::Invalid)?;
        check(invoke(&mut call))
    }
    fn record(&mut self, result: Result<usize, (Error, usize)>) -> Result<usize, (Error, usize)> {
        let n = match result {
            Ok(n) | Err((_, n)) => n,
        };
        if n > self.length - self.progress {
            return Err((Error::InvalidArgs, 0));
        }
        self.finish(17, n).map_err(|_| (Error::IoError, 0))?;
        self.progress += n;
        result
    }
    fn commit(mut self, bytes: usize) -> Result<(), PipeError> {
        let r = self.finish(18, bytes);
        if r.is_ok() {
            self.done = true
        }
        r
    }
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.finish(19, 0);
        }
    }
}
pub struct ProducerReservation<'a>(Reservation<'a>);
pub struct ConsumerReservation<'a>(Reservation<'a>);
impl Producer {
    pub fn try_reserve(&mut self, limit: usize) -> Result<ProducerReservation<'_>, PipeError> {
        self.0
            .reserve(limit)?
            .map(ProducerReservation)
            .ok_or(PipeError::Closed)
    }
}
impl Consumer {
    pub fn try_reserve(
        &mut self,
        limit: usize,
    ) -> Result<Option<ConsumerReservation<'_>>, PipeError> {
        Ok(self.0.reserve(limit)?.map(ConsumerReservation))
    }
}
impl ProducerReservation<'_> {
    pub fn produce(&mut self, src: &mut VmReader) -> Result<usize, (Error, usize)> {
        self.produce_with(|dst| src.read_fallible(dst))
    }
    pub fn produce_with<F>(&mut self, transfer: F) -> Result<usize, (Error, usize)>
    where
        F: for<'b> FnOnce(&mut VmWriter<'b, Infallible>) -> Result<usize, (Error, usize)>,
    {
        // SAFETY: the host authenticated this live reservation and retains its
        // initialized backing independently until commit/cancel or eviction.
        let mut dst = unsafe {
            VmWriter::from_kernel_space(
                (self.0.address + self.0.progress) as *mut u8,
                self.0.length - self.0.progress,
            )
        };
        let result = transfer(&mut dst);
        self.0.record(result)
    }
    pub fn commit(self, bytes: usize) -> Result<(), PipeError> {
        self.0.commit(bytes)
    }
}
impl ConsumerReservation<'_> {
    pub fn consume(&mut self, dst: &mut VmWriter) -> Result<usize, (Error, usize)> {
        self.consume_with(|src| src.read_fallible(dst))
    }
    pub fn consume_with<F>(&mut self, transfer: F) -> Result<usize, (Error, usize)>
    where
        F: for<'b> FnOnce(&mut VmReader<'b, Infallible>) -> Result<usize, (Error, usize)>,
    {
        // SAFETY: same host reservation/lifetime contract as production.
        let mut src = unsafe {
            VmReader::from_kernel_space(
                (self.0.address + self.0.progress) as *const u8,
                self.0.length - self.0.progress,
            )
        };
        let result = transfer(&mut src);
        self.0.record(result)
    }
    pub fn commit(self, bytes: usize) -> Result<(), PipeError> {
        self.0.commit(bytes)
    }
}
