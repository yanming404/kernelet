// SPDX-License-Identifier: MPL-2.0

//! Generic host pipe metadata. The independently retained, zero-initialized
//! storage lives in the trusted wrapper. This module never creates payload
//! slices or exposes a physical address as access authority.
//! Callers hold one pipe's metadata lock, and release it before payload access.

use crate::notification::{Binding, Notification};

/// Advisory space/data readiness for the corresponding endpoint.
pub const READY: u8 = 1;
/// A direction has reached permanent closure.
pub const CLOSED: u8 = 2;
/// The pipe has been reset and rejects subsequent publication.
pub const RESET: u8 = 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// An allocation-free pipe operation failure.
pub enum PipeError {
    /// No range or role is currently available.
    WouldBlock,
    /// The required direction was closed.
    Closed,
    /// Reset fenced this operation.
    Reset,
    /// The range, token or measured count is invalid.
    Invalid,
    /// An outstanding reservation or access prevents this operation.
    Busy,
    /// Token generation exhausted without wraparound.
    Exhausted,
    /// The caller does not own this endpoint role.
    Permission,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// One uniquely owned data endpoint role.
pub enum Role {
    /// The role that publishes bytes.
    Producer,
    /// The role that releases consumed bytes.
    Consumer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A bounded range and its measured prefix, with a private identity token.
pub struct Reservation {
    token: u64,
    role: Role,
    offset: usize,
    length: usize,
    progress: usize,
}
impl Reservation {
    /// Returns the reservation identity for trusted wrapper correlation.
    pub fn token(&self) -> u64 {
        self.token
    }
    /// Returns the byte offset in retained pipe backing.
    pub fn offset(&self) -> usize {
        self.offset
    }
    /// Returns the contiguous reservation bound.
    pub fn length(&self) -> usize {
        self.length
    }
    /// Returns the prefix measured by completed transfer operations.
    pub fn progress(&self) -> usize {
        self.progress
    }
    /// Returns the bound of the next transfer operation.
    pub fn remaining(&self) -> usize {
        self.length - self.progress
    }
}
struct Endpoint {
    owner: u64,
    reservation: Option<Reservation>,
    in_transfer: bool,
    notification: Notification,
}
impl Endpoint {
    fn new(owner: u64) -> Self {
        Self {
            owner,
            reservation: None,
            in_transfer: false,
            notification: Notification::new(CLOSED | RESET),
        }
    }
}
/// Per-pipe metadata, owned and locked by the trusted host wrapper.
pub struct PipeCore {
    capacity: usize,
    read: usize,
    used: usize,
    next_token: Option<u64>,
    producer_closed: bool,
    consumer_closed: bool,
    reset: bool,
    producer: Endpoint,
    consumer: Endpoint,
}
impl PipeCore {
    /// Creates metadata for nonempty, initialized host-owned storage.
    pub fn new(capacity: usize, producer_owner: u64, consumer_owner: u64) -> Option<Self> {
        if capacity == 0 {
            return None;
        }
        Some(Self {
            capacity,
            read: 0,
            used: 0,
            next_token: Some(1),
            producer_closed: false,
            consumer_closed: false,
            reset: false,
            producer: Endpoint::new(producer_owner),
            consumer: Endpoint::new(consumer_owner),
        })
    }
    fn write(&self) -> usize {
        advance(self.read, self.used, self.capacity)
    }
    fn producer_ready(&self) -> bool {
        !self.reset
            && !self.producer_closed
            && !self.consumer_closed
            && self.producer.reservation.is_none()
            && self.used < self.capacity
    }
    fn consumer_ready(&self) -> bool {
        !self.reset
            && !self.consumer_closed
            && self.consumer.reservation.is_none()
            && self.used != 0
    }
    fn endpoint(&mut self, role: Role) -> &mut Endpoint {
        match role {
            Role::Producer => &mut self.producer,
            Role::Consumer => &mut self.consumer,
        }
    }
    fn verify(&self, role: Role, owner: u64) -> Result<(), PipeError> {
        let e = match role {
            Role::Producer => &self.producer,
            Role::Consumer => &self.consumer,
        };
        if e.owner != owner {
            return Err(PipeError::Permission);
        }
        Ok(())
    }
    fn ready(&self) -> (bool, bool) {
        (self.producer_ready(), self.consumer_ready())
    }
    fn transitions(&mut self, before: (bool, bool)) {
        let after = self.ready();
        if !before.0 && after.0 {
            self.producer.notification.publish(READY);
        }
        if !before.1 && after.1 {
            self.consumer.notification.publish(READY);
        }
    }
    fn token(&mut self) -> Result<u64, PipeError> {
        let token = self.next_token.ok_or(PipeError::Exhausted)?;
        self.next_token = token.checked_add(1);
        Ok(token)
    }
    /// Claims one contiguous producer range without waiting.
    pub fn reserve_producer(&mut self, owner: u64, limit: usize) -> Result<Reservation, PipeError> {
        self.verify(Role::Producer, owner)?;
        if limit == 0 {
            return Err(PipeError::Invalid);
        }
        if self.reset {
            return Err(PipeError::Reset);
        }
        if self.producer_closed || self.consumer_closed {
            return Err(PipeError::Closed);
        }
        if !self.producer_ready() {
            return Err(PipeError::WouldBlock);
        }
        let offset = self.write();
        let length = limit
            .min(self.capacity - self.used)
            .min(self.capacity - offset);
        let reservation = Reservation {
            token: self.token()?,
            role: Role::Producer,
            offset,
            length,
            progress: 0,
        };
        // Reserving sends no readiness notification.
        self.producer.reservation = Some(reservation);
        Ok(reservation)
    }
    /// Claims published data, returning None for drained EOF.
    pub fn reserve_consumer(
        &mut self,
        owner: u64,
        limit: usize,
    ) -> Result<Option<Reservation>, PipeError> {
        self.verify(Role::Consumer, owner)?;
        if limit == 0 {
            return Err(PipeError::Invalid);
        }
        if self.reset {
            return Err(PipeError::Reset);
        }
        if self.consumer_closed {
            return Err(PipeError::Closed);
        }
        if self.consumer.reservation.is_some() {
            return Err(PipeError::WouldBlock);
        }
        if self.used == 0 {
            return if self.producer_closed {
                Ok(None)
            } else {
                Err(PipeError::WouldBlock)
            };
        }
        let length = limit.min(self.used).min(self.capacity - self.read);
        let reservation = Reservation {
            token: self.token()?,
            role: Role::Consumer,
            offset: self.read,
            length,
            progress: 0,
        };
        self.consumer.reservation = Some(reservation);
        Ok(Some(reservation))
    }
    /// Called after a bounded byte-copy operation. The opaque-copy adapter
    /// measures its own count; local callbacks cannot gain authority by lying.
    pub fn record_progress(
        &mut self,
        owner: u64,
        reservation: Reservation,
        bytes: usize,
    ) -> Result<Reservation, PipeError> {
        self.verify(reservation.role, owner)?;
        let current = self
            .endpoint(reservation.role)
            .reservation
            .as_mut()
            .ok_or(PipeError::Invalid)?;
        if *current != reservation {
            return Err(PipeError::Invalid);
        }
        if bytes > current.remaining() {
            return Err(PipeError::Invalid);
        }
        current.progress += bytes;
        Ok(*current)
    }
    /// Finalizes a measured prefix and releases the unused reservation.
    pub fn commit(
        &mut self,
        owner: u64,
        reservation: Reservation,
        bytes: usize,
    ) -> Result<(), PipeError> {
        self.verify(reservation.role, owner)?;
        if self.endpoint(reservation.role).in_transfer {
            return Err(PipeError::Busy);
        }
        let current = self
            .endpoint(reservation.role)
            .reservation
            .ok_or(PipeError::Invalid)?;
        if current != reservation {
            return Err(PipeError::Invalid);
        }
        // Invalid commit lengths do not strand the role; the borrow-owning
        // wrapper also cancels on errors, and finalization is allocation-free.
        if bytes > current.progress {
            return Err(PipeError::Invalid);
        }
        let before = self.ready();
        self.endpoint(reservation.role).reservation = None;
        let result = if self.reset {
            Err(PipeError::Reset)
        } else {
            match reservation.role {
                Role::Producer => {
                    if self.producer_closed || self.consumer_closed {
                        Err(PipeError::Closed)
                    } else {
                        self.used += bytes;
                        Ok(())
                    }
                }
                Role::Consumer => {
                    self.read = advance(self.read, bytes, self.capacity);
                    self.used -= bytes;
                    Ok(())
                }
            }
        };
        self.transitions(before);
        result
    }
    /// Reservation Drop cancels its complete uncommitted prefix. A consumer
    /// leaves the bytes available; a producer publishes none of its writes.
    pub fn cancel(&mut self, owner: u64, reservation: Reservation) -> Result<(), PipeError> {
        self.verify(reservation.role, owner)?;
        if self.endpoint(reservation.role).reservation != Some(reservation) {
            return Err(PipeError::Invalid);
        }
        if self.endpoint(reservation.role).in_transfer {
            return Err(PipeError::Busy);
        }
        let before = self.ready();
        self.endpoint(reservation.role).reservation = None;
        self.transitions(before);
        Ok(())
    }
    /// Fences new production while published bytes remain drainable.
    pub fn close_producer(&mut self) {
        self.producer_closed = true;
        self.consumer.notification.publish(CLOSED);
        self.producer.notification.publish(CLOSED);
    }
    /// Fences consumption and further producer publication.
    pub fn close_consumer(&mut self) {
        self.consumer_closed = true;
        self.producer.notification.publish(CLOSED);
        self.consumer.notification.publish(CLOSED);
    }
    /// Discards logical data without releasing active storage references.
    pub fn reset(&mut self) {
        self.reset = true;
        self.used = 0;
        self.read = 0;
        self.producer.notification.publish(RESET);
        self.consumer.notification.publish(RESET);
    }
    /// Checks transfer admissibility without changing ownership or IRQ state.
    pub fn can_transfer_role(&mut self, role: Role, owner: u64) -> Result<(), PipeError> {
        self.verify(role, owner)?;
        if self.endpoint(role).reservation.is_some() {
            return Err(PipeError::Busy);
        }
        Ok(())
    }
    /// Moves an unreserved role and withdraws its old IRQ binding.
    pub fn transfer_role(&mut self, role: Role, from: u64, to: u64) -> Result<(), PipeError> {
        self.verify(role, from)?;
        let endpoint = self.endpoint(role);
        if endpoint.reservation.is_some() {
            return Err(PipeError::Busy);
        }
        endpoint.notification.unbind();
        endpoint.notification.set_interest(0, 0, 0);
        endpoint.owner = to;
        Ok(())
    }
    /// Binds a route with empty readiness interest and suppressed delivery.
    pub fn bind_irq(&mut self, role: Role, owner: u64, binding: Binding) -> Result<(), PipeError> {
        self.verify(role, owner)?;
        let terminal = self.terminal(role);
        let e = self.endpoint(role);
        e.notification.bind(binding);
        e.notification.set_interest(0, 0, terminal);
        Ok(())
    }
    fn terminal(&self, role: Role) -> u8 {
        if self.reset {
            RESET
        } else if self.consumer_closed
            || (role == Role::Consumer && self.producer_closed)
            || (role == Role::Producer && self.producer_closed)
        {
            CLOSED
        } else {
            0
        }
    }
    /// Updates aggregate readiness interest under the metadata lock.
    pub fn set_interest(&mut self, role: Role, owner: u64, interest: u8) -> Result<(), PipeError> {
        self.verify(role, owner)?;
        let ready = match role {
            Role::Producer => self.producer_ready(),
            Role::Consumer => self.consumer_ready(),
        };
        let terminal = self.terminal(role);
        self.endpoint(role).notification.set_interest(
            interest & READY,
            if ready { READY } else { 0 },
            terminal,
        );
        Ok(())
    }
    /// Takes endpoint causes without reserving data or changing interest.
    pub fn take_events(&mut self, role: Role, owner: u64) -> Result<u8, PipeError> {
        self.verify(role, owner)?;
        Ok(self.endpoint(role).notification.take_events())
    }
    /// Rearms only after eligible latched causes have been consumed.
    pub fn rearm_irq(&mut self, role: Role, owner: u64) -> Result<bool, PipeError> {
        self.verify(role, owner)?;
        Ok(self.endpoint(role).notification.rearm(false))
    }
    /// Pin one actual payload access before leaving the metadata lock.
    /// Close/reset may proceed, but finalization cannot release its reservation.
    pub fn begin_transfer(
        &mut self,
        role: Role,
        owner: u64,
        token: u64,
    ) -> Result<Reservation, PipeError> {
        let reservation = self.reservation(role, owner, token)?;
        if self.reset {
            return Err(PipeError::Reset);
        }
        if self.consumer_closed || (role == Role::Producer && self.producer_closed) {
            return Err(PipeError::Closed);
        }
        let endpoint = self.endpoint(role);
        if endpoint.in_transfer {
            return Err(PipeError::Busy);
        }
        endpoint.in_transfer = true;
        Ok(reservation)
    }
    /// Ends an actual access and records its bounded transferred prefix.
    pub fn end_transfer(
        &mut self,
        role: Role,
        owner: u64,
        token: u64,
        bytes: usize,
    ) -> Result<(), PipeError> {
        let reservation = self.reservation(role, owner, token)?;
        let endpoint = self.endpoint(role);
        if !endpoint.in_transfer {
            return Err(PipeError::Invalid);
        }
        endpoint.in_transfer = false;
        self.record_progress(owner, reservation, bytes)?;
        Ok(())
    }
    /// Resolves a token only for the current authorized endpoint owner.
    pub fn reservation(
        &mut self,
        role: Role,
        owner: u64,
        token: u64,
    ) -> Result<Reservation, PipeError> {
        self.verify(role, owner)?;
        let r = self.endpoint(role).reservation.ok_or(PipeError::Invalid)?;
        if r.token != token {
            return Err(PipeError::Invalid);
        }
        Ok(r)
    }
    /// Observes advisory readiness/terminal state without reserving bytes.
    pub fn current_events(&self, role: Role, owner: u64) -> Result<u8, PipeError> {
        self.verify(role, owner)?;
        let (producer, consumer) = self.ready();
        let ready = match role {
            Role::Producer => producer,
            Role::Consumer => consumer,
        };
        Ok(self.terminal(role) | if ready { READY } else { 0 })
    }
    /// Takes a retained routing request after a metadata operation.
    pub fn take_delivery(&mut self, role: Role) -> Option<Binding> {
        self.endpoint(role).notification.take_delivery()
    }
    /// Reports logical closure and finalization of every reservation.
    pub fn can_release_storage(&self) -> bool {
        (self.reset || (self.producer_closed && self.consumer_closed))
            && self.producer.reservation.is_none()
            && self.consumer.reservation.is_none()
    }
}
fn advance(index: usize, bytes: usize, capacity: usize) -> usize {
    // index < capacity, bytes <= capacity: no addition can overflow.
    let to_end = capacity - index;
    if bytes >= to_end {
        bytes - to_end
    } else {
        index + bytes
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn produce(p: &mut PipeCore, n: usize) {
        let r = p.reserve_producer(1, n).unwrap();
        assert_eq!(r.length(), n);
        let r = p.record_progress(1, r, n).unwrap();
        p.commit(1, r, n).unwrap();
    }
    fn consume(p: &mut PipeCore, n: usize) {
        let r = p.reserve_consumer(2, n).unwrap().unwrap();
        assert_eq!(r.length(), n);
        let r = p.record_progress(2, r, n).unwrap();
        p.commit(2, r, n).unwrap();
    }
    #[test]
    fn active_payload_access_cannot_be_finalized_or_reused() {
        let mut p = PipeCore::new(8, 1, 2).unwrap();
        let r = p.reserve_producer(1, 8).unwrap();
        p.begin_transfer(Role::Producer, 1, r.token()).unwrap();
        assert_eq!(p.commit(1, r, 0), Err(PipeError::Busy));
        assert_eq!(p.cancel(1, r), Err(PipeError::Busy));
        p.reset();
        assert!(!p.can_release_storage());
        p.end_transfer(Role::Producer, 1, r.token(), 4).unwrap();
        let current = p.reservation(Role::Producer, 1, r.token()).unwrap();
        assert_eq!(p.commit(1, current, 4), Err(PipeError::Reset));
        assert!(p.can_release_storage());
    }
    #[test]
    fn partial_progress_wraparound_and_eof() {
        let mut p = PipeCore::new(8, 1, 2).unwrap();
        produce(&mut p, 6);
        consume(&mut p, 4);
        let r = p.reserve_producer(1, 8).unwrap();
        assert_eq!((r.offset(), r.length()), (6, 2));
        let r = p.record_progress(1, r, 2).unwrap();
        p.commit(1, r, 2).unwrap();
        consume(&mut p, 4);
        produce(&mut p, 4);
        p.close_producer();
        consume(&mut p, 4);
        assert_eq!(p.reserve_consumer(2, 1), Ok(None));
    }
    #[test]
    fn cancelled_consume_is_peek_and_partial_commit_retains_suffix() {
        let mut p = PipeCore::new(8, 1, 2).unwrap();
        produce(&mut p, 5);
        let r = p.reserve_consumer(2, 5).unwrap().unwrap();
        let r = p.record_progress(2, r, 4).unwrap();
        p.commit(2, r, 0).unwrap();
        let r = p.reserve_consumer(2, 5).unwrap().unwrap();
        assert_eq!(r.length(), 5);
        let r = p.record_progress(2, r, 4).unwrap();
        p.commit(2, r, 2).unwrap();
        assert_eq!(p.reserve_consumer(2, 8).unwrap().unwrap().length(), 3);
    }
    #[test]
    fn forgotten_reservation_blocks_reuse_and_transfer_but_not_reset() {
        let mut p = PipeCore::new(8, 1, 2).unwrap();
        let r = p.reserve_producer(1, 8).unwrap();
        assert_eq!(p.reserve_producer(1, 1), Err(PipeError::WouldBlock));
        assert_eq!(p.transfer_role(Role::Producer, 1, 3), Err(PipeError::Busy));
        p.reset();
        assert!(!p.can_release_storage());
        assert_eq!(p.commit(1, r, 0), Err(PipeError::Reset));
        assert!(p.can_release_storage());
    }
    #[test]
    fn zero_byte_cancel_restores_readiness_and_interest_is_independent() {
        let mut p = PipeCore::new(8, 1, 2).unwrap();
        p.set_interest(Role::Producer, 1, READY).unwrap();
        assert_eq!(p.take_events(Role::Producer, 1).unwrap(), READY);
        let r = p.reserve_producer(1, 2).unwrap();
        assert_eq!(p.take_events(Role::Producer, 1).unwrap(), 0);
        p.cancel(1, r).unwrap();
        assert_eq!(p.take_events(Role::Producer, 1).unwrap(), READY);
        assert!(p.rearm_irq(Role::Producer, 1).unwrap());
        assert!(p.rearm_irq(Role::Producer, 1).unwrap());
    }
    #[test]
    fn close_fences_active_production_and_preserves_published_data() {
        let mut p = PipeCore::new(8, 1, 2).unwrap();
        produce(&mut p, 2);
        let r = p.reserve_producer(1, 4).unwrap();
        let r = p.record_progress(1, r, 4).unwrap();
        p.close_producer();
        assert_eq!(p.commit(1, r, 4), Err(PipeError::Closed));
        consume(&mut p, 2);
        assert_eq!(p.reserve_consumer(2, 1), Ok(None));
    }
    #[test]
    fn progress_and_authority_cannot_be_forged() {
        let mut p = PipeCore::new(8, 1, 2).unwrap();
        assert_eq!(p.reserve_producer(2, 1), Err(PipeError::Permission));
        let r = p.reserve_producer(1, 4).unwrap();
        assert_eq!(p.record_progress(1, r, 5), Err(PipeError::Invalid));
        assert_eq!(p.commit(1, r, 1), Err(PipeError::Invalid));
        p.cancel(1, r).unwrap();
        assert!(p.commit(1, r, 0).is_err());
    }
}
