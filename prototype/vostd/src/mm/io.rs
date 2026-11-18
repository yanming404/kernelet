// SPDX-License-Identifier: MPL-2.0

//! Abstractions for reading and writing virtual memory (VM) objects.
//!
//! The one structural difference from real OSTD: a *fallible* reader or writer
//! over tenant memory does not hold a user virtual address that the kernel then
//! dereferences with `stac`/`clac` and an exception table. It holds the page
//! table's root and a user virtual address, and every copy walks the table in
//! vOSTD and goes through the linear-map alias of each frame. Tenant memory is
//! therefore never dereferenced through a user address, and a translation
//! failure is a plain `None` rather than a trap.
//!
//! The table walked is the tenant process's Linux page table, and each chunk's
//! walk and copy run inside an RCU read section ([`super::rcu`]): a copy takes
//! no cursor, so an `unmap` on
//! another virtual CPU may clear the entry it walked, and the frame is then
//! released only after the grace period, by which time the copy is done.

use core::marker::PhantomData;
use ostd_pod::{IntoBytes, Pod};
#[path = "io/util.rs"]
pub mod util;

use super::{paddr_to_vaddr, page_table, Paddr, Vaddr, PAGE_SIZE};
use crate::{prelude::Result, Error};

/// A marker type for infallible memory accesses (kernel memory).
pub struct Infallible;

/// A marker type for fallible memory accesses (tenant memory).
pub struct Fallible;

/// Where the bytes are.
#[derive(Clone, Copy)]
enum Region {
    /// A run of host kernel virtual addresses; always the linear map here.
    Kernel { cursor: usize, end: usize },
    /// A run of tenant virtual addresses, to be walked through the page table
    /// whose root is `root`.
    User {
        root: Paddr,
        cursor: Vaddr,
        end: Vaddr,
    },
}

impl Region {
    fn remain(&self) -> usize {
        match *self {
            Region::Kernel { cursor, end } => end - cursor,
            Region::User { cursor, end, .. } => end - cursor,
        }
    }

    /// Resolves the head of the region to a host virtual address and the number
    /// of bytes that are contiguous there.
    fn resolve(&self, write: bool) -> Option<(*mut u8, usize)> {
        match *self {
            Region::Kernel { cursor, end } => Some((cursor as *mut u8, end - cursor)),
            Region::User { root, cursor, end } => {
                let leaf = page_table::walk(root, cursor & !(PAGE_SIZE - 1))?;
                if write && !leaf.flags.contains(super::PageFlags::W) {
                    return None;
                }
                let paddr = leaf.paddr;
                let offset = cursor % PAGE_SIZE;
                let len = (PAGE_SIZE - offset).min(end - cursor);
                // The alias of the byte the cursor is at, not of the frame's
                // base: a chunk may start in the middle of a page.
                Some(((paddr_to_vaddr(paddr) + offset) as *mut u8, len))
            }
        }
    }

    fn advance(&mut self, n: usize) {
        match self {
            Region::Kernel { cursor, .. } => *cursor += n,
            Region::User { cursor, .. } => *cursor += n,
        }
    }

    // Called only while the page-table read section is held.
    fn fault(&self, write: bool) -> Option<(Paddr, Vaddr, bool, bool)> {
        match *self {
            Self::Kernel { .. } => None,
            Self::User { root, cursor, .. } => Some((
                root,
                cursor,
                write,
                page_table::walk(root, cursor & !(PAGE_SIZE - 1)).is_some(),
            )),
        }
    }
}

/// Copies `n` bytes with `rep movsb`, which the compiler cannot see through.
///
/// Tenant memory may change under the copy (another virtual CPU's tenant
/// thread, or a frame an unmap is about to release), so a plain
/// `copy_nonoverlapping` is a data race the optimizer may exploit: it is free to
/// serve a later load from the destination by loading the source again, after
/// the read section has ended. OSTD copies user memory with an
/// assembly routine for the same reason.
///
/// # Safety
///
/// `dst` must be valid for writes and `src` for reads of `n` bytes.
#[inline]
unsafe fn copy_opaque(dst: *mut u8, src: *const u8, n: usize) {
    // SAFETY: the caller's; the direction flag is clear, as the ABI promises.
    unsafe {
        core::arch::asm!(
            "rep movsb",
            inout("rcx") n => _,
            inout("rdi") dst => _,
            inout("rsi") src => _,
            options(nostack, preserves_flags),
        );
    }
}

