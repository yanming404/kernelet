// SPDX-License-Identifier: GPL-2.0
//! The exact shared protocol/transfer crate, built with Linux's core/sysroot.
extern crate self as typedio;
#[cfg(not(feature = "native-host"))]
use kernel as _;
#[path = "../../libs/typedio/src/api.rs"]
mod sdk;
pub use sdk::*;

mod typedio_host_api;
use typedio_host_api as host_api;
mod typedio_vsock;

mod typedio_host_socket;
