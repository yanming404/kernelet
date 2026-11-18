// SPDX-License-Identifier: MPL-2.0
#[derive(Clone, Copy)]
struct Unchecked(u32);
#[derive(typedio::Message)]
struct Bad {
    field: Unchecked,
}
fn main() {}