/// Copies from `src` to `dst`, up to the smaller of the two remainders.
///
/// Returns the number of bytes copied and whether a translation failed before
/// the copy was complete.
fn copy_regions(dst: &mut Region, src: &mut Region) -> (usize, bool) {
    let total = dst.remain().min(src.remain());
    let mut copied = 0;
    let mut retries = 0;

    let user = matches!(dst, Region::User { .. }) || matches!(src, Region::User { .. });
    while copied < total {
        let left = total - copied;
        let mut fault = None;
        let mut step = |dst: &Region, src: &Region| -> Option<usize> {
            let Some((dptr, dlen)) = dst.resolve(true) else {
                fault = dst.fault(true);
                return None;
            };
            let Some((sptr, slen)) = src.resolve(false) else {
                fault = src.fault(false);
                return None;
            };
            let n = dlen.min(slen).min(left);
            // SAFETY: both pointers are linear-map aliases of frames that stay
            // allocated for the read section (a frame an unmap removes waits for
            // the grace period), valid for `n` bytes; the two regions do not
            // overlap: one of them is always a kernel buffer in this prototype.
            unsafe { copy_opaque(dptr, sptr as *const u8, n) };
            Some(n)
        };
        let done = if user {
            super::rcu::read(|| step(dst, src))
        } else {
            step(dst, src)
        };
        let Some(n) = done else {
            if let Some((root, address, write, present)) = fault {
                if retries < 4 && super::fault::resolve(root, address, write, present) {
                    retries += 1;
                    continue;
                }
            }
            return (copied, false);
        };
        retries = 0;
        dst.advance(n);
        src.advance(n);
        copied += n;
    }

    (copied, true)
}

/// Fallible memory read from a `VmReader` to a `VmWriter`.
pub trait FallibleVmRead<F> {
    /// Reads all data into the writer.
    ///
    /// On success, the number of bytes read is returned; on error, both the
    /// error and the number of bytes read so far are returned.
    fn read_fallible(&mut self, writer: &mut VmWriter<'_, F>) -> Result<usize, (Error, usize)>;
}

/// Fallible memory write from a `VmReader`.
pub trait FallibleVmWrite<F> {
    /// Writes all data from the reader.
    fn write_fallible(&mut self, reader: &mut VmReader<'_, F>) -> Result<usize, (Error, usize)>;
}

/// A trait that enables reading/writing data from/to a VM object.
pub trait VmIo {
    /// Reads requested data at a specified offset into a given `VmWriter`.
    fn read(&self, offset: usize, writer: &mut VmWriter) -> Result<()>;

    /// Writes data from a given `VmReader` at a specified offset.
    fn write(&self, offset: usize, reader: &mut VmReader) -> Result<()>;

    /// Reads a specified number of bytes at a specified offset into a buffer.
    fn read_bytes(&self, offset: usize, buf: &mut [u8]) -> Result<()> {
        let mut writer = VmWriter::from(buf).to_fallible();
        self.read(offset, &mut writer)
    }

    /// Reads an initialized POD value through the object's byte-access contract.
    fn read_val<T: Pod>(&self, offset: usize) -> Result<T> {
        let mut value = T::new_zeroed();
        self.read_bytes(offset, value.as_mut_bytes())?;
        Ok(value)
    }
    /// Reads the entire typed slice, failing on a short transfer.
    fn read_slice<T: Pod>(&self, offset: usize, values: &mut [T]) -> Result<()> {
        self.read_bytes(offset, values.as_mut_bytes())
    }
    /// Writes a POD value without interpreting shared bytes as a Rust object.
    fn write_val<T: Pod>(&self, offset: usize, value: &T) -> Result<()> {
        self.write_bytes(offset, value.as_bytes())
    }
    /// Writes an entire POD slice.
    fn write_slice<T: Pod>(&self, offset: usize, values: &[T]) -> Result<()> {
        self.write_bytes(offset, values.as_bytes())
    }

