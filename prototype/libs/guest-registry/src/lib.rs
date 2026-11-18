// SPDX-License-Identifier: MPL-2.0
//! Explicit, instance-local inventory registration during guest platform boot.
//!
//! This does not invoke component initialization. The original component stage
//! and dependency ordering still select and invoke those functions afterwards.
#![no_std]

#[doc(hidden)]
pub use inventory as __inventory;

/// Registers a value in the existing inventory when the BSP boots the image.
/// The resulting function table is ordinary private, relocated image data.
#[macro_export]
macro_rules! submit {
    ($($value:tt)*) => {
        const _: () = {
            static NODE: $crate::__inventory::Node = $crate::__inventory::Node {
                value: &{ $($value)* },
                next: ::core::cell::UnsafeCell::new(None),
            };
            unsafe extern "C" fn register() {
                // SAFETY: this node's erased value and registry have the same
                // type, and boot invokes each table entry exactly once.
                unsafe { $crate::__inventory::ErasedNode::submit(NODE.value, &NODE) }
            }
            #[used]
            #[link_section = "kernelet_registrations"]
            static REGISTER: unsafe extern "C" fn() = register;
        };
    };
}
