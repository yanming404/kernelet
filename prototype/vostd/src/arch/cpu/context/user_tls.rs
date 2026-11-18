// SPDX-License-Identifier: MPL-2.0
//! User segment bases stored in the actual Linux carrier's CPU/task state.
use crate::irq::DisabledLocalIrqGuard;
fn access(which: u64, operation: u64, value: &mut u64) {
    // SAFETY: this initialized scalar remains alive across the synchronous
    // service; the host validates the operation/address and retains no pointer.
    let result = unsafe { (crate::abi::svc().user_tls)(which, operation, value) };
    assert_eq!(result, 0, "Host user TLS operation failed");
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FsBase(usize);
impl FsBase {
    pub fn new(address: usize) -> Self {
        Self(address)
    }
    pub fn addr(&self) -> usize {
        self.0
    }
    pub fn save(&mut self) {
        let mut v = 0;
        access(0, 0, &mut v);
        self.0 = v as usize;
    }
    pub fn load(&self) {
        let mut v = self.0 as u64;
        access(0, 1, &mut v);
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct GsBase(usize);
impl GsBase {
    pub fn new(address: usize) -> Self {
        Self(address)
    }
    pub fn addr(&self) -> usize {
        self.0
    }
    pub fn save(&mut self, _guard: &DisabledLocalIrqGuard) {
        let mut v = 0;
        access(1, 0, &mut v);
        self.0 = v as usize;
    }
    pub fn load(&self, _guard: &DisabledLocalIrqGuard) {
        let mut v = self.0 as u64;
        access(1, 1, &mut v);
    }
}