    /// Writes a specified number of bytes from a buffer at a specified offset.
    fn write_bytes(&self, offset: usize, buf: &[u8]) -> Result<()> {
        let mut reader = VmReader::from(buf).to_fallible();
        self.write(offset, &mut reader)
    }
}

/// `VmReader` is a reader for reading data from a contiguous range of memory.
pub struct VmReader<'a, Fallibility = Fallible> {
    region: Region,
    phantom: PhantomData<(&'a [u8], Fallibility)>,
}

impl<Fallibility> Clone for VmReader<'_, Fallibility> {
    fn clone(&self) -> Self {
        Self {
            region: self.region,
            phantom: PhantomData,
        }
    }
}

impl<'a, Fallibility> VmReader<'a, Fallibility> {
    /// Constructs a reader over a run of host kernel virtual addresses.
    ///
    /// # Safety
    ///
    /// The range must be valid for reads for the whole lifetime `'a`.
    pub(crate) unsafe fn from_kernel_range(vaddr: usize, len: usize) -> Self {
        Self {
            region: Region::Kernel {
                cursor: vaddr,
                end: vaddr + len,
            },
            phantom: PhantomData,
        }
    }

    /// Returns the number of bytes for the remaining data.
    pub fn remain(&self) -> usize {
        self.region.remain()
    }

    /// Returns the current address in the original address space.
    pub fn cursor(&self) -> *const u8 {
        match self.region {
            Region::Kernel { cursor, .. } | Region::User { cursor, .. } => cursor as *const u8,
        }
    }

    /// Returns if it has remaining data to read.
    pub fn has_remain(&self) -> bool {
        self.remain() > 0
    }
}

impl<'a> VmReader<'a, Infallible> {
    /// Constructs a `VmReader` from a pointer and a length in kernel space.
    ///
    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len` bytes for the lifetime `'a`.
    pub unsafe fn from_kernel_space(ptr: *const u8, len: usize) -> Self {
        Self::from_kernel_range(ptr as usize, len)
    }

    /// Converts to a fallible reader.
    pub fn to_fallible(self) -> VmReader<'a, Fallible> {
        VmReader {
            region: self.region,
            phantom: PhantomData,
        }
    }
}

impl VmReader<'_, Fallible> {
    /// Constructs a reader over tenant memory, to be walked through `root`.
    ///
    /// # Safety
    ///
    /// `root` must be the root of a page table of one of this kernelet's spaces, and
    /// `vaddr..vaddr + len` must be a user-space range.
    pub(crate) unsafe fn from_space(root: Paddr, vaddr: Vaddr, len: usize) -> Self {
        Self {
            region: Region::User {
                root,
                cursor: vaddr,
                end: vaddr + len,
            },
            phantom: PhantomData,
        }
    }
}

impl<'a> From<&'a [u8]> for VmReader<'a, Infallible> {
    fn from(slice: &'a [u8]) -> Self {
        // SAFETY: the slice outlives `'a` and is plain-old-data.
        unsafe { Self::from_kernel_space(slice.as_ptr(), slice.len()) }
    }
}

/// `VmWriter` is a writer for writing data to a contiguous range of memory.
pub struct VmWriter<'a, Fallibility = Fallible> {
    region: Region,
    phantom: PhantomData<(&'a mut [u8], Fallibility)>,
}

impl<'a, Fallibility> VmWriter<'a, Fallibility> {
    /// Constructs a writer over a run of host kernel virtual addresses.
    ///
    /// # Safety
    ///
    /// The range must be valid for writes for the whole lifetime `'a`.
    pub(crate) unsafe fn from_kernel_range(vaddr: usize, len: usize) -> Self {
        Self {
            region: Region::Kernel {
                cursor: vaddr,
                end: vaddr + len,
            },
            phantom: PhantomData,
        }
    }

