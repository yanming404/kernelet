// SPDX-License-Identifier: MPL-2.0
//! Regression for nested/packed move-only cleanup receipts after transfer.
use core::sync::atomic::{AtomicUsize, Ordering};
use typedio::{BoundedVec, Message, SchemaDigest};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Leaf(bool);
impl Drop for Leaf {
    fn drop(&mut self) {
        if self.0 {
            DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }
}
// SAFETY: this test leaf owns a counted cleanup receipt with no external memory.
unsafe impl Message for Leaf {
    const SCHEMA: SchemaDigest = SchemaDigest::of("test.cleanup", &[], &[]);
    fn __committed(&mut self) {
        self.0 = false;
    }
}
#[derive(Message)]
#[repr(C, packed)]
struct Packed {
    tag: u8,
    value: Option<Result<[(Leaf,); 2], Leaf>>,
}
#[derive(Message)]
enum Nested {
    Value(Packed, BoundedVec<Leaf, 2>),
    Empty,
}
#[test]
fn transfer_disarms_every_active_field_including_packed_and_inline_containers() {
    let before = DROPS.load(Ordering::SeqCst);
    let list = BoundedVec::try_from([Leaf(true), Leaf(true)]).ok().unwrap();
    let mut message = Nested::Value(
        Packed {
            tag: 1,
            value: Some(Ok([(Leaf(true),), (Leaf(true),)])),
        },
        list,
    );
    message.__committed();
    drop(message);
    assert_eq!(DROPS.load(Ordering::SeqCst), before);
    let rejected = Packed {
        tag: 0,
        value: Some(Err(Leaf(true))),
    };
    drop(rejected);
    assert_eq!(DROPS.load(Ordering::SeqCst), before + 1);
}
