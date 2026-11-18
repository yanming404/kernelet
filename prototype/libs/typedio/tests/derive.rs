// SPDX-License-Identifier: MPL-2.0
#[test]
fn rejected_message_types() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
