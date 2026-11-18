// SPDX-License-Identifier: MPL-2.0
//! Platform binding for the shared typed queue and byte-pipe frontend.
use crate::irq::IrqLine;
use ::typedio::{notification::Binding, service::Call};
#[path = "../../libs/typedio/src/frontend.rs"]
mod common;
pub use common::*;
pub(crate) fn invoke(call: &mut Call) -> i64 {
    // SAFETY: the image ABI is version-checked at bootstrap; this synchronous
    // service captures metadata and retains no pointer into this stack.
    unsafe { (crate::abi::svc().typedio)(call) }
}
fn irq_route(line: &IrqLine) -> Result<Binding, BindError> {
    Ok(Binding {
        line: line.num() as u32,
        generation: line.generation(),
        vcpu: line.target_vcpu(),
    })
}
