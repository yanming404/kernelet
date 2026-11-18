// SPDX-License-Identifier: MPL-2.0

//! Exchangeability and field-wise transfer. The wire helpers are trusted
//! implementation machinery; device frontends use Message and Protocol.

use crate::SchemaDigest;

/// An exchangeable type with a bounded, independently retained representation.
///
/// # Safety
/// All fields (including inactive variants) must satisfy recursive admission.
/// Transfer must preserve validity, omit native padding and allocator state,
/// and retain every usable external resource without sender destructors.
pub unsafe trait Message: Sized + 'static {
    const SCHEMA: SchemaDigest;
    // Generated/audited machinery omitted from the public design sketch.
    // Defaults keep unsupported manual implementations fail-closed.
    #[doc(hidden)]
    const __MAX_SIZE: usize = 0;
    // Called only after the framework owns every exported resource. Early
    // disarming can leak a handle, like forget, but cannot free live backing.
    #[doc(hidden)]
    fn __committed(&mut self) {}
    #[doc(hidden)]
    fn __encode(&self, _: &mut Encoder<'_>) -> Result<(), TransferError> {
        Err(TransferError::ResourceRequired)
    }
    #[doc(hidden)]
    fn __decode(_: &mut Decoder<'_>) -> Result<Self, TransferError> {
        Err(TransferError::ResourceRequired)
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolId(pub [u8; 16]);

pub trait Protocol {
    type Config: Message;
    type Request: Message;
    type Reply: Message;
    const ID: ProtocolId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    Truncated,
    InvalidValue,
    LengthLimit,
    SchemaMismatch,
    ResourceRequired,
}

#[doc(hidden)]
pub struct Encoder<'a> {
    bytes: &'a mut [u8],
    position: usize,
    resources: Option<&'a mut [crate::service::Resource]>,
    resource_position: usize,
}
impl<'a> Encoder<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            resources: None,
            resource_position: 0,
        }
    }
    pub fn with_resources(
        bytes: &'a mut [u8],
        resources: &'a mut [crate::service::Resource],
    ) -> Self {
        Self {
            bytes,
            position: 0,
            resources: Some(resources),
            resource_position: 0,
        }
    }
    pub fn resource_count(&self) -> usize {
        self.resource_position
    }
    pub fn resource(&mut self, resource: crate::service::Resource) -> Result<(), TransferError> {
        let index = self.resource_position;
        let entry = self
            .resources
            .as_mut()
            .and_then(|r| r.get_mut(index))
            .ok_or(TransferError::ResourceRequired)?;
        *entry = resource;
        self.resource_position += 1;
        self.write(&(index as u32).to_le_bytes())
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), TransferError> {
        let end = self
            .position
            .checked_add(bytes.len())
            .ok_or(TransferError::LengthLimit)?;
        let out = self
            .bytes
            .get_mut(self.position..end)
            .ok_or(TransferError::Truncated)?;
        out.copy_from_slice(bytes);
        self.position = end;
        Ok(())
    }
}
#[doc(hidden)]
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct ResourceContext {
    pub attachment: u64,
    pub owner: u64,
}
pub struct Decoder<'a> {
    bytes: &'a [u8],
    position: usize,
    resources: Option<&'a [crate::service::Resource]>,
    resource_position: usize,
    context: ResourceContext,
}
impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            resources: None,
            resource_position: 0,
            context: ResourceContext {
                attachment: 0,
                owner: 0,
            },
        }
    }
    /// # Safety
    /// Tickets must already have been transferred to this protection scope.
    /// Every ticket occurs exactly once and the backing outlives decoding.
    pub unsafe fn with_resources(
        bytes: &'a [u8],
        resources: &'a [crate::service::Resource],
        context: ResourceContext,
    ) -> Self {
        Self {
            bytes,
            position: 0,
            resources: Some(resources),
            resource_position: 0,
            context,
        }
    }
    pub fn resource(
        &mut self,
        kind: u32,
    ) -> Result<(ResourceContext, crate::service::Resource), TransferError> {
        let index = u32::from_le_bytes(self.read(4)?.try_into().unwrap()) as usize;
        if index != self.resource_position {
            return Err(TransferError::InvalidValue);
        }
        let resource = *self
            .resources
            .and_then(|r| r.get(index))
            .ok_or(TransferError::ResourceRequired)?;
        if resource.kind != kind || resource.reserved != 0 || resource.owner != self.context.owner {
            return Err(TransferError::InvalidValue);
        }
        self.resource_position += 1;
        Ok((self.context, resource))
    }
    pub fn resources_complete(&self) -> bool {
        self.resources
            .map_or(true, |r| r.len() == self.resource_position)
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub fn read(&mut self, n: usize) -> Result<&'a [u8], TransferError> {
        let end = self
            .position
            .checked_add(n)
            .ok_or(TransferError::LengthLimit)?;
        let out = self
            .bytes
            .get(self.position..end)
            .ok_or(TransferError::Truncated)?;
        self.position = end;
        Ok(out)
    }
}

