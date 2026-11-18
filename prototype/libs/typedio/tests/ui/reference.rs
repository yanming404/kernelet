// SPDX-License-Identifier: MPL-2.0
#[derive(typedio::Message)]
struct Bad {
    field: &'static u8,
}
fn main() {}
