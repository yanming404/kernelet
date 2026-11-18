// SPDX-License-Identifier: MPL-2.0
#[derive(typedio::Message)]
union Bad {
    a: u32,
    b: f32,
}
fn main() {}