/// Generated transfer support for resource-free messages. Resource owners
/// require the separately audited retention/role-transfer hooks; they cannot
/// become Wire merely by being Copy or having a C representation.
#[doc(hidden)]
pub trait Wire: Message {
    const MAX_SIZE: usize;
    fn encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError>;
    fn decode(input: &mut Decoder<'_>) -> Result<Self, TransferError>;
}

// Transfer availability follows the audited Message implementation, without
// adding a second public bound to discover<P: Protocol> or Queue<P>.
impl<T: Message> Wire for T {
    const MAX_SIZE: usize = T::__MAX_SIZE;
    fn encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
        self.__encode(out)
    }
    fn decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
        T::__decode(input)
    }
}

pub fn encode<T: Wire>(value: &T, output: &mut [u8]) -> Result<usize, TransferError> {
    let mut writer = Encoder::new(output);
    value.encode(&mut writer)?;
    Ok(writer.position())
}
pub fn decode<T: Wire>(schema: SchemaDigest, input: &[u8]) -> Result<T, TransferError> {
    if schema != T::SCHEMA {
        return Err(TransferError::SchemaMismatch);
    }
    if input.len() > T::MAX_SIZE {
        return Err(TransferError::LengthLimit);
    }
    let mut reader = Decoder::new(input);
    let value = T::decode(&mut reader)?;
    if reader.remaining() != 0 {
        return Err(TransferError::InvalidValue);
    }
    Ok(value)
}

macro_rules! integer {
    ($($ty:ty),* $(,)?)=>{$(
        // SAFETY: integers have no resource, reference, or destructor state.
        unsafe impl Message for $ty {
            const SCHEMA:SchemaDigest=SchemaDigest::of(stringify!($ty),&[],&[core::mem::size_of::<Self>() as u64]);

            const __MAX_SIZE:usize=core::mem::size_of::<Self>();
            fn __encode(&self,out:&mut Encoder<'_>)->Result<(),TransferError>{out.write(&self.to_le_bytes())}
            fn __decode(input:&mut Decoder<'_>)->Result<Self,TransferError>{
                Ok(Self::from_le_bytes(input.read(Self::__MAX_SIZE)?.try_into().map_err(|_|TransferError::Truncated)?))
            }
        }

    )*};
}
integer!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64);

// SAFETY: unit has neither fields nor resources.
unsafe impl Message for () {
    const SCHEMA: SchemaDigest = SchemaDigest::of("unit", &[], &[]);

    const __MAX_SIZE: usize = 0;
    // Called only after the framework owns every exported resource. Early
    // disarming can leak a handle, like forget, but cannot free live backing.
    #[doc(hidden)]
    fn __committed(&mut self) {}
    fn __encode(&self, _: &mut Encoder<'_>) -> Result<(), TransferError> {
        Ok(())
    }
    fn __decode(_: &mut Decoder<'_>) -> Result<Self, TransferError> {
        Ok(())
    }
}

// SAFETY: bool's transfer encoding validates its two legal values.
unsafe impl Message for bool {
    const SCHEMA: SchemaDigest = SchemaDigest::of("bool", &[], &[]);

    const __MAX_SIZE: usize = 1;
    fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
        (*self as u8).__encode(out)
    }
    fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
        match u8::__decode(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(TransferError::InvalidValue),
        }
    }
}

// SAFETY: char's transfer encoding validates Unicode scalar values.
unsafe impl Message for char {
    const SCHEMA: SchemaDigest = SchemaDigest::of("char", &[], &[]);

