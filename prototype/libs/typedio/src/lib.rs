// SPDX-License-Identifier: MPL-2.0

//! Shared TypedIO contracts and bounded state machines.
//!
//! This crate has no allocator or OS dependency. The same code can be built
//! into vOSTD and the Linux backend; host-native tests exercise these mechanisms,
//! not a replacement userspace backend.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate self as typedio;

include!("api.rs");
