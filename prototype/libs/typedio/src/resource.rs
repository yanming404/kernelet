// SPDX-License-Identifier: MPL-2.0

//! Preallocated resource-hook transactions. Identifiers contain no usable
//! address. A trusted host table validates owner, generation and directions;
//! committed references stay in that table until retirement explicitly drops
//! them. Forgetting guest handles never decrements host references.

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceId {
    slot: u32,
    generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dma,
    Producer,
    Consumer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    ReadWrite,
}
impl Access {
    fn allows(self, requested: Self) -> bool {
        self == Self::ReadWrite || self == requested
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Permission,
    Busy,
    Quota,
    Exhausted,
}
#[derive(Clone, Copy)]
struct Record {
    owner: u64,
    kind: Kind,
    length: u64,
    access: Access,
    pending: u32,
    active: u32,
    reservation: bool,
    revoked: bool,
    transfer_pending: bool,
}
struct Slot {
    generation: u64,
    record: Option<Record>,
}
pub struct ResourceTable<const N: usize> {
    slots: [Slot; N],
}
impl<const N: usize> Default for ResourceTable<N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<const N: usize> ResourceTable<N> {
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| Slot {
                generation: 0,
                record: None,
            }),
        }
    }
    /// Called only after trusted backing creation/registration and quota charge.
    pub fn register(
        &mut self,
        owner: u64,
        kind: Kind,
        length: u64,
        access: Access,
    ) -> Result<ResourceId, Error> {
        let slot = self
            .slots
            .iter()
            .position(|x| x.record.is_none() && x.generation != u64::MAX)
            .ok_or(Error::Quota)?;
        if slot > u32::MAX as usize {
            return Err(Error::Exhausted);
        }
        let entry = &mut self.slots[slot];
        entry.generation += 1;
        entry.record = Some(Record {
            owner,
            kind,
            length,
            access,
            pending: 0,
            active: 0,
            reservation: false,
            revoked: false,
            transfer_pending: false,
        });
        Ok(ResourceId {
            slot: slot as u32,
            generation: entry.generation,
        })
    }
    fn record(&mut self, id: ResourceId) -> Result<&mut Record, Error> {
        let slot = self.slots.get_mut(id.slot as usize).ok_or(Error::Invalid)?;
        if slot.generation != id.generation {
            return Err(Error::Invalid);
        }
        slot.record.as_mut().ok_or(Error::Invalid)
    }
    pub fn set_reserved(
        &mut self,
        id: ResourceId,
        owner: u64,
        reserved: bool,
    ) -> Result<(), Error> {
        let r = self.record(id)?;
        if r.owner != owner || r.revoked {
            return Err(Error::Permission);
        }
        if r.transfer_pending {
            return Err(Error::Busy);
        }
        r.reservation = reserved;
        Ok(())
    }
    pub fn revoke_owner(&mut self, owner: u64) {
        for record in self.slots.iter_mut().filter_map(|x| x.record.as_mut()) {
            if record.owner == owner {
                record.revoked = true;
            }
        }
    }
    pub fn unregister(&mut self, id: ResourceId) -> Result<(), Error> {
        let r = self.record(id)?;
        if r.active != 0 || r.pending != 0 || r.reservation || r.transfer_pending {
            return Err(Error::Busy);
        }
        self.slots[id.slot as usize].record = None;
        Ok(())
    }
    pub fn retire_reference(&mut self, id: ResourceId) -> Result<(), Error> {
        let r = self.record(id)?;
        r.active = r.active.checked_sub(1).ok_or(Error::Invalid)?;
        Ok(())
    }
    pub fn prepare<const M: usize>(&mut self, owner: u64) -> Transaction<'_, N, M> {
        Transaction {
            table: self,
            owner,
            actions: [None; M],
            len: 0,
            committed: false,
        }
    }
}
#[derive(Clone, Copy)]
enum Action {
    Retain(ResourceId),
    Move(ResourceId, u64),
}
/// Lives only during synchronous capture/admission. Its Drop rolls back every
/// prepared hook; after commit, independent host records own the resources.
pub struct Transaction<'a, const N: usize, const M: usize> {
    table: &'a mut ResourceTable<N>,
    owner: u64,
    actions: [Option<Action>; M],
    len: usize,
    committed: bool,
}
impl<const N: usize, const M: usize> Transaction<'_, N, M> {
    pub fn retain_dma(
        &mut self,
        id: ResourceId,
        offset: u64,
        length: u64,
        access: Access,
    ) -> Result<(), Error> {
        if self.len == M {
            return Err(Error::Quota);
        }
        let r = self.table.record(id)?;
        if r.owner != self.owner || r.revoked || r.kind != Kind::Dma || !r.access.allows(access) {
            return Err(Error::Permission);
        }
        if length == 0 || length > r.length || offset > r.length - length {
            return Err(Error::Invalid);
        }
        // Reserve both arithmetic and final-record capacity before acceptance.
        if r.active
            .checked_add(r.pending)
            .and_then(|n| n.checked_add(1))
            .is_none()
        {
            return Err(Error::Quota);
        }
        r.pending += 1;
        self.actions[self.len] = Some(Action::Retain(id));
        self.len += 1;
        Ok(())
    }
    pub fn move_endpoint(&mut self, id: ResourceId, destination: u64) -> Result<(), Error> {
        if self.len == M {
            return Err(Error::Quota);
        }
        let r = self.table.record(id)?;
        if r.owner != self.owner || r.revoked || r.kind == Kind::Dma {
            return Err(Error::Permission);
        }
        if r.reservation || r.transfer_pending {
            return Err(Error::Busy);
        }
        r.transfer_pending = true;
        self.actions[self.len] = Some(Action::Move(id, destination));
        self.len += 1;
        Ok(())
    }
    pub fn commit(mut self) {
        // The table lock is held continuously from preparation to this point.
        // All counters and moves were checked before acceptance; this cannot fail.
        for action in self.actions[..self.len].iter().flatten() {
            match *action {
                Action::Retain(id) => {
                    let r = self.table.record(id).unwrap();
                    r.pending -= 1;
                    r.active += 1;
                }
                Action::Move(id, destination) => {
                    let r = self.table.record(id).unwrap();
                    r.transfer_pending = false;
                    r.owner = destination;
                }
            }
        }
        self.committed = true;
    }
}
impl<const N: usize, const M: usize> Drop for Transaction<'_, N, M> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for action in self.actions[..self.len].iter().flatten() {
            match *action {
                Action::Retain(id) => self.table.record(id).unwrap().pending -= 1,
                Action::Move(id, _) => self.table.record(id).unwrap().transfer_pending = false,
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_capture_failure_rolls_back_prior_retention() {
        let mut t = ResourceTable::<2>::new();
        let a = t.register(1, Kind::Dma, 4096, Access::Read).unwrap();
        let b = t.register(1, Kind::Dma, 4096, Access::Write).unwrap();
        {
            let mut tx = t.prepare::<2>(1);
            tx.retain_dma(a, 0, 4096, Access::Read).unwrap();
            assert_eq!(tx.retain_dma(b, 0, 1, Access::Read), Err(Error::Permission));
        }
        t.unregister(a).unwrap();
        t.unregister(b).unwrap();
    }
    #[test]
    fn retained_backing_outlives_death_and_stale_generation() {
        let mut t = ResourceTable::<1>::new();
        let a = t.register(1, Kind::Dma, 4096, Access::ReadWrite).unwrap();
        {
            let mut tx = t.prepare::<1>(1);
            tx.retain_dma(a, 0, 4, Access::Read).unwrap();
            tx.commit();
        }
        t.revoke_owner(1);
        assert_eq!(t.unregister(a), Err(Error::Busy));
        t.retire_reference(a).unwrap();
        t.unregister(a).unwrap();
        let b = t.register(1, Kind::Dma, 4096, Access::ReadWrite).unwrap();
        assert_ne!(a, b);
        assert_eq!(t.retire_reference(a), Err(Error::Invalid));
    }
    #[test]
    fn endpoint_capture_rollback_preserves_original_owner() {
        let mut t = ResourceTable::<2>::new();
        let a = t.register(1, Kind::Producer, 128, Access::Write).unwrap();
        let b = t.register(1, Kind::Consumer, 128, Access::Read).unwrap();
        t.set_reserved(b, 1, true).unwrap();
        {
            let mut tx = t.prepare::<2>(1);
            tx.move_endpoint(a, 2).unwrap();
            assert_eq!(tx.move_endpoint(b, 2), Err(Error::Busy));
        }
        t.set_reserved(a, 1, true).unwrap();
        t.set_reserved(a, 1, false).unwrap();
        {
            let mut tx = t.prepare::<1>(1);
            tx.move_endpoint(a, 2).unwrap();
            tx.commit();
        }
        assert_eq!(t.set_reserved(a, 1, true), Err(Error::Permission));
        t.set_reserved(a, 2, true).unwrap();
    }
    #[test]
    fn overflowed_ranges_are_rejected_before_any_reference() {
        let mut t = ResourceTable::<1>::new();
        let a = t.register(1, Kind::Dma, 4096, Access::ReadWrite).unwrap();
        {
            let mut tx = t.prepare::<1>(1);
            assert_eq!(
                tx.retain_dma(a, u64::MAX - 1, 4, Access::Read),
                Err(Error::Invalid)
            );
        }
        t.unregister(a).unwrap();
    }
}