    const __MAX_SIZE: usize = 4;
    fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
        (*self as u32).__encode(out)
    }
    fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
        char::from_u32(u32::__decode(input)?).ok_or(TransferError::InvalidValue)
    }
}

macro_rules! pointer {
    ($mutability:ident,$ty:ty,$name:literal) => {
        // SAFETY: only the opaque address is transferred. The pointee is not
        // accessed or retained; decoding never manufactures a Rust reference.
        unsafe impl<T: Sized + 'static> Message for $ty {
            const SCHEMA: SchemaDigest =
                SchemaDigest::of($name, &[], &[core::mem::size_of::<usize>() as u64]);

            const __MAX_SIZE: usize = core::mem::size_of::<usize>();
            fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
                (*self as usize).__encode(out)
            }
            fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
                Ok(usize::__decode(input)? as $ty)
            }
        }
    };
}
pointer!(const, *const T, "opaque.const.pointer");
pointer!(mut, *mut T, "opaque.mut.pointer");

// SAFETY: array admission is recursive; only initialized elements transfer.
unsafe impl<T: Message, const N: usize> Message for [T; N] {
    fn __committed(&mut self) {
        for item in self {
            item.__committed();
        }
    }
    const SCHEMA: SchemaDigest = SchemaDigest::of("array", &[T::SCHEMA], &[N as u64]);

    const __MAX_SIZE: usize = N * T::__MAX_SIZE;
    fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
        for x in self {
            x.__encode(out)?
        }
        Ok(())
    }
    fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
        let mut slots: [Option<T>; N] = core::array::from_fn(|_| None);
        for slot in &mut slots {
            *slot = Some(T::__decode(input)?)
        }
        Ok(slots.map(|x| x.unwrap()))
    }
}

// SAFETY: both Option variants are admitted and encoded with a checked tag.
unsafe impl<T: Message> Message for Option<T> {
    fn __committed(&mut self) {
        if let Some(item) = self {
            item.__committed();
        }
    }
    const SCHEMA: SchemaDigest = SchemaDigest::of("option", &[T::SCHEMA], &[]);

    const __MAX_SIZE: usize = 1 + T::__MAX_SIZE;
    fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
        match self {
            None => 0u8.__encode(out),
            Some(x) => {
                1u8.__encode(out)?;
                x.__encode(out)
            }
        }
    }
    fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
        match u8::__decode(input)? {
            0 => Ok(None),
            1 => Ok(Some(T::__decode(input)?)),
            _ => Err(TransferError::InvalidValue),
        }
    }
}

// SAFETY: every field of both Result variants is recursively admitted.
unsafe impl<T: Message, E: Message> Message for Result<T, E> {
    fn __committed(&mut self) {
        match self {
            Ok(item) => item.__committed(),
            Err(item) => item.__committed(),
        }
    }
    const SCHEMA: SchemaDigest = SchemaDigest::of("result", &[T::SCHEMA, E::SCHEMA], &[]);

    const __MAX_SIZE: usize = 1 + if T::__MAX_SIZE > E::__MAX_SIZE {
        T::__MAX_SIZE
    } else {
        E::__MAX_SIZE
    };
    fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
        match self {
            Ok(x) => {
                0u8.__encode(out)?;
                x.__encode(out)
            }
            Err(e) => {
                1u8.__encode(out)?;
                e.__encode(out)
            }
        }
    }
    fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
        match u8::__decode(input)? {
            0 => Ok(Ok(T::__decode(input)?)),
            1 => Ok(Err(E::__decode(input)?)),
            _ => Err(TransferError::InvalidValue),
        }
    }
}

