// SPDX-License-Identifier: MPL-2.0

//! Preallocated host queue storage. The host registry owns this core through
//! completion/teardown; dropping a frontend Queue never destroys it.
//! Every method is called under the queue's host lock, without dispatching
//! backend code, waiting, allocating, or copying application payload under it.

use crate::notification::{Binding, Notification};

pub const COMPLETION_AVAILABLE: u8 = 1;
pub const CAPACITY_AVAILABLE: u8 = 2;
pub const REMOVED: u8 = 4;
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId {
    generation: u64,
    sequence: u64,
}
impl RequestId {
    #[doc(hidden)]
    pub fn __parts(self) -> (u64, u64) {
        (self.generation, self.sequence)
    }
    /// Reconstitutes an identity from the authenticated queue service.
    /// # Safety
    /// Caller must confine this correlation identity to the authenticated queue;
    /// it grants no memory/resource authority, and that queue validates it again.
    #[doc(hidden)]
    pub unsafe fn __from_record(generation: u64, sequence: u64) -> Self {
        Self {
            generation,
            sequence,
        }
    }
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestTag(pub u64);
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    Cancelled = 1,
    Removed,
    Backend,
    Invalid,
    Permission,
    ResourceLimit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    WouldBlock,
    DuplicateTag,
    Removed,
    Exhausted,
}
#[derive(Debug)]
pub struct SubmitError<R> {
    pub reason: Reject,
    pub request: R,
}
pub struct Completion<R> {
    pub id: RequestId,
    pub tag: RequestTag,
    pub result: Result<R, RequestError>,
}

