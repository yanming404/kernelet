// SPDX-License-Identifier: MPL-2.0
#[derive(typedio::Message)]
struct Bad {
    field: fn(),
}
fn main() {}
