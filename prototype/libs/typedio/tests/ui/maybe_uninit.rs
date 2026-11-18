// SPDX-License-Identifier: MPL-2.0
#[derive(typedio::Message)]
struct Bad {
    field: core::mem::MaybeUninit<u32>,
}
fn main() {}
