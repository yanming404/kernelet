// SPDX-License-Identifier: MPL-2.0
//! OSTD-shaped DMA storage for the CPU-side TypedIO service, confined to Grant.
//! Device addresses identify retained storage, never authorize physical devices.
//! Noncoherent streaming callers get separate CPU/device runs and actual copies.
//! This does not register hardware DMA or implement DMA-bearing Message hooks.
use super::{
    io::util::{HasVmReaderWriter, VmReaderWriterIdentity, VmReaderWriterResult},
    Daddr, FrameAllocOptions, HasDaddr, HasPaddr, HasSize, Infallible, Paddr, Split, USegment,
    VmReader, VmWriter,
};
use crate::{prelude::Result, Error};
use core::{
    fmt::Debug,
    marker::PhantomData,
    ops::Range,
    sync::atomic::{fence, Ordering},
};
mod private {
    pub trait Sealed: Send + Sync {}
}
pub trait DmaDirection: Debug + 'static + private::Sealed {
    const CAN_READ_FROM_DEVICE: bool;
    const CAN_WRITE_TO_DEVICE: bool;
}
#[derive(Debug)]
pub enum ToDevice {}
#[derive(Debug)]
pub enum FromDevice {}
#[derive(Debug)]
pub enum FromAndToDevice {}
macro_rules! direction {
    ($t:ty,$read:expr,$write:expr) => {
        impl private::Sealed for $t {}
        impl DmaDirection for $t {
            const CAN_READ_FROM_DEVICE: bool = $read;
            const CAN_WRITE_TO_DEVICE: bool = $write;
        }
    };
}
direction!(ToDevice, false, true);
direction!(FromDevice, true, false);
direction!(FromAndToDevice, true, true);
#[derive(Debug)]
pub struct DmaStream<D: DmaDirection = FromAndToDevice> {
    cpu: USegment,
    device: Option<USegment>,
    _direction: PhantomData<D>,
}
fn prepare() {
    assert!(
        crate::irq::is_local_enabled(),
        "DMA preparation requires virtual IRQs enabled"
    );
}
impl<D: DmaDirection> DmaStream<D> {
    pub fn alloc(nframes: usize, is_cache_coherent: bool) -> Result<Self> {
        const {
            assert!(D::CAN_WRITE_TO_DEVICE);
        }
        Self::alloc_uninit(nframes, is_cache_coherent)
    }
    pub fn alloc_uninit(nframes: usize, is_cache_coherent: bool) -> Result<Self> {
        prepare();
        let cpu = FrameAllocOptions::new().alloc_segment(nframes)?.into();
        Self::map(cpu, is_cache_coherent)
    }
    pub fn map(cpu: USegment, is_cache_coherent: bool) -> Result<Self> {
        prepare();
        // USegment can only own counted frames in this instance's Grant. The
        // caller's cache flag never admits arbitrary memory or physical devices.
        let device = if is_cache_coherent {
            None
        } else {
            Some(
                FrameAllocOptions::new()
                    .alloc_segment(cpu.size() / super::PAGE_SIZE)?
                    .into(),
            )
        };
        Ok(Self {
            cpu,
            device,
            _direction: PhantomData,
        })
    }
    pub fn sync_from_device(&self, range: Range<usize>) -> Result<()> {
        if !D::CAN_READ_FROM_DEVICE {
            return Err(Error::AccessDenied);
        }
        self.sync(range, true)
    }
    pub fn sync_to_device(&self, range: Range<usize>) -> Result<()> {
        if !D::CAN_WRITE_TO_DEVICE {
            return Err(Error::AccessDenied);
        }
        self.sync(range, false)
    }
    fn sync(&self, range: Range<usize>, from: bool) -> Result<()> {
        if range.start > range.end || range.end > self.size() {
            return Err(Error::InvalidArgs);
        }
        if let Some(device) = &self.device {
            let (src, dst) = if from {
                (device, &self.cpu)
            } else {
                (&self.cpu, device)
            };
            let mut reader = src.reader();
            let mut writer = dst.writer();
            reader.skip(range.start).limit(range.len());
            writer.skip(range.start).limit(range.len());
            assert_eq!(reader.read(&mut writer), range.len());
        }
        // Coherent Grant RAM still needs publication ordering. This also orders
        // the explicit bounce copy relative to backend access/completion.
        fence(Ordering::SeqCst);
        Ok(())
    }
}
impl<D: DmaDirection> HasSize for DmaStream<D> {
    fn size(&self) -> usize {
        self.cpu.size()
    }
}
impl<D: DmaDirection> HasPaddr for DmaStream<D> {
    fn paddr(&self) -> Paddr {
        self.cpu.paddr()
    }
}
impl<D: DmaDirection> HasDaddr for DmaStream<D> {
    fn daddr(&self) -> Daddr {
        self.device.as_ref().unwrap_or(&self.cpu).paddr()
    }
}
impl<D: DmaDirection> HasVmReaderWriter for DmaStream<D> {
    type Types = VmReaderWriterResult;
    fn reader(&self) -> Result<VmReader<'_, Infallible>> {
        if !D::CAN_READ_FROM_DEVICE {
            return Err(Error::AccessDenied);
        }
        Ok(self.cpu.reader())
    }
    fn writer(&self) -> Result<VmWriter<'_, Infallible>> {
        if !D::CAN_WRITE_TO_DEVICE {
            return Err(Error::AccessDenied);
        }
        Ok(self.cpu.writer())
    }
}
impl<D: DmaDirection> Split for DmaStream<D> {
    fn split(self, offset: usize) -> (Self, Self) {
        let (a, b) = self.cpu.split(offset);
        let (da, db) = match self.device {
            Some(d) => {
                let (x, y) = d.split(offset);
                (Some(x), Some(y))
            }
            None => (None, None),
        };
        (
            Self {
                cpu: a,
                device: da,
                _direction: PhantomData,
            },
            Self {
                cpu: b,
                device: db,
                _direction: PhantomData,
            },
        )
    }
}
#[derive(Debug)]
pub struct DmaCoherent {
    storage: USegment,
}
impl DmaCoherent {
    pub fn alloc(nframes: usize, is_cache_coherent: bool) -> Result<Self> {
        Self::alloc_uninit(nframes, is_cache_coherent)
    }
    pub fn alloc_uninit(nframes: usize, is_cache_coherent: bool) -> Result<Self> {
        prepare();
        // A coherent owner cannot emulate a noncoherent target with two copies.
        if !is_cache_coherent {
            return Err(Error::AccessDenied);
        }
        Ok(Self {
            storage: FrameAllocOptions::new().alloc_segment(nframes)?.into(),
        })
    }
}
impl HasSize for DmaCoherent {
    fn size(&self) -> usize {
        self.storage.size()
    }
}
impl HasPaddr for DmaCoherent {
    fn paddr(&self) -> Paddr {
        self.storage.paddr()
    }
}
impl HasDaddr for DmaCoherent {
    fn daddr(&self) -> Daddr {
        self.storage.paddr()
    }
}
impl HasVmReaderWriter for DmaCoherent {
    type Types = VmReaderWriterIdentity;
    fn reader(&self) -> VmReader<'_, Infallible> {
        self.storage.reader()
    }
    fn writer(&self) -> VmWriter<'_, Infallible> {
        self.storage.writer()
    }
}
impl Split for DmaCoherent {
    fn split(self, offset: usize) -> (Self, Self) {
        let (a, b) = self.storage.split(offset);
        (Self { storage: a }, Self { storage: b })
    }
}
// VmIo uses the common HasVmReaderWriter adapter and these owners' direction
// checks, exactly as BioSegment and Slice do.
