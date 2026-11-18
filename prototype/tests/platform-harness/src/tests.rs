// SPDX-License-Identifier: MPL-2.0
use crate::{
    mm::{io::*, memory_region::*},
    sync::*,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[test]
fn locks_preserve_exclusive_updates_and_upgrade() {
    let value = Arc::new(Mutex::new(0usize));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let value = value.clone();
            scope.spawn(move || {
                for _ in 0..1000 {
                    *value.lock() += 1;
                }
            });
        }
    });
    assert_eq!(*value.lock(), 4000);
    let value = RwMutex::new(1);
    let read = value.upread();
    let mut write = read.upgrade();
    *write = 2;
    drop(write);
    assert_eq!(*value.read(), 2);
    let value = RwLock::<usize>::new(7);
    let read = value.upread();
    let mut write = read.upgrade();
    *write = 8;
    drop(write);
    assert_eq!(*value.read(), 8);
    let mut value = RwArc::new(9usize);
    let ro = value.clone_ro();
    *value.write() = 10;
    assert_eq!(*ro.read(), 10);
    drop(ro);
    assert_eq!(*value.get().unwrap(), 10);
}
#[test]
fn rcu_defers_drop_until_every_cpu_reports_quiescence() {
    let _config = CONFIG.lock().unwrap();
    crate::cpu::set_count(2);
    crate::cpu::select(0);
    crate::sync::init();
    struct Tracked(Arc<AtomicUsize>);
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let value = RcuOption::new(Some(Arc::new(Tracked(drops.clone()))));
    let reader = value.read();
    value.update(Some(Arc::new(Tracked(drops.clone()))));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(reader);
    crate::sync::quiescent();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    crate::cpu::select(1);
    crate::sync::quiescent();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(value);
    crate::sync::quiescent();
    crate::cpu::select(0);
    crate::sync::quiescent();
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}
#[test]
fn vmio_bounds_pod_and_partial_progress() {
    let mut bytes = [0u8; 8];
    let mut w = VmWriter::from(bytes.as_mut_slice());
    w.write_val(&0x12345678u32).unwrap();
    assert_eq!(w.avail(), 4);
    w.limit(2);
    assert!(w.write_val(&0u32).is_err());
    assert_eq!(w.avail(), 2);
    assert_eq!(w.fill_zeros(999), 2);
    let mut r = VmReader::from(bytes.as_slice());
    assert_eq!(r.read_val::<u32>().unwrap(), 0x12345678);
    r.skip(2);
    assert!(r.read_val::<u32>().is_err());
    let mut invalid = unsafe { VmReader::from_space(1, 0x1000, 4) };
    assert_eq!(invalid.read_val::<u32>(), Err(crate::Error::PageFault));
    assert_eq!(invalid.remain(), 4);
}
#[test]
fn boot_memory_regions_reject_overflow_and_misalignment() {
    let r = MemoryRegion::new(0x2000, 0x4000, MemoryRegionType::Usable);
    assert_eq!(r.end(), 0x6000);
    assert!(r.typ().is_physical());
    assert!(std::panic::catch_unwind(|| MemoryRegion::new(
        usize::MAX - 4095,
        4096,
        MemoryRegionType::Usable
    ))
    .is_err());
    assert!(
        std::panic::catch_unwind(|| MemoryRegion::new(1, 4096, MemoryRegionType::Usable)).is_err()
    );
}

static CONFIG: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[test]
fn dynamic_cpu_local_ownership_and_slot_retirement() {
    use crate::cpu::local::DynCpuLocalChunk;
    let _config = CONFIG.lock().unwrap();
    crate::cpu::set_count(2);
    crate::cpu::select(0);
    let drops = Arc::new(AtomicUsize::new(0));
    struct Tracked(usize, Arc<AtomicUsize>);
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }
    let mut a = DynCpuLocalChunk::<4096>::new().unwrap();
    let mut b = DynCpuLocalChunk::<4096>::new().unwrap();
    let value = a
        .alloc(&mut |cpu| Tracked(cpu.as_usize(), drops.clone()))
        .unwrap();
    assert!(a.is_full());
    assert!(!a.is_empty());
    assert!(a.alloc(&mut |_| 0usize).is_none());
    {
        let pin = crate::task::disable_preempt();
        assert_eq!(value.get_with(&pin).0, 0);
    }
    let value = match b.try_dealloc(value) {
        Err(value) => value,
        Ok(()) => panic!("foreign chunk accepted"),
    };
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(a.try_dealloc(value).is_ok());
    assert!(a.is_empty());
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}

