// SPDX-License-Identifier: MPL-2.0
//! Boot information derived from the sandbox's authenticated Grant and image.

pub mod memory_region;
pub mod smp;

use crate::{log::LevelFilter, utils::Once};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use memory_region::{MemoryRegion, MemoryRegionType};

/// The boot information provided by the host.
pub struct BootInfo {
    /// Identifies the platform, not a hardware firmware implementation.
    pub bootloader_name: String,
    /// Kernel command line passed through KLET_CREATE.
    pub kernel_cmdline: String,
    /// Optional immutable initramfs embedded in the private image build.
    pub initramfs: Option<&'static [u8]>,
    /// No host framebuffer is exposed by the current service ABI.
    pub framebuffer_arg: Option<BootloaderFramebufferArg>,
    /// Only this sandbox's Grant is described as available physical memory.
    pub memory_regions: Vec<MemoryRegion>,
}
/// Framebuffer description, when supported by a future authorized platform.
#[derive(Clone, Copy, Debug)]
pub struct BootloaderFramebufferArg {
    pub address: usize,
    pub width: usize,
    pub height: usize,
    pub bpp: usize,
}
/// Early parameters consumed before kernel components are initialized.
pub struct EarlyCmdline {
    pub log_level: LevelFilter,
    pub has_early_console: bool,
}
#[linkage = "weak"]
#[no_mangle]
fn __early_cmdline_parser(_cmdline: &str) -> EarlyCmdline {
    EarlyCmdline {
        log_level: LevelFilter::Debug,
        has_early_console: true,
    }
}
/// Image-supplied archive. Weak default preserves existing prototype images.
#[linkage = "weak"]
#[no_mangle]
fn __kernelet_initramfs() -> &'static [u8] {
    &[]
}

static INFO: Once<BootInfo> = Once::new();
/// Performs explicit registry construction once, on the BSP, before parsing
/// registered kernel parameters. No host loader runs these callbacks.
pub(crate) unsafe fn register_image_components() {
    extern "C" {
        static __kernelet_registrations_start: u8;
        static __kernelet_registrations_end: u8;
    }
    let start = core::ptr::addr_of!(__kernelet_registrations_start) as usize;
    let end = core::ptr::addr_of!(__kernelet_registrations_end) as usize;
    let size = core::mem::size_of::<unsafe extern "C" fn()>();
    assert!(end >= start && start % size == 0 && (end - start) % size == 0);
    // SAFETY: the final link retains only trusted registration function pointers
    // in this private section. The loader validates their RELATIVE relocations.
    for register in
        core::slice::from_raw_parts(start as *const unsafe extern "C" fn(), (end - start) / size)
    {
        register();
    }
}
/// Gets the boot information after the platform heap is available.
pub fn boot_info() -> &'static BootInfo {
    INFO.get()
        .expect("platform boot information not initialized")
}

pub(crate) fn init(cmdline: &str, grant_paddr: usize, grant_len: usize) {
    let grant = MemoryRegion::new(grant_paddr, grant_len, MemoryRegionType::Usable);
    let archive = __kernelet_initramfs();
    INFO.call_once(|| BootInfo {
        bootloader_name: "Kernelet".to_string(),
        kernel_cmdline: cmdline.to_string(),
        initramfs: if archive.is_empty() {
            None
        } else {
            Some(archive)
        },
        framebuffer_arg: None,
        memory_regions: alloc::vec![grant],
    });
    crate::log::init(&__early_cmdline_parser(cmdline));
}