macro_rules! tuple {
    ($($ty:ident:$index:tt),+)=>{
        // SAFETY: every element is recursively admitted; native tuple padding
        // and layout are never observed.
        unsafe impl<$($ty:Message),+> Message for ($($ty,)+){
            fn __committed(&mut self){$(self.$index.__committed();)+}
            const SCHEMA:SchemaDigest=SchemaDigest::of("tuple",&[$($ty::SCHEMA),+],&[]);

            const __MAX_SIZE:usize=0$(+$ty::__MAX_SIZE)+;
            fn __encode(&self,out:&mut Encoder<'_>)->Result<(),TransferError>{$ (self.$index.__encode(out)?;)+ Ok(())}
            fn __decode(input:&mut Decoder<'_>)->Result<Self,TransferError>{Ok(($($ty::__decode(input)?,)+))}
        }

    }
}
tuple!(A:0);
tuple!(A:0,B:1);
tuple!(A:0,B:1,C:2);
tuple!(A:0,B:1,C:2,D:3);
tuple!(A:0,B:1,C:2,D:3,E:4);
tuple!(A:0,B:1,C:2,D:3,E:4,F:5);
tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6);
tuple!(A:0,B:1,C:2,D:3,E:4,F:5,G:6,H:7);

/// An inline, schema-bounded list. Local Option discriminants and unused slots
/// are never copied into a transfer record. It does not require heap allocation.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BoundedVec<T, const N: usize = 16> {
    slots: [Option<T>; N],
    len: usize,
}
impl<T, const N: usize> BoundedVec<T, N> {
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| None),
            len: 0,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.len == N {
            return Err(value);
        }
        self.slots[self.len] = Some(value);
        self.len += 1;
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots[..self.len].iter().map(|x| x.as_ref().unwrap())
    }
}
impl<T, const N: usize> Default for BoundedVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T, const M: usize, const N: usize> TryFrom<[T; M]> for BoundedVec<T, N> {
    type Error = [T; M];
    fn try_from(values: [T; M]) -> Result<Self, Self::Error> {
        if M > N {
            return Err(values);
        }
        let mut out = Self::new();
        for value in values {
            let _ = out.push(value);
        }
        Ok(out)
    }
}
// SAFETY: the schema bounds the list; only live elements transfer recursively.
unsafe impl<T: Message, const N: usize> Message for BoundedVec<T, N> {
    fn __committed(&mut self) {
        for item in &mut self.slots[..self.len] {
            item.__committed();
        }
    }
    const SCHEMA: SchemaDigest = SchemaDigest::of("bounded-vec", &[T::SCHEMA], &[N as u64]);

    const __MAX_SIZE: usize = 8 + N * T::__MAX_SIZE;
    fn __encode(&self, out: &mut Encoder<'_>) -> Result<(), TransferError> {
        (self.len as u64).__encode(out)?;
        for x in self.iter() {
            x.__encode(out)?
        }
        Ok(())
    }
    fn __decode(input: &mut Decoder<'_>) -> Result<Self, TransferError> {
        let len = u64::__decode(input)?;
        if len > N as u64 {
            return Err(TransferError::LengthLimit);
        }
        let mut out = Self::new();
        for _ in 0..len {
            let value = T::__decode(input)?;
            let _ = out.push(value);
        }
        Ok(out)
    }
}

/// Encodes a packed field without constructing an unaligned Rust reference.
///
/// # Safety
/// The field must be a live, valid and stable admitted value for this call.
/// Its Wire implementation must obey Message's no-interior-mutable-metadata
/// capture contract. Only the trusted derive calls this helper.
#[doc(hidden)]
pub unsafe fn __encode_unaligned<T: Wire>(
    field: *const T,
    out: &mut Encoder<'_>,
) -> Result<(), TransferError> {
    // SAFETY: the enclosing shared borrow protects the complete valid field.
    // ManuallyDrop prevents this temporary aligned snapshot from running the
    // original owner's destructor. Only admitted field-wise transfer observes
    // it; neither native padding nor allocator/ownership bytes are transmitted.
    let snapshot = core::mem::ManuallyDrop::new(unsafe { core::ptr::read_unaligned(field) });
    snapshot.encode(out)
}

/// Disarms an admitted packed field through an aligned, non-dropping snapshot.
/// # Safety
/// `field` must be exclusively owned and point to a valid admitted field. No
/// destructor runs for the overwritten snapshot or for the original bytes.
#[doc(hidden)]
pub unsafe fn __committed_unaligned<T: Message>(field: *mut T) {
    let mut snapshot = core::mem::ManuallyDrop::new(unsafe { field.read_unaligned() });
    snapshot.__committed();
    unsafe { field.write_unaligned(core::mem::ManuallyDrop::into_inner(snapshot)) };
}

/// Canonical audited pipe-leaf schemas, shared by native companions and tools.
pub const PIPE_PRODUCER_SCHEMA: SchemaDigest = SchemaDigest::of("TypedIO.Producer.v1", &[], &[]);
pub const PIPE_CONSUMER_SCHEMA: SchemaDigest = SchemaDigest::of("TypedIO.Consumer.v1", &[], &[]);