    /// Returns the number of bytes for the available space.
    pub fn avail(&self) -> usize {
        self.region.remain()
    }

    /// Returns the current address in the original address space.
    pub fn cursor(&self) -> *mut u8 {
        match self.region {
            Region::Kernel { cursor, .. } | Region::User { cursor, .. } => cursor as *mut u8,
        }
    }

    /// Returns if it has available space to write.
    pub fn has_avail(&self) -> bool {
        self.avail() > 0
    }
}

impl<'a> VmWriter<'a, Infallible> {
    /// Constructs a `VmWriter` from a pointer and a length in kernel space.
    ///
    /// # Safety
    ///
    /// `ptr` must be valid for writes of `len` bytes for the lifetime `'a`.
    pub unsafe fn from_kernel_space(ptr: *mut u8, len: usize) -> Self {
        Self::from_kernel_range(ptr as usize, len)
    }

    /// Converts to a fallible writer.
    pub fn to_fallible(self) -> VmWriter<'a, Fallible> {
        VmWriter {
            region: self.region,
            phantom: PhantomData,
        }
    }
}

impl VmWriter<'_, Fallible> {
    /// Constructs a writer over tenant memory, to be walked through `root`.
    ///
    /// # Safety
    ///
    /// As [`VmReader::from_space`].
    pub(crate) unsafe fn from_space(root: Paddr, vaddr: Vaddr, len: usize) -> Self {
        Self {
            region: Region::User {
                root,
                cursor: vaddr,
                end: vaddr + len,
            },
            phantom: PhantomData,
        }
    }
}

impl<'a> From<&'a mut [u8]> for VmWriter<'a, Infallible> {
    fn from(slice: &'a mut [u8]) -> Self {
        // SAFETY: the slice outlives `'a`, is exclusively borrowed and is
        // plain-old-data.
        unsafe { Self::from_kernel_space(slice.as_mut_ptr(), slice.len()) }
    }
}

macro_rules! impl_read_fallible {
    ($reader_fallibility:ty, $writer_fallibility:ty) => {
        impl FallibleVmRead<$writer_fallibility> for VmReader<'_, $reader_fallibility> {
            fn read_fallible(
                &mut self,
                writer: &mut VmWriter<'_, $writer_fallibility>,
            ) -> Result<usize, (Error, usize)> {
                let (copied, complete) = copy_regions(&mut writer.region, &mut self.region);
                if complete {
                    Ok(copied)
                } else {
                    Err((Error::PageFault, copied))
                }
            }
        }
    };
}

macro_rules! impl_write_fallible {
    ($writer_fallibility:ty, $reader_fallibility:ty) => {
        impl FallibleVmWrite<$reader_fallibility> for VmWriter<'_, $writer_fallibility> {
            fn write_fallible(
                &mut self,
                reader: &mut VmReader<'_, $reader_fallibility>,
            ) -> Result<usize, (Error, usize)> {
                reader.read_fallible(self)
            }
        }
    };
}

impl_read_fallible!(Fallible, Infallible);
impl_read_fallible!(Fallible, Fallible);
impl_read_fallible!(Infallible, Fallible);
impl_write_fallible!(Fallible, Infallible);
impl_write_fallible!(Fallible, Fallible);
impl_write_fallible!(Infallible, Fallible);

