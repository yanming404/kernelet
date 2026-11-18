// SPDX-License-Identifier: MPL-2.0
#[derive(typedio::Message)]
enum Bad {
    Good(u32),
    Hidden(Box<u8>),
}
fn main() {}