#[test]
fn frame_metadata_is_retained_and_dropped_before_backing_reuse() {
    use crate::mm::{
        frame::{allocator, untyped::AnyUFrameMeta, AnyFrameMeta},
        HasPaddr,
    };
    let _config = CONFIG.lock().unwrap();
    struct NativeGrant(*mut u8, std::alloc::Layout);
    impl Drop for NativeGrant {
        fn drop(&mut self) {
            unsafe { std::alloc::dealloc(self.0, self.1) }
        }
    }
    let layout = std::alloc::Layout::from_size_align(4 * 4096, 4096).unwrap();
    let backing = NativeGrant(unsafe { std::alloc::alloc_zeroed(layout) }, layout);
    assert!(!backing.0.is_null());
    allocator::init(backing.0 as usize, layout.size());
    let drops = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AtomicUsize::new(0));
    struct Meta {
        drops: Arc<AtomicUsize>,
        hooks: Arc<AtomicUsize>,
        tag: usize,
    }
    unsafe impl AnyFrameMeta for Meta {
        fn is_untyped(&self) -> bool {
            true
        }
        fn on_drop(&mut self, reader: &mut VmReader<Infallible>) {
            assert_eq!(reader.read_val::<u8>().unwrap(), 0);
            assert!(allocator::frames_in_use() > 0);
            self.hooks.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl AnyUFrameMeta for Meta {}
    impl Drop for Meta {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    let options = allocator::FrameAllocOptions::new();
    let frame = options
        .alloc_frame_with(Meta {
            drops: drops.clone(),
            hooks: hooks.clone(),
            tag: 37,
        })
        .unwrap();
    assert_eq!(frame.meta().tag, 37);
    let address = frame.paddr();
    let other = frame.clone();
    drop(frame);
    assert_eq!(other.meta().tag, 37);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    use crate::sync::non_null::NonNullPtr;
    type Owned = crate::mm::frame::Frame<Meta>;
    let identity = <Owned as NonNullPtr>::into_raw(other);
    assert_eq!(identity.as_ptr() as usize % 64, 0);
    assert_ne!(identity.as_ptr() as usize, address);
    let borrowed = unsafe { <Owned as NonNullPtr>::raw_as_ref(identity) };
    assert_eq!(borrowed.meta().tag, 37);
    assert_eq!(<Owned as NonNullPtr>::ref_as_raw(borrowed), identity);
    let other = unsafe { <Owned as NonNullPtr>::from_raw(identity) };
    assert_eq!(other.reference_count(), 1);
    drop(other);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let frame = options.alloc_frame().unwrap();
    assert_eq!(frame.paddr(), address);
    drop(frame);
    let mut invoked = 0;
    let segment = options
        .alloc_segment_with(3, |_| {
            invoked += 1;
            Meta {
                drops: drops.clone(),
                hooks: hooks.clone(),
                tag: invoked,
            }
        })
        .unwrap();
    assert_eq!(invoked, 3);
    let frames: Vec<_> = segment.collect();
    assert_eq!(frames[2].meta().tag, 3);
    drop(frames);
    assert_eq!(drops.load(Ordering::SeqCst), 4);
    assert_eq!(hooks.load(Ordering::SeqCst), 4);
    assert_eq!(allocator::frames_in_use(), 0);
}

#[test]
fn tls_wrappers_read_live_state_instead_of_caching_last_write() {
    use crate::user_tls::{FsBase, GsBase};
    let fs = FsBase::new(0x1000);
    fs.load();
    let mut read_fs = FsBase::default();
    read_fs.save();
    assert_eq!(read_fs.addr(), 0x1000);
    crate::abi::simulate_userspace_fsgsbase(0, 0x3000);
    read_fs.save();
    assert_eq!(read_fs.addr(), 0x3000);
    let guard = crate::task::disable_preempt();
    let gs = GsBase::new(0x2000);
    gs.load(&guard);
    let mut read_gs = GsBase::default();
    read_gs.save(&guard);
    assert_eq!(read_gs.addr(), 0x2000);
    crate::abi::simulate_userspace_fsgsbase(1, 0x4000);
    read_gs.save(&guard);
    assert_eq!(read_gs.addr(), 0x4000);
    read_fs.save();
    assert_eq!(read_fs.addr(), 0x3000);

    // A real WRFSBASE/WRGSBASE may establish a high-half canonical base.
    // Save and restore must preserve the exact bits across a logical switch.
    crate::abi::simulate_userspace_fsgsbase(0, 0xffff_8000_0000_1234);
    crate::abi::simulate_userspace_fsgsbase(1, 0xffff_ffff_ffff_f000);
    read_fs.save();
    read_gs.save(&guard);
    FsBase::new(0x7000).load();
    GsBase::new(0x8000).load(&guard);
    read_fs.load();
    read_gs.load(&guard);
    let mut restored_fs = FsBase::default();
    let mut restored_gs = GsBase::default();
    restored_fs.save();
    restored_gs.save(&guard);
    assert_eq!(restored_fs.addr(), 0xffff_8000_0000_1234);
    assert_eq!(restored_gs.addr(), 0xffff_ffff_ffff_f000);
}

#[test]
fn pod_partial_fault_rolls_cursor_back_and_once_is_nonadvancing_on_error() {
    #[repr(align(4096))]
    struct Page([u8; 4096]);
    let mut page = Box::new(Page([0; 4096]));
    page.0[4094..].copy_from_slice(&[0x12, 0x34]);
    crate::mm::page_table::map(73, 0x1000, page.0.as_mut_ptr() as usize);
    let mut reader = unsafe { VmReader::from_space(73, 0x1ffe, 4) };
    assert_eq!(reader.read_val::<u32>(), Err(crate::Error::PageFault));
    assert_eq!(reader.remain(), 4);
    let mut writer = unsafe { VmWriter::from_space(73, 0x1ffe, 4) };
    assert_eq!(
        writer.write_val(&0xaaaabbbbu32),
        Err(crate::Error::PageFault)
    );
    assert_eq!(writer.avail(), 4);
    assert_eq!(&page.0[4094..], &[0xbb, 0xbb]);
    let mut writer = VmWriter::from(page.0.as_mut_slice());
    writer.write_once(&0x11223344u32).unwrap();
    let mut reader = VmReader::from(page.0.as_slice());
    assert_eq!(reader.read_once::<u32>().unwrap(), 0x11223344);
    assert_eq!(reader.remain(), 4092);
    let mut absent = unsafe { VmReader::from_space(73, 0x2000, 4) };
    assert_eq!(absent.read_once::<u32>(), Err(crate::Error::PageFault));
    assert_eq!(absent.remain(), 4);
    crate::mm::page_table::clear();
}

#[test]
fn dma_separate_views_sync_actual_bytes_and_enforce_direction() {
    use crate::mm::{
        dma::*, frame::allocator, io::util::HasVmReaderWriter, FrameAllocOptions, HasDaddr,
        HasPaddr, HasSize, Split,
    };
    let _config = CONFIG.lock().unwrap();
    struct Grant(*mut u8, std::alloc::Layout);
    impl Drop for Grant {
        fn drop(&mut self) {
            unsafe { std::alloc::dealloc(self.0, self.1) }
        }
    }
    let layout = std::alloc::Layout::from_size_align(16 * 4096, 4096).unwrap();
    let grant = Grant(unsafe { std::alloc::alloc_zeroed(layout) }, layout);
    assert!(!grant.0.is_null());
    allocator::init(grant.0 as usize, layout.size());
    let dma = DmaStream::<FromAndToDevice>::alloc(2, false).unwrap();
    assert_ne!(dma.paddr(), dma.daddr());
    dma.writer()
        .unwrap()
        .skip(32)
        .write_val(&0x11223344u32)
        .unwrap();
    let mut device = unsafe { VmReader::from_kernel_space(dma.daddr() as *const u8, dma.size()) };
    assert_eq!(device.skip(32).read_val::<u32>().unwrap(), 0);
    dma.sync_to_device(32..36).unwrap();
    let mut device = unsafe { VmReader::from_kernel_space(dma.daddr() as *const u8, dma.size()) };
    assert_eq!(device.skip(32).read_val::<u32>().unwrap(), 0x11223344);
    let mut device = unsafe { VmWriter::from_kernel_space(dma.daddr() as *mut u8, dma.size()) };
    device.skip(32).write_val(&0x55667788u32).unwrap();
    dma.sync_from_device(32..36).unwrap();
    assert_eq!(
        dma.reader().unwrap().skip(32).read_val::<u32>().unwrap(),
        0x55667788
    );
    assert_eq!(dma.sync_from_device(40..32), Err(crate::Error::InvalidArgs));
    assert_eq!(
        dma.sync_to_device(0..dma.size() + 1),
        Err(crate::Error::InvalidArgs)
    );
    let (a, b) = dma.split(4096);
    assert_eq!(a.size(), 4096);
    assert_eq!(b.size(), 4096);
    drop((a, b));
    let stream = DmaStream::<ToDevice>::alloc(1, true).unwrap();
    assert!(matches!(stream.reader(), Err(crate::Error::AccessDenied)));
    assert_eq!(
        stream.sync_from_device(0..1),
        Err(crate::Error::AccessDenied)
    );
    drop(stream);
    let source = FrameAllocOptions::new().alloc_segment(1).unwrap();
    let clone = source.clone();
    let stream = DmaStream::<FromDevice>::map(clone.into(), true).unwrap();
    assert!(stream.writer().is_err());
    assert_eq!(stream.daddr(), source.paddr());
    drop(stream);
    drop(source);
    assert!(matches!(
        DmaCoherent::alloc(1, false),
        Err(crate::Error::AccessDenied)
    ));
    let coherent = DmaCoherent::alloc(1, true).unwrap();
    assert_eq!(coherent.paddr(), coherent.daddr());
    drop(coherent);
    assert_eq!(allocator::frames_in_use(), 0);
    assert_eq!(allocator::free_bytes(), layout.size());
}

#[test]
fn recoverable_faults_check_current_root_and_leave_atomic_context_before_handler() {
    use crate::mm::{fault, page_table, vm_space};
    use std::cell::Cell;
    thread_local! {
        static TARGET: Cell<usize> = const { Cell::new(0) };
        static CALLS: Cell<usize> = const { Cell::new(0) };
    }
    fn handler(exception: &crate::arch::cpu::context::CpuException) -> Result<(), ()> {
        crate::task::assert_quiescent();
        let crate::arch::cpu::context::CpuException::PageFault(info) = exception;
        assert_eq!(info.error_code.bits(), 0);
        CALLS.with(|c| c.set(c.get() + 1));
        if info.addr == 0x5000 {
            page_table::map(99, 0x5000, TARGET.with(Cell::get));
        }
        Ok(())
    }
    #[repr(align(4096))]
    struct Page([u8; 4096]);
    let mut page = Box::new(Page([0; 4096]));
    page.0[..4].copy_from_slice(&37u32.to_ne_bytes());
    TARGET.with(|t| t.set(page.0.as_mut_ptr() as usize));
    fault::inject_user_page_fault_handler(handler);
    vm_space::activate_for_test(Some(99));
    assert!(!fault::resolve(100, 0x5000, false, false));
    assert!(!fault::resolve(
        99,
        crate::mm::MAX_USERSPACE_VADDR,
        false,
        false
    ));
    {
        let _guard = crate::task::disable_preempt();
        assert!(!fault::resolve(99, 0x5000, false, false));
    }
    assert_eq!(CALLS.with(Cell::get), 0);
    let mut reader = unsafe { VmReader::from_space(99, 0x5000, 4) };
    assert_eq!(reader.read_val::<u32>().unwrap(), 37);
    assert_eq!(CALLS.with(Cell::get), 1);
    assert_eq!(reader.cursor() as usize, 0x5004);
    let reader = unsafe { VmReader::from_space(99, 0x5000, 4) };
    let writer = unsafe { VmWriter::from_space(99, 0x5000, 4) };
    assert_eq!(reader.atomic_load::<u32>().unwrap(), 37);
    assert_eq!(
        writer
            .atomic_compare_exchange(&reader, 36u32, 51u32)
            .unwrap(),
        (37, false)
    );
    assert_eq!(
        writer
            .atomic_compare_exchange(&reader, 37u32, 51u32)
            .unwrap(),
        (37, true)
    );
    assert_eq!(reader.atomic_load::<u32>().unwrap(), 51);
    assert_eq!((reader.remain(), writer.avail()), (4, 4));
    assert_eq!(u32::from_ne_bytes(page.0[..4].try_into().unwrap()), 51);
    assert_eq!(
        unsafe { <u32 as PodAtomic>::atomic_load_fallible(0x5000 as *const u32) }.unwrap(),
        51
    );
    assert_eq!(
        unsafe { <u32 as PodAtomic>::atomic_cmpxchg_fallible(0x5000 as *mut u32, 51, 71) }.unwrap(),
        51
    );
    let absent = unsafe { VmReader::from_space(99, 0x6000, 4) };
    assert_eq!(absent.atomic_load::<u32>(), Err(crate::Error::PageFault));
    assert_eq!(CALLS.with(Cell::get), 4); // Three bounded recovery attempts.
    vm_space::activate_for_test(None);
    page_table::clear();
}

struct RegisteredValue(usize);
inventory::collect!(RegisteredValue);
kernelet_registry::submit!(RegisteredValue(17));
kernelet_registry::submit!(RegisteredValue(25));

#[test]
fn registrations_are_explicit_and_reach_the_original_inventory() {
    // The native ELF loader must not run these registrations automatically.
    assert_eq!(inventory::iter::<RegisteredValue>.into_iter().count(), 0);
    unsafe extern "C" {
        static __start_kernelet_registrations: u8;
        static __stop_kernelet_registrations: u8;
    }
    let start = core::ptr::addr_of!(__start_kernelet_registrations) as usize;
    let end = core::ptr::addr_of!(__stop_kernelet_registrations) as usize;
    assert_eq!(
        end - start,
        2 * core::mem::size_of::<unsafe extern "C" fn()>()
    );
    for entry in unsafe { core::slice::from_raw_parts(start as *const unsafe extern "C" fn(), 2) } {
        unsafe { entry() };
    }
    let mut values: Vec<_> = inventory::iter::<RegisteredValue>
        .into_iter()
        .map(|v| v.0)
        .collect();
    values.sort();
    assert_eq!(values, [17, 25]);
}

#[test]
fn actual_asterinas_cpu_sync_keeps_threads_separate_across_native_carriers() {
    use crate::{
        cpu_sync::CpuSync,
        user_tls::{FsBase, GsBase},
    };
    let a_fs = CpuSync::new(FsBase::new(0x1000));
    let a_gs = CpuSync::new(GsBase::new(0x2000));
    let b_fs = CpuSync::new(FsBase::new(0x3000));
    let b_gs = CpuSync::new(GsBase::new(0x4000));
    let guard = crate::irq::disable_local();
    a_fs.before_user_exec(&guard);
    a_gs.before_user_exec(&guard);
    crate::abi::simulate_userspace_fsgsbase(0, 0xffff_8000_0000_5678);
    crate::abi::simulate_userspace_fsgsbase(1, 0xffff_ffff_ffff_f234);
    a_fs.before_schedule(&guard);
    a_gs.before_schedule(&guard);
    b_fs.before_user_exec(&guard);
    b_gs.before_user_exec(&guard);
    assert_eq!(b_fs.get().addr(), 0x3000);
    assert_eq!(b_gs.get().addr(), 0x4000);
    b_fs.set(FsBase::new(0x6000));
    b_fs.before_schedule(&guard); // New memory state must survive without a save.
    b_gs.before_schedule(&guard);
    drop(guard);
    std::thread::spawn(move || {
        let guard = crate::irq::disable_local();
        a_fs.before_user_exec(&guard);
        a_gs.before_user_exec(&guard);
        assert_eq!(a_fs.get().addr(), 0xffff_8000_0000_5678);
        assert_eq!(a_gs.get().addr(), 0xffff_ffff_ffff_f234);
        a_fs.before_schedule(&guard);
        a_gs.before_schedule(&guard);
        b_fs.before_user_exec(&guard);
        b_gs.before_user_exec(&guard);
        assert_eq!(b_fs.get().addr(), 0x6000);
        assert_eq!(b_gs.get().addr(), 0x4000);
    })
    .join()
    .unwrap();
}