impl<F> VmReader<'_, F> {
    /// Narrows this reader's remaining range; it cannot enlarge authority.
    pub fn limit(&mut self, limit: usize) -> &mut Self {
        let n = limit.min(self.remain());
        match &mut self.region {
            Region::Kernel { cursor, end } | Region::User { cursor, end, .. } => *end = *cursor + n,
        }
        self
    }
    /// Advances within this reader's authorized range.
    pub fn skip(&mut self, n: usize) -> &mut Self {
        assert!(n <= self.remain(), "reader skip out of range");
        self.region.advance(n);
        self
    }
    /// Copies exactly one POD value. Failed partial copies never return a value.
    pub fn read_val<T: Pod>(&mut self) -> Result<T> {
        if self.remain() < core::mem::size_of::<T>() {
            return Err(Error::InvalidArgs);
        }
        let mut value = T::new_zeroed();
        let mut dst = VmWriter::from(value.as_mut_bytes());
        let original = self.region;
        let (_, complete) = copy_regions(&mut dst.region, &mut self.region);
        if complete {
            Ok(value)
        } else {
            self.region = original;
            Err(Error::PageFault)
        }
    }
}
impl<F> VmWriter<'_, F> {
    /// Narrows the remaining range without enlarging authority.
    pub fn limit(&mut self, limit: usize) -> &mut Self {
        let n = limit.min(self.avail());
        match &mut self.region {
            Region::Kernel { cursor, end } | Region::User { cursor, end, .. } => *end = *cursor + n,
        }
        self
    }
    /// Advances within this writer's authorized range.
    pub fn skip(&mut self, n: usize) -> &mut Self {
        assert!(n <= self.avail(), "writer skip out of range");
        self.region.advance(n);
        self
    }
    /// Copies exactly one initialized POD value using opaque byte accesses.
    pub fn write_val<T: Pod>(&mut self, value: &T) -> Result<()> {
        if self.avail() < core::mem::size_of::<T>() {
            return Err(Error::InvalidArgs);
        }
        let mut src = VmReader::from(value.as_bytes());
        let original = self.region;
        let (_, complete) = copy_regions(&mut self.region, &mut src.region);
        if complete {
            Ok(())
        } else {
            self.region = original;
            Err(Error::PageFault)
        }
    }
}
impl VmReader<'_, Infallible> {
    /// Transfers the bounded kernel range without materializing shared slices.
    pub fn read(&mut self, writer: &mut VmWriter<'_, Infallible>) -> usize {
        let (n, complete) = copy_regions(&mut writer.region, &mut self.region);
        assert!(complete, "infallible copy failed");
        n
    }
}
impl VmWriter<'_, Infallible> {
    /// Transfers the bounded kernel range without materializing shared slices.
    pub fn write(&mut self, reader: &mut VmReader<'_, Infallible>) -> usize {
        reader.read(self)
    }
    /// Fills at most the remaining capacity, using bounded opaque copies.
    pub fn fill_zeros(&mut self, len: usize) -> usize {
        let total = self.avail().min(len);
        let zeros = [0u8; 128];
        let mut done = 0;
        while done < total {
            let mut src = VmReader::from(&zeros[..(total - done).min(zeros.len())]);
            done += self.write(&mut src);
        }
        done
    }
}

