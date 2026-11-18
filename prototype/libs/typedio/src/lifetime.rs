// SPDX-License-Identifier: MPL-2.0

//! Host-owned lifecycle decisions, independent of guest destructors.
//! No method frees memory: the wrapper releases backing only after `release`
//! succeeds while holding the admission/teardown lock.

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Prepared,
    Active,
    Retired,
    Detached,
    Released,
    Quarantine,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    State,
    Overflow,
    Busy,
    Quota,
}

pub struct Retention {
    state: State,
    accesses: u32,
    backend_owned: bool,
    finish_requested: bool,
    detached: bool,
    carriers_evicted: bool,
    mappings_gone: bool,
}
impl Retention {
    pub const fn prepared() -> Self {
        Self {
            state: State::Prepared,
            accesses: 0,
            backend_owned: false,
            finish_requested: false,
            detached: false,
            carriers_evicted: false,
            mappings_gone: false,
        }
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn accept(&mut self) -> Result<(), Error> {
        if self.state != State::Prepared {
            return Err(Error::State);
        }
        self.backend_owned = true;
        self.state = State::Active;
        Ok(())
    }
    pub fn begin_access(&mut self) -> Result<(), Error> {
        if self.detached || self.finish_requested || self.state != State::Active {
            return Err(Error::State);
        }
        self.accesses = self.accesses.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub fn end_access(&mut self) -> Result<(), Error> {
        self.accesses = self.accesses.checked_sub(1).ok_or(Error::State)?;
        self.retire_if_quiescent();
        Ok(())
    }
    /// Consumes backend completion authority, fences new accesses, and waits
    /// for registered asynchronous accesses. Cancellation alone never calls it.
    pub fn finish(&mut self) -> Result<(), Error> {
        if !self.backend_owned {
            return Err(Error::State);
        }
        self.backend_owned = false;
        self.finish_requested = true;
        self.retire_if_quiescent();
        Ok(())
    }
    fn retire_if_quiescent(&mut self) {
        if self.finish_requested && self.accesses == 0 && !self.detached {
            self.state = State::Retired;
        }
    }
    pub fn detach(&mut self) {
        if self.state == State::Released {
            return;
        }
        self.detached = true;
        self.state = State::Detached;
    }
    pub fn carrier_eviction_complete(&mut self) {
        self.carriers_evicted = true;
    }
    pub fn mapping_teardown_complete(&mut self) {
        self.mappings_gone = true;
    }
    pub fn quarantine(&mut self) {
        if self.state != State::Released {
            self.state = State::Quarantine;
        }
    }
    pub fn release(&mut self) -> Result<(), Error> {
        if self.state == State::Released {
            return Err(Error::State);
        }
        let ready = if self.detached {
            !self.backend_owned && self.accesses == 0 && self.carriers_evicted && self.mappings_gone
        } else {
            self.state == State::Prepared || (self.finish_requested && self.accesses == 0)
        };
        if !ready {
            return Err(Error::Busy);
        }
        self.state = State::Released;
        Ok(())
    }
}

/// Charge full grant extents, not merely the submitted subrange. Shared pipe
/// charges have one designated attachment owner and remain until reclamation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Charge {
    pub bytes: u64,
    pub requests: u64,
    pub bindings: u64,
    pub workers: u64,
}
impl Charge {
    fn add(self, rhs: Self) -> Option<Self> {
        Some(Self {
            bytes: self.bytes.checked_add(rhs.bytes)?,
            requests: self.requests.checked_add(rhs.requests)?,
            bindings: self.bindings.checked_add(rhs.bindings)?,
            workers: self.workers.checked_add(rhs.workers)?,
        })
    }
    fn fits(self, limit: Self) -> bool {
        self.bytes <= limit.bytes
            && self.requests <= limit.requests
            && self.bindings <= limit.bindings
            && self.workers <= limit.workers
    }
    fn sub(self, rhs: Self) -> Option<Self> {
        Some(Self {
            bytes: self.bytes.checked_sub(rhs.bytes)?,
            requests: self.requests.checked_sub(rhs.requests)?,
            bindings: self.bindings.checked_sub(rhs.bindings)?,
            workers: self.workers.checked_sub(rhs.workers)?,
        })
    }
}
pub struct Account {
    limit: Charge,
    used: Charge,
}
impl Account {
    pub const fn new(limit: Charge) -> Self {
        Self {
            limit,
            used: Charge {
                bytes: 0,
                requests: 0,
                bindings: 0,
                workers: 0,
            },
        }
    }
    pub fn used(&self) -> Charge {
        self.used
    }
    /// Both counters are checked before either is modified. The host wrapper
    /// serializes admission with instance death and holds both accounting locks.
    pub fn reserve(local: &mut Self, global: &mut Self, charge: Charge) -> Result<(), Error> {
        let l = local.used.add(charge).ok_or(Error::Overflow)?;
        let g = global.used.add(charge).ok_or(Error::Overflow)?;
        if !l.fits(local.limit) || !g.fits(global.limit) {
            return Err(Error::Quota);
        }
        local.used = l;
        global.used = g;
        Ok(())
    }
    pub fn release(local: &mut Self, global: &mut Self, charge: Charge) -> Result<(), Error> {
        let l = local.used.sub(charge).ok_or(Error::State)?;
        let g = global.used.sub(charge).ok_or(Error::State)?;
        local.used = l;
        global.used = g;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_completion_waits_for_access() {
        let mut r = Retention::prepared();
        r.accept().unwrap();
        r.begin_access().unwrap();
        r.finish().unwrap();
        assert_eq!(r.state(), State::Active);
        assert_eq!(r.release(), Err(Error::Busy));
        assert!(r.begin_access().is_err());
        r.end_access().unwrap();
        assert_eq!(r.state(), State::Retired);
        r.release().unwrap();
        assert!(r.finish().is_err());
    }
    #[test]
    fn death_and_quarantine_do_not_release_active_backing() {
        let mut r = Retention::prepared();
        r.accept().unwrap();
        r.begin_access().unwrap();
        r.detach();
        r.carrier_eviction_complete();
        r.mapping_teardown_complete();
        r.quarantine();
        assert_eq!(r.release(), Err(Error::Busy));
        r.end_access().unwrap();
        assert_eq!(r.release(), Err(Error::Busy));
        r.finish().unwrap();
        r.release().unwrap();
    }
    #[test]
    fn death_requires_both_eviction_and_mapping_teardown() {
        let mut r = Retention::prepared();
        r.accept().unwrap();
        r.detach();
        r.finish().unwrap();
        assert_eq!(r.release(), Err(Error::Busy));
        r.carrier_eviction_complete();
        assert_eq!(r.release(), Err(Error::Busy));
        r.mapping_teardown_complete();
        r.release().unwrap();
    }
    #[test]
    fn quota_rejection_changes_neither_account() {
        let c = Charge {
            bytes: 4096,
            requests: 1,
            bindings: 1,
            workers: 1,
        };
        let mut local = Account::new(c);
        let mut global = Account::new(c);
        Account::reserve(&mut local, &mut global, c).unwrap();
        assert_eq!(
            Account::reserve(&mut local, &mut global, c),
            Err(Error::Quota)
        );
        assert_eq!(local.used(), c);
        assert_eq!(global.used(), c);
        Account::release(&mut local, &mut global, c).unwrap();
        assert_eq!(global.used(), Charge::default());
    }
}
