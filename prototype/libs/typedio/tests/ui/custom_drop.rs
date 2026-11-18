// SPDX-License-Identifier: MPL-2.0
#[derive(typedio::Message)]
struct Bad {
    field: u32,
}
impl Drop for Bad {
    fn drop(&mut self) {}
}
fn main() {}