/// POD that can be copied by a naturally aligned, single x86 instruction.
pub trait PodOnce: Pod {}
macro_rules! pod_once {($($t:ty),*)=>{$(impl PodOnce for $t {})*};}
pod_once!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);
pub trait VmIoOnce {
    fn read_once<T: PodOnce>(&self, offset: usize) -> Result<T>;
    fn write_once<T: PodOnce>(&self, offset: usize, value: &T) -> Result<()>;
}
pub trait VmIoFill {
    fn fill_zeros(&self, offset: usize, len: usize) -> Result<(), (Error, usize)>;
}
fn with_resolved<R>(
    region: &Region,
    write: bool,
    size: usize,
    f: impl FnOnce(*mut u8) -> R,
) -> Result<R> {
    if region.remain() < size {
        return Err(Error::InvalidArgs);
    }
    let mut callback = Some(f);
    for retry in 0..4 {
        let mut fault = None;
        let mut run = || {
            let Some((ptr, len)) = region.resolve(write) else {
                fault = region.fault(write);
                return Err(Error::PageFault);
            };
            if len < size {
                return Err(Error::PageFault);
            }
            assert!(
                (ptr as usize) % size == 0,
                "single access requires natural alignment"
            );
            Ok(callback.take().expect("memory operation executed twice")(
                ptr,
            ))
        };
        let result = if matches!(region, Region::User { .. }) {
            super::rcu::read(run)
        } else {
            run()
        };
        if result.is_ok() {
            return result;
        }
        if let Some((root, address, write, present)) = fault {
            if retry < 3 && super::fault::resolve(root, address, write, present) {
                continue;
            }
        }
        return result;
    }
    Err(Error::PageFault)
}
unsafe fn load_once(ptr: *const u8, n: usize) -> u64 {
    let value: u64;
    // SAFETY: caller retained/validated this aligned range. Assembly, rather
    // than Rust references or volatile memcpy, performs exactly one shared load.
    unsafe {
        match n {
            1 => {
                core::arch::asm!("movzx {v:e},byte ptr [{p}]",v=out(reg)value,p=in(reg)ptr,options(nostack,preserves_flags,readonly))
            }
            2 => {
                core::arch::asm!("movzx {v:e},word ptr [{p}]",v=out(reg)value,p=in(reg)ptr,options(nostack,preserves_flags,readonly))
            }
            4 => {
                core::arch::asm!("mov {v:e},dword ptr [{p}]",v=out(reg)value,p=in(reg)ptr,options(nostack,preserves_flags,readonly))
            }
            8 => {
                core::arch::asm!("mov {v},qword ptr [{p}]",v=out(reg)value,p=in(reg)ptr,options(nostack,preserves_flags,readonly))
            }
            _ => unreachable!("unsupported single access width"),
        }
    };
    value
}
unsafe fn store_once(ptr: *mut u8, n: usize, value: u64) {
    // SAFETY: as load_once, with write authority for this range.
    unsafe {
        match n {
            1 => {
                core::arch::asm!("mov byte ptr [{p}],{v}",v=in(reg_byte)value as u8,p=in(reg)ptr,options(nostack,preserves_flags))
            }
            2 => {
                core::arch::asm!("mov word ptr [{p}],{v:x}",v=in(reg)value as u16,p=in(reg)ptr,options(nostack,preserves_flags))
            }
            4 => {
                core::arch::asm!("mov dword ptr [{p}],{v:e}",v=in(reg)value as u32,p=in(reg)ptr,options(nostack,preserves_flags))
            }
            8 => {
                core::arch::asm!("mov qword ptr [{p}],{v}",v=in(reg)value,p=in(reg)ptr,options(nostack,preserves_flags))
            }
            _ => unreachable!("unsupported single access width"),
        }
    }
}
impl<F> VmReader<'_, F> {
    pub fn read_once<T: PodOnce>(&mut self) -> Result<T> {
        const {
            assert!(matches!(core::mem::size_of::<T>(), 1 | 2 | 4 | 8));
        }
        let n = core::mem::size_of::<T>();
        let raw = with_resolved(&self.region, false, n, |p| unsafe { load_once(p, n) })?;
        let value = T::from_bytes(&raw.to_ne_bytes()[..n]);
        self.region.advance(n);
        Ok(value)
    }
}
impl<F> VmWriter<'_, F> {
    pub fn write_once<T: PodOnce>(&mut self, value: &T) -> Result<()> {
        const {
            assert!(matches!(core::mem::size_of::<T>(), 1 | 2 | 4 | 8));
        }
        let n = core::mem::size_of::<T>();
        let mut bytes = [0u8; 8];
        bytes[..n].copy_from_slice(value.as_bytes());
        let raw = u64::from_ne_bytes(bytes);
        with_resolved(&self.region, true, n, |p| unsafe { store_once(p, n, raw) })?;
        self.region.advance(n);
        Ok(())
    }
    pub fn clone_exclusive(&mut self) -> VmWriter<'_, F> {
        VmWriter {
            region: self.region,
            phantom: PhantomData,
        }
    }
}
impl VmWriter<'_, Fallible> {
    pub fn fill_zeros(&mut self, len: usize) -> Result<usize, (Error, usize)> {
        let total = self.avail().min(len);
        let zeros = [0u8; 128];
        let mut done = 0;
        while done < total {
            let mut source = VmReader::from(&zeros[..(total - done).min(zeros.len())]);
            let (n, complete) = copy_regions(&mut self.region, &mut source.region);
            done += n;
            if !complete {
                return Err((Error::PageFault, done));
            }
        }
        Ok(done)
    }
}

