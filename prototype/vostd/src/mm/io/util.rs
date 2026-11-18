// SPDX-License-Identifier: MPL-2.0
//! Readers/writers obtained from retained untyped memory owners.
use super::{FallibleVmRead, FallibleVmWrite};
use super::{Infallible, VmReader, VmWriter};
use crate::prelude::Result;
/// Owner that can lend bounded byte access without exporting Rust references.
pub trait HasVmReaderWriter {
    type Types: VmReaderWriterTypes;
    fn reader(&self) -> <Self::Types as VmReaderWriterTypes>::Reader<'_>;
    fn writer(&self) -> <Self::Types as VmReaderWriterTypes>::Writer<'_>;
}
pub trait VmReaderWriterTypes {
    type Reader<'a>;
    type Writer<'a>;
    fn to_reader_result(reader: Self::Reader<'_>) -> Result<VmReader<'_, Infallible>>;
    fn to_writer_result(writer: Self::Writer<'_>) -> Result<VmWriter<'_, Infallible>>;
}
pub enum VmReaderWriterIdentity {}
impl VmReaderWriterTypes for VmReaderWriterIdentity {
    type Reader<'a> = VmReader<'a, Infallible>;
    type Writer<'a> = VmWriter<'a, Infallible>;
    fn to_reader_result(reader: Self::Reader<'_>) -> Result<VmReader<'_, Infallible>> {
        Ok(reader)
    }
    fn to_writer_result(writer: Self::Writer<'_>) -> Result<VmWriter<'_, Infallible>> {
        Ok(writer)
    }
}
pub enum VmReaderWriterResult {}
impl VmReaderWriterTypes for VmReaderWriterResult {
    type Reader<'a> = Result<VmReader<'a, Infallible>>;
    type Writer<'a> = Result<VmWriter<'a, Infallible>>;
    fn to_reader_result(reader: Self::Reader<'_>) -> Result<VmReader<'_, Infallible>> {
        reader
    }
    fn to_writer_result(writer: Self::Writer<'_>) -> Result<VmWriter<'_, Infallible>> {
        writer
    }
}

impl<T: HasVmReaderWriter + Send + Sync> super::VmIo for T {
    fn read(&self, offset: usize, output: &mut VmWriter) -> Result<()> {
        let mut reader = T::Types::to_reader_result(self.reader())?;
        let end = offset
            .checked_add(output.avail())
            .ok_or(crate::Error::Overflow)?;
        if end > reader.remain() {
            return Err(crate::Error::InvalidArgs);
        }
        reader.skip(offset);
        reader.read_fallible(output).map_err(|(error, _)| error)?;
        Ok(())
    }
    fn write(&self, offset: usize, input: &mut VmReader) -> Result<()> {
        let mut writer = T::Types::to_writer_result(self.writer())?;
        let end = offset
            .checked_add(input.remain())
            .ok_or(crate::Error::Overflow)?;
        if end > writer.avail() {
            return Err(crate::Error::InvalidArgs);
        }
        writer.skip(offset);
        writer.write_fallible(input).map_err(|(error, _)| error)?;
        Ok(())
    }
}

macro_rules! impl_vm_io_pointer {
    ($typ:ty) => {
        impl<T: HasVmReaderWriter> HasVmReaderWriter for $typ {
            type Types = T::Types;
            fn reader(&self) -> <Self::Types as VmReaderWriterTypes>::Reader<'_> {
                (**self).reader()
            }
            fn writer(&self) -> <Self::Types as VmReaderWriterTypes>::Writer<'_> {
                (**self).writer()
            }
        }
    };
}
impl_vm_io_pointer!(&T);
impl_vm_io_pointer!(&mut T);
impl_vm_io_pointer!(alloc::boxed::Box<T>);
impl_vm_io_pointer!(alloc::sync::Arc<T>);

impl<T: HasVmReaderWriter> super::VmIoFill for T {
    fn fill_zeros(&self, offset: usize, len: usize) -> Result<(), (crate::Error, usize)> {
        let mut writer = T::Types::to_writer_result(self.writer()).map_err(|e| (e, 0))?;
        if offset > writer.avail() {
            return Err((crate::Error::InvalidArgs, 0));
        }
        let done = writer.skip(offset).fill_zeros(len);
        if done == len {
            Ok(())
        } else {
            Err((crate::Error::InvalidArgs, done))
        }
    }
}
impl<T: HasVmReaderWriter> super::VmIoOnce for T {
    fn read_once<P: super::PodOnce>(&self, offset: usize) -> Result<P> {
        let mut reader = T::Types::to_reader_result(self.reader())?;
        if offset > reader.remain() {
            return Err(crate::Error::InvalidArgs);
        }
        reader.skip(offset).read_once()
    }
    fn write_once<P: super::PodOnce>(&self, offset: usize, value: &P) -> Result<()> {
        let mut writer = T::Types::to_writer_result(self.writer())?;
        if offset > writer.avail() {
            return Err(crate::Error::InvalidArgs);
        }
        writer.skip(offset).write_once(value)
    }
}