struct Slot<R, T> {
    id: RequestId,
    tag: RequestTag,
    request: Option<R>,
    result: Option<Result<T, RequestError>>,
    started: bool,
    cancelled: bool,
}
pub struct QueueCore<R, T, const N: usize> {
    generation: u64,
    capacity: usize,
    next_sequence: Option<u64>,
    slots: [Option<Slot<R, T>>; N],
    completed: [usize; N],
    head: usize,
    completed_len: usize,
    used: usize,
    removed: bool,
    notification: Notification,
}
impl<R, T, const N: usize> QueueCore<R, T, N> {
    pub fn new(generation: u64) -> Option<Self> {
        if generation == 0 || N == 0 {
            return None;
        }
        let mut notification = Notification::new(REMOVED);
        notification.set_interest(COMPLETION_AVAILABLE | CAPACITY_AVAILABLE, 0, 0);
        Some(Self {
            generation,
            capacity: N,
            next_sequence: Some(1),
            slots: core::array::from_fn(|_| None),
            completed: [0; N],
            head: 0,
            completed_len: 0,
            used: 0,
            removed: false,
            notification,
        })
    }
    /// Negotiates a smaller admission bound within preallocated storage.
    pub fn with_capacity(generation: u64, capacity: usize) -> Option<Self> {
        if capacity == 0 || capacity > N {
            return None;
        }
        let mut queue = Self::new(generation)?;
        queue.capacity = capacity;
        Some(queue)
    }
    /// Caller establishes all resource retention before calling this method;
    /// rejection returns the captured request intact for transactional rollback.
    pub fn try_submit(&mut self, request: R, tag: RequestTag) -> Result<RequestId, SubmitError<R>> {
        let reject = if self.removed {
            Some(Reject::Removed)
        } else if self.slots.iter().flatten().any(|x| x.tag == tag) {
            Some(Reject::DuplicateTag)
        } else if self.next_sequence.is_none() {
            Some(Reject::Exhausted)
        } else if self.used == self.capacity {
            Some(Reject::WouldBlock)
        } else {
            None
        };
        if let Some(reason) = reject {
            return Err(SubmitError { reason, request });
        }
        let sequence = self.next_sequence.unwrap();
        self.next_sequence = sequence.checked_add(1);
        let id = RequestId {
            generation: self.generation,
            sequence,
        };
        let index = self.slots.iter().position(Option::is_none).unwrap();
        // Occupying the slot reserves its final completion credit before the
        // caller can publish the dispatch record or run an inline completion.
        self.slots[index] = Some(Slot {
            id,
            tag,
            request: Some(request),
            result: None,
            started: false,
            cancelled: false,
        });
        self.used += 1;
        Ok(id)
    }
    /// Rolls back provisional admission while the host still owns the lock.
    /// It is illegal after dispatch/publication; identities are never reused.
    #[doc(hidden)]
    pub fn reject_before_dispatch(&mut self, id: RequestId) -> Result<R, RequestError> {
        let index = self
            .slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|slot| slot.id == id))
            .ok_or(RequestError::Invalid)?;
        let slot = self.slots[index].as_ref().unwrap();
        if slot.started || slot.result.is_some() {
            return Err(RequestError::Invalid);
        }
        let mut slot = self.slots[index].take().unwrap();
        self.used -= 1;
        Ok(slot.request.take().unwrap())
    }
    pub fn take_for_dispatch(&mut self, id: RequestId) -> Option<R> {
        let slot = self.slot_mut(id)?;
        if slot.started || slot.result.is_some() {
            return None;
        }
        slot.started = true;
        slot.request.take()
    }
    pub fn cancel(&mut self, id: RequestId) -> Result<(), RequestError> {
        let slot = self.slot_mut(id).ok_or(RequestError::Invalid)?;
        if slot.result.is_some() {
            return Err(RequestError::Invalid);
        }
        slot.cancelled = true;
        Ok(())
    }
    pub fn cancellation_requested(&self, id: RequestId) -> bool {
        self.slots
            .iter()
            .flatten()
            .find(|x| x.id == id)
            .is_some_and(|x| x.cancelled)
    }
    /// Only the sole backend retirement owner may call this after accesses
    /// end. A duplicate finish cannot insert a second completion record.
    pub fn finish(
        &mut self,
        id: RequestId,
        result: Result<T, RequestError>,
    ) -> Result<Option<Binding>, RequestError> {
        let index = self
            .slots
            .iter()
            .position(|x| x.as_ref().is_some_and(|s| s.id == id))
            .ok_or(RequestError::Invalid)?;
        let slot = self.slots[index].as_mut().unwrap();
        if slot.result.is_some() {
            return Err(RequestError::Invalid);
        }
        slot.request = None;
        slot.result = Some(result);
        let tail = (self.head + self.completed_len) % N;
        self.completed[tail] = index;
        self.completed_len += 1;
        Ok(self.notification.publish(COMPLETION_AVAILABLE))
    }
    pub fn try_pop_completion(&mut self) -> Option<Completion<T>> {
        if self.completed_len == 0 {
            return None;
        }
        let index = self.completed[self.head];
        self.head = (self.head + 1) % N;
        self.completed_len -= 1;
        let mut slot = self.slots[index].take().unwrap();
        let was_full = self.used == self.capacity;
        self.used -= 1;
        if was_full {
            self.notification.publish(CAPACITY_AVAILABLE);
        }
        Some(Completion {
            id: slot.id,
            tag: slot.tag,
            result: slot.result.take().unwrap(),
        })
    }
    pub fn remove(&mut self) -> Option<Binding> {
        self.removed = true;
        self.notification.publish(REMOVED)
    }
    pub fn bind_irq(&mut self, binding: Binding) {
        self.notification.bind(binding);
    }
    pub fn unbind_irq(&mut self) {
        self.notification.unbind();
    }
    /// Takes a retained IRQ-routing request, including capacity notifications.
    pub fn take_delivery(&mut self) -> Option<Binding> {
        self.notification.take_delivery()
    }
    pub fn take_events(&mut self) -> u8 {
        self.notification.take_events()
    }
    pub fn rearm_irq(&mut self) -> bool {
        self.notification.rearm(self.completed_len != 0)
    }
    pub fn used(&self) -> usize {
        self.used
    }
    fn slot_mut(&mut self, id: RequestId) -> Option<&mut Slot<R, T>> {
        self.slots.iter_mut().flatten().find(|x| x.id == id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_prepublication_commit_restores_credit_without_reusing_identity() {
        let mut q = QueueCore::<u32, u32, 1>::new(9).unwrap();
        let first = q.try_submit(42, RequestTag(1)).unwrap();
        assert_eq!(q.reject_before_dispatch(first), Ok(42));
        assert_eq!(q.used(), 0);
        let second = q.try_submit(43, RequestTag(1)).unwrap();
        assert_ne!(first, second);
        assert!(q.finish(first, Ok(0)).is_err());
        assert_eq!(q.take_for_dispatch(second), Some(43));
        assert!(q.reject_before_dispatch(second).is_err());
        q.finish(second, Ok(44)).unwrap();
        assert_eq!(q.try_pop_completion().unwrap().result, Ok(44));
    }
    #[test]
    fn credits_duplicate_tags_and_unread_completions() {
        let mut q = QueueCore::<u32, u32, 1>::new(1).unwrap();
        let id = q.try_submit(7, RequestTag(5)).unwrap();
        assert_eq!(
            q.try_submit(8, RequestTag(5)).unwrap_err().reason,
            Reject::DuplicateTag
        );
        assert_eq!(q.try_submit(9, RequestTag(6)).unwrap_err().request, 9);
        q.finish(id, Ok(8)).unwrap();
        assert_eq!(
            q.try_submit(9, RequestTag(5)).unwrap_err().reason,
            Reject::DuplicateTag
        );
        assert_eq!(q.try_pop_completion().unwrap().result, Ok(8));
        assert!(q.try_submit(9, RequestTag(5)).is_ok());
        assert_eq!(q.take_events(), COMPLETION_AVAILABLE | CAPACITY_AVAILABLE);
    }
    #[test]
    fn completions_are_out_of_order_and_once_only() {
        let mut q = QueueCore::<(), u8, 3>::new(3).unwrap();
        let ids = [
            q.try_submit((), RequestTag(1)).unwrap(),
            q.try_submit((), RequestTag(2)).unwrap(),
            q.try_submit((), RequestTag(3)).unwrap(),
        ];
        for i in [2, 0, 1] {
            q.finish(ids[i], Ok(i as u8)).unwrap();
        }
        assert!(q.finish(ids[0], Ok(8)).is_err());
        for i in [2, 0, 1] {
            assert_eq!(q.try_pop_completion().unwrap().id, ids[i]);
        }
        assert!(q.try_pop_completion().is_none());
    }
    #[test]
    fn cancel_and_remove_do_not_complete_active_work() {
        let mut q = QueueCore::<u8, u8, 2>::new(1).unwrap();
        let id = q.try_submit(4, RequestTag(1)).unwrap();
        assert_eq!(q.take_for_dispatch(id), Some(4));
        q.cancel(id).unwrap();
        q.remove();
        assert!(q.try_pop_completion().is_none());
        assert!(q.cancellation_requested(id));
        assert_eq!(
            q.try_submit(5, RequestTag(2)).unwrap_err().reason,
            Reject::Removed
        );
        q.finish(id, Err(RequestError::Cancelled)).unwrap();
        assert_eq!(
            q.try_pop_completion().unwrap().result,
            Err(RequestError::Cancelled)
        );
    }
    #[test]
    fn request_sequence_and_generation_never_wrap() {
        let mut q = QueueCore::<u8, u8, 2>::new(1).unwrap();
        q.next_sequence = Some(u64::MAX);
        let id = q.try_submit(4, RequestTag(1)).unwrap();
        assert_eq!(
            q.try_submit(5, RequestTag(2)).unwrap_err().reason,
            Reject::Exhausted
        );
        let mut other = QueueCore::<u8, u8, 2>::new(2).unwrap();
        assert!(other.finish(id, Ok(5)).is_err());
    }
}