/// POD supporting the user-space atomic operations required by futexes.
pub trait PodAtomic: Pod {
    /// # Safety
    /// Pointer must be aligned and valid kernel memory or a user address in the
    /// current task. User translation and recoverable faults are checked here.
    unsafe fn atomic_load_fallible(pointer: *const Self) -> Result<Self>;
    /// # Safety
    /// Same as atomic_load_fallible, with write permission for the target.
    unsafe fn atomic_cmpxchg_fallible(pointer: *mut Self, old: Self, new: Self) -> Result<Self>;
}
fn atomic_region(address: usize) -> Result<Region> {
    let end = address.checked_add(4).ok_or(Error::InvalidArgs)?;
    if address < super::MAX_USERSPACE_VADDR {
        if end > super::MAX_USERSPACE_VADDR {
            return Err(Error::PageFault);
        }
        let root = super::vm_space::current_root().ok_or(Error::PageFault)?;
        Ok(Region::User {
            root,
            cursor: address,
            end,
        })
    } else if super::KERNEL_VADDR_RANGE.contains(&address) {
        Ok(Region::Kernel {
            cursor: address,
            end,
        })
    } else {
        Err(Error::PageFault)
    }
}
unsafe fn cmpxchg_u32(pointer: *mut u8, old: u32, new: u32) -> u32 {
    let previous: u32;
    // SAFETY: caller retains and checks the writable, naturally aligned bytes.
    // The single locked instruction does not construct shared Rust references.
    unsafe {
        core::arch::asm!("lock cmpxchg dword ptr [{p}],{new:e}",
        p=in(reg)pointer,new=in(reg)new,inout("eax")old=>previous,options(nostack));
    }
    previous
}
impl PodAtomic for u32 {
    unsafe fn atomic_load_fallible(pointer: *const Self) -> Result<Self> {
        let region = atomic_region(pointer as usize)?;
        with_resolved(&region, false, 4, |p| unsafe { load_once(p, 4) as u32 })
    }
    unsafe fn atomic_cmpxchg_fallible(pointer: *mut Self, old: Self, new: Self) -> Result<Self> {
        let region = atomic_region(pointer as usize)?;
        with_resolved(&region, true, 4, |p| unsafe { cmpxchg_u32(p, old, new) })
    }
}
impl VmReader<'_, Fallible> {
    pub fn atomic_load<T: PodAtomic>(&self) -> Result<T> {
        const {
            assert!(core::mem::size_of::<T>() == 4);
        }
        let value = with_resolved(&self.region, false, 4, |p| unsafe {
            load_once(p, 4) as u32
        })?;
        Ok(T::from_bytes(&value.to_ne_bytes()))
    }
}
impl VmWriter<'_, Fallible> {
    pub fn atomic_compare_exchange<T: PodAtomic + Eq>(
        &self,
        reader: &VmReader,
        old: T,
        new: T,
    ) -> Result<(T, bool)> {
        const {
            assert!(core::mem::size_of::<T>() == 4);
        }
        if self.avail() < 4 || reader.remain() < 4 {
            return Err(Error::InvalidArgs);
        }
        let same = match (self.region, reader.region) {
            (Region::Kernel { cursor: a, .. }, Region::Kernel { cursor: b, .. }) => a == b,
            (
                Region::User {
                    root: a, cursor: x, ..
                },
                Region::User {
                    root: b, cursor: y, ..
                },
            ) => a == b && x == y,
            _ => false,
        };
        assert!(same, "atomic read and write must identify the same memory");
        let old_bits = u32::from_ne_bytes(old.as_bytes().try_into().unwrap());
        let new_bits = u32::from_ne_bytes(new.as_bytes().try_into().unwrap());
        let previous = with_resolved(&self.region, true, 4, |p| unsafe {
            cmpxchg_u32(p, old_bits, new_bits)
        })?;
        let value = T::from_bytes(&previous.to_ne_bytes());
        Ok((value, previous == old_bits))
    }
}
